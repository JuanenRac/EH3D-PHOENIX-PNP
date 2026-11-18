//! Whether the feeders that are loaded hold enough for a number of boards, the parts that a person still has to buy after the ones in the feeders, and the commands `stock` and `bom-order --have`.

use eh3d_phoenix_pnp::bom::{buy_after_stock, buy_csv, order, Bom, MAX_BOM_ROWS};
use eh3d_phoenix_pnp::error::Code;
use eh3d_phoenix_pnp::feeding::from_csv;
use eh3d_phoenix_pnp::parts::{from_csv as parts_from_csv, Part, Side};
use eh3d_phoenix_pnp::stock::{stock_plan, MAX_STOCK_BOARDS};
use std::path::PathBuf;
use std::process::{Command, Output};

const BOARD: &str = "ref,val,package,x,y,rot,side\nR1,10k,R_0603,1,1,0,top\nR2,10k,R_0603,2,1,0,top\nR3,10k,R_0603,3,1,0,top\nC1,100n,C_0603,1,2,0,top\nC2,100n,C_0603,2,2,0,top\nU1,MCU,QFN-32,5,5,0,top\nD1,LED,LED_0603,6,6,0,bottom\n";
const FEEDERS: &str = "id,value,package,x,y,count\nF1,10k,R_0603,-20,10,100\nF2,100n,C_0603,-20,20,100\nF3,MCU,QFN-32,-20,40,10\nF4,LED,LED_0603,-20,50,50\n";

fn board() -> Vec<Part> {
    parts_from_csv(BOARD).unwrap()
}

#[test]
fn one_board_is_enough_for_the_example_and_forty_are_not() {
    let feeders = from_csv(FEEDERS).unwrap();
    let one = stock_plan(&board(), &feeders, None, 1).unwrap();
    assert!(one.is_enough() && one.short_kinds() == 0 && one.short_parts() == 0);
    assert_eq!(
        (one.boards, one.parts_per_board, one.lines.len()),
        (1, 7, 4)
    );
    assert!(one
        .text()
        .starts_with("ok: the feeders hold enough for 1 boards (7 parts on each, 4 kinds)"));
    let forty = stock_plan(&board(), &feeders, None, 40).unwrap();
    assert!(!forty.is_enough());
    assert_eq!(forty.short_kinds(), 2);
    // 120 resistors against 100, 40 chips against 10
    let r = forty.lines.iter().find(|l| l.value == "10k").unwrap();
    assert_eq!(
        (
            r.per_board,
            r.available,
            r.needed,
            r.short,
            r.boards_covered
        ),
        (3, 100, 120, 20, 33)
    );
    assert_eq!(r.feeders, ["F1"]);
    let u = forty.lines.iter().find(|l| l.value == "MCU").unwrap();
    assert_eq!(
        (u.available, u.needed, u.short, u.boards_covered),
        (10, 40, 30, 10)
    );
    assert_eq!(forty.short_parts(), 50);
    assert_eq!(forty.boards_before_reload(), Some(10));
    let t = forty.text();
    assert!(t.contains("10k / R_0603: 120 needed (3 per board), 100 in F1, 20 short; the feeders serve 33 boards\n"), "{t}");
    assert!(t.ends_with("short: 2 kinds of 4 lack 50 parts for 40 boards; the first reel runs out after 10 boards\n"), "{t}");
}

