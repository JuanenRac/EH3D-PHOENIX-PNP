//! The offset of the placement of each package learned from readings: the file of readings, the learning with the strays left out, the cap, the memory and the minimum, the file of what is
//! learned with its check number, the settings that keep it off, a job that takes the offsets out of its places, and the command `place-learn`.

use eh3d_phoenix_pnp::catalog::Catalog;
use eh3d_phoenix_pnp::config::{MachineConfig, KEYS};
use eh3d_phoenix_pnp::error::Code;
use eh3d_phoenix_pnp::feeding::{self, assign};
use eh3d_phoenix_pnp::job::{Job, Phase};
use eh3d_phoenix_pnp::learn::{
    allowed, apply_allowed, parse_readings, LearnSettings, Learned, Reading, MAX_PACKAGES,
    MAX_READINGS,
};
use eh3d_phoenix_pnp::parts::{from_csv, Side};
use eh3d_phoenix_pnp::planner::{plan_advanced, Options};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const MACHINE: &str = include_str!("../examples/machine.cfg");
const SETTINGS: LearnSettings = LearnSettings {
    cap_mm: 0.3,
    outlier_k: 3.5,
    min_readings: 3,
    memory: 100,
};

fn near(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

fn reading(package: &str, dx: f64, dy: f64) -> Reading {
    Reading {
        package: package.to_string(),
        dx_mm: dx,
        dy_mm: dy,
    }
}

/// `n` readings around `(dx, dy)` with a small fixed scatter.
fn around(package: &str, dx: f64, dy: f64, n: usize) -> Vec<Reading> {
    (0..n)
        .map(|i| {
            let s = ((i * 7 % 5) as f64 - 2.0) * 0.004;
            reading(package, dx + s, dy - s)
        })
        .collect()
}

fn refused(text: &str) -> String {
    let e = Learned::parse(text).unwrap_err();
    assert_eq!(e.code, Code::CalibrationBad);
    e.detail
}

// ---- the file of readings

#[test]
fn a_file_of_readings_is_read_with_its_header_comments_and_delimiters() {
    let text = "\u{feff}# the errors after the job\npackage,dx_mm,dy_mm\nR_0603,0.05,-0.02\r\n\"C, 0603\",0.1,0\n";
    let r = parse_readings(text).unwrap();
    assert_eq!(r.len(), 2, "{r:?}");
    assert_eq!(r[1].package, "C, 0603");
    // a semicolon between the columns
    let r = parse_readings("package;dx_mm;dy_mm\nU1;0.1;0.2\n").unwrap();
    assert_eq!((r[0].package.as_str(), r[0].dy_mm), ("U1", 0.2));
    // no header
    let r = parse_readings("A,0.1,0.2\nB,-0.1,-0.2\n").unwrap();
    assert_eq!((r[0].dx_mm, r[1].dy_mm), (0.1, -0.2));
    assert!(parse_readings("").unwrap().is_empty());
    assert!(parse_readings("package,dx_mm,dy_mm\n").unwrap().is_empty());
}

#[test]
fn a_damaged_file_of_readings_is_refused_with_its_line() {
    for (text, want) in [
        ("A,0.1\n", "line 1: a reading needs 3 columns"),
        ("A,0.1,0.2,0.3\n", "needs 3 columns"),
        ("A,0.1,x\n", "'x' is not a number"),
        ("A,nan,0\n", "not a believable error"),
        ("A,6,0\n", "not a believable error"),
        ("A,0,-5.5\n", "not a believable error"),
        (",0.1,0.1\n", "the name of the package is empty"),
        ("A,0.1,0.1\n\u{7},0,0\n", "line 2: the name of the package"),
        ("\"A,0.1,0.2\n", "line"),
    ] {
        let e = parse_readings(text).unwrap_err();
        assert_eq!(e.code, Code::CalibrationBad);
        assert!(e.detail.contains(want), "{text:?}: {}", e.detail);
    }
    let long = format!("{},0,0\n", "x".repeat(65));
    assert!(parse_readings(&long).is_err());
    let many = "A,0,0\n".repeat(MAX_READINGS + 1);
    assert!(parse_readings(&many)
        .unwrap_err()
        .detail
        .contains("more than"));
    assert_eq!(
        parse_readings(&"A,0,0\n".repeat(MAX_READINGS))
            .unwrap()
            .len(),
        MAX_READINGS
    );
}

// ---- the learning

#[test]
fn the_mean_of_the_readings_of_a_package_is_its_offset() {
    let mut l = Learned::new();
    let mut readings = around("R_0603", 0.1, -0.05, 8);
    readings.extend(around("c_0603", -0.02, 0.03, 4));
    let out = l.learn(&readings, &SETTINGS, false).unwrap();
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].package, "R_0603");
    assert_eq!((out[0].readings, out[0].outliers), (8, 0));
    assert!(out[0].skipped.is_none() && !out[0].capped);
    let (x, y) = l.offset_of("r_0603").unwrap();
    assert!(near(x, 0.1, 0.005) && near(y, -0.05, 0.005), "{x} {y}");
    let (x, y) = l.offset_of("C_0603").unwrap();
    assert!(near(x, -0.02, 0.005) && near(y, 0.03, 0.005), "{x} {y}");
    assert_eq!(l.offset_of("QFN-32"), None);
    assert_eq!(l.rows[0].used, 8);
    assert_eq!(l.rows[0].left_out, 0);
    // the rows are in the order the packages came in
    assert_eq!(l.rows[0].package, "R_0603");
    assert!(out[0].line().contains("8 readings, 0 left out"));
}

