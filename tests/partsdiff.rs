//! What changed between two tables of parts: each kind of change, the tolerances, the angles that are the same turn, damaged input, the report and `parts-diff`.

use eh3d_phoenix_pnp::error::Code;
use eh3d_phoenix_pnp::parts::{from_csv, Part};
use eh3d_phoenix_pnp::partsdiff::{diff, Settings, KINDS};
use std::path::PathBuf;
use std::process::{Command, Output};

const BASE: &str = "ref,val,package,x,y,rot,side\nR1,10k,R_0603,10,10,0,top\nR2,10k,R_0603,20,10,90,top\nC1,100n,C_0603,10,20,0,top\nU1,MCU,QFN-32,40,15,0,top\n";

fn parts(text: &str) -> Vec<Part> {
    from_csv(text).unwrap()
}

fn kinds(text: &str, other: &str) -> Vec<(String, &'static str)> {
    diff(&parts(text), &parts(other), &Settings::default())
        .unwrap()
        .changes
        .into_iter()
        .map(|c| (c.reference, c.kind))
        .collect()
}

#[test]
fn the_same_table_is_the_same_and_every_part_is_counted() {
    let d = diff(&parts(BASE), &parts(BASE), &Settings::default()).unwrap();
    assert!(d.is_same());
    assert_eq!((d.old_parts, d.new_parts, d.same), (4, 4, 4));
    assert_eq!(d.parts_changed(), 0);
    assert_eq!(
        d.summary(),
        "4 parts in the first table, 4 in the second, 4 the same"
    );
    assert_eq!(
        d.text(),
        "4 parts in the first table, 4 in the second, 4 the same\n"
    );
    assert_eq!(d.csv(), "reference,change,old,new\n");
}

#[test]
fn a_part_that_is_added_or_taken_away_is_told_with_what_it_is() {
    let more = format!("{BASE}R3,4k7,R_0603,30,10,180,bottom\n");
    let d = diff(&parts(BASE), &parts(&more), &Settings::default()).unwrap();
    assert_eq!(d.count("added"), 1);
    assert_eq!(
        d.changes[0].new,
        "4k7 R_0603 at 30.000,10.000 turned 180.0 on the bottom"
    );
    assert!(d.text().contains("R3: added: 4k7 R_0603 at 30.000,10.000"));
    let back = diff(&parts(&more), &parts(BASE), &Settings::default()).unwrap();
    assert_eq!(back.count("removed"), 1);
    assert!(back.text().contains("R3: removed: 4k7 R_0603"));
    assert_eq!(
        back.summary(),
        "5 parts in the first table, 4 in the second, 4 the same: 1 removed"
    );
}

#[test]
fn each_kind_of_change_is_found_and_a_part_may_have_several() {
    let changed = "ref,val,package,x,y,rot,side\nR1,10k,R_0603,11.5,10,0,top\nR2,22k,R_0805,20,10,180,bottom\nC1,100n,C_0603,10,20,0,top\nU1,MCU,QFN-32,40,15,0,top\n";
    let d = diff(&parts(BASE), &parts(changed), &Settings::default()).unwrap();
    let names: Vec<(String, &str)> = d
        .changes
        .iter()
        .map(|c| (c.reference.clone(), c.kind))
        .collect();
    assert_eq!(
        names,
        vec![
            ("R1".to_string(), "moved"),
            ("R2".to_string(), "turned"),
            ("R2".to_string(), "value"),
            ("R2".to_string(), "package"),
            ("R2".to_string(), "side"),
        ]
    );
    assert_eq!(d.parts_changed(), 2);
    assert_eq!(d.same, 2);
    let moved = &d.changes[0];
    assert_eq!(
        (moved.old.as_str(), moved.new.as_str()),
        ("10.000,10.000", "11.500,10.000")
    );
    assert!(d.text().contains("R2: turned: 90.0 -> 180.0\n"));
    assert!(d
        .summary()
        .ends_with(": 1 moved, 1 turned, 1 value, 1 package, 1 side"));
    // every kind has a name that the counts know
    for k in KINDS {
        let _ = d.count(k);
    }
}

#[test]
fn the_tolerances_hold_the_noise_of_the_decimals_and_the_same_turn_written_twice() {
    let noisy = "ref,val,package,x,y,rot,side\nR1,10K,r_0603,10.004,10.003,359.8,top\nR2,10k,R_0603,20,10,450,top\nC1,100n,C_0603,10,20,-0,top\nU1,MCU,QFN-32,40,15,0,top\n";
    // 10K and r_0603 are the same in another case, 359.8 is 0.2 from 0, 450 is 90
    assert!(kinds(BASE, noisy).is_empty());
    // a tight tolerance sees the 0.005 mm and the 0.2 degrees
    let tight = Settings {
        tol_mm: 0.001,
        rot_tol_deg: 0.1,
    };
    let d = diff(&parts(BASE), &parts(noisy), &tight).unwrap();
    assert_eq!(d.count("moved"), 1);
    assert_eq!(d.count("turned"), 1);
    // the angle is measured the short way round: 350 and 10 are 20 apart
    let a = "ref,val,package,x,y,rot,side\nR1,1,R,0,0,350,top\n";
    let b = "ref,val,package,x,y,rot,side\nR1,1,R,0,0,10,top\n";
    let s = |t: f64| Settings {
        tol_mm: 0.01,
        rot_tol_deg: t,
    };
    assert!(diff(&parts(a), &parts(b), &s(20.0)).unwrap().is_same());
    assert!(!diff(&parts(a), &parts(b), &s(19.9)).unwrap().is_same());
}