#[test]
fn the_kinds_are_sorted_pooled_over_feeders_and_matched_without_regard_to_case() {
    let feeders = from_csv(
        "id,value,package,x,y,count\nA2,10K,r_0603,0,0,40\nA1,10k,R_0603,0,10,30\nB1,100n,C_0603,0,20,500\n",
    )
    .unwrap();
    let p = stock_plan(&board(), &feeders, Some(Side::Top), 20).unwrap();
    // the kinds come sorted by package and value
    let order: Vec<&str> = p.lines.iter().map(|l| l.package.as_str()).collect();
    let mut sorted = order.clone();
    sorted.sort_by_key(|s| s.to_lowercase());
    assert_eq!(order, sorted);
    let r = p.lines.iter().find(|l| l.value == "10k").unwrap();
    assert_eq!((r.available, r.needed, r.short), (70, 60, 0));
    assert_eq!(r.feeders, ["A1", "A2"]);
    // the chip has no feeder at all
    let u = p.lines.iter().find(|l| l.value == "MCU").unwrap();
    assert_eq!((u.available, u.boards_covered, u.short), (0, 0, 20));
    assert!(u.feeders.is_empty());
    assert!(
        p.text()
            .contains("MCU / QFN-32: 20 needed (1 per board), no feeder holds it, 20 short"),
        "{}",
        p.text()
    );
    // one side only: the LED of the bottom side is not counted
    assert!(p.lines.iter().all(|l| l.value != "LED"));
    let bottom = stock_plan(&board(), &feeders, Some(Side::Bottom), 1).unwrap();
    assert_eq!(bottom.lines.len(), 1);
    assert_eq!(bottom.parts_per_board, 1);
}

#[test]
fn a_feeder_that_is_switched_off_holds_nothing_and_the_boards_are_limited() {
    let feeders = from_csv(
        "id,value,package,x,y,count,priority,kind,cols,rows,pitch_x,pitch_y,pitch,rotation,enabled,warn_below,slot,fill\nF1,10k,R_0603,0,0,100,normal,tape,,,,,,0,no,0,,\n",
    )
    .unwrap();
    let p = stock_plan(&board(), &feeders, None, 1).unwrap();
    let r = p.lines.iter().find(|l| l.value == "10k").unwrap();
    assert_eq!((r.available, r.short), (0, 3));
    assert!(r.feeders.is_empty());
    for bad in [0, MAX_STOCK_BOARDS + 1] {
        let e = stock_plan(&board(), &feeders, None, bad).unwrap_err();
        assert_eq!(e.code, Code::PlanSetting, "{bad}");
    }
    assert!(stock_plan(&board(), &feeders, None, MAX_STOCK_BOARDS).is_ok());
    // no part, no kinds: nothing to be short of, and no board count to give
    let none = stock_plan(&[], &feeders, None, 3).unwrap();
    assert!(none.is_enough() && none.boards_before_reload().is_none());
    assert_eq!(
        none.csv(),
        "value,package,per_board,available,needed,short,boards_covered,feeders\n"
    );
}

#[test]
fn the_csv_has_a_row_for_every_kind() {
    let feeders = from_csv(FEEDERS).unwrap();
    let csv = stock_plan(&board(), &feeders, None, 40).unwrap().csv();
    assert!(
        csv.starts_with("value,package,per_board,available,needed,short,boards_covered,feeders\n")
    );
    assert!(csv.contains("\n10k,R_0603,3,100,120,20,33,F1\n"), "{csv}");
    assert!(csv.contains("\nMCU,QFN-32,1,10,40,30,10,F3\n"), "{csv}");
    assert_eq!(csv.lines().count(), 5);
}

// ---- the parts still to buy

fn lines(text: &str, boards: usize) -> Vec<eh3d_phoenix_pnp::bom::OrderLine> {
    order(
        &Bom::parse(text, MAX_BOM_ROWS).unwrap(),
        boards,
        0.0,
        0,
        &[],
    )
    .unwrap()
}

