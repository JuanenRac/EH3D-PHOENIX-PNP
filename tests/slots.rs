//! Where the feeders stand so that the plan is the fastest: swaps of places of feeders of the same kind, what a swap saves, the feeders that never move, the slot names, the limit on plans and
//! `feeder-slots`.

use eh3d_phoenix_pnp::catalog::Catalog;
use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::error::Code;
use eh3d_phoenix_pnp::feeding::{self, assign};
use eh3d_phoenix_pnp::parts::{from_csv, Part, Side};
use eh3d_phoenix_pnp::planner::{plan_advanced, Options};
use eh3d_phoenix_pnp::slots::optimize;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Eight resistors and one capacitor on the board.
fn parts_csv() -> String {
    let mut s = String::from("ref,val,package,x,y,rot,side\n");
    for i in 0..8 {
        s.push_str(&format!("R{},10k,R,{},10,0,top\n", i + 1, 10 + i * 5));
    }
    s.push_str("C1,100n,C,30,25,0,top\n");
    s
}

/// The resistors come from a feeder that stands far from the board and the capacitor from one that stands near it.
const BAD: &str =
    "id,value,package,x,y,count,slot\nF1,10k,R,-200,10,50,S1\nF2,100n,C,-20,10,50,S2\n";
/// The same two feeders the other way round.
const GOOD: &str =
    "id,value,package,x,y,count,slot\nF1,10k,R,-20,10,50,S1\nF2,100n,C,-200,10,50,S2\n";

fn parts() -> Vec<Part> {
    from_csv(&parts_csv()).unwrap()
}

fn seconds(table: &str) -> f64 {
    let cfg = MachineConfig::parse("").unwrap();
    let parts = parts();
    let feeders = feeding::from_csv(table).unwrap();
    let picks = assign(&parts, &feeders, Some(Side::Top));
    plan_advanced(
        &parts,
        &feeders,
        &picks,
        &Catalog::new(),
        &cfg,
        &Options::default(),
    )
    .unwrap()
    .plan
    .seconds
}

fn best(
    table: &str,
    max_plans: usize,
) -> Result<eh3d_phoenix_pnp::slots::Slots, eh3d_phoenix_pnp::error::PnpError> {
    optimize(
        &parts(),
        table,
        &Catalog::new(),
        &MachineConfig::parse("").unwrap(),
        &Options::default(),
        max_plans,
    )
}

#[test]
fn a_feeder_far_away_that_holds_most_of_the_parts_trades_places_with_one_that_stands_near() {
    assert!(seconds(GOOD) < seconds(BAD));
    let s = best(BAD, 100).unwrap();
    assert_eq!(s.moves.len(), 2);
    let f1 = s.moves.iter().find(|m| m.id == "F1").unwrap();
    assert_eq!((f1.from, f1.to), ((-200.0, 10.0), (-20.0, 10.0)));
    assert_eq!(f1.slot_from.as_deref(), Some("S1"));
    assert_eq!(f1.slot_to.as_deref(), Some("S2"));
    assert!(s.after_s < s.before_s && s.saved_s() > 0.0);
    assert!(s.after_mm < s.before_mm);
    assert!(!s.truncated);
    // what it says it saves is what the plan of the new table takes
    assert!((s.before_s - seconds(BAD)).abs() < 1e-9);
    assert!(
        (s.after_s - seconds(&s.csv)).abs() < 1e-9,
        "{} {}",
        s.after_s,
        seconds(&s.csv)
    );
    assert_eq!(
        s.csv,
        "id,value,package,x,y,count,slot
F1,10k,R,-20,10,50,S2
F2,100n,C,-200,10,50,S1
"
    );
    let text = s.text();
    assert!(
        text.contains("F1: -200.000,10.000 -> -20.000,10.000 (slot S1 -> S2)\n"),
        "{text}"
    );
    assert!(text.contains("2 feeders move: the plan takes "), "{text}");
    assert!(
        text.contains("look at each tape before it is moved"),
        "{text}"
    );
}

#[test]
fn feeders_that_are_well_placed_are_left_alone_and_the_table_comes_back_as_it_was() {
    let s = best(GOOD, 100).unwrap();
    assert!(s.moves.is_empty());
    assert_eq!(s.csv, GOOD);
    assert_eq!(s.saved_s(), 0.0);
    assert_eq!((s.before_s, s.before_mm), (s.after_s, s.after_mm));
    assert!(s.text().starts_with(
        "the feeders are well placed: no swap of two feeders makes the plan faster ("
    ));
    // a single feeder has nobody to trade with
    let one = "id,value,package,x,y,count\nF1,10k,R,-200,10,50\nF2,100n,C,-200,10,50\n";
    let s = best(one, 100).unwrap();
    assert!(s.moves.is_empty());
    assert_eq!(s.plans, 1);
}

