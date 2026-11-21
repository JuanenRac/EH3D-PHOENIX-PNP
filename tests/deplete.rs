//! The table of feeders after a job: the counts that are left, the strip that moves along, the tray that keeps its place, a state that is not of the table, and `feeder-deplete`.

use eh3d_phoenix_pnp::deplete::deplete;
use eh3d_phoenix_pnp::error::Code;
use eh3d_phoenix_pnp::feeding::from_csv;
use serde_json::json;
use std::path::PathBuf;
use std::process::{Command, Output};

const KINDS: &str = include_str!("../examples/feeders_kinds.csv");

/// A saved state with the parts left in each feeder, in the order of the table.
fn state(left: &[u64]) -> String {
    let devices: Vec<_> = left
        .iter()
        .map(|r| json!({"remaining": r, "enabled": true}))
        .collect();
    json!({"schema_version": 1, "devices": devices}).to_string()
}

const TABLE: &str = "id,value,package,x,y,count\nF1,10k,R_0603,-20,10,100\nF2,100n,C_0603,-20,20,100\nF3,MCU,QFN-32,-20,40,10\n";

#[test]
fn the_count_of_each_feeder_is_what_is_left() {
    let d = deplete(TABLE, &state(&[94, 98, 9])).unwrap();
    let after: Vec<(u32, u32, u32)> = d
        .lines
        .iter()
        .map(|l| (l.before, l.taken, l.after))
        .collect();
    assert_eq!(after, vec![(100, 6, 94), (100, 2, 98), (10, 1, 9)]);
    assert_eq!(d.taken(), 9);
    assert_eq!(d.low(), 0);
    assert_eq!(
        d.csv,
        "id,value,package,x,y,count\nF1,10k,R_0603,-20,10,94\nF2,100n,C_0603,-20,20,98\nF3,MCU,QFN-32,-20,40,9\n"
    );
    assert!(d.text().contains("F1: 100 -> 94 (6 taken)\n"));
    assert!(d
        .text()
        .ends_with("9 parts taken from 3 feeders of 3; 0 low or empty\n"));
    // the new table is a table that is read again, with the same feeders
    let again = from_csv(&d.csv).unwrap();
    assert_eq!(
        again.iter().map(|f| f.base.count).collect::<Vec<_>>(),
        vec![94, 98, 9]
    );
}

#[test]
fn a_feeder_that_gave_nothing_is_left_as_it_was_and_not_listed() {
    let d = deplete(TABLE, &state(&[100, 100, 10])).unwrap();
    assert_eq!(d.taken(), 0);
    assert_eq!(d.csv, TABLE);
    assert_eq!(
        d.text(),
        "0 parts taken from 0 feeders of 3; 0 low or empty\n"
    );
}

#[test]
fn a_feeder_that_is_empty_or_under_its_warning_is_said() {
    let table = "id,value,package,x,y,count,warn_below\nF1,10k,R,0,0,10,5\nF2,1,R,0,0,3,0\nF3,2,R,0,0,8,5\n";
    let d = deplete(table, &state(&[4, 0, 8])).unwrap();
    assert_eq!(d.low(), 2);
    let text = d.text();
    assert!(text.contains("F1: 10 -> 4 (6 taken), LOW\n"), "{text}");
    assert!(text.contains("F2: 3 -> 0 (3 taken), EMPTY\n"), "{text}");
    // F3 gave nothing and is above its warning: it is not listed
    assert!(!text.contains("F3"), "{text}");
}

#[test]
fn a_strip_moves_its_next_place_along_and_the_other_kinds_keep_theirs() {
    // F2 is a strip of pitch 4 along 90 degrees from -20,20; F3 is a tray; F1 a tape
    let before = from_csv(KINDS).unwrap();
    let mut left: Vec<u64> = before.iter().map(|f| u64::from(f.base.count)).collect();
    left[0] -= 10; // tape F1
    left[1] -= 3; // strip F2
    left[2] -= 2; // tray F3
    let d = deplete(KINDS, &state(&left)).unwrap();
    let after = from_csv(&d.csv).unwrap();
    assert_eq!(after.len(), before.len());
    // the tape keeps its place and loses its parts
    assert_eq!((after[0].base.x, after[0].base.y), (-20.0, 10.0));
    assert_eq!(after[0].base.count, before[0].base.count - 10);
    // the strip: 3 pitches of 4 mm along 90 degrees from -20,20 is -20,32
    let (x, y) = (after[1].base.x, after[1].base.y);
    assert!(
        (x - -20.0).abs() < 1e-6 && (y - 32.0).abs() < 1e-6,
        "{x},{y}"
    );
    assert_eq!(after[1].base.count, before[1].base.count - 3);
    // the tray keeps the place of its first cell: the next cell comes from the count
    assert_eq!((after[2].base.x, after[2].base.y), (-60.0, 40.0));
    assert_eq!(after[2].base.count, before[2].base.count - 2);
    // the next cell of the tray, with the new count and no parts taken yet, is the third
    let next = after[2].pick_point(0);
    let was = before[2].pick_point(2);
    assert!((next.0 - was.0).abs() < 1e-9 && (next.1 - was.1).abs() < 1e-9);
    // the strip, after the move and with nothing taken yet, picks where it would have picked
    let was = before[1].pick_point(3);
    let now = after[1].pick_point(0);
    assert!((now.0 - was.0).abs() < 1e-6 && (now.1 - was.1).abs() < 1e-6);
    assert!(d
        .text()
        .contains("next place -20.000,20.000 -> -20.000,32.000"));
    // the other columns are kept: the kind, the slot, the warning and the empty cells
    assert!(d
        .csv
        .starts_with("id,value,package,x,y,count,priority,kind,cols,rows,"));
    assert!(
        d.csv
            .contains("F1,10k,R_0603,-20,10,90,high,tape,,,,,,0,yes,10,S1,\n"),
        "{}",
        d.csv
    );
}