#[test]
fn the_order_of_the_readings_of_a_package_does_not_change_what_is_learned() {
    let mut a = around("R", 0.08, 0.02, 9);
    a.push(reading("R", 1.5, -1.5));
    let mut b = a.clone();
    b.reverse();
    let mut c = a.clone();
    c.rotate_left(4);
    let learn = |r: &[Reading]| {
        let mut l = Learned::new();
        l.learn(r, &SETTINGS, false).unwrap();
        l
    };
    assert_eq!(learn(&a), learn(&b));
    assert_eq!(learn(&a), learn(&c));
}

#[test]
fn a_reading_far_from_the_others_is_left_out() {
    let mut r = around("R", 0.1, 0.0, 9);
    r.push(reading("R", 2.0, 2.0));
    let mut l = Learned::new();
    let out = l.learn(&r, &SETTINGS, false).unwrap();
    assert_eq!((out[0].readings, out[0].outliers), (10, 1));
    let (x, y) = l.offset_of("R").unwrap();
    assert!(near(x, 0.1, 0.005) && near(y, 0.0, 0.005), "{x} {y}");
    assert_eq!((l.rows[0].used, l.rows[0].left_out), (9, 1));
    // with the rule set loose the stray stays in (and the cap holds the mean)
    let mut loose = Learned::new();
    let wide = LearnSettings {
        outlier_k: 1000.0,
        cap_mm: 1.0,
        ..SETTINGS
    };
    let out = loose.learn(&r, &wide, false).unwrap();
    assert_eq!(out[0].outliers, 0);
    assert!(loose.offset_of("R").unwrap().0 > 0.2);
    // a package of fewer than five readings has none left out
    let few = vec![
        reading("S", 0.1, 0.0),
        reading("S", 0.1, 0.0),
        reading("S", 3.0, 3.0),
    ];
    let mut f = Learned::new();
    let out = f
        .learn(
            &few,
            &LearnSettings {
                cap_mm: 1.0,
                ..SETTINGS
            },
            false,
        )
        .unwrap();
    assert_eq!(out[0].outliers, 0);
    // readings that agree to the micron do not make a hair a stray
    let tight: Vec<Reading> = (0..8)
        .map(|_| reading("T", 0.05, 0.05))
        .chain([reading("T", 0.06, 0.05)])
        .collect();
    let mut t = Learned::new();
    assert_eq!(t.learn(&tight, &SETTINGS, false).unwrap()[0].outliers, 0);
}

