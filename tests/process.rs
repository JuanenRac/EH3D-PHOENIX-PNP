//! The process of a board: its phases, their order, the load-out that is fixed once a phase begins, the state kept between two sessions, and the guided manual mode that walks it.

use eh3d_phoenix_pnp::error::Lang;
use eh3d_phoenix_pnp::parts::Side;
use eh3d_phoenix_pnp::process::{
    confirm_prompt, phase_header, Doer, PhaseState, Process, ProcessError, Task, MAX_NAME,
    MAX_PHASES, MAX_PROCESS_BYTES, MAX_TASKS,
};
use std::path::PathBuf;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_phoenix-pnp");

fn example(name: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join(name)
        .display()
        .to_string()
}

const FILE: &str =
    "# a board with one side placed by the machine and the parts by hand after the oven\n\
phase = Machine side | load, place top, reflow\n\
phase = Hand side | place bottom by hand, solder by hand, inspect\n";

fn parse(text: &str) -> Process {
    Process::parse(text).unwrap()
}

#[test]
fn every_task_has_a_word_that_reads_back_and_the_words_do_not_mind_case_or_spaces() {
    let all = [
        Task::Load,
        Task::Place(Side::Top, Doer::Machine),
        Task::Place(Side::Bottom, Doer::Machine),
        Task::Place(Side::Top, Doer::Hand),
        Task::Place(Side::Bottom, Doer::Hand),
        Task::Reflow,
        Task::HandSolder,
        Task::Glue,
        Task::ThroughHole,
        Task::Inspect,
    ];
    for t in all {
        assert_eq!(Task::parse(&t.word()), Some(t), "{}", t.word());
    }
    assert_eq!(
        Task::parse("  PLACE   Top  By   Hand "),
        Some(Task::Place(Side::Top, Doer::Hand))
    );
    assert_eq!(
        Task::parse("place bottom by machine"),
        Some(Task::Place(Side::Bottom, Doer::Machine))
    );
    for bad in [
        "",
        "place",
        "place left",
        "place top by robot",
        "reflow twice",
        "solder",
        "paste",
    ] {
        assert_eq!(Task::parse(bad), None, "{bad}");
    }
}

#[test]
fn every_task_is_told_to_the_person_in_the_eight_languages() {
    for t in [
        Task::Load,
        Task::Place(Side::Top, Doer::Hand),
        Task::Place(Side::Bottom, Doer::Machine),
        Task::Reflow,
        Task::HandSolder,
        Task::Glue,
        Task::ThroughHole,
        Task::Inspect,
    ] {
        let mut seen = std::collections::BTreeSet::new();
        for lang in Lang::ALL {
            let text = t.describe(lang);
            assert!(
                !text.is_empty() && !text.contains('{'),
                "{lang:?} {}",
                t.word()
            );
            seen.insert(text);
        }
        assert!(seen.len() >= 6, "{}: {seen:?}", t.word());
    }
    assert!(Task::Place(Side::Top, Doer::Hand)
        .describe(Lang::En)
        .contains("top"));
    assert!(Task::Place(Side::Top, Doer::Hand)
        .describe(Lang::Es)
        .contains("superior"));
}

#[test]
fn a_process_file_is_read_with_its_comments_and_written_back() {
    let p = parse(FILE);
    assert_eq!(p.phases.len(), 2);
    assert_eq!(p.phases[0].name, "Machine side");
    assert_eq!(
        p.phases[0].tasks,
        [
            Task::Load,
            Task::Place(Side::Top, Doer::Machine),
            Task::Reflow
        ]
    );
    assert_eq!(p.phases[1].tasks[0], Task::Place(Side::Bottom, Doer::Hand));
    assert!(p
        .phases
        .iter()
        .all(|f| f.state == PhaseState::Pending && f.loadout.is_empty()));
    assert_eq!(parse(&p.to_text()), p);
    // a byte order mark and Windows line ends change nothing
    let crlf = format!("\u{feff}{}", FILE.replace('\n', "\r\n"));
    assert_eq!(parse(&crlf), p);
}