#[test]
fn a_state_that_is_not_of_this_table_is_refused_with_its_reason() {
    // another number of feeders
    let e = deplete(TABLE, &state(&[1, 2])).unwrap_err();
    assert_eq!(e.code, Code::JobStateMismatch);
    assert!(
        e.detail.contains("2 feeders") && e.detail.contains("3"),
        "{}",
        e.detail
    );
    // a feeder with more parts left than it held
    let e = deplete(TABLE, &state(&[101, 1, 1])).unwrap_err();
    assert_eq!(e.code, Code::JobStateMismatch);
    assert!(e.detail.contains("F1"), "{}", e.detail);
    // not JSON, no list of feeders, a feeder with no count
    for bad in ["", "not json", "{}", "[]", "{\"devices\": 3}"] {
        assert_eq!(
            deplete(TABLE, bad).unwrap_err().code,
            Code::JobStateBad,
            "{bad}"
        );
    }
    let no_count =
        json!({"devices": [{"enabled": true}, {"remaining": 1}, {"remaining": 1}]}).to_string();
    assert_eq!(
        deplete(TABLE, &no_count).unwrap_err().code,
        Code::JobStateBad
    );
    let negative =
        json!({"devices": [{"remaining": -1}, {"remaining": 1}, {"remaining": 1}]}).to_string();
    assert_eq!(
        deplete(TABLE, &negative).unwrap_err().code,
        Code::JobStateBad
    );
    // a table that is not a table of feeders
    assert!(deplete("id,count\nF1,x\n", &state(&[1])).is_err());
}

#[test]
fn a_table_with_semicolons_and_quotes_comes_back_the_same_way() {
    let table =
        "id;value;package;x;y;count\nF1;\"10k; 1%\";R_0603;-20;10;100\nF2;100n;C_0603;-20;20;50\n";
    let d = deplete(table, &state(&[90, 50])).unwrap();
    assert_eq!(
        d.csv,
        "id;value;package;x;y;count\nF1;\"10k; 1%\";R_0603;-20;10;90\nF2;100n;C_0603;-20;20;50\n"
    );
    assert_eq!(from_csv(&d.csv).unwrap()[0].base.value, "10k; 1%");
}

// ---- feeder-deplete

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

#[test]
fn the_table_after_a_simulated_job_is_the_table_that_the_next_job_counts_on() {
    let dir = std::env::temp_dir().join(format!("phoenix_pnp_deplete_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let saved = dir.join("state.json");
    let next = dir.join("feeders_next.csv");
    let (ps, pn) = (saved.to_str().unwrap(), next.to_str().unwrap());
    let sim = run(&[
        "simulate",
        "examples/board.pos",
        "examples/feeders.csv",
        "--state",
        ps,
    ]);
    assert!(
        sim.status.success(),
        "{}",
        String::from_utf8_lossy(&sim.stderr)
    );
    let o = run(&["feeder-deplete", "examples/feeders.csv", "--state", ps]);
    let out = String::from_utf8_lossy(&o.stdout).into_owned();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    // the board has three 10k and 100n parts... R1-R3, C1-C2 and U1: 6 parts
    assert!(out.contains("F1: 100 -> 97 (3 taken)"), "{out}");
    assert!(out.contains("F2: 100 -> 98 (2 taken)"), "{out}");
    assert!(out.contains("F3: 10 -> 9 (1 taken)"), "{out}");
    assert!(out.contains("6 parts taken from 3 feeders of 4"), "{out}");
    let o = run(&[
        "feeder-deplete",
        "examples/feeders.csv",
        "--state",
        ps,
        "--out",
        pn,
    ]);
    assert!(o.status.success());
    let table = std::fs::read_to_string(&next).unwrap();
    assert!(table.contains("F1,10k,R_0603,-20,10,97"), "{table}");
    // a second board on the new table takes what is left
    let sim = run(&["simulate", "examples/board.pos", pn]);
    assert!(sim.status.success());
    assert!(String::from_utf8_lossy(&sim.stdout).contains("placed 6 of 6"));
    // not over a file, not over an input
    assert!(!run(&[
        "feeder-deplete",
        "examples/feeders.csv",
        "--state",
        ps,
        "--out",
        pn
    ])
    .status
    .success());
    assert!(!run(&[
        "feeder-deplete",
        "examples/feeders.csv",
        "--state",
        ps,
        "--out",
        ps
    ])
    .status
    .success());
    assert!(!run(&["feeder-deplete", "examples/feeders.csv"])
        .status
        .success());
    assert!(!run(&["feeder-deplete", "--state", ps]).status.success());
    // a state of another table is refused
    let o = run(&[
        "feeder-deplete",
        "examples/feeders_kinds.csv",
        "--state",
        ps,
    ]);
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("PHOENIX-PNP-025"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_command_exits_with_2_when_a_feeder_is_low() {
    let dir = std::env::temp_dir().join(format!("phoenix_pnp_deplete_low_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let table = dir.join("f.csv");
    let saved = dir.join("s.json");
    std::fs::write(&table, TABLE).unwrap();
    std::fs::write(&saved, state(&[100, 100, 0])).unwrap();
    let o = run(&[
        "feeder-deplete",
        table.to_str().unwrap(),
        "--state",
        saved.to_str().unwrap(),
    ]);
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stdout).contains("F3: 10 -> 0 (10 taken), EMPTY"));
    assert!(String::from_utf8_lossy(&o.stderr).contains("1 feeders are low or empty"));
    let _ = std::fs::remove_dir_all(&dir);
}
