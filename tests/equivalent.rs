//! Parts that another part already on a feeder can stand for: a part that has a feeder of its own stays on it, one that has not goes to the feeder with the most parts left that holds an
//! equivalent, a part is never served by a feeder that ran out, the answer does not depend on the order of the files, and the command ends with 2 when a part still has no feeder.

use eh3d_phoenix_pnp::equivalents::{parts_csv, resolve, Groups};
use eh3d_phoenix_pnp::feeders::Feeder;
use eh3d_phoenix_pnp::parts::{from_csv, Part, Side};
use std::process::{Command, Output};

const GROUPS: &str = include_str!("../examples/equivalents.csv");

fn part(reference: &str, value: &str, package: &str) -> Part {
    Part {
        reference: reference.into(),
        value: value.into(),
        package: package.into(),
        x: 1.0,
        y: 2.0,
        rotation: 0.0,
        side: Side::Top,
    }
}

fn feeder(id: &str, value: &str, package: &str, count: u32) -> Feeder {
    Feeder {
        id: id.into(),
        value: value.into(),
        package: package.into(),
        x: 0.0,
        y: 0.0,
        count,
    }
}

fn groups() -> Groups {
    Groups::parse(&GROUPS.replace("\r\n", "\n")).unwrap()
}

#[test]
fn a_part_with_a_feeder_of_its_own_stays_and_the_others_go_to_an_equivalent() {
    let parts = [
        part("R1", "10k", "R_0603"),
        part("R2", "10k", "R_0402"),
        part("R3", "10.0k", "R_0603"),
    ];
    let feeders = [feeder("F1", "10k", "R_0402", 10)];
    let o = resolve(&parts, &feeders, &groups());
    assert_eq!(o.own, 1);
    assert_eq!(
        o.swaps
            .iter()
            .map(|s| s.reference.as_str())
            .collect::<Vec<_>>(),
        ["R1", "R3"]
    );
    assert!(o.swaps.iter().all(|s| s.feeder == "F1"));
    assert!(o.missing.is_empty());
    // two kinds of part did not need a reel of their own
    assert_eq!(o.reels_saved(), 2);
}

#[test]
fn the_case_of_a_value_does_not_matter_and_a_part_outside_the_groups_has_no_feeder() {
    let parts = [part("C1", "0.1U", "c_0603"), part("L1", "10u", "L_0805")];
    let feeders = [feeder("F1", "100N", "C_0603", 5)];
    let o = resolve(&parts, &feeders, &groups());
    assert_eq!(o.swaps.len(), 1);
    assert_eq!(o.swaps[0].feeder, "F1");
    assert_eq!(o.missing, ["L1"]);
}

#[test]
fn a_feeder_that_ran_out_does_not_serve_and_the_one_with_most_left_is_taken() {
    let parts: Vec<Part> = (1..=6)
        .map(|i| part(&format!("R{i}"), "10k", "R_0603"))
        .collect();
    let feeders = [
        feeder("A", "10k", "R_0402", 2),
        feeder("B", "10.0k", "R_0603", 3),
        feeder("C", "10k", "R_0402", 0),
    ];
    // the parts have no feeder of their own: B has "10.0k R_0603", which is its own key only for a part of that name
    let o = resolve(&parts, &feeders, &groups());
    let by: Vec<&str> = o.swaps.iter().map(|s| s.feeder.as_str()).collect();
    // B has 3 and A has 2: B first, then whichever has the most left each time, the lowest id when they tie
    assert_eq!(by, ["B", "A", "B", "A", "B"], "{by:?}");
    assert_eq!(o.swaps.len(), 5);
    // C is empty and was never used, and the sixth part has nowhere to go
    assert!(!by.contains(&"C"));
    assert_eq!(o.missing, ["R6"]);
}

#[test]
fn what_a_feeder_has_left_after_its_own_parts_is_all_that_it_can_give() {
    // F1 holds 3 and 2 parts need it for themselves, so one is left for a part that has no feeder
    let parts = [
        part("R1", "10k", "R_0402"),
        part("R2", "10k", "R_0402"),
        part("R3", "10k", "R_0603"),
        part("R4", "10k", "R_0603"),
    ];
    let o = resolve(&parts, &[feeder("F1", "10k", "R_0402", 3)], &groups());
    assert_eq!((o.own, o.swaps.len(), o.missing.len()), (2, 1, 1));
    assert_eq!(o.missing, ["R4"]);
}

