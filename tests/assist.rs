//! The guided manual mode: the order of the walk, the check of each reference, skip and back, the checklist that can be read again, and the words in the eight languages. No machine is involved.

use eh3d_phoenix_pnp::assist::{feedback, prompt, Answer, Assist, Order, Status};
use eh3d_phoenix_pnp::error::Lang;
use eh3d_phoenix_pnp::feeders::from_csv as feeders_csv;
use eh3d_phoenix_pnp::parts::{parse, Part};

fn board() -> Vec<Part> {
    parse(include_str!("../examples/board.pos")).unwrap()
}

fn walk(order: Order) -> Assist {
    let feeders = feeders_csv(include_str!("../examples/feeders.csv")).unwrap();
    Assist::new(&board(), &feeders, order, &["D*".to_string()], &[]).unwrap()
}

fn refs(a: &Assist) -> Vec<String> {
    a.steps().iter().map(|s| s.part.reference.clone()).collect()
}

#[test]
fn the_walk_takes_a_reel_at_a_time_the_top_side_before_the_bottom() {
    let a = walk(Order::Reel);
    // packages in order: C_0603, QFN-32, R_0603 (top), then LED_0603 (bottom)
    assert_eq!(refs(&a), vec!["C1", "C2", "U1", "R1", "R2", "R3", "D1"]);
    let a = walk(Order::Reference);
    assert_eq!(refs(&a), vec!["C1", "C2", "R1", "R2", "R3", "U1", "D1"]);
}

#[test]
fn the_references_of_a_reel_are_in_the_natural_order() {
    let parts: Vec<Part> = ["R10", "R2", "R1", "R11", "R9"]
        .iter()
        .map(|r| Part {
            reference: r.to_string(),
            value: "10k".into(),
            package: "R_0603".into(),
            x: 1.0,
            y: 2.0,
            rotation: 0.0,
            side: eh3d_phoenix_pnp::parts::Side::Top,
        })
        .collect();
    let a = Assist::new(&parts, &[], Order::Reel, &[], &[]).unwrap();
    assert_eq!(refs(&a), vec!["R1", "R2", "R9", "R10", "R11"]);
}

#[test]
fn a_step_knows_its_feeder_its_place_and_whether_it_has_a_direction() {
    let a = walk(Order::Reel);
    let c1 = &a.steps()[0];
    assert_eq!(c1.feeder.as_deref(), Some("F2"));
    assert_eq!(c1.pick, Some((-20.0, 20.0)));
    assert!(!c1.polar);
    let d1 = a.steps().iter().find(|s| s.part.reference == "D1").unwrap();
    assert!(d1.polar);
    let none = Assist::new(&board(), &[], Order::Reel, &[], &[]).unwrap();
    assert!(none
        .steps()
        .iter()
        .all(|s| s.feeder.is_none() && s.pick.is_none()));
}

#[test]
fn only_the_right_reference_goes_on_and_a_wrong_one_does_not_move_the_walk() {
    let mut a = walk(Order::Reel);
    assert_eq!(
        a.respond("c2"),
        Answer::Wrong {
            expected: "C1".into(),
            got: "c2".into()
        }
    );
    assert_eq!(a.cursor(), Some(0));
    assert_eq!(a.respond("  c1 "), Answer::Placed("C1".into()));
    assert_eq!(a.cursor(), Some(1));
    assert_eq!(a.progress(), (1, 0, 7));
    assert_eq!(
        a.respond(""),
        Answer::Wrong {
            expected: "C2".into(),
            got: String::new()
        }
    );
}

#[test]
fn skip_leaves_a_part_for_later_and_back_takes_the_last_step_back() {
    let mut a = walk(Order::Reel);
    assert_eq!(a.respond("skip"), Answer::Skipped("C1".into()));
    assert_eq!(a.status()[0], Status::Skipped);
    assert_eq!(a.current().unwrap().part.reference, "C2");
    assert_eq!(a.respond("C2"), Answer::Placed("C2".into()));
    assert_eq!(a.respond("back"), Answer::Undone("C2".into()));
    assert_eq!(a.current().unwrap().part.reference, "C2");
    assert_eq!(a.respond("back"), Answer::Undone("C1".into()));
    assert_eq!(a.respond("back"), Answer::NothingToUndo);
    assert_eq!(a.current().unwrap().part.reference, "C1");
}

