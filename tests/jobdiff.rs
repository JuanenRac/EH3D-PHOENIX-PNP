//! What differs between two jobs in files: the parts that come and go, the feeder, the nozzle, the places and the turns within their tolerance, the status, the order, the plan as a whole, and
//! `job-diff`.

use eh3d_phoenix_pnp::catalog::Catalog;
use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::exchange::{self, Exchange};
use eh3d_phoenix_pnp::feeding::{self, assign};
use eh3d_phoenix_pnp::job::Job;
use eh3d_phoenix_pnp::jobdiff::{diff, PLACE_TOL_MM, TURN_TOL_DEG};
use eh3d_phoenix_pnp::parts::{from_csv, Side};
use eh3d_phoenix_pnp::planner::{plan_advanced, Options};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const PARTS: &str = "ref,val,package,x,y,rot,side\nR1,10k,R,10,10,0,top\nR2,10k,R,20,10,90,top\nR3,10k,R,30,10,180,top\nC1,100n,C,10,20,0,top\nC2,100n,C,20,20,0,top\nU1,MCU,Q,40,15,0,top\n";
const FEEDERS: &str =
    "id,value,package,x,y,count\nF1,10k,R,-20,10,10\nF2,100n,C,-20,20,10\nF3,MCU,Q,-20,40,2\n";

/// The plan of the tables as a file of `job-export`, and the job after `steps` steps when they are given.
fn text_of(parts_csv: &str, feeders_csv: &str, steps: Option<usize>) -> String {
    let cfg = MachineConfig::parse("").unwrap();
    let parts = from_csv(parts_csv).unwrap();
    let feeders = feeding::from_csv(feeders_csv).unwrap();
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
    match steps {
        None => exchange::from_plan(&plan),
        Some(n) => {
            let mut job = Job::new(plan, feeders, cfg).unwrap();
            let _ = job.run(Some(n));
            exchange::from_job(&job)
        }
    }
}

fn read(text: &str) -> Exchange {
    exchange::parse(text).unwrap()
}

fn base() -> String {
    text_of(PARTS, FEEDERS, None)
}

/// The file with `edit` applied to its JSON.
fn edited(text: &str, edit: impl FnOnce(&mut Value)) -> String {
    let mut v: Value = serde_json::from_str(text).unwrap();
    edit(&mut v);
    serde_json::to_string_pretty(&v).unwrap()
}

fn kinds(d: &eh3d_phoenix_pnp::jobdiff::JobDiff) -> Vec<(String, &'static str)> {
    d.changes
        .iter()
        .map(|c| (c.reference.clone(), c.kind))
        .collect()
}

#[test]
fn a_job_is_the_same_as_itself_and_the_text_and_the_table_say_so() {
    let a = read(&base());
    let d = diff(&a, &read(&base()));
    assert!(d.is_same() && d.same_sequence);
    assert_eq!(d.parts_changed(), 0);
    assert_eq!(d.text(), "the two jobs are the same\n");
    assert_eq!(d.csv(), "reference,change,first,second\n");
}

#[test]
fn the_state_of_a_job_that_was_started_is_a_difference_of_status_and_of_phase() {
    let before = read(&base());
    let after = read(&text_of(PARTS, FEEDERS, Some(4)));
    let d = diff(&before, &after);
    assert!(!d.is_same());
    assert!(d
        .changes
        .iter()
        .any(|c| c.kind == "phase" && c.reference.is_empty()));
    let placed: Vec<_> = d.changes.iter().filter(|c| c.kind == "status").collect();
    assert!(!placed.is_empty());
    assert!(placed
        .iter()
        .all(|c| c.first == "pending" && c.second == "placed"));
    // nothing else of the plan moved
    assert!(d
        .changes
        .iter()
        .all(|c| matches!(c.kind, "phase" | "status" | "reason")));
    assert!(d.same_sequence);
    let text = d.text();
    assert!(text.contains("plan phase: ready -> paused\n"), "{text}");
    assert!(text.contains(": status pending -> placed\n"), "{text}");
    assert!(text.ends_with("differences in all\n"), "{text}");
}

