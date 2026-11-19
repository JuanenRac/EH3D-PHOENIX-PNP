//! The journal of a simulated job: one row for each step, the same steps as a job that is not recorded, the error that stopped it, a cut at a number of steps and `simulate --journal`.

use eh3d_phoenix_pnp::catalog::Catalog;
use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::error::Code;
use eh3d_phoenix_pnp::feeding::{self, assign};
use eh3d_phoenix_pnp::job::{Job, Phase};
use eh3d_phoenix_pnp::journal::{csv, run_recorded, summary, HEADER};
use eh3d_phoenix_pnp::model::Machine;
use eh3d_phoenix_pnp::parts::{from_csv, Side};
use eh3d_phoenix_pnp::planner::{plan_advanced, Options};
use eh3d_phoenix_pnp::sim::SimOp;
use std::path::PathBuf;
use std::process::{Command, Output};

const PARTS: &str = "ref,val,package,x,y,rot,side\nR1,10k,R,10,10,0,top\nR2,10k,R,20,10,90,top\nR3,10k,R,30,10,180,top\nC1,100n,C,10,20,0,top\nC2,100n,C,20,20,0,top\nU1,MCU,Q,40,15,0,top\n";
const FEEDERS: &str =
    "id,value,package,x,y,count\nF1,10k,R,-20,10,10\nF2,100n,C,-20,20,10\nF3,MCU,Q,-20,40,2\n";

fn job() -> Job {
    let cfg = MachineConfig::parse("").unwrap();
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

#[test]
fn every_step_of_the_job_is_a_row_and_the_job_is_the_one_that_is_not_recorded() {
    let mut plain = job();
    assert_eq!(plain.run(None).unwrap(), Phase::Finished);
    let mut seen = job();
    let (outcome, rows, cut) = run_recorded(&mut seen, None, 0);
    assert_eq!(outcome.unwrap(), Phase::Finished);
    assert!(!cut);
    // six parts, a pick and a place for each
    assert_eq!(rows.len(), 12);
    assert_eq!(rows.iter().filter(|r| r.action == "pick").count(), 6);
    assert_eq!(rows.iter().filter(|r| r.action == "place").count(), 6);
    assert!(rows.iter().all(|r| r.result == "ok"));
    // the journal only looks: the state of the two jobs is the same
    assert_eq!(seen.save_state(), plain.save_state());
    assert_eq!(seen.machine().now_us(), plain.machine().now_us());
    // the steps are numbered from 1 and the clock only goes forward
    for (i, r) in rows.iter().enumerate() {
        assert_eq!(r.step, i + 1);
        assert!(r.took_s >= 0.0);
        if i > 0 {
            assert!(r.at_s >= rows[i - 1].at_s);
        }
    }
    // a part is placed after it is picked, and its row says so
    for refx in ["R1", "R2", "R3", "C1", "C2", "U1"] {
        let pick = rows
            .iter()
            .position(|r| r.reference == refx && r.action == "pick")
            .unwrap();
        let place = rows
            .iter()
            .position(|r| r.reference == refx && r.action == "place")
            .unwrap();
        assert!(pick < place, "{refx}");
        assert_eq!(rows[place].status, "placed");
    }
    assert_eq!(rows[0].feeder, "F1");
    assert_eq!(rows[0].nozzle, 1);
}

#[test]
fn the_csv_has_the_header_and_a_row_for_each_step() {
    let mut j = job();
    let (_, rows, _) = run_recorded(&mut j, None, 0);
    let text = csv(&rows);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], HEADER);
    assert_eq!(lines.len(), rows.len() + 1);
    assert!(lines[1].starts_with("1,0.000,"), "{}", lines[1]);
    for l in &lines[1..] {
        assert_eq!(l.split(',').count(), 9, "{l}");
    }
    assert_eq!(
        summary(&rows),
        "12 steps (6 picks, 6 places), 0 that did not end well"
    );
}