#[test]
fn a_package_with_too_few_readings_is_not_touched() {
    let mut l = Learned::new();
    let out = l
        .learn(&around("R", 0.1, 0.0, 2), &SETTINGS, false)
        .unwrap();
    assert!(out[0].skipped.as_ref().unwrap().contains("at least 3"));
    assert!(l.rows.is_empty());
    assert!(out[0].line().contains("not changed"));
    // what was learned stays when a later run has too few
    l.learn(&around("R", 0.1, 0.0, 6), &SETTINGS, false)
        .unwrap();
    let before = l.clone();
    l.learn(&around("R", 0.2, 0.2, 2), &SETTINGS, false)
        .unwrap();
    assert_eq!(l, before);
    // too few left after the strays
    let mut r = vec![reading("U", 0.0, 0.0); 3];
    r.extend([
        reading("U", 1.0, 1.0),
        reading("U", -1.0, 1.0),
        reading("U", 1.0, -1.0),
        reading("U", 0.0, 0.0),
    ]);
    let mut u = Learned::new();
    let out = u
        .learn(
            &r,
            &LearnSettings {
                min_readings: 6,
                ..SETTINGS
            },
            false,
        )
        .unwrap();
    assert!(out[0].skipped.is_some(), "{:?}", out[0]);
}

#[test]
fn no_offset_is_larger_than_the_cap_and_a_smaller_cap_holds_what_is_applied() {
    let mut l = Learned::new();
    let out = l
        .learn(&around("R", 0.9, -0.8, 6), &SETTINGS, false)
        .unwrap();
    assert!(out[0].capped);
    assert_eq!(l.offset_of("R"), Some((0.3, -0.3)));
    assert!(out[0].line().contains("held at the cap"));
    // the file was made under a cap of 0.3; a job under 0.1 takes 0.1
    let job = l.for_job(0.1);
    assert_eq!(job, vec![("R".to_string(), 0.1, -0.1)]);
    assert_eq!(l.for_job(1.0), vec![("R".to_string(), 0.3, -0.3)]);
}

#[test]
fn the_offset_that_is_known_counts_for_at_most_the_memory() {
    let mut l = Learned::new();
    l.learn(&around("R", 0.1, 0.0, 50), &SETTINGS, false)
        .unwrap();
    let first = l.offset_of("R").unwrap().0;
    // 50 known readings and 50 new ones weigh the same
    l.learn(&around("R", 0.2, 0.0, 50), &SETTINGS, false)
        .unwrap();
    let both = l.offset_of("R").unwrap().0;
    assert!(near(both, (first + 0.2) / 2.0, 0.002), "{both}");
    assert_eq!(l.rows[0].used, 100);
    // with a memory of 10 the new readings win: 10 against 50
    let mut m = Learned::new();
    let short = LearnSettings {
        memory: 10,
        ..SETTINGS
    };
    m.learn(&around("R", 0.1, 0.0, 50), &short, false).unwrap();
    m.learn(&around("R", 0.2, 0.0, 50), &short, false).unwrap();
    let x = m.offset_of("R").unwrap().0;
    assert!(near(x, (0.1 * 10.0 + 0.2 * 50.0) / 60.0, 0.003), "{x}");
    assert_eq!(m.rows[0].used, 100);
}

#[test]
fn residual_readings_are_added_to_what_is_known_before_they_are_averaged() {
    let mut l = Learned::new();
    l.learn(&around("R", 0.1, 0.0, 50), &SETTINGS, false)
        .unwrap();
    let known = l.offset_of("R").unwrap();
    // the parts were placed with the offset applied and ended 0.02 off: the machine is really 0.12 off
    let mut r = l.clone();
    r.learn(&around("R", 0.02, 0.0, 50), &SETTINGS, true)
        .unwrap();
    let x = r.offset_of("R").unwrap().0;
    assert!(near(x, (known.0 + known.0 + 0.02) / 2.0, 0.003), "{x}");
    // without it they are taken for raw errors of 0.02
    let mut raw = l.clone();
    raw.learn(&around("R", 0.02, 0.0, 50), &SETTINGS, false)
        .unwrap();
    assert!(raw.offset_of("R").unwrap().0 < x - 0.03);
    // a package that is not known yet has nothing to add
    let mut n = Learned::new();
    n.learn(&around("S", 0.02, 0.0, 5), &SETTINGS, true)
        .unwrap();
    assert!(near(n.offset_of("S").unwrap().0, 0.02, 0.005));
}

#[test]
fn a_package_can_be_forgotten_and_the_packages_are_limited() {
    let mut l = Learned::new();
    l.learn(&around("R", 0.1, 0.0, 5), &SETTINGS, false)
        .unwrap();
    assert!(l.summary().contains("R: X +0.1"));
    assert!(!l.forget("nothing"));
    assert!(l.forget(" r "));
    assert!(l.rows.is_empty());
    assert_eq!(l.summary(), "nothing is learned yet");
    let many: Vec<Reading> = (0..=MAX_PACKAGES)
        .flat_map(|i| around(&format!("P{i}"), 0.01, 0.0, 3))
        .collect();
    let e = Learned::new().learn(&many, &SETTINGS, false).unwrap_err();
    assert!(e.detail.contains("more than"), "{}", e.detail);
}

