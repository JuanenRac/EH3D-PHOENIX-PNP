//! The trace of the lots: which lot each placed part came from, what is missing, the table of lots that is read with care, and `lot-trace`.

use eh3d_phoenix_pnp::feeding::{assign, from_csv as feeders_from_csv};
use eh3d_phoenix_pnp::lottrace::{parse_lots, trace, Lot, MAX_LOTS};
use eh3d_phoenix_pnp::parts::parse;
use std::path::PathBuf;
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_phoenix-pnp");
const LOTS: &str = include_str!("../examples/lots.csv");

fn run(args: &[&str]) -> Output {
    Command::new(BIN)
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

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "phoenix_pnp_lottrace_{name}_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn example(name: &str) -> String {
    std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("examples")
            .join(name),
    )
    .unwrap()
}

fn traced(lots: &str) -> eh3d_phoenix_pnp::lottrace::Trace {
    let parts = parse(&example("board.pos")).unwrap();
    let feeders = feeders_from_csv(&example("feeders.csv")).unwrap();
    let picks = assign(&parts, &feeders, None);
    trace(&parts, &feeders, &picks, &parse_lots(lots).unwrap())
}

#[test]
fn every_part_has_the_lot_of_the_feeder_it_is_picked_from() {
    let t = traced(LOTS);
    assert!(t.is_complete());
    let rows: Vec<(&str, &str, &str)> = t
        .rows
        .iter()
        .map(|r| {
            (
                r.reference.as_str(),
                r.feeder.as_str(),
                r.lot.as_ref().unwrap().lot.as_str(),
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            ("C1", "F2", "L2409-B"),
            ("C2", "F2", "L2409-B"),
            ("D1", "F4", "LED-4410"),
            ("R1", "F1", "L2411-A"),
            ("R2", "F1", "L2411-A"),
            ("R3", "F1", "L2411-A"),
            ("U1", "F3", "MCU-77812"),
        ]
    );
    let lots: Vec<(&str, usize)> = t
        .lots
        .iter()
        .map(|u| (u.lot.lot.as_str(), u.references.len()))
        .collect();
    assert_eq!(
        lots,
        [
            ("L2411-A", 3),
            ("L2409-B", 2),
            ("MCU-77812", 1),
            ("LED-4410", 1)
        ]
    );
    assert_eq!(t.lots[0].references, ["R1", "R2", "R3"]);
}

#[test]
fn a_feeder_that_gives_parts_and_has_no_lot_is_told_and_one_that_gives_none_is_not() {
    let t = traced("feeder,lot\nF1,A\nF2,B\n");
    assert!(!t.is_complete());
    assert_eq!(t.without_lot, ["F3", "F4"]);
    assert!(t
        .rows
        .iter()
        .find(|r| r.reference == "U1")
        .unwrap()
        .lot
        .is_none());
    // F4 gives a part (the LED) but a feeder that gives none needs no lot: nothing is said of it
    let only_f1 = traced("feeder,lot\nF1,A\nF2,B\nF3,C\nF4,D\nF5,E\n");
    assert!(only_f1.is_complete());
    assert_eq!(only_f1.unknown_feeders, ["F5"]);
    assert!(only_f1.text().contains("not in the table of feeders: F5"));
}

#[test]
fn a_part_that_no_feeder_holds_is_told() {
    let parts = parse(&example("board.pos")).unwrap();
    let feeders = feeders_from_csv("id,value,package,x,y,count\nF1,10k,R_0603,0,0,100\n").unwrap();
    let picks = assign(&parts, &feeders, None);
    let t = trace(
        &parts,
        &feeders,
        &picks,
        &parse_lots("feeder,lot\nF1,A\n").unwrap(),
    );
    assert!(!t.is_complete());
    assert_eq!(t.without_feeder, ["C1", "C2", "D1", "U1"]);
    assert_eq!(t.rows.len(), 3);
    assert!(t.text().contains("no feeder holds: C1 C2 D1 U1"));
}

#[test]
fn the_table_of_lots_is_read_with_its_columns_in_any_order_and_the_extra_ones_are_optional() {
    let lots = parse_lots("Lot;Supplier;FEEDER\nL1;Acme;F1\n").unwrap();
    assert_eq!(
        lots,
        [Lot {
            feeder: "F1".into(),
            lot: "L1".into(),
            reel: String::new(),
            date_code: String::new(),
            supplier: "Acme".into()
        }]
    );
    assert_eq!(parse_lots(LOTS).unwrap().len(), 4);
    assert_eq!(
        parse_lots("\u{feff}feeder,lot\r\nF1,A\r\n").unwrap().len(),
        1
    );
}

#[test]
fn a_table_of_lots_that_cannot_be_trusted_is_refused_with_its_row() {
    for (text, what) in [
        ("", "empty"),
        ("feeder\nF1\n", "needs the columns feeder and lot"),
        ("lot\nA\n", "needs the columns feeder and lot"),
        ("feeder,lot\nF1,\n", "row 2: a feeder and a lot are needed"),
        ("feeder,lot\n,A\n", "row 2: a feeder and a lot are needed"),
        (
            "feeder,lot\nF1,A\nF1,B\n",
            "row 3: the feeder F1 is in the table twice",
        ),
        ("feeder,lot\nF1,\u{7}\n", "control character"),
        ("feeder,lot\n\"F1,A\n", "quote"),
    ] {
        let e = parse_lots(text).unwrap_err();
        assert!(e.to_string().contains(what), "{text:?}: {e}");
        assert!(e.to_string().contains("PHOENIX-PNP-013"));
    }
    let long = format!("feeder,lot\nF1,{}\n", "x".repeat(65));
    assert!(parse_lots(&long).is_err());
    let ok = format!("feeder,lot\nF1,{}\n", "x".repeat(64));
    assert!(parse_lots(&ok).is_ok());
    let mut many = String::from("feeder,lot\n");
    for i in 0..=MAX_LOTS {
        many.push_str(&format!("F{i},L\n"));
    }
    assert!(parse_lots(&many).is_err());
}

#[test]
fn the_csv_has_a_row_for_each_part_with_the_serial_and_a_cell_that_holds_a_comma_is_quoted() {
    let t = traced(
        "feeder,lot,reel,supplier\nF1,\"A,1\",R1,\"Smith, Jones\"\nF2,B,,\nF3,C,,\nF4,D,,\n",
    );
    let csv = t.csv("SN-001");
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(
        lines[0],
        "board,reference,value,package,feeder,lot,reel,date_code,supplier"
    );
    assert_eq!(lines.len(), 8);
    assert_eq!(
        lines[4],
        "SN-001,R1,10k,R_0603,F1,\"A,1\",R1,,\"Smith, Jones\""
    );
    // without a serial the board cell is empty, and a part with no lot has empty cells
    let none = traced("feeder,lot\nF1,A\n").csv("");
    assert!(none.lines().any(|l| l == ",U1,MCU,QFN-32,F3,,,,"), "{none}");
}

#[test]
fn the_same_tables_give_the_same_trace_whatever_the_order_of_the_parts() {
    let a = traced(LOTS);
    let mut parts = parse(&example("board.pos")).unwrap();
    parts.reverse();
    let feeders = feeders_from_csv(&example("feeders.csv")).unwrap();
    let picks = assign(&parts, &feeders, None);
    let b = trace(&parts, &feeders, &picks, &parse_lots(LOTS).unwrap());
    assert_eq!(a, b);
}

#[test]
fn the_command_prints_the_lots_and_writes_the_rows() {
    let dir = temp("cmd");
    let file = dir.join("trace.csv").to_string_lossy().into_owned();
    let o = run(&[
        "lot-trace",
        "examples/board.pos",
        "examples/feeders.csv",
        "examples/lots.csv",
        "--serial",
        "SN-7",
        "--out",
        &file,
    ]);
    assert!(o.status.success(), "{}", err(&o));
    let text = out(&o);
    assert!(
        text.contains("lot L2411-A (feeder F1, reel R-0007, date code 2411, supplier Supplier A): 3 parts: R1 R2 R3"),
        "{text}"
    );
    assert!(text.contains("7 parts traced, 4 lots"), "{text}");
    let csv = std::fs::read_to_string(&file).unwrap();
    assert!(
        csv.contains("SN-7,U1,MCU,QFN-32,F3,MCU-77812,R-0011,2352,Supplier B"),
        "{csv}"
    );
    let again = run(&[
        "lot-trace",
        "examples/board.pos",
        "examples/feeders.csv",
        "examples/lots.csv",
        "--out",
        &file,
    ]);
    assert_eq!(again.status.code(), Some(1));
    assert!(err(&again).contains("PHOENIX-PNP-040"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_command_exits_with_1_when_a_lot_is_missing_and_still_says_which() {
    let dir = temp("missing");
    let lots = dir.join("lots.csv");
    std::fs::write(&lots, "feeder,lot\nF1,A\n").unwrap();
    let o = run(&[
        "lot-trace",
        "examples/board.pos",
        "examples/feeders.csv",
        &lots.to_string_lossy(),
    ]);
    assert_eq!(o.status.code(), Some(1));
    assert!(
        out(&o).contains("no lot for the feeders: F2, F3, F4"),
        "{}",
        out(&o)
    );
    assert!(err(&o).contains("no lot"), "{}", err(&o));
    // the output is never one of the inputs
    let same = run(&[
        "lot-trace",
        "examples/board.pos",
        "examples/feeders.csv",
        "examples/lots.csv",
        "--out",
        "examples/lots.csv",
    ]);
    assert_eq!(same.status.code(), Some(1));
    assert!(err(&same).contains("also an input"), "{}", err(&same));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_command_checks_its_arguments() {
    for args in [
        vec!["lot-trace", "examples/board.pos", "examples/feeders.csv"],
        vec![
            "lot-trace",
            "examples/board.pos",
            "examples/feeders.csv",
            "examples/nope.csv",
        ],
        vec![
            "lot-trace",
            "examples/board.pos",
            "examples/feeders.csv",
            "examples/lots.csv",
            "--serial",
        ],
        vec![
            "lot-trace",
            "examples/board.pos",
            "examples/feeders.csv",
            "examples/lots.csv",
            "extra",
        ],
    ] {
        let o = run(&args);
        assert_eq!(o.status.code(), Some(1), "{args:?}");
        assert!(err(&o).starts_with("error:"));
    }
    let long = "x".repeat(65);
    let o = run(&[
        "lot-trace",
        "examples/board.pos",
        "examples/feeders.csv",
        "examples/lots.csv",
        "--serial",
        &long,
    ]);
    assert!(err(&o).contains("--serial"), "{}", err(&o));
}