#[test]
fn a_tolerance_out_of_range_and_a_reference_twice_are_errors() {
    for (tol, rot) in [
        (-1.0, 0.5),
        (101.0, 0.5),
        (f64::NAN, 0.5),
        (0.01, 181.0),
        (0.01, f64::INFINITY),
    ] {
        let e = diff(
            &parts(BASE),
            &parts(BASE),
            &Settings {
                tol_mm: tol,
                rot_tol_deg: rot,
            },
        )
        .unwrap_err();
        assert_eq!(e.code, Code::PlanSetting, "{tol} {rot}");
    }
    // the table is built by hand: a file with a reference twice may be refused before the comparison
    let mut p = parts(BASE);
    p.push(p[0].clone());
    let e = diff(&p, &parts(BASE), &Settings::default()).unwrap_err();
    assert_eq!(e.code, Code::PartsBad);
    assert!(
        e.detail.contains("R1") && e.detail.contains("first"),
        "{}",
        e.detail
    );
    let e = diff(&parts(BASE), &p, &Settings::default()).unwrap_err();
    assert!(e.detail.contains("second"), "{}", e.detail);
}

#[test]
fn the_changes_are_in_the_order_of_the_references_and_the_csv_quotes_what_it_must() {
    let a = "ref,val,package,x,y,rot,side\nB2,\"1,5k\",R,0,0,0,top\nA1,1,R,0,0,0,top\n";
    let b = "ref,val,package,x,y,rot,side\nB2,\"2,2k\",R,0,0,0,top\nC3,1,R,0,0,0,top\n";
    let d = diff(&parts(a), &parts(b), &Settings::default()).unwrap();
    let refs: Vec<&str> = d.changes.iter().map(|c| c.reference.as_str()).collect();
    assert_eq!(refs, vec!["A1", "B2", "C3"]);
    let csv = d.csv();
    assert!(csv.contains("B2,value,\"1,5k\",\"2,2k\"\n"), "{csv}");
    assert!(csv.starts_with("reference,change,old,new\nA1,removed,"));
}

// ---- parts-diff

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
fn the_command_exits_with_1_when_the_tables_differ_and_writes_the_changes_to_a_new_file() {
    let dir = std::env::temp_dir().join(format!("phoenix_pnp_partsdiff_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let a = dir.join("a.csv");
    let b = dir.join("b.csv");
    std::fs::write(&a, BASE).unwrap();
    std::fs::write(
        &b,
        BASE.replace("R2,10k,R_0603,20,10,90", "R2,10k,R_0603,20,12,90"),
    )
    .unwrap();
    let (pa, pb) = (a.to_str().unwrap(), b.to_str().unwrap());
    let same = run(&["parts-diff", pa, pa]);
    assert!(same.status.success());
    assert!(String::from_utf8_lossy(&same.stdout).contains("4 the same"));
    let out_file = dir.join("changes.csv");
    let po = out_file.to_str().unwrap();
    let o = run(&["parts-diff", pa, pb, "--out", po]);
    assert!(!o.status.success());
    let stdout = String::from_utf8_lossy(&o.stdout).into_owned();
    assert!(
        stdout.contains("R2: moved: 20.000,10.000 -> 20.000,12.000"),
        "{stdout}"
    );
    assert!(String::from_utf8_lossy(&o.stderr).contains("1 parts differ"));
    let csv = std::fs::read_to_string(&out_file).unwrap();
    assert!(csv.contains("R2,moved,"), "{csv}");
    // a tolerance of 3 mm takes the move for the same
    let o = run(&["parts-diff", pa, pb, "--tol", "3"]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    // the file is not written over, and an input is not an output
    assert!(!run(&["parts-diff", pa, pb, "--out", po]).status.success());
    assert!(!run(&["parts-diff", pa, pb, "--out", pa]).status.success());
    // bad words
    assert!(!run(&["parts-diff", pa]).status.success());
    assert!(!run(&["parts-diff", pa, pb, "--tol", "x"]).status.success());
    assert!(!run(&["parts-diff", pa, pb, "--nothing"]).status.success());
    let _ = std::fs::remove_dir_all(&dir);
}
