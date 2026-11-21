//! The exit code 2 for a check that is not passed, command by command: each command that verifies something ends with 2 when what it verifies is not so, with 1 when it could not be run (a
//! file that is not there, a wrong option), and with 0 when all is well. Every file is made up here or one of `examples/`; nothing is random.

use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("phoenix_pnp_exit_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn make(d: &Path, name: &str, text: &str) -> String {
    let p = d.join(name);
    std::fs::write(&p, text).unwrap();
    p.to_string_lossy().into_owned()
}

fn code(args: &[&str]) -> i32 {
    Command::new(env!("CARGO_BIN_EXE_phoenix-pnp"))
        .args(args)
        .current_dir(root())
        .output()
        .unwrap()
        .status
        .code()
        .unwrap()
}

/// One command: the arguments that give 0 (when there are some), the ones that give 2 and the ones that give 1.
fn expect(name: &str, good: Option<&[&str]>, not_passed: &[&str], not_run: &[&str]) {
    if let Some(g) = good {
        assert_eq!(code(g), 0, "{name}: {g:?} should be well");
    }
    assert_eq!(
        code(not_passed),
        2,
        "{name}: {not_passed:?} should be a check not passed"
    );
    assert_eq!(
        code(not_run),
        1,
        "{name}: {not_run:?} should be a command that could not be run"
    );
}

const FEEDERS_SHORT: &str = "id,value,package,x,y,count\nF1,10k,R_0603,-20,10,100\n";