#[test]
fn only_feeders_of_the_same_kind_that_are_on_trade_and_nothing_else_moves() {
    // the strip and the tray stand where the table puts them; a feeder that is off does not trade
    let table =
        "id,value,package,x,y,count,kind,cols,rows,pitch_x,pitch_y,pitch,rotation,enabled,slot\n\
F1,10k,R,-200,10,50,strip,,,,,4,0,yes,S1\n\
F2,100n,C,-20,10,50,tray,5,10,5,5,,0,yes,S2\n\
F3,spare,X,-20,40,50,tape,,,,,,0,no,S3\n\
F4,spare2,Y,-20,60,50,tape,,,,,,0,yes,S4\n";
    let s = best(table, 100).unwrap();
    assert!(s.moves.is_empty(), "{:?}", s.moves);
    assert_eq!(s.csv, table);
    // a tape trades with a tape and not with a tube
    let mixed =
        "id,value,package,x,y,count,kind\nF1,10k,R,-200,10,50,tape\nF2,100n,C,-20,10,50,tube\n";
    assert!(best(mixed, 100).unwrap().moves.is_empty());
    let tubes =
        "id,value,package,x,y,count,kind\nF1,10k,R,-200,10,50,tube\nF2,100n,C,-20,10,50,tube\n";
    assert_eq!(best(tubes, 100).unwrap().moves.len(), 2);
}

#[test]
fn the_other_columns_and_the_delimiter_of_the_table_are_kept() {
    let table = "id;value;package;x;y;count;priority\nF1;10k;R;-200;10;50;high\nF2;\"100n; X7R\";C;-20;10;50;low\n";
    let s = best(table, 100).unwrap();
    assert_eq!(s.moves.len(), 2);
    assert_eq!(
        s.csv,
        "id;value;package;x;y;count;priority\nF1;10k;R;-20;10;50;high\nF2;\"100n; X7R\";C;-200;10;50;low\n"
    );
    // without a slot column no slot is written, and the moves say no slot
    assert!(s
        .moves
        .iter()
        .all(|m| m.slot_from.is_none() && m.slot_to.is_none()));
    assert!(!s.text().contains("(slot "));
}

#[test]
fn the_result_is_the_same_every_time_and_the_search_can_be_cut() {
    let a = best(BAD, 100).unwrap();
    let b = best(BAD, 100).unwrap();
    assert_eq!(a, b);
    // one plan only: nothing is tried
    let cut = best(BAD, 1).unwrap();
    assert!(cut.truncated && cut.moves.is_empty());
    assert_eq!(cut.plans, 1);
    assert_eq!(cut.csv, BAD);
    assert!(cut.text().contains("the search was cut"));
    // few plans, a result that is no better than the full one
    let some = best(BAD, 2).unwrap();
    assert!(some.after_s >= a.after_s - 1e-9);
    assert!(some.plans <= 2);
}

#[test]
fn a_number_of_plans_of_0_a_table_that_is_not_valid_and_a_plan_that_cannot_be_made_are_refused() {
    assert_eq!(best(BAD, 0).unwrap_err().code, Code::PlanSetting);
    assert_eq!(
        best("id,value\nF1,1\n", 10).unwrap_err().code,
        Code::FeederBadRow
    );
    let no_xy = "id,value,package,count\nF1,10k,R,5\n";
    assert!(best(no_xy, 10).is_err());
}

// ---- feeder-slots

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_phoenix-pnp"))
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap()
}

fn p(path: &Path) -> &str {
    path.to_str().unwrap()
}

#[test]
fn the_command_says_how_to_stand_the_feeders_and_writes_the_table_once() {
    let dir: PathBuf =
        std::env::temp_dir().join(format!("phoenix_pnp_slots_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let (pos, bad, good) = (
        dir.join("board.csv"),
        dir.join("bad.csv"),
        dir.join("good.csv"),
    );
    std::fs::write(&pos, parts_csv()).unwrap();
    std::fs::write(&bad, BAD).unwrap();
    std::fs::write(&good, GOOD).unwrap();
    let o = run(&["feeder-slots", p(&pos), p(&bad)]);
    let text = String::from_utf8_lossy(&o.stdout).into_owned();
    assert_eq!(
        o.status.code(),
        Some(1),
        "{text}{}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert_eq!(text, best(BAD, 5000).unwrap().text());
    // well placed: exit 0
    let o = run(&["feeder-slots", p(&pos), p(&good)]);
    assert!(o.status.success());
    assert!(String::from_utf8_lossy(&o.stdout).contains("well placed"));
    // --out writes the table once, never over what was read
    let out = dir.join("after.csv");
    let o = run(&["feeder-slots", p(&pos), p(&bad), "--out", p(&out)]);
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(
        std::fs::read_to_string(&out).unwrap(),
        best(BAD, 5000).unwrap().csv
    );
    assert!(!run(&["feeder-slots", p(&pos), p(&bad), "--out", p(&out)])
        .status
        .success());
    assert!(!run(&["feeder-slots", p(&pos), p(&bad), "--out", p(&bad)])
        .status
        .success());
    assert!(!run(&["feeder-slots", p(&pos), p(&bad), "--out", p(&pos)])
        .status
        .success());
    assert_eq!(std::fs::read_to_string(&bad).unwrap(), BAD);
    // the limit on plans
    let o = run(&["feeder-slots", p(&pos), p(&bad), "--max-plans", "1"]);
    assert!(o.status.success());
    assert!(String::from_utf8_lossy(&o.stdout).contains("the search was cut"));
    for wrong in ["0", "2.5", "many", "2000000"] {
        assert!(
            !run(&["feeder-slots", p(&pos), p(&bad), "--max-plans", wrong])
                .status
                .success(),
            "{wrong}"
        );
    }
    assert!(!run(&["feeder-slots", p(&pos)]).status.success());
    let _ = std::fs::remove_dir_all(&dir);
}