#[test]
fn a_part_that_comes_or_goes_and_a_feeder_that_moves_are_told_and_so_are_the_totals_of_the_plan() {
    let first = read(&base());
    // a part taken out of the table
    let fewer = PARTS.replace("C2,100n,C,20,20,0,top\n", "");
    let d = diff(&first, &read(&text_of(&fewer, FEEDERS, None)));
    assert!(
        kinds(&d).contains(&("C2".to_string(), "removed")),
        "{:?}",
        kinds(&d)
    );
    assert!(d.changes.iter().any(|c| c.kind == "seconds"));
    assert_eq!(
        d.parts_changed(),
        1 + d
            .changes
            .iter()
            .filter(|c| !c.reference.is_empty() && c.reference != "C2" && c.kind != "sequence")
            .map(|c| c.reference.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len()
    );
    // and the other way round it is added
    let d = diff(&read(&text_of(&fewer, FEEDERS, None)), &first);
    assert!(kinds(&d).contains(&("C2".to_string(), "added")));
    assert!(d.text().contains("C2: added, from the feeder F2\n"));
    // a feeder that stands somewhere else: the pick of its parts changes, the place does not
    let moved = FEEDERS.replace("F1,10k,R,-20,10,10", "F1,10k,R,-20,15,10");
    let d = diff(&first, &read(&text_of(PARTS, &moved, None)));
    let picks: Vec<_> = d.changes.iter().filter(|c| c.kind == "pick").collect();
    assert_eq!(picks.len(), 3);
    assert!(picks
        .iter()
        .all(|c| c.first == "-20.000,10.000" && c.second == "-20.000,15.000"));
    assert!(!d.changes.iter().any(|c| c.kind == "place"));
    assert!(d.changes.iter().any(|c| c.kind == "travel"));
}

#[test]
fn a_place_and_a_turn_within_their_tolerance_are_the_same_and_beyond_it_are_not() {
    let first = base();
    let nudge = |dx: f64, dturn: f64| {
        edited(&first, |v| {
            let p = &mut v["plan"]["placements"][0]["place"];
            let x = p["x"].as_f64().unwrap();
            p["x"] = Value::from(x + dx);
            let r = p["rotation"].as_f64().unwrap();
            p["rotation"] = Value::from(r + dturn);
        })
    };
    let a = read(&first);
    assert!(diff(&a, &read(&nudge(PLACE_TOL_MM / 2.0, TURN_TOL_DEG / 2.0))).is_same());
    let d = diff(&a, &read(&nudge(PLACE_TOL_MM * 4.0, 0.0)));
    assert_eq!(d.changes.len(), 1);
    assert_eq!(d.changes[0].kind, "place");
    let d = diff(&a, &read(&nudge(0.0, TURN_TOL_DEG * 4.0)));
    assert_eq!(d.changes.len(), 1);
    assert_eq!(d.changes[0].kind, "turn");
    // a turn is measured the short way round: 359.995 and 0 are 0.005 degrees apart
    let wrap = edited(&first, |v| {
        let first_ref = v["plan"]["placements"][0]["place"]["rotation"]
            .as_f64()
            .unwrap();
        v["plan"]["placements"][0]["place"]["rotation"] = Value::from(first_ref + 360.0 - 0.005);
    });
    assert!(diff(&a, &read(&wrap)).is_same());
}

#[test]
fn the_nozzle_the_batch_the_tip_and_the_feeder_of_a_part_are_each_a_change_of_their_own() {
    let first = base();
    let a = read(&first);
    let other = edited(&first, |v| {
        let p = &mut v["plan"]["placements"][1];
        p["nozzle"] = Value::from(p["nozzle"].as_u64().unwrap() + 1);
        p["feeder"] = Value::from("F9");
        p["tip"] = Value::from("T9");
    });
    let d = diff(&a, &read(&other));
    let reference = a.parts[1].reference.clone();
    let got: Vec<&str> = d.changes.iter().map(|c| c.kind).collect();
    assert_eq!(got, vec!["feeder", "nozzle", "tip"], "{d:?}");
    assert!(d.changes.iter().all(|c| c.reference == reference));
    assert_eq!(d.parts_changed(), 1);
    assert!(
        d.text().contains(&format!("{reference}: nozzle ")),
        "{}",
        d.text()
    );
    assert!(
        d.text().contains("1 parts differ, 3 differences in all\n"),
        "{}",
        d.text()
    );
    let csv = d.csv();
    assert!(
        csv.contains(&format!("{reference},feeder,F1,F9\n")) || csv.contains(",feeder,"),
        "{csv}"
    );
}

#[test]
fn the_order_of_the_parts_is_compared_among_the_parts_that_are_in_both() {
    let first = base();
    let a = read(&first);
    // the first two placements change places in the list (and in the state)
    let swapped = edited(&first, |v| {
        v["plan"]["placements"].as_array_mut().unwrap().swap(0, 1);
        v["state"]["parts"].as_array_mut().unwrap().swap(0, 1);
    });
    let d = diff(&a, &read(&swapped));
    assert!(!d.same_sequence);
    let seq: Vec<_> = d.changes.iter().filter(|c| c.kind == "sequence").collect();
    assert_eq!(seq.len(), 1);
    assert_eq!(seq[0].reference, a.parts[0].reference);
    assert_eq!(seq[0].first, "step 1 of 6");
    assert!(
        d.text()
            .contains("the order of the parts that are in both differs: "),
        "{}",
        d.text()
    );
    assert!(d.text().contains("the order of the parts is not the same"));
    // a part that goes does not change the order of the others
    let fewer = PARTS.replace("R1,10k,R,10,10,0,top\n", "");
    let b = read(&text_of(&fewer, FEEDERS, None));
    let d = diff(&a, &b);
    assert!(kinds(&d).contains(&("R1".to_string(), "removed")));
}

// ---- job-diff

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
fn the_command_compares_two_files_and_never_writes_over_a_file() {
    let dir: PathBuf =
        std::env::temp_dir().join(format!("phoenix_pnp_jobdiff_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let (a, b, same) = (
        dir.join("a.json"),
        dir.join("b.json"),
        dir.join("same.json"),
    );
    std::fs::write(&a, base()).unwrap();
    std::fs::write(&same, base()).unwrap();
    std::fs::write(&b, text_of(PARTS, FEEDERS, Some(4))).unwrap();
    // the same job: exit 0
    let o = run(&["job-diff", p(&a), p(&same)]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(
        String::from_utf8_lossy(&o.stdout),
        "the two jobs are the same\n"
    );
    // different jobs: exit 1, the same text as the library
    let o = run(&["job-diff", p(&a), p(&b)]);
    assert_eq!(o.status.code(), Some(2));
    let expected = diff(&read(&base()), &read(&text_of(PARTS, FEEDERS, Some(4)))).text();
    assert_eq!(String::from_utf8_lossy(&o.stdout), expected);
    // --out writes the table once, never over a file that exists or was read
    let out = dir.join("changes.csv");
    let o = run(&["job-diff", p(&a), p(&b), "--out", p(&out)]);
    assert_eq!(o.status.code(), Some(2));
    assert!(std::fs::read_to_string(&out)
        .unwrap()
        .starts_with("reference,change,first,second\n"));
    assert!(!run(&["job-diff", p(&a), p(&b), "--out", p(&out)])
        .status
        .success());
    assert!(!run(&["job-diff", p(&a), p(&b), "--out", p(&a)])
        .status
        .success());
    assert!(!run(&["job-diff", p(&a), p(&b), "--out", p(&b)])
        .status
        .success());
    // a file that is not a job is named, with its code
    let junk = dir.join("junk.json");
    std::fs::write(&junk, "{\"kind\": \"other\"}").unwrap();
    let o = run(&["job-diff", p(&a), p(&junk)]);
    assert!(!o.status.success());
    let err = String::from_utf8_lossy(&o.stderr).into_owned();
    assert!(
        err.contains("junk.json") && err.contains("PHOENIX-PNP-026"),
        "{err}"
    );
    assert!(!run(&["job-diff", p(&a)]).status.success());
    assert!(!run(&["job-diff", p(&a), p(&dir.join("none.json"))])
        .status
        .success());
    let _ = std::fs::remove_dir_all(&dir);
}