// ---- the file of what is learned

fn sample() -> Learned {
    let mut l = Learned::new();
    let mut r = around("R_0603", 0.1, -0.05, 8);
    r.extend(around("QFN-32", -0.03, 0.02, 5));
    l.learn(&r, &SETTINGS, false).unwrap();
    l
}

#[test]
fn the_file_is_read_back_as_it_was_written() {
    let l = sample();
    let text = l.to_text();
    assert!(text.contains("\"kind\": \"place-offsets\"") && text.contains("\"check\""));
    assert_eq!(Learned::parse(&text).unwrap(), l);
    assert_eq!(
        Learned::parse(&Learned::new().to_text()).unwrap(),
        Learned::new()
    );
    // a byte order mark in front does no harm
    assert_eq!(Learned::parse(&format!("\u{feff}{text}")).unwrap(), l);
}

#[test]
fn a_file_that_was_cut_or_changed_is_refused_with_its_reason() {
    let text = sample().to_text();
    // a number changed by hand does not agree with the check number
    let edited = text.replacen("0.1", "0.2", 1);
    assert_ne!(edited, text);
    assert!(refused(&edited).contains("check number"));
    // so does a package taken out, or a count changed
    let mut v: Value = serde_json::from_str(&text).unwrap();
    v["packages"].as_array_mut().unwrap().pop();
    assert!(refused(&v.to_string()).contains("check number"));
    let mut v: Value = serde_json::from_str(&text).unwrap();
    v["packages"][0]["used"] = 99.into();
    assert!(refused(&v.to_string()).contains("check number"));
    // the other ways of being wrong
    let doc = |f: &dyn Fn(&mut Value)| {
        let mut v: Value = serde_json::from_str(&text).unwrap();
        f(&mut v);
        v.to_string()
    };
    assert!(refused("{").contains("not valid JSON"));
    assert!(refused("[]").contains("schema_version"));
    assert!(refused(&doc(&|v| v["schema_version"] = 2.into())).contains("newer"));
    assert!(refused(&doc(&|v| v["schema_version"] = 0.into())).contains("schema_version"));
    assert!(refused(&doc(&|v| v["kind"] = "other".into())).contains("not a file of learned"));
    assert!(refused(&doc(&|v| {
        v.as_object_mut().unwrap().remove("packages");
    }))
    .contains("no list of packages"));
    assert!(refused(&doc(&|v| v["packages"][0]["dx_mm"] = 9.into())).contains("dx_mm"));
    assert!(refused(&doc(&|v| v["packages"][0]["dy_mm"] = "x".into())).contains("dy_mm"));
    assert!(refused(&doc(&|v| v["packages"][0]["used"] = (-1).into())).contains("used"));
    assert!(refused(&doc(&|v| v["packages"][0]["package"] = "".into())).contains("usable name"));
    assert!(refused(&doc(&|v| v["packages"][1]["package"] = "r_0603".into())).contains("twice"));
    assert!(refused(&doc(&|v| {
        v.as_object_mut().unwrap().remove("check");
    }))
    .contains("check number"));
    // bigger than it can be
    let big = format!("{{\"x\": \"{}\"}}", "a".repeat(5 * 1024 * 1024));
    assert!(refused(&big).contains("bigger than"));
}

#[test]
fn a_cut_file_never_panics_at_any_length() {
    let text = sample().to_text();
    for i in 0..text.len() {
        if text.is_char_boundary(i) {
            let _ = Learned::parse(&text[..i]);
        }
    }
}

// ---- the settings

