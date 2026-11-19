//! The table of feeders after reels are changed: the count of the new reel, the strip that starts again, the tray that cannot hold more than its cells, every refusal, and `feeder-refill`.

use eh3d_phoenix_pnp::deplete::deplete;
use eh3d_phoenix_pnp::error::Code;
use eh3d_phoenix_pnp::feeding::from_csv;
use eh3d_phoenix_pnp::refill::{parse_refills, refill, MAX_COUNT, MAX_REFILLS};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const KINDS: &str = include_str!("../examples/feeders_kinds.csv");

#[test]
fn the_table_of_refills_is_read_with_its_counts_and_places() {
    let rows = parse_refills("id,count,x,y\nF1,500,,\nF2,300,-20.5,12\n").unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(
        (rows[0].id.as_str(), rows[0].count, rows[0].start),
        ("F1", 500, None)
    );
    assert_eq!(rows[1].start, Some((-20.5, 12.0)));
    // the columns may come in any order and in any case, the delimiter may be a semicolon, and a table with no place columns is allowed
    let rows = parse_refills("\u{feff}Count;ID\n7;F9\n").unwrap();
    assert_eq!((rows[0].id.as_str(), rows[0].count), ("F9", 7));
    // no rows is no refills
    assert!(parse_refills("id,count\n").unwrap().is_empty());
}

#[test]
fn a_table_of_refills_that_is_not_right_is_refused_with_its_line() {
    for (text, why) in [
        ("", "empty"),
        ("id\nF1\n", "needs the columns id and count"),
        ("id,count,x\nF1,5,1\n", "x or y but not both"),
        (
            "id,count\nF1,5\nF1,6\n",
            "line 3: the feeder F1 is named twice",
        ),
        ("id,count\n,5\n", "line 2: the feeder has no id"),
        ("id,count\nF1,0\n", "line 2: '0' is not a count"),
        ("id,count\nF1,many\n", "line 2: 'many' is not a count"),
        (
            "id,count\nF1,1000001\n",
            "is not a count of parts from 1 to 1000000",
        ),
        ("id,count\nF1,-3\n", "line 2: '-3' is not a count"),
        ("id,count,x,y\nF1,5,1,\n", "is not a y"),
        ("id,count,x,y\nF1,5,abc,2\n", "is not a x"),
        ("id,count,x,y\nF1,5,1e9,2\n", "within 100000 mm"),
        ("id,count\n\"F1,5\n", "never closed"),
    ] {
        let e = parse_refills(text).unwrap_err();
        assert_eq!(e.code, Code::FeederBadRow, "{text}");
        assert!(e.detail.contains(why), "{text}: {}", e.detail);
    }
    let many: String = std::iter::once("id,count\n".to_string())
        .chain((0..=MAX_REFILLS).map(|i| format!("F{i},1\n")))
        .collect();
    assert!(parse_refills(&many)
        .unwrap_err()
        .detail
        .contains("the most is 5000"));
    assert_eq!(MAX_COUNT, 1_000_000);
}

#[test]
fn a_refilled_feeder_has_the_parts_of_the_new_reel_and_the_rest_of_the_table_is_kept() {
    let rows = parse_refills("id,count\nF1,250\n").unwrap();
    let r = refill(KINDS, &rows).unwrap();
    assert_eq!(r.lines.len(), 1);
    assert_eq!((r.lines[0].before, r.lines[0].after), (100, 250));
    assert_eq!(r.added(), 150);
    let after = from_csv(&r.csv).unwrap();
    let before = from_csv(KINDS).unwrap();
    assert_eq!(after.len(), before.len());
    assert_eq!(after[0].base.count, 250);
    // nothing else moved, and the other columns are as they were
    for i in 1..after.len() {
        assert_eq!(after[i], before[i], "feeder {i}");
    }
    assert!(r
        .csv
        .starts_with("id,value,package,x,y,count,priority,kind,cols,rows,"));
    assert!(
        r.csv
            .contains("F1,10k,R_0603,-20,10,250,high,tape,,,,,,0,yes,10,S1,\n"),
        "{}",
        r.csv
    );
    assert_eq!(
        r.text(),
        "F1: 100 -> 250 (refilled)\n1 feeders refilled, 150 parts more\n"
    );
}

#[test]
fn a_strip_starts_again_where_the_refill_says_and_keeps_its_place_when_it_does_not() {
    let rows = parse_refills("id,count,x,y\nF2,40,-20,20\n").unwrap();
    let r = refill(KINDS, &rows).unwrap();
    let after = from_csv(&r.csv).unwrap();
    assert_eq!((after[1].base.x, after[1].base.y), (-20.0, 20.0));
    // a strip that was depleted by a job comes back to its first place with the refill
    let state = json!({"schema_version": 1, "devices": [
        {"remaining": 100}, {"remaining": 30}, {"remaining": 6}, {"remaining": 50}, {"remaining": 100}
    ]})
    .to_string();
    let used = deplete(KINDS, &state).unwrap();
    let moved = from_csv(&used.csv).unwrap();
    assert_ne!((moved[1].base.x, moved[1].base.y), (-20.0, 20.0));
    let again = refill(
        &used.csv,
        &parse_refills("id,count,x,y\nF2,40,-20,20\n").unwrap(),
    )
    .unwrap();
    let back = from_csv(&again.csv).unwrap();
    assert_eq!((back[1].base.x, back[1].base.y), (-20.0, 20.0));
    assert_eq!(back[1].base.count, 40);
    assert_eq!(
        again.lines[0].moved,
        Some(((moved[1].base.x, moved[1].base.y), (-20.0, 20.0)))
    );
    assert!(
        again
            .text()
            .contains("first place -20.000,60.000 -> -20.000,20.000"),
        "{}",
        again.text()
    );
    // with no place the strip keeps the one it has, and the report says so
    let kept = refill(&used.csv, &parse_refills("id,count\nF2,40\n").unwrap()).unwrap();
    assert!(kept.lines[0].kept_place && kept.lines[0].moved.is_none());
    assert!(kept
        .text()
        .contains("the first place of the strip was kept"));
    let still = from_csv(&kept.csv).unwrap();
    assert_eq!(
        (still[1].base.x, still[1].base.y),
        (moved[1].base.x, moved[1].base.y)
    );
}