#[test]
fn a_whole_walk_ends_done_and_the_skipped_parts_come_back() {
    let mut a = walk(Order::Reel);
    for r in ["C1", "C2", "U1", "R1", "R2"] {
        assert!(matches!(a.respond(r), Answer::Placed(_)));
    }
    assert_eq!(a.respond("skip"), Answer::Skipped("R3".into()));
    assert_eq!(a.respond("D1"), Answer::Placed("D1".into()));
    assert!(a.finished() && a.current().is_none());
    assert_eq!(a.skipped().len(), 1);
    a.revisit_skipped();
    assert_eq!(a.current().unwrap().part.reference, "R3");
    assert_eq!(a.respond("R3"), Answer::Placed("R3".into()));
    assert_eq!(a.progress(), (7, 0, 7));
    assert_eq!(a.respond("anything"), Answer::Done);
}

#[test]
fn quit_stops_the_walk_and_the_checklist_keeps_what_was_placed() {
    let mut a = walk(Order::Reel);
    a.respond("C1");
    assert_eq!(a.respond("quit"), Answer::Quit);
    assert!(a.current().is_none());
    let csv = a.checklist_csv();
    assert!(
        csv.starts_with("reference,status\nC1,placed\nC2,pending\n"),
        "{csv}"
    );
}

#[test]
fn a_checklist_is_read_again_and_the_walk_goes_on_from_it() {
    let mut a = walk(Order::Reel);
    for r in ["C1", "C2", "U1"] {
        a.respond(r);
    }
    a.respond("skip");
    let saved = a.checklist_csv();
    let mut b = walk(Order::Reel);
    assert_eq!(b.resume(&saved).unwrap(), 4);
    assert_eq!(b.progress(), (3, 1, 7));
    assert_eq!(b.current().unwrap().part.reference, "R2");
    assert_eq!(b.checklist_csv(), saved);
}

#[test]
fn a_checklist_that_is_wrong_changes_nothing_and_says_where() {
    let mut a = walk(Order::Reel);
    a.respond("C1");
    let before = a.checklist_csv();
    for (text, line) in [
        ("reference,status\nC1,placed\nZZ9,placed\n", "line 3"),
        ("reference,status\nC2,done\n", "line 2"),
        ("reference,status\nC2\n", "line 2"),
    ] {
        let e = a.resume(text).unwrap_err();
        assert!(e.contains(line), "{e}");
        assert_eq!(a.checklist_csv(), before);
    }
    assert_eq!(a.resume("").unwrap(), 0);
}

#[test]
fn two_parts_with_the_same_reference_are_refused() {
    let mut p = board();
    p.push(p[0].clone());
    assert!(Assist::new(&p, &[], Order::Reel, &[], &[])
        .unwrap_err()
        .contains("twice"));
    let mut q = board();
    let mut dup = q[0].clone();
    dup.reference = dup.reference.to_lowercase();
    q.push(dup);
    assert!(Assist::new(&q, &[], Order::Reel, &[], &[]).is_err());
}

#[test]
fn an_empty_board_has_nothing_to_walk() {
    let mut a = Assist::new(&[], &[], Order::Reel, &[], &[]).unwrap();
    assert!(a.current().is_none() && a.finished());
    assert_eq!(a.respond("R1"), Answer::Done);
    assert_eq!(a.checklist_csv(), "reference,status\n");
}

#[test]
fn the_prompt_and_every_answer_are_in_the_eight_languages() {
    let a = walk(Order::Reel);
    let mut prompts = std::collections::BTreeSet::new();
    for l in Lang::ALL {
        let p = prompt(&a, l);
        assert!(
            p.contains("C1") && p.contains("F2") && !p.contains('{'),
            "{p}"
        );
        prompts.insert(p);
    }
    assert_eq!(prompts.len(), 8);
    // a part with a direction and on the bottom side
    let mut b = walk(Order::Reel);
    for r in ["C1", "C2", "U1", "R1", "R2", "R3"] {
        b.respond(r);
    }
    for l in Lang::ALL {
        let p = prompt(&b, l);
        assert!(p.contains("D1") && !p.contains('{'), "{p}");
    }
    assert!(
        prompt(&b, Lang::En).contains("bottom side") && prompt(&b, Lang::En).contains("direction")
    );
    assert!(prompt(&b, Lang::Pt).contains("face inferior"));
    for ans in [
        Answer::Placed("R1".into()),
        Answer::Wrong {
            expected: "R1".into(),
            got: "R2".into(),
        },
        Answer::Skipped("R1".into()),
        Answer::Undone("R1".into()),
        Answer::NothingToUndo,
        Answer::Done,
        Answer::Quit,
    ] {
        let texts: std::collections::BTreeSet<String> =
            Lang::ALL.iter().map(|l| feedback(&ans, &a, *l)).collect();
        // Spanish and Portuguese say some short things with the same words
        assert!(texts.len() >= 7, "{ans:?}");
        assert!(texts.iter().all(|t| !t.contains('{')));
    }
}