#[test]
fn a_reference_with_a_comma_is_quoted() {
    let mut j = job();
    let (_, mut rows, _) = run_recorded(&mut j, Some(1), 0);
    rows[0].reference = "R,1\"x".to_string();
    let text = csv(&rows);
    assert!(text.contains("\"R,1\"\"x\""), "{text}");
}

#[test]
fn the_error_that_stops_the_job_is_the_last_row_and_the_rows_before_it_are_kept() {
    let mut j = job();
    for n in 1..=3 {
        j.machine_mut().force_fail(SimOp::Pick, n);
    }
    let (outcome, rows, _) = run_recorded(&mut j, None, 0);
    let e = outcome.unwrap_err();
    assert_eq!(e.code, Code::PickFailed);
    assert_eq!(j.phase(), Phase::Stopped);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].action, "pick");
    assert_eq!(rows[0].result, Code::PickFailed.label());
    assert_eq!(rows[0].status, "pending");
    assert_eq!(
        summary(&rows),
        "1 steps (1 picks, 0 places), 1 that did not end well"
    );
}

#[test]
fn the_steps_can_be_cut_and_the_job_goes_on_from_there() {
    let mut j = job();
    let (outcome, first, _) = run_recorded(&mut j, Some(5), 0);
    assert_eq!(outcome.unwrap(), Phase::Paused);
    assert_eq!(first.len(), 5);
    j.resume();
    // the numbers go on from the steps that were done
    let (outcome, rest, _) = run_recorded(&mut j, None, first.len());
    assert_eq!(outcome.unwrap(), Phase::Finished);
    assert_eq!(rest[0].step, 6);
    assert_eq!(first.len() + rest.len(), 12);
    // the two halves are the rows of the whole job
    let mut whole = job();
    let (_, all, _) = run_recorded(&mut whole, None, 0);
    let joined: Vec<_> = first.iter().chain(rest.iter()).cloned().collect();
    assert_eq!(joined, all);
}

#[test]
fn a_finished_job_gives_no_rows() {
    let mut j = job();
    j.run(None).unwrap();
    let (outcome, rows, cut) = run_recorded(&mut j, None, 0);
    assert_eq!(outcome.unwrap(), Phase::Finished);
    assert!(rows.is_empty());
    assert!(!cut);
    assert_eq!(csv(&rows), format!("{HEADER}\n"));
}

// ---- simulate --journal

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
fn simulate_writes_the_journal_and_never_over_a_file() {
    let dir = std::env::temp_dir().join(format!("phoenix_pnp_journal_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("journal.csv");
    let path = file.to_str().unwrap();
    let base = [
        "simulate",
        "examples/board.pos",
        "examples/feeders.csv",
        "--journal",
        path,
    ];
    let o = run(&base);
    let out = String::from_utf8_lossy(&o.stdout).into_owned();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(out.contains("placed 6 of 6"), "{out}");
    assert!(
        out.contains("journal: 12 steps (6 picks, 6 places), 0 that did not end well"),
        "{out}"
    );
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.starts_with(HEADER));
    assert_eq!(text.lines().count(), 13);
    // a second run does not go over it, and tells so before the job is run
    let o = run(&base);
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("PHOENIX-PNP-040"));
    assert!(!String::from_utf8_lossy(&o.stdout).contains("placed"));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), text);
    // the file of the parts is not a place for it
    let o = run(&[
        "simulate",
        "examples/board.pos",
        "examples/feeders.csv",
        "--journal",
        "examples/board.pos",
    ]);
    assert!(!o.status.success());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_journal_of_a_cut_job_has_the_steps_that_were_done() {
    let dir = std::env::temp_dir().join(format!("phoenix_pnp_journal_cut_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("j.csv");
    let o = run(&[
        "simulate",
        "examples/board.pos",
        "examples/feeders.csv",
        "--steps",
        "3",
        "--journal",
        file.to_str().unwrap(),
    ]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let text = std::fs::read_to_string(&file).unwrap();
    assert_eq!(text.lines().count(), 4);
    let _ = std::fs::remove_dir_all(&dir);
}