#[test]
fn learning_and_applying_are_off_by_default_and_the_settings_have_ranges() {
    let d = MachineConfig::default();
    assert!(!d.place_learn && !d.place_learn_apply);
    assert_eq!(
        (
            d.place_learn_cap_mm,
            d.place_learn_outlier,
            d.place_learn_min,
            d.place_learn_memory
        ),
        (0.3, 3.5, 3, 100)
    );
    assert_eq!(LearnSettings::from_config(&d), SETTINGS);
    assert!(allowed(&d).unwrap_err().contains("place_learn = on"));
    assert!(apply_allowed(&d)
        .unwrap_err()
        .contains("place_learn_apply = on"));
    let e = MachineConfig::parse(MACHINE).unwrap();
    assert!(!e.place_learn && !e.place_learn_apply);
    for key in [
        "place_learn",
        "place_learn_apply",
        "place_learn_cap_mm",
        "place_learn_outlier",
        "place_learn_min",
        "place_learn_memory",
    ] {
        assert!(KEYS.contains(&key), "{key}");
        assert!(MACHINE.contains(&format!("\n{key} = ")), "{key}");
    }
    let on = MachineConfig::parse("place_learn = on\nplace_learn_apply = on\n").unwrap();
    assert!(allowed(&on).is_ok() && apply_allowed(&on).is_ok());
    for (key, ok, out) in [
        ("place_learn_cap_mm", ["0.01", "1"], ["0.001", "1.5"]),
        ("place_learn_outlier", ["1.5", "10"], ["1", "11"]),
        ("place_learn_min", ["1", "1000"], ["0", "1001"]),
        ("place_learn_memory", ["1", "100000"], ["0", "100001"]),
    ] {
        for v in ok {
            assert!(
                MachineConfig::parse(&format!("{key} = {v}")).is_ok(),
                "{key} {v}"
            );
        }
        for v in out {
            assert_eq!(
                MachineConfig::parse(&format!("{key} = {v}"))
                    .unwrap_err()
                    .code,
                Code::ConfigOutOfRange,
                "{key} {v}"
            );
        }
    }
    assert!(MachineConfig::parse("place_learn = maybe").is_err());
}

// ---- a job that takes the offsets out of its places

const PARTS: &str = "ref,val,package,x,y,rot,side\nR1,10k,R,10,10,0,top\nR2,10k,R,20,10,90,top\nC1,100n,C,10,20,0,top\n";
const FEEDERS: &str = "id,value,package,x,y,count\nF1,10k,R,-20,10,10\nF2,100n,C,-20,20,10\n";

fn job() -> Job {
    let cfg = MachineConfig::default();
    let parts = from_csv(PARTS).unwrap();
    let feeders = feeding::from_csv(FEEDERS).unwrap();
    let picks = assign(&parts, &feeders, Some(Side::Top));
    let plan = plan_advanced(
        &parts,
        &feeders,
        &picks,
        &Catalog::new(),
        &cfg,
        &Options::default(),
    )
    .unwrap();
    Job::new(plan, feeders, cfg).unwrap()
}

fn notes(j: &Job) -> Vec<String> {
    j.machine()
        .events
        .iter()
        .map(|e| e.text.clone())
        .filter(|t| t.contains("learned offset"))
        .collect()
}

#[test]
fn a_job_with_offsets_places_the_parts_of_that_package_off_by_them() {
    let mut plain = job();
    assert_eq!(plain.run(None).unwrap(), Phase::Finished);
    assert!(notes(&plain).is_empty());
    // an offset of nothing changes nothing
    let mut zero = job();
    zero.set_place_offsets(&[("R".to_string(), 0.0, 0.0)]);
    zero.run(None).unwrap();
    assert!(notes(&zero).is_empty());
    assert_eq!(zero.stats(), plain.stats());
    assert_eq!(zero.machine().events.len(), plain.machine().events.len());
    // an offset for R: the two resistors are placed with it taken out, the capacitor is not
    let mut j = job();
    j.set_place_offsets(&[
        (" r ".to_string(), 0.12, -0.07),
        ("Q".to_string(), 0.2, 0.2),
    ]);
    assert_eq!(j.run(None).unwrap(), Phase::Finished);
    let n = notes(&j);
    assert_eq!(n.len(), 2, "{n:?}");
    assert!(n.iter().all(|t| t.contains("X +0.1200 Y -0.0700")), "{n:?}");
    assert_eq!(j.stats().placed, 3);
    // the way of the head is not the one without the offset
    assert_ne!(j.trace().travel_us, plain.trace().travel_us);
}

// ---- the command

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

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("phoenix_pnp_learn_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn p(path: &Path) -> &str {
    path.to_str().unwrap()
}

