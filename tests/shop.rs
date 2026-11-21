//! The commands of the shop floor: job-fingerprint, serial-sign, serial-verify, shift-plan, head-split, footprint-check, reject-report, retry-policy, clearance, bom-cost, lot-age and
//! server-discover. Each is run as the person runs it, with the files of `examples/`, and says what its module says; none of them writes over a file or prints a key.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_phoenix-pnp"))
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap()
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn code(o: &Output) -> i32 {
    o.status.code().unwrap_or(-1)
}

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("phoenix_pnp_shop_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn p(path: &Path) -> &str {
    path.to_str().unwrap()
}

fn write(dir: &Path, name: &str, text: &str) -> PathBuf {
    let f = dir.join(name);
    std::fs::write(&f, text).unwrap();
    f
}

const BOARD: &str = "examples/board.pos";
const FEEDERS: &str = "examples/feeders.csv";

// ---- fingerprints and signed records

#[test]
fn job_fingerprint_is_the_same_for_the_same_table_on_windows_and_on_linux() {
    let dir = scratch("fp");
    let lf = write(&dir, "a.csv", "id,v\nF1,10k\nF2,100n\n");
    let crlf = write(&dir, "b.csv", "\u{feff}id,v  \r\nF1,10k\r\nF2,100n\r\n\r\n");
    let other = write(&dir, "c.csv", "id,v\nF1,10k\nF2,100u\n");
    let (a, b, c) = (
        out(&run(&["job-fingerprint", p(&lf)])),
        out(&run(&["job-fingerprint", p(&crlf)])),
        out(&run(&["job-fingerprint", p(&other)])),
    );
    let hash = |s: &str| s.split_whitespace().next().unwrap().to_string();
    assert_eq!(hash(&a).len(), 64);
    assert_eq!(hash(&a), hash(&b));
    assert_ne!(hash(&a), hash(&c));
    // several files: one line each and one for the set
    let o = run(&["job-fingerprint", p(&lf), p(&other)]);
    assert_eq!(code(&o), 0);
    assert_eq!(out(&o).lines().count(), 3);
    assert!(out(&o).contains("(the set of 2 files)"));
    // the order of the files is part of the set
    let swapped = out(&run(&["job-fingerprint", p(&other), p(&lf)]));
    assert_ne!(out(&o).lines().last(), swapped.lines().last());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn job_fingerprint_needs_files_that_are_there() {
    assert_eq!(code(&run(&["job-fingerprint"])), 1);
    let o = run(&["job-fingerprint", "no_such_table.csv"]);
    assert_eq!(code(&o), 1);
    assert!(err(&o).starts_with("error:"));
    let many: Vec<&str> = std::iter::once("job-fingerprint")
        .chain(std::iter::repeat_n(FEEDERS, 17))
        .collect();
    assert_eq!(code(&run(&many)), 1);
}

#[test]
fn a_signed_record_is_whole_and_a_changed_one_is_found() {
    let dir = scratch("sign");
    let rec = dir.join("0042.txt");
    let o = run(&[
        "serial-sign",
        BOARD,
        FEEDERS,
        "--serial",
        "0042",
        "--lots",
        "examples/lots.csv",
        "--out",
        p(&rec),
    ]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    let text = std::fs::read_to_string(&rec).unwrap();
    assert!(text.starts_with("phoenix-pnp serial record 1\nserial: 0042\n"));
    assert!(text.contains("placed: 7"));
    assert!(text.contains("R1,F1,L2411-A"));
    assert!(text.contains("\nsignature: sha256 "));
    let o = run(&["serial-verify", p(&rec)]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert!(out(&o).contains("serial 0042: 7 references placed, 4 lots"));
    assert!(out(&o).contains("the record is whole"));
    // a reel changed in the record, a serial changed, a line taken out
    for (from, to) in [
        ("R1,F1,L2411-A", "R1,F1,L2411-B"),
        ("serial: 0042", "serial: 0043"),
    ] {
        let changed = write(&dir, "changed.txt", &text.replacen(from, to, 1));
        let o = run(&["serial-verify", p(&changed)]);
        assert_eq!(code(&o), 2, "{from}");
        assert!(
            err(&o).contains("the signature does not match"),
            "{}",
            err(&o)
        );
    }
    let cut: String = text
        .lines()
        .filter(|l| !l.starts_with("R2,"))
        .map(|l| format!("{l}\n"))
        .collect();
    let cut = write(&dir, "cut.txt", &cut);
    assert_eq!(code(&run(&["serial-verify", p(&cut)])), 2);
    // the same board and the same serial give the same record
    let again = dir.join("again.txt");
    run(&[
        "serial-sign",
        BOARD,
        FEEDERS,
        "--serial",
        "0042",
        "--lots",
        "examples/lots.csv",
        "--out",
        p(&again),
    ]);
    assert_eq!(std::fs::read_to_string(&again).unwrap(), text);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_record_is_never_written_over_a_file_and_the_serial_is_asked_for() {
    let dir = scratch("over");
    let there = write(&dir, "there.txt", "keep me\n");
    let o = run(&[
        "serial-sign",
        BOARD,
        FEEDERS,
        "--serial",
        "1",
        "--out",
        p(&there),
    ]);
    assert_eq!(code(&o), 1);
    assert_eq!(std::fs::read_to_string(&there).unwrap(), "keep me\n");
    // nor over a table that was read
    let o = run(&[
        "serial-sign",
        BOARD,
        FEEDERS,
        "--serial",
        "1",
        "--out",
        BOARD,
    ]);
    assert_eq!(code(&o), 1);
    assert!(err(&o).contains("also an input"), "{}", err(&o));
    for args in [
        vec!["serial-sign", BOARD, FEEDERS],
        vec!["serial-sign", BOARD, FEEDERS, "--serial", ""],
        vec!["serial-sign", BOARD, "--serial", "1"],
        vec![
            "serial-sign",
            BOARD,
            FEEDERS,
            "--serial",
            "1",
            "--prev",
            "abc",
        ],
        vec![
            "serial-sign",
            BOARD,
            FEEDERS,
            "--serial",
            "1",
            "--lots",
            "no_such_lots.csv",
        ],
        vec![
            "serial-sign",
            BOARD,
            FEEDERS,
            "--serial",
            "1",
            "--key-file",
            "no_such_key",
        ],
    ] {
        let o = run(&args);
        assert_eq!(code(&o), 1, "{args:?}");
        assert!(err(&o).starts_with("error:"), "{args:?}");
    }
    let long = "x".repeat(65);
    assert_eq!(
        code(&run(&["serial-sign", BOARD, FEEDERS, "--serial", &long])),
        1
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_key_signs_the_record_and_is_needed_to_check_it_and_is_never_printed() {
    let dir = scratch("key");
    let key = write(&dir, "key.txt", "the secret of the line\n");
    let wrong = write(&dir, "wrong.txt", "another secret\n");
    let rec = dir.join("keyed.txt");
    let o = run(&[
        "serial-sign",
        BOARD,
        FEEDERS,
        "--serial",
        "7",
        "--key-file",
        p(&key),
        "--out",
        p(&rec),
    ]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert!(!out(&o).contains("secret") && !err(&o).contains("secret"));
    let text = std::fs::read_to_string(&rec).unwrap();
    assert!(text.contains("\nsignature: hmac-sha256 "));
    assert!(!text.contains("secret"));
    let ok = run(&["serial-verify", p(&rec), "--key-file", p(&key)]);
    assert_eq!(code(&ok), 0, "{}", err(&ok));
    for args in [
        vec!["serial-verify", p(&rec)],
        vec!["serial-verify", p(&rec), "--key-file", p(&wrong)],
    ] {
        let o = run(&args);
        assert_eq!(code(&o), 2, "{args:?}");
        assert!(!err(&o).contains("secret") || err(&o).contains("give the key"));
    }
    // a record with no key does not pass for one that has it
    let plain = dir.join("plain.txt");
    run(&[
        "serial-sign",
        BOARD,
        FEEDERS,
        "--serial",
        "7",
        "--out",
        p(&plain),
    ]);
    assert_eq!(
        code(&run(&["serial-verify", p(&plain), "--key-file", p(&key)])),
        2
    );
    // an empty key is refused
    let empty = write(&dir, "empty.txt", "\n");
    assert_eq!(
        code(&run(&[
            "serial-sign",
            BOARD,
            FEEDERS,
            "--serial",
            "7",
            "--key-file",
            p(&empty)
        ])),
        1
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_records_of_a_run_make_a_chain() {
    let dir = scratch("chain");
    let first = dir.join("1.txt");
    let second = dir.join("2.txt");
    run(&[
        "serial-sign",
        BOARD,
        FEEDERS,
        "--serial",
        "1",
        "--out",
        p(&first),
    ]);
    let text = std::fs::read_to_string(&first).unwrap();
    let sig = text
        .lines()
        .last()
        .unwrap()
        .rsplit(' ')
        .next()
        .unwrap()
        .to_string();
    assert_eq!(sig.len(), 64);
    let o = run(&[
        "serial-sign",
        BOARD,
        FEEDERS,
        "--serial",
        "2",
        "--prev",
        &sig,
        "--out",
        p(&second),
    ]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert!(std::fs::read_to_string(&second)
        .unwrap()
        .contains(&format!("prev: {sig}")));
    assert_eq!(
        code(&run(&["serial-verify", p(&second), "--prev", &sig])),
        0
    );
    let wrong = "0".repeat(64);
    assert_eq!(
        code(&run(&["serial-verify", p(&second), "--prev", &wrong])),
        2
    );
    assert_eq!(code(&run(&["serial-verify", p(&first), "--prev", "-"])), 0);
    assert_eq!(code(&run(&["serial-verify", p(&first), "--prev", &sig])), 2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_board_with_a_part_no_feeder_holds_is_not_signed_unless_it_is_said_to_be_partial() {
    let dir = scratch("partial");
    let feeders = write(
        &dir,
        "feeders.csv",
        "id,value,package,x,y,count\nF1,10k,R_0603,-20,10,100\n",
    );
    let o = run(&["serial-sign", BOARD, p(&feeders), "--serial", "9"]);
    assert_eq!(code(&o), 1);
    assert!(err(&o).contains("no feeder holds"), "{}", err(&o));
    let o = run(&[
        "serial-sign",
        BOARD,
        p(&feeders),
        "--serial",
        "9",
        "--partial",
    ]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert!(out(&o).contains("placed: 3"));
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- the shift and the heads

#[test]
fn shift_plan_says_where_the_reels_are_changed_and_how_many_boards_fit() {
    let o = run(&["shift-plan", BOARD, FEEDERS]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    let text = out(&o);
    assert!(text.contains("change the reel of feeder"), "{text}");
    assert!(text.contains("boards in a shift of 480 min"), "{text}");
    assert!(text.contains("more changes (all of them are in the table of --out)"));
    // few boards need no change at all
    let o = run(&["shift-plan", BOARD, FEEDERS, "--boards", "5"]);
    assert_eq!(code(&o), 0);
    assert!(
        out(&o).starts_with(
            "5 boards in a shift of 480 min (the boards that were asked are made); 0 changes"
        ),
        "{}",
        out(&o)
    );
    // a shorter shift and a faster change
    let short = out(&run(&[
        "shift-plan",
        BOARD,
        FEEDERS,
        "--shift",
        "30",
        "--reel-change",
        "0",
    ]));
    assert!(short.contains("in a shift of 30 min"), "{short}");
    assert!(short.contains("0.0% of the shift"), "{short}");
}

#[test]
fn shift_plan_writes_the_table_of_changes_once_and_checks_its_options() {
    let dir = scratch("shiftplan");
    let table = dir.join("changes.csv");
    let o = run(&["shift-plan", BOARD, FEEDERS, "--out", p(&table)]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    let csv = std::fs::read_to_string(&table).unwrap();
    assert!(csv.starts_with("board,at_s,feeder\n"));
    let changes = out(&o)
        .lines()
        .filter(|l| l.contains("change the reel"))
        .count();
    assert!(csv.lines().count() - 1 > changes, "the table has them all");
    assert_eq!(
        code(&run(&["shift-plan", BOARD, FEEDERS, "--out", p(&table)])),
        1
    );
    assert_eq!(
        code(&run(&["shift-plan", BOARD, FEEDERS, "--out", BOARD])),
        1
    );
    for args in [
        vec!["--boards", "0"],
        vec!["--boards", "1000001"],
        vec!["--boards", "2.5"],
        vec!["--shift", "29"],
        vec!["--shift", "1441"],
        vec!["--reel-change", "-1"],
        vec!["--reel-change", "3601"],
    ] {
        let mut a = vec!["shift-plan", BOARD, FEEDERS];
        a.extend(&args);
        let o = run(&a);
        assert_eq!(code(&o), 1, "{args:?}");
        assert!(err(&o).starts_with("error:"), "{args:?}");
    }
    // a board whose parts have no feeder does not plan unless it is partial
    let feeders = write(
        &dir,
        "feeders.csv",
        "id,value,package,x,y,count\nF1,10k,R_0603,-20,10,100\n",
    );
    assert_eq!(code(&run(&["shift-plan", BOARD, p(&feeders)])), 2);
    assert_eq!(
        code(&run(&["shift-plan", BOARD, p(&feeders), "--partial"])),
        0
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn head_split_compares_the_bands_with_whole_feeders() {
    let o = run(&["head-split", BOARD, FEEDERS, "--heads", "3"]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    let text = out(&o);
    assert!(
        text.contains("strips:") && text.contains("by-feeder:"),
        "{text}"
    );
    assert_eq!(text.matches("  head ").count(), 6);
    assert!(text.contains("the quicker way is "), "{text}");
    // whole feeders never share a reel
    let by_feeder = text.split("by-feeder:").nth(1).unwrap();
    assert!(by_feeder.contains("0 feeders are shared"), "{by_feeder}");
    // the default is two heads
    assert_eq!(
        out(&run(&["head-split", BOARD, FEEDERS]))
            .matches("  head ")
            .count(),
        4
    );
}

#[test]
fn head_split_writes_a_row_for_each_head_of_each_way_and_checks_its_options() {
    let dir = scratch("heads");
    let table = dir.join("heads.csv");
    let o = run(&[
        "head-split",
        BOARD,
        FEEDERS,
        "--heads",
        "4",
        "--out",
        p(&table),
    ]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    let csv = std::fs::read_to_string(&table).unwrap();
    assert_eq!(csv.lines().count(), 1 + 8);
    assert_eq!(
        code(&run(&["head-split", BOARD, FEEDERS, "--out", p(&table)])),
        1
    );
    for n in ["0", "9", "two", "-1"] {
        assert_eq!(
            code(&run(&["head-split", BOARD, FEEDERS, "--heads", n])),
            1,
            "{n}"
        );
    }
    assert_eq!(code(&run(&["head-split", BOARD])), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- footprints, clearance and rejects

#[test]
fn footprint_check_says_how_each_package_stands_against_the_table() {
    let o = run(&[
        "footprint-check",
        BOARD,
        "--packages",
        "examples/packages_bodies.csv",
    ]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert!(out(&o).contains("R_0603: 3 parts, known"));
    assert!(out(&o).ends_with("4 packages, 0 to look at\n"));
    // names from a CAD library: an alias of a row, one made from its name, a mistake and a stranger
    let dir = scratch("foot");
    let parts = write(
        &dir,
        "board.pos",
        "# Ref Val Package PosX PosY Rot Side\nR1 10k Resistor_SMD:R_0603_1608Metric 1 1 0 top\nR2 10k R_0805_2012Metric 2 1 0 top\nR3 10k R_0603x 3 1 0 top\nJ1 conn Mystery_Header_2x5 4 1 0 top\n",
    );
    let o = run(&[
        "footprint-check",
        p(&parts),
        "--packages",
        "examples/packages_bodies.csv",
    ]);
    assert_eq!(code(&o), 2);
    let text = out(&o);
    assert!(
        text.contains("Resistor_SMD:R_0603_1608Metric: 1 parts, alias as R_0603"),
        "{text}"
    );
    assert!(
        text.contains("R_0805_2012Metric: 1 parts, generated"),
        "{text}"
    );
    assert!(
        text.contains("R_0603x: 1 parts, similar to R_0603?"),
        "{text}"
    );
    assert!(
        text.contains("Mystery_Header_2x5: 1 parts, unknown"),
        "{text}"
    );
    assert!(err(&o).contains("2 packages need a look"), "{}", err(&o));
    let table = dir.join("foot.csv");
    run(&[
        "footprint-check",
        p(&parts),
        "--packages",
        "examples/packages_bodies.csv",
        "--out",
        p(&table),
    ]);
    let csv = std::fs::read_to_string(&table).unwrap();
    assert!(csv.starts_with("package,parts,status,library_name\n"));
    assert!(csv.contains("R_0603x,1,similar,R_0603\n"), "{csv}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn clearance_finds_the_parts_that_are_too_near_and_asks_for_the_sizes() {
    let o = run(&[
        "clearance",
        BOARD,
        "--packages",
        "examples/packages_bodies.csv",
    ]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert!(out(&o).contains("7 parts compared: 0 pairs nearer than 0.2 mm"));
    let dir = scratch("clear");
    let parts = write(
        &dir,
        "board.pos",
        "# Ref Val Package PosX PosY Rot Side\nR1 10k R_0603 10 10 0 top\nR2 10k R_0603 11 10 0 top\nR3 10k R_0603 20 20 0 top\nR4 10k R_0603 21.7 20 0 top\n",
    );
    let o = run(&[
        "clearance",
        p(&parts),
        "--packages",
        "examples/packages_bodies.csv",
    ]);
    assert_eq!(code(&o), 2);
    let text = out(&o);
    assert!(text.contains("R1 and R2 overlap by 0.600 mm"), "{text}");
    assert!(text.contains("R3 and R4 are 0.100 mm apart"), "{text}");
    // a smaller space asked for leaves only the overlap
    let o = run(&[
        "clearance",
        p(&parts),
        "--packages",
        "examples/packages_bodies.csv",
        "--min",
        "0.05",
    ]);
    assert!(
        out(&o).contains("1 pairs nearer than 0.05 mm, 1 of them overlapping"),
        "{}",
        out(&o)
    );
    let table = dir.join("clear.csv");
    run(&[
        "clearance",
        p(&parts),
        "--packages",
        "examples/packages_bodies.csv",
        "--out",
        p(&table),
    ]);
    assert!(std::fs::read_to_string(&table)
        .unwrap()
        .starts_with("first,second,gap_mm\nR1,R2,-0.6000\n"));
    // with no sizes there is nothing to compare, and that is told
    let o = run(&["clearance", p(&parts)]);
    assert_eq!(code(&o), 1);
    assert!(
        err(&o).contains("no part has the size of a body"),
        "{}",
        err(&o)
    );
    for min in ["-1", "5.5", "wide"] {
        assert_eq!(
            code(&run(&[
                "clearance",
                p(&parts),
                "--packages",
                "examples/packages_bodies.csv",
                "--min",
                min
            ])),
            1,
            "{min}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn reject_report_ranks_the_reasons_and_fails_over_the_share_that_is_asked() {
    let o = run(&["reject-report", BOARD, FEEDERS, "examples/rejects.csv"]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    let text = out(&o);
    assert!(text.starts_with("5 rejects on 3 boards\n"), "{text}");
    assert!(text.contains("reason tombstone: 3 (60.0%, 60.0% together with the commoner ones)"));
    assert!(text.contains("reference R1: 2 times (66.7% of the boards)"));
    assert!(text.contains("package R_0603: 3 of 9 placements (33.333%)"));
    assert!(text.contains("feeder F1: 3 of 9 placements (33.333%)"));
    let o = run(&[
        "reject-report",
        BOARD,
        FEEDERS,
        "examples/rejects.csv",
        "--max-rate",
        "50",
    ]);
    assert_eq!(code(&o), 2);
    assert!(err(&o).contains("1 references were rejected on 50% of the boards or more"));
    assert_eq!(
        code(&run(&[
            "reject-report",
            BOARD,
            FEEDERS,
            "examples/rejects.csv",
            "--max-rate",
            "70"
        ])),
        0
    );
    // the boards of the run: more than the serials of the table, never fewer
    let o = run(&[
        "reject-report",
        BOARD,
        FEEDERS,
        "examples/rejects.csv",
        "--boards",
        "30",
    ]);
    assert!(out(&o).starts_with("5 rejects on 30 boards\n"));
    assert_eq!(
        code(&run(&[
            "reject-report",
            BOARD,
            FEEDERS,
            "examples/rejects.csv",
            "--boards",
            "2"
        ])),
        1
    );
    for args in [
        vec!["--boards", "0"],
        vec!["--max-rate", "101"],
        vec!["--max-rate", "-1"],
    ] {
        let mut a = vec!["reject-report", BOARD, FEEDERS, "examples/rejects.csv"];
        a.extend(&args);
        assert_eq!(code(&run(&a)), 1, "{args:?}");
    }
}

#[test]
fn reject_report_writes_the_table_and_tells_a_reference_that_is_not_on_the_board() {
    let dir = scratch("rej");
    let rejects = write(
        &dir,
        "rejects.csv",
        "reference,reason\nX9,wrong place\nR1,tombstone\n",
    );
    let table = dir.join("rej.csv");
    let o = run(&[
        "reject-report",
        BOARD,
        FEEDERS,
        p(&rejects),
        "--out",
        p(&table),
    ]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert!(out(&o).contains("not parts of the board: X9"));
    let csv = std::fs::read_to_string(&table).unwrap();
    assert!(csv.starts_with("kind,name,rejected,placed,percent\nreason,"));
    assert!(csv.contains("reference,R1,1,1,100.000\n"), "{csv}");
    assert_eq!(
        code(&run(&[
            "reject-report",
            BOARD,
            FEEDERS,
            p(&rejects),
            "--out",
            p(&table)
        ])),
        1
    );
    let bad = write(&dir, "bad.csv", "part,why\nR1,x\n");
    assert_eq!(code(&run(&["reject-report", BOARD, FEEDERS, p(&bad)])), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- retries, costs and lots

#[test]
fn retry_policy_gives_each_part_the_retries_of_the_first_rule_that_fits() {
    let o = run(&["retry-policy", BOARD, FEEDERS, "examples/retry_rules.csv"]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    let text = out(&o);
    assert!(text.contains("rule 1 (reference U*): 1 parts"), "{text}");
    assert!(text.contains("rule 3 (value 10k): 3 parts"), "{text}");
    assert!(text.contains("the settings: 2 parts"), "{text}");
    // the retries move the chance of a good board
    let dir = scratch("retry");
    let cfg = write(
        &dir,
        "machine.cfg",
        "sim_pick_fail_rate = 0.2\npick_retries = 1\n",
    );
    let few = write(
        &dir,
        "few.csv",
        "field,pattern,pick_retries\nreference,U*,0\n",
    );
    let many = write(
        &dir,
        "many.csv",
        "field,pattern,pick_retries\nreference,U*,8\n",
    );
    let chance = |rules: &Path| -> f64 {
        let o = run(&[
            "retry-policy",
            BOARD,
            FEEDERS,
            p(rules),
            "--config",
            p(&cfg),
        ]);
        assert_eq!(code(&o), 0, "{}", err(&o));
        let line = out(&o).lines().last().unwrap().to_string();
        line.split('%').next().unwrap().parse().unwrap()
    };
    assert!(chance(&few) < chance(&many));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn retry_policy_fails_for_a_rule_that_fits_nothing_and_writes_the_table() {
    let dir = scratch("retry2");
    let rules = write(
        &dir,
        "rules.csv",
        "field,pattern,pick_retries\nreference,Q*,3\nreference,U*,4\nreference,U1,5\n",
    );
    let table = dir.join("retry.csv");
    let o = run(&[
        "retry-policy",
        BOARD,
        FEEDERS,
        p(&rules),
        "--out",
        p(&table),
    ]);
    assert_eq!(code(&o), 2);
    assert!(out(&o).contains("rules that fit no part: 1"), "{}", out(&o));
    assert!(out(&o).contains("rules that never act: 3"), "{}", out(&o));
    assert!(
        err(&o).contains("1 rules fit no part and 1 never act"),
        "{}",
        err(&o)
    );
    let csv = std::fs::read_to_string(&table).unwrap();
    assert!(csv.lines().any(|l| l.starts_with("U1,F3,2,")), "{csv}");
    assert_eq!(csv.lines().count(), 1 + 7);
    for text in [
        "",
        "field,pattern\nreference,U*\n",
        "field,pattern,pick_retries\ncolour,U*,1\n",
    ] {
        let bad = write(&dir, "bad.csv", text);
        assert_eq!(
            code(&run(&["retry-policy", BOARD, FEEDERS, p(&bad)])),
            1,
            "{text:?}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn bom_cost_prices_the_run_with_the_breaks_and_the_spares() {
    let o = run(&[
        "bom-cost",
        "examples/bom.csv",
        "examples/prices.csv",
        "--boards",
        "50",
        "--spare",
        "5",
        "--currency",
        "EUR",
    ]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    let text = out(&o);
    assert!(
        text.contains(
            "10k R_0603 (RC0603FR-0710KL): 158 needed, 158 bought at 0.0200 EUR = 3.16 EUR"
        ),
        "{text}"
    );
    assert!(
        text.contains("50 boards: 147.77 EUR in all, 2.9554 EUR for a board"),
        "{text}"
    );
    // the lowest quantity sold is bought when fewer are needed
    let o = run(&[
        "bom-cost",
        "examples/bom.csv",
        "examples/prices.csv",
        "--boards",
        "1",
    ]);
    assert!(
        out(&o).contains("(9 more than needed: the lowest quantity sold)"),
        "{}",
        out(&o)
    );
    let dir = scratch("cost");
    let table = dir.join("cost.csv");
    run(&[
        "bom-cost",
        "examples/bom.csv",
        "examples/prices.csv",
        "--out",
        p(&table),
    ]);
    assert!(std::fs::read_to_string(&table)
        .unwrap()
        .starts_with("value,package,part_number,per_board,needed,bought,each,cost\n"));
    // a part with no price fails the command
    let some = write(&dir, "some.csv", "part_number,price\nRC0603FR-0710KL,0.1\n");
    let o = run(&["bom-cost", "examples/bom.csv", p(&some)]);
    assert_eq!(code(&o), 2);
    assert!(out(&o).contains("no price for: "), "{}", out(&o));
    for args in [
        vec!["--boards", "0"],
        vec!["--spare", "-1"],
        vec!["--spare", "1001"],
        vec!["--currency", "TOOLONGCURRENCY"],
    ] {
        let mut a = vec!["bom-cost", "examples/bom.csv", "examples/prices.csv"];
        a.extend(&args);
        assert_eq!(code(&run(&a)), 1, "{args:?}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn lot_age_judges_the_date_codes_against_the_week_given() {
    let lots = "examples/lots.csv";
    let o = run(&["lot-age", BOARD, FEEDERS, lots, "--week", "2411"]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert!(
        out(&o).contains("feeder F3: lot MCU-77812 date code 2352, 11 weeks old: ok"),
        "{}",
        out(&o)
    );
    assert!(out(&o).ends_with("4 feeders, 0 to look at (the limit is 104 weeks)\n"));
    // two years on, the old lots are called out
    let o = run(&["lot-age", BOARD, FEEDERS, lots, "--week", "2611"]);
    assert_eq!(code(&o), 2);
    assert!(
        out(&o).contains("feeder F3: lot MCU-77812 date code 2352, 115 weeks old: old"),
        "{}",
        out(&o)
    );
    assert!(
        err(&o).contains("3 feeders have a lot to look at"),
        "{}",
        err(&o)
    );
    // a longer limit lets them through; a week before the lots is a typing mistake
    assert_eq!(
        code(&run(&[
            "lot-age",
            BOARD,
            FEEDERS,
            lots,
            "--week",
            "2611",
            "--max-weeks",
            "200"
        ])),
        0
    );
    let o = run(&["lot-age", BOARD, FEEDERS, lots, "--week", "2301"]);
    assert_eq!(code(&o), 2);
    assert!(out(&o).contains("future"), "{}", out(&o));
    for args in [
        vec![],
        vec!["--week", "99"],
        vec!["--week", "2460"],
        vec!["--week", "2411", "--max-weeks", "0"],
        vec!["--week", "2411", "--max-weeks", "1041"],
    ] {
        let mut a = vec!["lot-age", BOARD, FEEDERS, lots];
        a.extend(&args);
        assert_eq!(code(&run(&a)), 1, "{args:?}");
    }
}

// ---- the network and the old names of keys

#[test]
fn server_discover_asks_nothing_of_the_network_unless_it_is_turned_on() {
    let o = run(&["server-discover"]);
    assert_eq!(code(&o), 1);
    assert!(err(&o).contains("server_mdns is off"), "{}", err(&o));
    let dir = scratch("mdns");
    let on = write(&dir, "machine.cfg", "server_mdns = on\n");
    let o = run(&["server-discover", "--sim", "--config", p(&on)]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert_eq!(out(&o).trim_end(), "192.168.0.180:18200");
    // with the switch off the simulated resolver is not asked either
    assert_eq!(code(&run(&["server-discover", "--sim"])), 1);
    if !eh3d_phoenix_pnp::server::HAS_MDNS {
        let o = run(&["server-discover", "--config", p(&on)]);
        assert_eq!(code(&o), 1);
        assert!(err(&o).contains("--features mdns"), "{}", err(&o));
    }
    assert_eq!(code(&run(&["server-discover", "extra"])), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_old_name_of_server_retries_still_works_and_says_it_is_the_old_name() {
    let dir = scratch("alias");
    let old = write(&dir, "old.cfg", "server_attempts = 4\n");
    let o = run(&[
        "lot-age",
        BOARD,
        FEEDERS,
        "examples/lots.csv",
        "--week",
        "2411",
        "--config",
        p(&old),
    ]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert!(
        err(&o).contains("note: server_attempts is the old name of server_retries"),
        "{}",
        err(&o)
    );
    // no note when the new key is written too (the example writes both), or when neither is
    for text in [
        "server_attempts = 4\nserver_retries = 1\n",
        "server_retries = 1\n",
        "# server_attempts = 3\n",
    ] {
        let cfg = write(&dir, "new.cfg", text);
        let o = run(&[
            "lot-age",
            BOARD,
            FEEDERS,
            "examples/lots.csv",
            "--week",
            "2411",
            "--config",
            p(&cfg),
        ]);
        assert_eq!(code(&o), 0, "{}", err(&o));
        assert!(!err(&o).contains("note:"), "{text:?}: {}", err(&o));
    }
    let o = run(&[
        "lot-age",
        BOARD,
        FEEDERS,
        "examples/lots.csv",
        "--week",
        "2411",
        "--config",
        "examples/machine.cfg",
    ]);
    assert_eq!(code(&o), 0, "{}", err(&o));
    assert!(!err(&o).contains("note:"));
    let _ = std::fs::remove_dir_all(&dir);
}