#[test]
fn check_plan_and_preflight_end_with_2_when_the_feeders_do_not_cover_the_board() {
    let d = dir("cover");
    let short = make(&d, "short.csv", FEEDERS_SHORT);
    let out = d.join("plan.json").to_string_lossy().into_owned();
    let (b, f) = ("examples/board.pos", "examples/feeders.csv");
    expect(
        "check",
        Some(&["check", b, f]),
        &["check", b, &short],
        &["check", b, "no_such.csv"],
    );
    expect(
        "plan",
        None,
        &["plan", b, &short, &out],
        &["plan", b, "no_such.csv", &out],
    );
    assert!(!Path::new(&out).exists());
    expect(
        "preflight",
        Some(&["preflight", b, f]),
        &["preflight", b, &short],
        &["preflight", b, "no_such.csv"],
    );
    let out2 = d.join("sched.json").to_string_lossy().into_owned();
    expect(
        "schedule",
        None,
        &["schedule", b, &short, &out2],
        &["schedule", b, "no_such.csv", &out2],
    );
    expect(
        "simulate",
        None,
        &["simulate", b, &short],
        &["simulate", b, "no_such.csv"],
    );
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn the_checks_of_a_file_end_with_2_when_the_file_is_not_valid() {
    let d = dir("valid");
    let bad_cfg = make(&d, "bad.cfg", "no_such_setting = 1\n");
    expect(
        "config-check",
        Some(&["config-check", "examples/machine.cfg"]),
        &["config-check", &bad_cfg],
        &["config-check", "no_such.cfg"],
    );
    let bad_recipe = make(&d, "bad.recipe", "this is not a recipe\n");
    expect(
        "recipe-check",
        Some(&["recipe-check", "examples/job.recipe"]),
        &["recipe-check", &bad_recipe],
        &["recipe-check", "no_such.recipe"],
    );
    let bad_table = make(&d, "bad_feeders.csv", "id,value\nF1,10k\n");
    expect(
        "tables-check",
        Some(&["tables-check", "--feeders", "examples/feeders.csv"]),
        &["tables-check", "--feeders", &bad_table],
        &["tables-check", "--feeders", "no_such.csv"],
    );
    let bad_profile = make(&d, "bad_driver.cfg", "dialect = nothing\n");
    expect(
        "driver-check",
        Some(&["driver-check", "examples/driver_marlin.cfg"]),
        &["driver-check", &bad_profile],
        &["driver-check", "no_such.cfg"],
    );
    let bad_job = make(&d, "bad_job.txt", "not a job file\n");
    expect(
        "job-import",
        None,
        &["job-import", &bad_job],
        &["job-import", "no_such.txt"],
    );
    let bad_record = make(&d, "bad_record.txt", "not a record\n");
    expect(
        "serial-verify",
        None,
        &["serial-verify", &bad_record],
        &["serial-verify", "no_such.txt"],
    );
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn the_comparisons_end_with_2_when_the_two_differ() {
    let d = dir("diff");
    let board = std::fs::read_to_string(root().join("examples/board.pos")).unwrap();
    // one line less
    let fewer: String = board
        .lines()
        .take(board.lines().count() - 1)
        .collect::<Vec<_>>()
        .join("\n");
    let fewer = make(&d, "fewer.pos", &fewer);
    expect(
        "parts-diff",
        Some(&["parts-diff", "examples/board.pos", "examples/board.pos"]),
        &["parts-diff", "examples/board.pos", &fewer],
        &["parts-diff", "examples/board.pos", "no_such.pos"],
    );
    let other = make(&d, "other.cfg", "order = part\n");
    let same = make(&d, "same.cfg", "order = nearest\n");
    let nearest = make(&d, "nearest.cfg", "order = nearest\n");
    expect(
        "config-diff",
        Some(&["config-diff", &same, &nearest]),
        &["config-diff", &same, &other],
        &["config-diff", &same, "no_such.cfg"],
    );
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn the_reports_end_with_2_when_they_find_what_they_look_for() {
    let (b, f) = ("examples/board.pos", "examples/feeders.csv");
    expect(
        "board-stats",
        Some(&["board-stats", b]),
        &["board-stats", b, "--near", "6"],
        &["board-stats", "no_such.pos", "--near", "6"],
    );
    expect(
        "stock",
        Some(&["stock", b, f, "--boards", "1"]),
        &["stock", b, f, "--boards", "100000"],
        &["stock", b, "no_such.csv"],
    );
    expect(
        "nozzle-life",
        Some(&[
            "nozzle-life",
            "examples/nozzle_life.csv",
            "--warn",
            "100000000",
        ]),
        &["nozzle-life", "examples/nozzle_life.csv", "--warn", "1"],
        &["nozzle-life", "no_such.csv"],
    );
    expect(
        "reel-runway",
        Some(&["reel-runway", b, f, "--boards", "1"]),
        &["reel-runway", b, f, "--boards", "100000"],
        &["reel-runway", b, "no_such.csv"],
    );
}

#[test]
fn the_checks_of_the_driver_end_with_2_when_they_fail() {
    let d = dir("driver");
    let events = make(
        &d,
        "events.txt",
        "error\nerror\nerror\nerror\nerror\nerror\n",
    );
    expect(
        "safety-replay",
        Some(&["safety-replay", "examples/safety_events.txt"]),
        &["safety-replay", &events],
        &["safety-replay", "no_such.txt"],
    );
    expect(
        "homing-repeat",
        Some(&["homing-repeat", "examples/driver_marlin.cfg", "--seed", "3"]),
        &[
            "homing-repeat",
            "examples/driver_marlin.cfg",
            "--seed",
            "3",
            "--jitter",
            "5",
            "--max-spread",
            "0.001",
        ],
        &["homing-repeat", "no_such.cfg"],
    );
    expect(
        "bringup",
        Some(&["bringup", "examples/driver_marlin.cfg"]),
        &[
            "bringup",
            "examples/driver_marlin.cfg",
            "--fault",
            "error@2",
        ],
        &["bringup", "no_such.cfg"],
    );
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn the_other_commands_that_verify_end_with_2_when_a_check_is_not_passed() {
    let d = dir("others");
    let lines = make(
        &d,
        "fab.csv",
        "Designator,Mid X,Mid Y,Layer,Rotation\n\"R1, R2\",1mm,2mm,Top,0\n",
    );
    expect(
        "fab-check",
        None,
        &["fab-check", &lines],
        &["fab-check", "no_such.csv"],
    );
    expect(
        "footprint-find",
        None,
        &[
            "footprint-find",
            "zzzz_nothing_like_this",
            "--packages",
            "examples/packages.csv",
        ],
        &["footprint-find"],
    );
    let bad_marks = make(&d, "measured.csv", "id,x,y\nf1,50,50\nf2,0,0\n");
    expect(
        "fiducial-check",
        Some(&[
            "fiducial-check",
            "examples/fid_design.csv",
            "examples/fid_measured.csv",
        ]),
        &["fiducial-check", "examples/fid_design.csv", &bad_marks],
        &["fiducial-check", "examples/fid_design.csv", "no_such.csv"],
    );
    let weak = make(
        &d,
        "readings.csv",
        "state,kpa\nfree,10\nfree,12\nheld,11\nheld,12\nreleased,10\nreleased,13\n",
    );
    expect(
        "vacuum-calibrate",
        Some(&["vacuum-calibrate", "examples/vacuum_readings.csv"]),
        &["vacuum-calibrate", &weak],
        &["vacuum-calibrate", "no_such.csv"],
    );
    expect(
        "camera-selftest",
        Some(&["camera-selftest", "--seed", "3"]),
        &[
            "camera-selftest",
            "--seed",
            "3",
            "--noise",
            "40",
            "--tol",
            "0.0001",
        ],
        &["camera-selftest", "--seed", "x"],
    );
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_wrong_option_is_a_command_that_could_not_be_run_for_all_of_them() {
    // 1, never 2: a mistake of the arguments is not the answer of a check
    for args in [
        &["check"][..],
        &["slot-bank"],
        &["config-diff", "only_one"],
        &["wiring-sheet"],
        &["parts-diff", "--nothing"],
        &["stock", "--boards", "x", "a", "b"],
    ] {
        assert_eq!(code(args), 1, "{args:?}");
    }
}
