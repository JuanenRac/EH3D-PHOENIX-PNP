//! The commands of the G-code driver as a person runs them with the example files: driver-check, driver-test, driver-run, safety-replay, homing-repeat and bringup. All of them talk to the
//! simulated controller only.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_phoenix-pnp"))
        .args(args)
        .current_dir(root())
        .output()
        .unwrap()
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("phoenix_pnp_drive_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn p(path: &Path) -> &str {
    path.to_str().unwrap()
}

const BOARD: &str = "examples/board.pos";
const FEEDERS: &str = "examples/feeders.csv";
const MARLIN: &str = "examples/driver_marlin.cfg";
const KLIPPER: &str = "examples/driver_klipper.cfg";

#[test]
fn driver_check_reads_both_examples_and_says_it_is_a_simulator() {
    for (file, word) in [
        (MARLIN, "marlin over serial"),
        (KLIPPER, "klipper over moonraker"),
    ] {
        let o = run(&["driver-check", file]);
        assert!(o.status.success(), "{}", err(&o));
        let text = out(&o);
        assert!(text.contains(word), "{text}");
        assert!(text.contains("evidence=simulator"));
    }
    let o = run(&["driver-check", MARLIN, "--lang", "pt"]);
    assert!(out(&o).contains("eixos") && out(&o).contains("só foi experimentado"));
}