#[test]
fn a_file_that_is_not_a_process_is_refused_with_its_line() {
    assert_eq!(Process::parse(""), Err(ProcessError::Empty));
    assert_eq!(
        Process::parse("# only a comment\n"),
        Err(ProcessError::Empty)
    );
    for (text, line) in [
        ("phase\n", 1),
        ("stage = A | load\n", 1),
        ("phase = A\n", 1),
        ("phase = | load\n", 1),
        ("phase = A | \n", 1),
        ("phase = A | load,, reflow\n", 1),
        ("# c\nphase = A | load\nphase = B load\n", 3),
        ("phase = A,B | load\n", 1),
    ] {
        assert_eq!(
            Process::parse(text),
            Err(ProcessError::Syntax(line)),
            "{text:?}"
        );
    }
    assert_eq!(
        Process::parse("phase = A | load, bake\n"),
        Err(ProcessError::BadTask(1, "bake".into()))
    );
    assert_eq!(
        Process::parse("phase = A | load\nphase = a | reflow\n"),
        Err(ProcessError::Duplicate(2))
    );
}

#[test]
fn the_limits_of_a_process_hold() {
    let many: String = (0..=MAX_PHASES)
        .map(|i| format!("phase = P{i} | load\n"))
        .collect();
    assert_eq!(Process::parse(&many), Err(ProcessError::Limit("phases")));
    let ok: String = (0..MAX_PHASES)
        .map(|i| format!("phase = P{i} | load\n"))
        .collect();
    assert_eq!(parse(&ok).phases.len(), MAX_PHASES);
    let tasks = vec!["load"; MAX_TASKS + 1].join(", ");
    assert_eq!(
        Process::parse(&format!("phase = A | {tasks}\n")),
        Err(ProcessError::Limit("tasks"))
    );
    let long = "n".repeat(MAX_NAME + 1);
    assert_eq!(
        Process::parse(&format!("phase = {long} | load\n")),
        Err(ProcessError::Syntax(1))
    );
    let huge = format!("{}\n", "# padding ".repeat(MAX_PROCESS_BYTES));
    assert_eq!(Process::parse(&huge), Err(ProcessError::Limit("bytes")));
}

#[test]
fn every_refusal_is_told_in_the_eight_languages() {
    let all = [
        ProcessError::Syntax(7),
        ProcessError::BadTask(7, "bake".into()),
        ProcessError::Limit("phases"),
        ProcessError::Duplicate(7),
        ProcessError::Empty,
        ProcessError::NotYet("Oven".into()),
        ProcessError::NotGoing("Oven".into()),
        ProcessError::Locked("Oven".into()),
        ProcessError::NoPhase(2),
    ];
    for e in &all {
        let mut seen = std::collections::BTreeSet::new();
        for lang in Lang::ALL {
            let text = e.localized(lang);
            assert!(!text.is_empty() && !text.contains('{'), "{lang:?} {e:?}");
            seen.insert(text);
        }
        assert!(seen.len() >= 6, "{e:?}");
    }
    assert!(ProcessError::BadTask(7, "bake".into())
        .localized(Lang::De)
        .contains("bake"));
    assert!(ProcessError::NotYet("Oven".into())
        .localized(Lang::Fr)
        .contains("Oven"));
    assert!(ProcessError::NoPhase(2).localized(Lang::En).contains('3'));
}