#[test]
fn what_the_feeders_hold_is_set_against_the_order_once() {
    let l = lines(
        "Designator,Comment,Footprint,Quantity\n\"R1,R2,R3\",10k,R_0603,3\nU1,MCU,QFN-32,1\n",
        10,
    );
    let stock = vec![
        ("10 kΩ".to_string(), "r_0603".to_string(), 25),
        ("10k".to_string(), "R_0603".to_string(), 10),
        ("MCU".to_string(), "QFN-32".to_string(), 50),
        ("LED".to_string(), "LED_0603".to_string(), 99),
    ];
    let rows = buy_after_stock(&l, &stock);
    assert_eq!(rows.len(), 2);
    let r = rows.iter().find(|r| r.line.value == "10k").unwrap();
    // 30 are needed, 35 are held: all held, none to buy
    assert_eq!((r.line.total, r.on_hand, r.to_buy), (30, 30, 0));
    // the chip: 10 needed, 50 held, so 10 are taken and 0 are left to buy
    let u = rows.iter().find(|r| r.line.value == "MCU").unwrap();
    assert_eq!((u.on_hand, u.to_buy), (10, 0));
    // with less in the feeders the rest is to be bought
    let short = buy_after_stock(&l, &[("10k".to_string(), "R_0603".to_string(), 12)]);
    let r = short.iter().find(|r| r.line.value == "10k").unwrap();
    assert_eq!((r.on_hand, r.to_buy), (12, 18));
    let u = short.iter().find(|r| r.line.value == "MCU").unwrap();
    assert_eq!((u.on_hand, u.to_buy), (0, 10));
}

#[test]
fn two_lines_of_the_same_value_and_package_share_what_is_held() {
    // two part numbers of the same value and package are two lines; the 15 held cover the first and then the rest
    let l = lines(
        "Designator,Comment,Footprint,Part Number\nR1,10k,R_0603,AAA\nR2,10k,R_0603,BBB\n",
        10,
    );
    assert_eq!(l.len(), 2);
    let rows = buy_after_stock(&l, &[("10k".to_string(), "R_0603".to_string(), 15)]);
    let held: Vec<usize> = rows.iter().map(|r| r.on_hand).collect();
    assert_eq!(held.iter().sum::<usize>(), 15);
    assert_eq!(held, [10, 5]);
    assert_eq!(rows.iter().map(|r| r.to_buy).sum::<usize>(), 5);
    let csv = buy_csv(&rows);
    assert!(csv.starts_with(
        "value,package,part_number,per_board,boards,spare,total,on_hand,to_buy,references\n"
    ));
    assert!(
        csv.contains(",10,10,10,0,R1\n") || csv.contains("AAA,1,10,0,10,10,0,R1\n"),
        "{csv}"
    );
    assert_eq!(csv.lines().count(), 3);
}

// ---- the commands

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

fn text(o: &[u8]) -> String {
    String::from_utf8_lossy(o).into_owned()
}