#[test]
fn the_answer_does_not_depend_on_the_order_of_the_feeders() {
    let parts: Vec<Part> = (1..=7)
        .map(|i| part(&format!("R{i}"), "10k", "R_0603"))
        .collect();
    let mut feeders = vec![
        feeder("F3", "10k", "R_0402", 4),
        feeder("F1", "10.0k", "R_0603", 4),
        feeder("F2", "10k", "R_0402", 4),
    ];
    let base = resolve(&parts, &feeders, &groups());
    let mut seed: u64 = 31;
    for _ in 0..30 {
        // a shuffle from a fixed seed
        for i in (1..feeders.len()).rev() {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            feeders.swap(i, ((seed >> 33) % (i as u64 + 1)) as usize);
        }
        assert_eq!(resolve(&parts, &feeders, &groups()), base);
    }
}

#[test]
fn a_part_in_two_groups_or_a_table_that_is_not_one_is_refused() {
    let head = "group,value,package\n";
    assert!(Groups::parse("").is_err());
    assert!(Groups::parse("a,b,c\n").is_err());
    assert!(Groups::parse(head).is_err());
    assert!(Groups::parse(&format!("{head}g1,10k,R_0603\ng2,10K,r_0603\n")).is_err());
    assert!(Groups::parse(&format!("{head},10k,R_0603\n")).is_err());
    assert!(Groups::parse(&format!("{head}g1,,R_0603\n")).is_err());
    assert!(Groups::parse(&format!("{head}g1,10k\n")).is_err());
    // the same row twice is not a conflict
    let g = Groups::parse(&format!("{head}g1,10k,R_0603\ng1,10K,r_0603\n")).unwrap();
    assert_eq!(g.len(), 1);
    assert_eq!(g.group_of("10K", "R_0603"), Some("g1"));
}

#[test]
fn the_table_written_gives_the_swapped_parts_the_value_of_their_feeder_and_reads_back() {
    let parts = from_csv(
        "ref,val,package,x,y,rot,side\nR1,10k,R_0603,1,2,0,top\nR2,10k,R_0402,3,4,90,bottom\n",
    )
    .unwrap();
    let o = resolve(&parts, &[feeder("F1", "10k", "R_0402", 9)], &groups());
    let csv = parts_csv(&parts, &o);
    let back = from_csv(&csv).unwrap();
    assert_eq!(back.len(), 2);
    assert_eq!(
        (back[0].value.as_str(), back[0].package.as_str()),
        ("10k", "R_0402")
    );
    assert_eq!(back[1].side, Side::Bottom);
    assert_eq!(back[1].rotation, 90.0);
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_phoenix-pnp"))
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap()
}

#[test]
fn the_command_lists_the_swaps_and_ends_with_2_when_a_part_has_no_feeder() {
    let args = [
        "equivalents",
        "examples/board_equiv.pos",
        "examples/feeders_drag.csv",
        "examples/equivalents.csv",
    ];
    let o = run(&args);
    let text = String::from_utf8_lossy(&o.stdout).into_owned();
    assert_eq!(o.status.code(), Some(0), "{text}");
    assert!(text.contains("R1: 10k R_0603 -> F1 (10k R_0402)"), "{text}");
    assert!(
        text.contains("3 reels are not needed; 0 have no feeder"),
        "{text}"
    );
    // a board that has a part no group names
    let dir = std::env::temp_dir().join(format!("phoenix_pnp_equiv_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let pos = dir.join("odd.pos");
    std::fs::write(
        &pos,
        "ref,val,package,x,y,rot,side\nL1,10u,L_0805,1,2,0,top\n",
    )
    .unwrap();
    let o = run(&["equivalents", pos.to_str().unwrap(), args[2], args[3]]);
    assert_eq!(o.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&o.stderr).contains("L1"));
    // the table of parts is written once and never over a file
    let table = dir.join("new.csv");
    let mut with_out = args.to_vec();
    with_out.extend(["--out", table.to_str().unwrap()]);
    assert_eq!(run(&with_out).status.code(), Some(0));
    assert!(std::fs::read_to_string(&table)
        .unwrap()
        .contains("R1,10k,R_0402,10.000,20.000,0.0,top"));
    assert_eq!(run(&with_out).status.code(), Some(1));
    // wrong arguments
    assert_eq!(run(&args[..3]).status.code(), Some(1));
    let _ = std::fs::remove_dir_all(&dir);
}