#[test]
fn the_two_presets_place_each_side_by_the_machine_or_by_hand() {
    let pnp = Process::preset("pnp").unwrap();
    let manual = Process::preset(" Manual ").unwrap();
    assert_eq!(pnp.phases.len(), 2);
    assert_eq!(pnp.phases[0].name, "Top side");
    assert_eq!(pnp.phases[1].name, "Bottom side");
    assert!(pnp.phases[0]
        .tasks
        .contains(&Task::Place(Side::Top, Doer::Machine)));
    assert!(manual.phases[0]
        .tasks
        .contains(&Task::Place(Side::Top, Doer::Hand)));
    assert!(manual.phases[1]
        .tasks
        .contains(&Task::Place(Side::Bottom, Doer::Hand)));
    assert_eq!(Process::preset("robot"), None);
    // the preset is a file that can be written and read back
    assert_eq!(parse(&manual.to_text()), manual);
}

#[test]
fn a_side_with_no_parts_takes_its_phase_out_and_a_phase_without_a_place_stays() {
    let mut p = Process::preset("manual").unwrap();
    p.only_sides(true, false);
    assert_eq!(p.phases.len(), 1);
    assert_eq!(p.phases[0].name, "Top side");
    let mut p = Process::preset("manual").unwrap();
    p.only_sides(false, true);
    assert_eq!(p.phases[0].name, "Bottom side");
    let mut p = parse("phase = Oven | reflow\nphase = Top | place top\n");
    p.only_sides(false, false);
    assert_eq!(p.phases.len(), 1);
    assert_eq!(p.phases[0].name, "Oven");
}

#[test]
fn a_phase_starts_only_after_the_earlier_ones_and_when_none_is_going() {
    let mut p = parse("phase = A | load\nphase = B | reflow\nphase = C | inspect\n");
    assert_eq!(p.current(), Some(0));
    assert_eq!(p.start(1), Err(ProcessError::NotYet("B".into())));
    assert_eq!(p.start(2), Err(ProcessError::NotYet("C".into())));
    p.start(0).unwrap();
    assert_eq!(p.current(), Some(0));
    // nothing else starts while one is going, and one that is going does not start again
    assert_eq!(p.start(1), Err(ProcessError::NotYet("B".into())));
    assert_eq!(p.start(0), Err(ProcessError::NotYet("A".into())));
    assert_eq!(p.finish(1), Err(ProcessError::NotGoing("B".into())));
    p.finish(0).unwrap();
    assert_eq!(p.finish(0), Err(ProcessError::NotGoing("A".into())));
    assert_eq!(p.progress(), (1, 3));
    p.start(1).unwrap();
    p.finish(1).unwrap();
    assert!(!p.finished());
    p.start(2).unwrap();
    p.finish(2).unwrap();
    assert!(p.finished());
    assert_eq!(p.current(), None);
    assert_eq!(p.progress(), (3, 3));
    assert_eq!(p.start(9), Err(ProcessError::NoPhase(9)));
    assert_eq!(p.finish(9), Err(ProcessError::NoPhase(9)));
}

#[test]
fn the_load_out_of_a_phase_can_be_changed_only_while_it_is_pending() {
    let mut p = parse("phase = A | place top\nphase = B | place bottom\n");
    let ids = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<String>>();
    p.set_loadout(1, &ids(&["F9", "F2", "F9"])).unwrap();
    assert_eq!(
        p.phases[1].loadout,
        ["F9", "F2"],
        "a feeder is once in a load-out"
    );
    p.set_loadout(0, &ids(&["F1", "F2"])).unwrap();
    p.set_loadout(0, &ids(&["F1", "F3"])).unwrap();
    p.start(0).unwrap();
    assert_eq!(
        p.set_loadout(0, &ids(&["F4"])),
        Err(ProcessError::Locked("A".into()))
    );
    assert_eq!(
        p.phases[0].loadout,
        ["F1", "F3"],
        "a refused change changes nothing"
    );
    p.finish(0).unwrap();
    assert_eq!(
        p.set_loadout(0, &ids(&["F4"])),
        Err(ProcessError::Locked("A".into()))
    );
    // a phase that has not begun can still be loaded, and a bad name is refused
    p.set_loadout(1, &ids(&["F5"])).unwrap();
    for bad in ["", "a,b", "a|b", "tab\there"] {
        assert!(p.set_loadout(1, &ids(&[bad])).is_err(), "{bad:?}");
    }
    assert_eq!(p.phases[1].loadout, ["F5"]);
    let too_many: Vec<String> = (0..65).map(|i| format!("F{i}")).collect();
    assert_eq!(
        p.set_loadout(1, &too_many),
        Err(ProcessError::Limit("feeders"))
    );
    assert_eq!(p.set_loadout(5, &[]), Err(ProcessError::NoPhase(5)));
}