fn readings_file(dir: &Path, name: &str, rows: &[Reading]) -> PathBuf {
    let mut s = String::from("package,dx_mm,dy_mm\n");
    for r in rows {
        s.push_str(&format!("{},{},{}\n", r.package, r.dx_mm, r.dy_mm));
    }
    let path = dir.join(name);
    std::fs::write(&path, s).unwrap();
    path
}

#[test]
fn the_command_learns_shows_and_forgets_only_when_it_is_turned_on() {
    let dir = scratch("cmd");
    let on = dir.join("on.cfg");
    std::fs::write(&on, "place_learn = on\n").unwrap();
    let state = dir.join("offsets.json");
    let mut rows = around("R_0603", 0.1, -0.05, 8);
    rows.push(reading("R_0603", 3.0, 3.0));
    rows.extend(around("U1", 0.01, 0.0, 2));
    let file = readings_file(&dir, "readings.csv", &rows);
    // off: refused with what to turn on, nothing is written
    let o = run(&["place-learn", p(&file), "--state", p(&state)]);
    assert!(!o.status.success());
    assert!(
        text(&o.stderr).contains("place_learn = on"),
        "{}",
        text(&o.stderr)
    );
    assert!(!state.exists());
    // dry run: says what would change and writes nothing
    let o = run(&[
        "place-learn",
        p(&file),
        "--state",
        p(&state),
        "--config",
        p(&on),
        "--dry-run",
    ]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    let out = text(&o.stdout);
    assert!(out.contains("R_0603: 9 readings, 1 left out"), "{out}");
    assert!(
        out.contains("U1: not changed, 2 readings, at least 3"),
        "{out}"
    );
    assert!(out.contains("1 packages would change"), "{out}");
    assert!(!state.exists());
    // on: written
    let o = run(&[
        "place-learn",
        p(&file),
        "--state",
        p(&state),
        "--config",
        p(&on),
    ]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert!(text(&o.stdout).contains("1 packages learned"));
    let learned = Learned::parse(&std::fs::read_to_string(&state).unwrap()).unwrap();
    let (x, y) = learned.offset_of("R_0603").unwrap();
    assert!(near(x, 0.1, 0.005) && near(y, -0.05, 0.005), "{x} {y}");
    assert_eq!(learned.offset_of("U1"), None);
    // the file can be shown with the mode off
    let o = run(&["place-learn", "--state", p(&state), "--show"]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert!(
        text(&o.stdout).contains("R_0603: X +0.1"),
        "{}",
        text(&o.stdout)
    );
    // a second run joins the first
    let more = readings_file(&dir, "more.csv", &around("R_0603", 0.2, -0.05, 8));
    let o = run(&[
        "place-learn",
        p(&more),
        "--state",
        p(&state),
        "--config",
        p(&on),
    ]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    let two = Learned::parse(&std::fs::read_to_string(&state).unwrap()).unwrap();
    assert!(near(two.offset_of("R_0603").unwrap().0, 0.15, 0.01));
    assert_eq!(two.rows[0].used, 16);
    // nothing to learn: the exit code is 1 and the file is left alone
    let thin = readings_file(&dir, "thin.csv", &around("Z", 0.1, 0.0, 2));
    let before = std::fs::read_to_string(&state).unwrap();
    let o = run(&[
        "place-learn",
        p(&thin),
        "--state",
        p(&state),
        "--config",
        p(&on),
    ]);
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(std::fs::read_to_string(&state).unwrap(), before);
    // forget one package
    let o = run(&["place-learn", "--state", p(&state), "--forget", "r_0603"]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert!(Learned::parse(&std::fs::read_to_string(&state).unwrap())
        .unwrap()
        .rows
        .is_empty());
    let o = run(&["place-learn", "--state", p(&state), "--forget", "r_0603"]);
    assert!(!o.status.success());
    assert!(text(&o.stderr).contains("nothing is learned of"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_command_refuses_a_file_that_was_changed_and_the_mistakes() {
    let dir = scratch("bad");
    let on = dir.join("on.cfg");
    std::fs::write(&on, "place_learn = on\n").unwrap();
    let state = dir.join("offsets.json");
    let file = readings_file(&dir, "readings.csv", &around("R", 0.1, 0.0, 5));
    assert!(run(&[
        "place-learn",
        p(&file),
        "--state",
        p(&state),
        "--config",
        p(&on)
    ])
    .status
    .success());
    let good = std::fs::read_to_string(&state).unwrap();
    std::fs::write(&state, good.replacen("0.1", "0.2", 1)).unwrap();
    let o = run(&[
        "place-learn",
        p(&file),
        "--state",
        p(&state),
        "--config",
        p(&on),
    ]);
    assert!(!o.status.success());
    assert!(
        text(&o.stderr).contains("PHOENIX-PNP-058"),
        "{}",
        text(&o.stderr)
    );
    assert!(text(&o.stderr).contains("check number"));
    assert!(!run(&["place-learn", "--state", p(&state), "--show"])
        .status
        .success());
    // the readings may not be the state file, and a file of readings that is wrong is told
    std::fs::write(&state, &good).unwrap();
    let o = run(&[
        "place-learn",
        p(&state),
        "--state",
        p(&state),
        "--config",
        p(&on),
    ]);
    assert!(!o.status.success());
    let junk = dir.join("junk.csv");
    std::fs::write(&junk, "R,1,2,3\n").unwrap();
    let o = run(&[
        "place-learn",
        p(&junk),
        "--state",
        p(&state),
        "--config",
        p(&on),
    ]);
    assert!(!o.status.success());
    assert!(text(&o.stderr).contains("PHOENIX-PNP-058"));
    let empty = dir.join("empty.csv");
    std::fs::write(&empty, "package,dx_mm,dy_mm\n").unwrap();
    assert!(!run(&[
        "place-learn",
        p(&empty),
        "--state",
        p(&state),
        "--config",
        p(&on)
    ])
    .status
    .success());
    // mistakes
    assert!(!run(&["place-learn", p(&file)]).status.success());
    assert!(!run(&["place-learn", "--state", p(&state)]).status.success());
    assert!(
        !run(&["place-learn", p(&file), "--state", p(&state), "--oops"])
            .status
            .success()
    );
    assert!(
        !run(&["place-learn", p(&file), "--state", p(&state), "--show"])
            .status
            .success()
    );
    assert!(!run(&[
        "place-learn",
        "nothing.csv",
        "--state",
        p(&state),
        "--config",
        p(&on)
    ])
    .status
    .success());
    assert!(text(&run(&["help"]).stdout).contains("place-learn"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn simulate_takes_the_learned_offsets_only_when_they_are_allowed_and_valid() {
    let dir = scratch("sim");
    let state = dir.join("offsets.json");
    let mut l = Learned::new();
    l.learn(&around("R_0603", 0.1, -0.05, 5), &SETTINGS, false)
        .unwrap();
    std::fs::write(&state, l.to_text()).unwrap();
    let base = ["simulate", "examples/board.pos", "examples/feeders.csv"];
    let args = |extra: &[&str]| -> Vec<String> {
        base.iter()
            .chain(extra.iter())
            .map(ToString::to_string)
            .collect()
    };
    let go = |extra: &[&str]| {
        let v = args(extra);
        let refs: Vec<&str> = v.iter().map(String::as_str).collect();
        run(&refs)
    };
    // off: refused with what to turn on
    let o = go(&["--place-offsets", p(&state)]);
    assert!(!o.status.success());
    assert!(
        text(&o.stderr).contains("place_learn_apply = on"),
        "{}",
        text(&o.stderr)
    );
    let on = dir.join("apply.cfg");
    std::fs::write(&on, "place_learn_apply = on\n").unwrap();
    let plain = go(&["--config", p(&on)]);
    let with = go(&["--config", p(&on), "--place-offsets", p(&state)]);
    assert!(with.status.success(), "{}", text(&with.stderr));
    assert!(
        text(&with.stdout).contains("placed 6 of 6"),
        "{}",
        text(&with.stdout)
    );
    assert!(plain.status.success());
    // a file that was changed is refused before the job runs
    std::fs::write(&state, l.to_text().replacen("0.1", "0.2", 1)).unwrap();
    let o = go(&["--config", p(&on), "--place-offsets", p(&state)]);
    assert!(!o.status.success());
    assert!(
        text(&o.stderr).contains("check number"),
        "{}",
        text(&o.stderr)
    );
    assert!(
        !go(&["--config", p(&on), "--place-offsets", "nothing.json"])
            .status
            .success()
    );
    let _ = std::fs::remove_dir_all(&dir);
}
