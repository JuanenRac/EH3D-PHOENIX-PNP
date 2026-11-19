//! How many boards each reel lasts: what the plan takes from each feeder for a board, the whole boards a reel lasts, the reel that runs out first, the feeders that are off or not used, and
//! `reel-runway`.

use eh3d_phoenix_pnp::feeding::from_csv as feeders_from_csv;
use eh3d_phoenix_pnp::runway::{boards_possible, csv, runway, short_of, text};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn feeders(rows: &str) -> Vec<eh3d_phoenix_pnp::feeding::FeederSpec> {
    feeders_from_csv(&format!("id,value,package,x,y,count,enabled\n{rows}")).unwrap()
}

const TABLE: &str = "F1,10k,R_0603,-20,10,100,yes\nF2,100n,C_0603,-20,20,100,yes\nF3,MCU,QFN-32,-20,40,10,yes\nF4,LED,LED_0603,-20,50,50,yes\nF5,1k,R_0603,-20,60,5,no\n";

/// The feeders that the 6 parts of `examples/board.pos` take from.
const USED: [&str; 6] = ["F1", "F1", "F1", "F2", "F2", "F3"];

#[test]
fn a_reel_lasts_the_whole_boards_it_holds_and_the_one_that_runs_out_first_comes_first() {
    let rows = runway(&USED, &feeders(TABLE));
    let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids, ["F3", "F1", "F2", "F4"]);
    let f3 = &rows[0];
    assert_eq!(
        (f3.count, f3.per_board, f3.boards, f3.left_over),
        (10, 1, Some(10), 0)
    );
    let f1 = &rows[1];
    assert_eq!((f1.per_board, f1.boards, f1.left_over), (3, Some(33), 1));
    let f2 = &rows[2];
    assert_eq!((f2.per_board, f2.boards, f2.left_over), (2, Some(50), 0));
    // a feeder that the plan does not use lasts for ever and holds all its parts
    let f4 = &rows[3];
    assert_eq!((f4.per_board, f4.boards, f4.left_over), (0, None, 50));
    assert_eq!(boards_possible(&rows), Some(10));
}

#[test]
fn a_feeder_that_is_switched_off_is_not_listed_and_does_not_count() {
    let rows = runway(&USED, &feeders(TABLE));
    assert!(rows.iter().all(|r| r.id != "F5"));
    // a plan that takes from a feeder that is off: the feeder is not counted (the plan checks say it)
    let rows = runway(&["F5", "F5"], &feeders(TABLE));
    assert!(rows.iter().all(|r| r.boards.is_none()));
    assert_eq!(boards_possible(&rows), None);
}

#[test]
fn a_reel_that_does_not_hold_a_board_lasts_none_and_a_tie_is_told_by_the_id() {
    let rows = runway(
        &["B", "B", "B", "A", "A", "A"],
        &feeders("B,x,p,0,0,2,yes\nA,y,p,0,0,2,yes\nC,z,p,0,0,9,yes\n"),
    );
    let got: Vec<(&str, Option<u64>, u64)> = rows
        .iter()
        .map(|r| (r.id.as_str(), r.boards, r.left_over))
        .collect();
    assert_eq!(got, [("A", Some(0), 2), ("B", Some(0), 2), ("C", None, 9)]);
    assert_eq!(boards_possible(&rows), Some(0));
    assert_eq!(short_of(&rows, 1).len(), 2);
    assert_eq!(short_of(&rows, 0).len(), 0);
}

#[test]
fn nothing_taken_gives_the_line_that_says_so_and_a_huge_count_does_not_overflow() {
    let rows = runway(&[], &feeders(TABLE));
    assert_eq!(boards_possible(&rows), None);
    assert!(
        text(&rows, 1).ends_with("the plan takes nothing from the feeders that are switched on\n")
    );
    assert!(
        text(&[], 1).ends_with("the plan takes nothing from the feeders that are switched on\n")
    );
    let many: Vec<&str> = std::iter::repeat_n("F1", 70_000).collect();
    let rows = runway(&many, &feeders("F1,x,p,0,0,1000000,yes\n"));
    assert_eq!(rows[0].per_board, 70_000);
    assert_eq!(rows[0].boards, Some(1_000_000 / 70_000));
    assert_eq!(rows[0].left_over, 1_000_000 % 70_000);
}