#[test]
fn stock_says_ok_or_what_falls_short_and_exits_with_the_answer() {
    let o = run(&["stock", "examples/board.pos", "examples/feeders.csv"]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert!(
        text(&o.stdout).starts_with("ok: the feeders hold enough for 1 boards (7 parts on each"),
        "{}",
        text(&o.stdout)
    );
    let o = run(&[
        "stock",
        "examples/board.pos",
        "examples/feeders.csv",
        "--boards",
        "40",
    ]);
    assert_eq!(o.status.code(), Some(1));
    assert!(
        text(&o.stdout).contains("MCU / QFN-32: 40 needed"),
        "{}",
        text(&o.stdout)
    );
    assert!(
        text(&o.stderr).contains("the feeders lack 50 parts for 40 boards"),
        "{}",
        text(&o.stderr)
    );
    // one side only
    let o = run(&[
        "stock",
        "examples/board.pos",
        "examples/feeders.csv",
        "--side",
        "bottom",
        "--boards",
        "60",
    ]);
    assert_eq!(o.status.code(), Some(1));
    assert!(
        text(&o.stderr).contains("lack 10 parts"),
        "{}",
        text(&o.stderr)
    );
}

#[test]
fn stock_takes_the_boards_from_the_configuration_unless_the_option_says_otherwise() {
    let dir = std::env::temp_dir().join(format!("phoenix_pnp_stock_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("run.cfg");
    std::fs::write(&cfg, "order_boards = 40\n").unwrap();
    let c = cfg.to_str().unwrap();
    let o = run(&[
        "stock",
        "examples/board.pos",
        "examples/feeders.csv",
        "--config",
        c,
    ]);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o.stdout));
    let o = run(&[
        "stock",
        "examples/board.pos",
        "examples/feeders.csv",
        "--config",
        c,
        "--boards",
        "2",
    ]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn stock_writes_the_table_once_and_is_strict() {
    let dir = std::env::temp_dir().join(format!("phoenix_pnp_stock_out_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("stock.csv");
    let o = run(&[
        "stock",
        "examples/board.pos",
        "examples/feeders.csv",
        "--out",
        out.to_str().unwrap(),
    ]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    let csv = std::fs::read_to_string(&out).unwrap();
    assert!(
        csv.starts_with("value,package,per_board,available,needed,short,boards_covered,feeders\n")
    );
    let again = run(&[
        "stock",
        "examples/board.pos",
        "examples/feeders.csv",
        "--out",
        out.to_str().unwrap(),
    ]);
    assert!(!again.status.success());
    assert!(
        text(&again.stderr).contains("PHOENIX-PNP-040"),
        "{}",
        text(&again.stderr)
    );
    assert_eq!(std::fs::read_to_string(&out).unwrap(), csv);
    // the table may not be written over one of the inputs
    let over = run(&[
        "stock",
        "examples/board.pos",
        "examples/feeders.csv",
        "--out",
        "examples/feeders.csv",
    ]);
    assert!(!over.status.success());
    for bad in [
        vec!["stock"],
        vec!["stock", "examples/board.pos"],
        vec![
            "stock",
            "examples/board.pos",
            "examples/feeders.csv",
            "extra",
        ],
        vec![
            "stock",
            "examples/board.pos",
            "examples/feeders.csv",
            "--boards",
            "x",
        ],
        vec![
            "stock",
            "examples/board.pos",
            "examples/feeders.csv",
            "--boards",
            "0",
        ],
        vec![
            "stock",
            "examples/board.pos",
            "examples/feeders.csv",
            "--side",
            "left",
        ],
        vec![
            "stock",
            "examples/board.pos",
            "examples/feeders.csv",
            "--oops",
        ],
        vec!["stock", "examples/nothing.pos", "examples/feeders.csv"],
    ] {
        assert!(!run(&bad).status.success(), "{bad:?}");
    }
    // an error comes in the language asked
    let o = run(&[
        "stock",
        "examples/board.pos",
        "examples/feeders.csv",
        "--boards",
        "0",
        "--lang",
        "es",
    ]);
    assert!(
        text(&o.stderr).contains("PHOENIX-PNP-"),
        "{}",
        text(&o.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn bom_order_with_the_feeders_buys_only_what_is_missing() {
    let o = run(&[
        "bom-order",
        "examples/bom.csv",
        "--boards",
        "50",
        "--have",
        "examples/feeders.csv",
    ]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    let out = text(&o.stdout);
    assert!(
        out.starts_with(
            "value,package,part_number,per_board,boards,spare,total,on_hand,to_buy,references\n"
        ),
        "{out}"
    );
    // 150 resistors, 100 held: 50 to buy; 50 chips, 10 held: 40 to buy
    assert!(out.contains(",3,50,0,150,100,50,R1 R2 R3"), "{out}");
    assert!(out.contains("MCU,QFN-32,,1,50,0,50,10,40,U1"), "{out}");
    assert!(
        text(&o.stderr).contains("; 260 are in the feeders, 90 to buy"),
        "{}",
        text(&o.stderr)
    );
    // without --have the file is the one it was
    let plain = run(&["bom-order", "examples/bom.csv"]);
    assert!(text(&plain.stdout)
        .starts_with("value,package,part_number,per_board,boards,spare,total,references\n"));
    // a feeders table that is not one is refused
    let o = run(&[
        "bom-order",
        "examples/bom.csv",
        "--have",
        "examples/bom.csv",
    ]);
    assert!(!o.status.success());
}