#[test]
fn the_state_is_kept_between_two_sessions_and_a_wrong_one_changes_nothing() {
    let mut p = parse(FILE);
    p.set_loadout(0, &["F1".to_string(), "F2".to_string()])
        .unwrap();
    p.start(0).unwrap();
    p.finish(0).unwrap();
    p.start(1).unwrap();
    let saved = p.state_text();
    let mut again = parse(FILE);
    again.resume(&saved).unwrap();
    assert_eq!(again, {
        let mut q = p.clone();
        q.phases[0].loadout = vec!["F1".into(), "F2".into()];
        q
    });
    assert_eq!(again.current(), Some(1));
    // wrong states are refused and the process stays as it was
    let before = parse(FILE);
    for bad in [
        "Machine side | done |\n",
        "Other | pending |\nHand side | pending |\n",
        "Hand side | pending |\nMachine side | pending |\n",
        "Machine side | maybe |\nHand side | pending |\n",
        "Machine side | pending |\nHand side | done |\n",
        "Machine side | going |\nHand side | going |\n",
        "Machine side | done |\nHand side | pending |\nExtra | pending |\n",
        "Machine side done\nHand side | pending |\n",
    ] {
        let mut q = parse(FILE);
        assert!(q.resume(bad).is_err(), "{bad:?}");
        assert_eq!(q, before, "{bad:?}");
    }
}

#[test]
fn what_is_done_by_chance_keeps_the_phases_in_order_and_one_going_at_most() {
    // a fixed seed, so a failure can be run again
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = move || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    for _ in 0..200 {
        let mut p = parse(
            "phase = A | load\nphase = B | reflow\nphase = C | inspect\nphase = D | inspect\n",
        );
        for _ in 0..30 {
            let i = (next() % 5) as usize;
            let _ = match next() % 3 {
                0 => p.start(i),
                1 => p.finish(i),
                _ => p.set_loadout(i, &[format!("F{}", next() % 4)]),
            };
            let states: Vec<PhaseState> = p.phases.iter().map(|f| f.state).collect();
            // done phases first, then at most one going, then pending ones
            let done = states
                .iter()
                .take_while(|s| **s == PhaseState::Done)
                .count();
            let going = states
                .iter()
                .skip(done)
                .take_while(|s| **s == PhaseState::Going)
                .count();
            assert!(going <= 1, "{states:?}");
            assert!(
                states
                    .iter()
                    .skip(done + going)
                    .all(|s| *s == PhaseState::Pending),
                "{states:?}"
            );
        }
    }
}

#[test]
fn the_summary_lists_each_phase_with_its_state_and_its_tasks() {
    let mut p = parse(FILE);
    p.start(0).unwrap();
    let en = p.summary(Lang::En);
    assert!(en.contains("1. Machine side [going]"), "{en}");
    assert!(en.contains("2. Hand side [pending]"), "{en}");
    assert_eq!(en.matches("     - ").count(), 6);
    assert!(p.summary(Lang::Es).contains("[en curso]"));
    for lang in Lang::ALL {
        assert!(!p.summary(lang).contains('{'));
    }
    assert!(phase_header(Lang::En, 1, 2, "Top side").contains("Top side"));
    for lang in Lang::ALL {
        let h = phase_header(lang, 1, 2, "Top side");
        assert!(
            h.contains("1") && h.contains("2") && h.contains("Top side"),
            "{lang:?} {h}"
        );
        assert!(!confirm_prompt(lang).is_empty() && confirm_prompt(lang).contains("done"));
    }
}