#[test]
fn driver_check_refuses_a_bad_profile_with_its_line_in_the_language_asked() {
    let dir = temp("check");
    let bad = dir.join("bad.cfg");
    std::fs::write(&bad, "dialect = marlin\nretries = 99\n").unwrap();
    let o = run(&["driver-check", p(&bad), "--lang", "de"]);
    assert_eq!(o.status.code(), Some(1));
    let e = err(&o);
    assert!(e.contains("Zeile 2") && e.contains("retries"), "{e}");
    let o = run(&["driver-check", "examples/nothing.cfg"]);
    assert_eq!(o.status.code(), Some(1));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_test_passes_the_examples_and_fails_on_a_row_read_differently() {
    let o = run(&["driver-test", MARLIN, "examples/marlin_answers.txt"]);
    assert!(o.status.success(), "{}{}", out(&o), err(&o));
    assert!(out(&o).contains("0 read differently"));
    let o = run(&["driver-test", KLIPPER, "examples/klipper_answers.txt"]);
    assert!(o.status.success());
    let dir = temp("test");
    let t = dir.join("t.txt");
    std::fs::write(
        &t,
        "ok | ok\nok | echo:SD card ok\nerror | Error:x\nbusy | ok\n",
    )
    .unwrap();
    let o = run(&["driver-test", MARLIN, p(&t), "--lang", "es"]);
    assert_eq!(o.status.code(), Some(1));
    let text = out(&o);
    assert!(
        text.contains("línea 2") && text.contains("línea 4") && !text.contains("línea 1:"),
        "{text}"
    );
    assert!(err(&o).contains("2 leídas de otra manera"), "{}", err(&o));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_run_plans_the_board_and_runs_it_on_the_simulated_controller() {
    let dir = temp("run");
    let script = dir.join("script.gcode");
    let log = dir.join("log.csv");
    let o = run(&[
        "driver-run",
        BOARD,
        FEEDERS,
        MARLIN,
        "--script-out",
        p(&script),
        "--log-out",
        p(&log),
    ]);
    assert!(o.status.success(), "{}{}", out(&o), err(&o));
    let text = out(&o);
    assert!(
        text.contains("evidence=simulator") && text.contains("6 parts placed"),
        "{text}"
    );
    let g = std::fs::read_to_string(&script).unwrap();
    let lines: Vec<&str> = g.lines().collect();
    assert_eq!(lines[0], "M115");
    assert!(g.contains("G28") && g.contains("M42 P5 S255") && g.trim_end().ends_with("M84"));
    for (i, l) in lines.iter().enumerate() {
        if l.starts_with("G1 ") {
            assert_eq!(lines[i + 1], "M400");
        }
    }
    let l = std::fs::read_to_string(&log).unwrap();
    assert!(l.starts_with("at_s,line,result\n") && l.contains(",ok"));
    // the files are never written over
    let o = run(&[
        "driver-run",
        BOARD,
        FEEDERS,
        MARLIN,
        "--script-out",
        p(&script),
    ]);
    assert_eq!(o.status.code(), Some(1));
    // through Moonraker the same board goes the same way
    let o = run(&["driver-run", BOARD, FEEDERS, KLIPPER]);
    assert!(o.status.success(), "{}{}", out(&o), err(&o));
    assert!(out(&o).contains("klipper"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_run_fails_closed_on_every_fault_and_says_where() {
    for (fault, word) in [
        ("error@20:stepper", "Rejected"),
        ("silent@19", "Timeout"),
        ("halt@20", "Rejected"),
        ("offset@0.5", "Mismatch"),
    ] {
        let o = run(&["driver-run", BOARD, FEEDERS, MARLIN, "--fault", fault]);
        assert_eq!(o.status.code(), Some(1), "{fault}");
        let e = err(&o);
        assert!(e.contains("stopped at") && e.contains(word), "{fault}: {e}");
    }
    let o = run(&[
        "driver-run",
        BOARD,
        FEEDERS,
        MARLIN,
        "--fault",
        "nonsense@1",
    ]);
    assert_eq!(o.status.code(), Some(1));
    let o = run(&["driver-run", BOARD, FEEDERS, MARLIN, "--fault", "busy@2"]);
    assert!(
        o.status.success(),
        "busy lines before an ok are no fault: {}",
        err(&o)
    );
}

#[test]
fn driver_run_refuses_a_board_the_profile_cannot_reach_before_anything_is_sent() {
    let dir = temp("reach");
    let small = dir.join("small.cfg");
    let text = std::fs::read_to_string(root().join(MARLIN))
        .unwrap()
        .replace("limit_x_mm = 400", "limit_x_mm = 30");
    std::fs::write(&small, text).unwrap();
    let o = run(&["driver-run", BOARD, FEEDERS, p(&small)]);
    assert_eq!(o.status.code(), Some(1));
    assert!(err(&o).contains("outside"), "{}", err(&o));
    assert!(!out(&o).contains("parts placed"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn safety_replay_prints_the_levels_and_exits_with_one_when_the_machine_ends_held() {
    let o = run(&["safety-replay", "examples/safety_events.txt"]);
    assert!(o.status.success(), "{}", err(&o));
    assert!(out(&o).contains("mismatch") && out(&o).contains("slow"));
    let dir = temp("safety");
    let f = dir.join("e.txt");
    std::fs::write(&f, "success\nerror\nsuccess\n").unwrap();
    let o = run(&["safety-replay", p(&f), "--profile", MARLIN, "--lang", "fr"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(out(&o).contains("stopped") && out(&o).contains("niveau"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn homing_repeat_measures_the_spread_and_fails_over_the_limit() {
    let o = run(&["homing-repeat", MARLIN, "--runs", "8"]);
    assert!(o.status.success(), "{}", err(&o));
    assert!(
        out(&o).contains("8 homings: spread 0 mm in X and 0 mm in Y"),
        "{}",
        out(&o)
    );
    let o = run(&[
        "homing-repeat",
        MARLIN,
        "--runs",
        "12",
        "--jitter",
        "0.4",
        "--seed",
        "5",
    ]);
    assert_eq!(o.status.code(), Some(1));
    assert!(err(&o).contains("is over"), "{}", err(&o));
    let o = run(&[
        "homing-repeat",
        MARLIN,
        "--runs",
        "12",
        "--jitter",
        "0.4",
        "--seed",
        "5",
        "--max-spread",
        "1",
    ]);
    assert!(o.status.success());
    // the same seed gives the same figures
    let a = out(&run(&[
        "homing-repeat",
        MARLIN,
        "--jitter",
        "0.2",
        "--seed",
        "9",
        "--max-spread",
        "5",
    ]));
    let b = out(&run(&[
        "homing-repeat",
        MARLIN,
        "--jitter",
        "0.2",
        "--seed",
        "9",
        "--max-spread",
        "5",
    ]));
    assert_eq!(a, b);
    let o = run(&["homing-repeat", MARLIN, "--runs", "1"]);
    assert_eq!(o.status.code(), Some(1));
}

#[test]
fn bringup_passes_on_a_good_machine_and_names_the_step_that_fails() {
    let o = run(&["bringup", MARLIN]);
    assert!(o.status.success(), "{}{}", out(&o), err(&o));
    let text = out(&o);
    for step in [
        "connect",
        "home",
        "position",
        "actuator pump",
        "actuator blow",
        "move 4",
        "close",
    ] {
        assert!(text.contains(&format!("PASS  {step}")), "{step}: {text}");
    }
    assert!(text.contains("evidence=simulator"));
    let o = run(&["bringup", MARLIN, "--fault", "error@12"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(out(&o).contains("FAIL"), "{}", out(&o));
    assert!(err(&o).contains("steps failed"));
}

#[test]
fn every_new_command_without_its_files_answers_with_an_error_line() {
    for cmd in [
        "driver-check",
        "driver-test",
        "driver-run",
        "safety-replay",
        "homing-repeat",
        "bringup",
    ] {
        let o = run(&[cmd]);
        assert_eq!(o.status.code(), Some(1), "{cmd}");
        assert!(
            !err(&o).contains("panicked") && !err(&o).is_empty(),
            "{cmd}"
        );
        let o = run(&[
            cmd,
            "nothing.txt",
            "nothing2.txt",
            "nothing3.txt",
            "--bogus",
        ]);
        assert_eq!(o.status.code(), Some(1), "{cmd}");
    }
}