#[test]
fn the_text_marks_the_reels_that_fall_short_and_says_how_many_boards_can_be_made() {
    let rows = runway(&USED, &feeders(TABLE));
    let t = text(&rows, 20);
    let lines: Vec<&str> = t.lines().collect();
    assert_eq!(
        lines[0],
        "F3: MCU / QFN-32, 10 parts, 1 for a board, lasts 10 boards, 0 parts over  <- short"
    );
    assert_eq!(
        lines[1],
        "F1: 10k / R_0603, 100 parts, 3 for a board, lasts 33 boards, 1 parts over"
    );
    assert_eq!(
        lines[3],
        "F4: LED / LED_0603, 50 parts, not used by this board"
    );
    assert_eq!(
        lines[4],
        "10 boards can be made before a reel runs out: F3 runs out first"
    );
    assert_eq!(
        lines[5],
        "short: 1 feeders last fewer than the 20 boards asked for"
    );
    // with a number that every reel reaches there is no line of short reels and no mark
    let t = text(&rows, 5);
    assert!(!t.contains("short") && !t.contains("<-"), "{t}");
}

#[test]
fn the_csv_has_a_row_for_each_feeder_and_leaves_the_boards_empty_when_not_used() {
    let rows = runway(&USED, &feeders(TABLE));
    let t = csv(&rows);
    let lines: Vec<&str> = t.lines().collect();
    assert_eq!(
        lines[0],
        "feeder,value,package,count,per_board,boards,left_over"
    );
    assert_eq!(lines[1], "F3,MCU,QFN-32,10,1,10,0");
    assert_eq!(lines[2], "F1,10k,R_0603,100,3,33,1");
    assert_eq!(lines[4], "F4,LED,LED_0603,50,0,,50");
    assert_eq!(lines.len(), 5);
}

// ---- reel-runway

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

fn p(path: &Path) -> &str {
    path.to_str().unwrap()
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

#[test]
fn the_command_says_how_many_boards_the_reels_last_and_exits_with_1_for_a_reel_that_is_short() {
    let o = run(&["reel-runway", "examples/board.pos", "examples/feeders.csv"]);
    let t = out(&o);
    assert!(o.status.success(), "{t}{}", err(&o));
    assert!(
        t.contains("F3: MCU / QFN-32, 10 parts, 1 for a board, lasts 10 boards, 0 parts over\n"),
        "{t}"
    );
    assert!(
        t.contains("10 boards can be made before a reel runs out: F3 runs out first"),
        "{t}"
    );
    // asked for more boards than a reel holds
    let o = run(&[
        "reel-runway",
        "examples/board.pos",
        "examples/feeders.csv",
        "--boards",
        "20",
    ]);
    assert!(!o.status.success());
    assert!(
        err(&o).contains("1 reels last fewer than 20 boards"),
        "{}",
        err(&o)
    );
    assert!(out(&o).contains("<- short"));
    // exactly what a reel holds is enough
    let o = run(&[
        "reel-runway",
        "examples/board.pos",
        "examples/feeders.csv",
        "--boards",
        "10",
    ]);
    assert!(o.status.success(), "{}", out(&o));
}

#[test]
fn the_command_writes_the_table_never_over_a_file_and_refuses_bad_words() {
    let dir = std::env::temp_dir().join(format!("phoenix_pnp_runway_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("runway.csv");
    let base = ["reel-runway", "examples/board.pos", "examples/feeders.csv"];
    let mut args = base.to_vec();
    args.extend(["--out", p(&file)]);
    let o = run(&args);
    assert!(o.status.success(), "{}{}", out(&o), err(&o));
    assert!(out(&o).contains("written to"));
    let csv_text = std::fs::read_to_string(&file).unwrap();
    assert!(
        csv_text.starts_with(
            "feeder,value,package,count,per_board,boards,left_over\nF3,MCU,QFN-32,10,1,10,0\n"
        ),
        "{csv_text}"
    );
    // not over the file that is there, not over an input
    assert!(!run(&args).status.success());
    let mut over = base.to_vec();
    over.extend(["--out", "examples/feeders.csv"]);
    assert!(!run(&over).status.success());
    for bad in ["0", "1.5", "-1", "many", "1000000001"] {
        let mut a = base.to_vec();
        a.extend(["--boards", bad]);
        assert!(!run(&a).status.success(), "{bad}");
    }
    assert!(!run(&["reel-runway", "examples/board.pos"]).status.success());
    assert!(!run(&[
        "reel-runway",
        "examples/board.pos",
        "examples/feeders.csv",
        "--nothing"
    ])
    .status
    .success());
    let help = out(&run(&["help"]));
    assert!(help.contains("phoenix-pnp reel-runway PARTS FEEDERS [--boards N]"));
    let _ = std::fs::remove_dir_all(&dir);
}