// ---- the guided manual mode walks a process

fn assist(args: &[&str], answers: &str) -> (i32, String) {
    let dir = std::env::temp_dir().join(format!(
        "eh3d_pnp_process_{}_{}",
        std::process::id(),
        answers.len()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("answers.txt");
    std::fs::write(&file, answers).unwrap();
    let out = Command::new(BIN)
        .arg("assist")
        .arg(example("board.pos"))
        .args(args)
        .arg("--answers")
        .arg(&file)
        .output()
        .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    (
        out.status.code().unwrap_or(-1),
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    )
}

const WALK: &str = "done\nC1\nC2\nU1\nR1\nR2\nR3\ndone\ndone\ndone\nD1\ndone\ndone\n";

#[test]
fn the_manual_process_walks_the_phases_in_order_and_places_each_side_by_hand() {
    let (code, text) = assist(&["--process", "manual"], WALK);
    assert_eq!(code, 0, "{text}");
    assert!(
        text.contains("Top side") && text.contains("Bottom side"),
        "{text}"
    );
    let top = text.find("Top side").unwrap();
    let bottom = text.find("Bottom side").unwrap();
    assert!(top < bottom);
    // the parts of the top side are asked for before the phase of the bottom side, and the other way round never
    assert!(text.find("C1").unwrap() < bottom);
    assert!(text.rfind("D1").unwrap() > bottom);
    assert!(text.contains("pass the board through the oven"), "{text}");
}

#[test]
fn a_phase_that_the_machine_does_is_told_and_waits_for_done() {
    let walk = "done\ndone\ndone\ndone\ndone\ndone\ndone\ndone\n";
    let (code, text) = assist(&["--process", "pnp"], walk);
    assert_eq!(code, 0, "{text}");
    assert!(
        text.contains("the machine places the parts of the top side"),
        "{text}"
    );
    assert!(
        text.contains("the machine places the parts of the bottom side"),
        "{text}"
    );
}

#[test]
fn quitting_in_the_middle_of_a_process_is_not_a_finished_job() {
    let (code, text) = assist(&["--process", "manual"], "done\nC1\nquit\n");
    assert_eq!(code, 2, "{text}");
    // something other than done at a task that is not a placing asks again
    let (code, text) = assist(&["--process", "manual"], "maybe\nquit\n");
    assert_eq!(code, 2, "{text}");
    assert!(text.contains("done"), "{text}");
}

#[test]
fn a_process_file_of_the_person_is_walked_and_a_wrong_one_is_refused_with_its_line() {
    let dir = std::env::temp_dir().join(format!("eh3d_pnp_process_file_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let good = dir.join("one.process");
    std::fs::write(
        &good,
        "phase = Everything | load, place top by hand, place bottom by hand, inspect\n",
    )
    .unwrap();
    let bad = dir.join("bad.process");
    std::fs::write(&bad, "phase = Oops | load, bake\n").unwrap();
    let (code, text) = assist(
        &["--process", &good.display().to_string()],
        "done\nC1\nC2\nU1\nR1\nR2\nR3\nD1\ndone\n",
    );
    assert_eq!(code, 0, "{text}");
    let (code, text) = assist(&["--process", &bad.display().to_string()], "");
    assert_eq!(code, 1, "{text}");
    assert!(text.contains("bake") && text.contains('1'), "{text}");
    let (code, text) = assist(&["--process", "no_such_preset_or_file"], "");
    assert_eq!(code, 1, "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_process_is_off_unless_it_is_asked_for_and_the_walk_is_the_one_it_was() {
    let (code, text) = assist(&[], "C1\nC2\nU1\nR1\nR2\nR3\nD1\n");
    assert_eq!(code, 0, "{text}");
    assert!(!text.contains("pass the board through the oven"));
}