#[test]
fn a_refill_that_does_not_fit_the_table_is_refused_and_changes_nothing() {
    for (text, why) in [
        ("id,count\nF9,5\n", "the feeder F9 is not in the table"),
        (
            "id,count\nF3,7\n",
            "a tray of 6 cells and the refill says 7",
        ),
        ("id,count,x,y\nF1,5,1,2\n", "F1 is not a strip"),
        ("id,count,x,y\nF3,5,1,2\n", "F3 is not a strip"),
    ] {
        let e = refill(KINDS, &parse_refills(text).unwrap()).unwrap_err();
        assert_eq!(e.code, Code::FeederBadRow, "{text}");
        assert!(e.detail.contains(why), "{text}: {}", e.detail);
    }
    // a tray refilled to all its cells is fine, and a table of feeders that is not valid is refused as a table
    let ok = refill(KINDS, &parse_refills("id,count\nF3,6\n").unwrap()).unwrap();
    assert_eq!(from_csv(&ok.csv).unwrap()[2].base.count, 6);
    let e = refill("id,value\nF1,1\n", &[]).unwrap_err();
    assert_eq!(e.code, Code::FeederBadRow);
    // one refill that is refused refuses all: no table comes back
    assert!(refill(KINDS, &parse_refills("id,count\nF1,5\nF9,5\n").unwrap()).is_err());
}

#[test]
fn a_semicolon_table_and_a_quoted_field_come_back_as_they_were() {
    let table =
        "id;value;package;x;y;count\nF1;\"10k; 1%\";R_0603;-20;10;100\nF2;100n;C_0603;-20;20;100\n";
    let r = refill(table, &parse_refills("id,count\nF2,30\n").unwrap()).unwrap();
    assert_eq!(
        r.csv,
        "id;value;package;x;y;count\nF1;\"10k; 1%\";R_0603;-20;10;100\nF2;100n;C_0603;-20;20;30\n"
    );
    assert_eq!(r.added(), 0);
    assert_eq!(
        r.text(),
        "F2: 100 -> 30 (refilled)\n1 feeders refilled, 0 parts more\n"
    );
    // several refills, in the order that was given
    let r = refill(table, &parse_refills("id,count\nF2,300\nF1,200\n").unwrap()).unwrap();
    assert_eq!(
        r.lines.iter().map(|l| l.id.as_str()).collect::<Vec<_>>(),
        vec!["F2", "F1"]
    );
    assert_eq!(r.added(), 300);
}

// ---- feeder-refill

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
fn the_command_writes_the_table_with_the_new_reels_and_never_over_a_file() {
    let dir = std::env::temp_dir().join(format!("phoenix_pnp_refill_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let (f, r) = (dir.join("f.csv"), dir.join("r.csv"));
    std::fs::write(&f, KINDS).unwrap();
    std::fs::write(&r, "id,count,x,y\nF1,500,,\nF2,40,-20,20\n").unwrap();
    // the report alone writes nothing
    let o = run(&["feeder-refill", p(&f), p(&r)]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let text = String::from_utf8_lossy(&o.stdout).into_owned();
    assert!(text.contains("F1: 100 -> 500 (refilled)\n"), "{text}");
    assert!(
        text.ends_with("2 feeders refilled, 400 parts more\n"),
        "{text}"
    );
    assert_eq!(std::fs::read_to_string(&f).unwrap(), KINDS);
    // --out writes the table, once
    let out = dir.join("after.csv");
    let o = run(&["feeder-refill", p(&f), p(&r), "--out", p(&out)]);
    assert!(o.status.success());
    let after = from_csv(&std::fs::read_to_string(&out).unwrap()).unwrap();
    assert_eq!((after[0].base.count, after[1].base.count), (500, 40));
    assert!(!run(&["feeder-refill", p(&f), p(&r), "--out", p(&out)])
        .status
        .success());
    // never over the table that was read, nor over the refills
    assert!(!run(&["feeder-refill", p(&f), p(&r), "--out", p(&f)])
        .status
        .success());
    assert!(!run(&["feeder-refill", p(&f), p(&r), "--out", p(&r)])
        .status
        .success());
    assert_eq!(std::fs::read_to_string(&f).unwrap(), KINDS);
    // a refill that does not fit is an error with its code and writes nothing
    std::fs::write(&r, "id,count\nF9,5\n").unwrap();
    let bad = dir.join("bad.csv");
    let o = run(&["feeder-refill", p(&f), p(&r), "--out", p(&bad)]);
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("PHOENIX-PNP-013"));
    assert!(!bad.exists());
    // arguments
    assert!(!run(&["feeder-refill", p(&f)]).status.success());
    assert!(!run(&["feeder-refill", p(&f), p(&r), "--other"])
        .status
        .success());
    assert!(!run(&["feeder-refill", p(&f), p(&dir.join("none.csv"))])
        .status
        .success());
    let _ = std::fs::remove_dir_all(PathBuf::from(&dir));
}
