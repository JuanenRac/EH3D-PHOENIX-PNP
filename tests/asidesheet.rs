//! The sheet of the parts that a person places by hand: the parts put aside and left out, the ones a stopped job did not get to, the order of the sheet, the table and `aside-sheet`.

use eh3d_phoenix_pnp::asidesheet::build;
use eh3d_phoenix_pnp::catalog::Catalog;
use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::feeding::{self, assign};
use eh3d_phoenix_pnp::job::{Action, Job, Phase, Status};
use eh3d_phoenix_pnp::parts::{from_csv, Part, Side};
use eh3d_phoenix_pnp::planner::{plan_advanced, Options};
use eh3d_phoenix_pnp::sim::SimOp;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const PARTS: &str = "ref,val,package,x,y,rot,side\nR1,10k,R,10,10,0,top\nR2,10k,R,20,10,90,top\nR3,10k,R,30,10,180,top\nC1,100n,C,10,20,0,top\nC2,100n,C,20,20,0,top\nU1,MCU,Q,40,15,0,top\n";
const FEEDERS: &str =
    "id,value,package,x,y,count\nF1,10k,R,-20,10,10\nF2,100n,C,-20,20,10\nF3,MCU,Q,-20,40,2\n";

fn parts() -> Vec<Part> {
    from_csv(PARTS).unwrap()
}

fn job_with(policy: &str) -> Job {
    let cfg = MachineConfig::parse(policy).unwrap();
    let feeders = feeding::from_csv(FEEDERS).unwrap();
    let parts = parts();
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

/// A job whose first pick fails every time: with `defer` the part is put aside and the job goes on.
fn deferred_job() -> Job {
    let mut j = job_with("error_policy = defer\n");
    for n in 1..=3 {
        j.machine_mut().force_fail(SimOp::Pick, n);
    }
    assert_eq!(j.run(None).unwrap(), Phase::Finished);
    j
}

#[test]
fn a_part_that_was_put_aside_is_on_the_sheet_with_its_reason_and_the_place_of_the_table() {
    let j = deferred_job();
    let aside: Vec<usize> = j
        .status()
        .iter()
        .enumerate()
        .filter(|(_, s)| **s == Status::Deferred)
        .map(|(i, _)| i)
        .collect();
    assert_eq!(aside.len(), 1);
    let sheet = build(&j, &parts(), false);
    assert_eq!(sheet.rows.len(), 1);
    let row = &sheet.rows[0];
    let placement = &j.plan().plan.placements[aside[0]];
    assert_eq!(row.reference, placement.reference);
    assert_eq!(row.status, Status::Deferred);
    assert_eq!(row.feeder, placement.feeder);
    assert!(!row.reason.is_empty());
    // the value, the package and the place are those of the table
    let original = parts()
        .into_iter()
        .find(|p| p.reference == row.reference)
        .unwrap();
    assert_eq!(
        (
            row.value.as_str(),
            row.package.as_str(),
            row.x,
            row.y,
            row.rotation
        ),
        (
            original.value.as_str(),
            original.package.as_str(),
            original.x,
            original.y,
            original.rotation
        )
    );
    assert_eq!(sheet.counts(), [1, 0, 0]);
    let text = sheet.text();
    assert!(text.contains(&format!("{}: ", row.reference)), "{text}");
    assert!(text.contains("- put aside: "), "{text}");
    assert!(
        text.ends_with(
            "1 parts for a person to place: 1 put aside, 0 left out, 0 not placed yet\n"
        ),
        "{text}"
    );
    let csv = sheet.csv();
    assert_eq!(csv.lines().count(), 2);
    assert!(csv.starts_with("reference,value,package,side,x,y,rotation,status,reason,feeder\n"));
    assert!(csv.contains(",top,"), "{csv}");
    assert!(csv.contains(",deferred,"), "{csv}");
}

#[test]
fn a_part_the_person_left_out_is_on_the_sheet_and_a_finished_job_without_any_gives_an_empty_one() {
    // the first pick fails and the policy stops the job; the person leaves the part out and the job goes on
    let mut j = job_with("");
    for n in 1..=3 {
        j.machine_mut().force_fail(SimOp::Pick, n);
    }
    assert!(j.run(None).is_err());
    j.act(Action::Skip).unwrap();
    assert_eq!(j.run(None).unwrap(), Phase::Finished);
    let sheet = build(&j, &parts(), false);
    assert_eq!(sheet.counts(), [0, 1, 0]);
    assert_eq!(sheet.rows[0].status, Status::Skipped);
    assert!(sheet.text().contains("- left out"));
    // a job in which every part was placed leaves nothing for a person
    let mut clean = job_with("");
    assert_eq!(clean.run(None).unwrap(), Phase::Finished);
    let none = build(&clean, &parts(), true);
    assert!(none.rows.is_empty());
    assert_eq!(none.text(), "no part is left for a person to place\n");
    assert_eq!(
        none.csv(),
        "reference,value,package,side,x,y,rotation,status,reason,feeder\n"
    );
}

#[test]
fn the_parts_a_stopped_job_did_not_get_to_are_on_the_sheet_only_when_asked_for() {
    let mut j = job_with("");
    let _ = j.run(Some(4));
    let done = j.status().iter().filter(|s| **s == Status::Placed).count();
    assert!(done < 6);
    assert!(build(&j, &parts(), false).rows.is_empty());
    let all = build(&j, &parts(), true);
    assert_eq!(all.rows.len(), 6 - done);
    assert!(all.rows.iter().all(|r| r.status == Status::Pending));
    assert!(all.text().contains("not placed yet"));
    assert_eq!(all.counts(), [0, 0, 6 - done]);
}

#[test]
fn the_sheet_is_sorted_by_value_and_package_so_a_reel_is_picked_up_once() {
    let mut j = job_with("error_policy = defer\n");
    // every pick of every part fails: all six are put aside
    for n in 1..=200 {
        j.machine_mut().force_fail(SimOp::Pick, n);
    }
    let _ = j.run(None);
    let sheet = build(&j, &parts(), true);
    let order: Vec<&str> = sheet.rows.iter().map(|r| r.reference.as_str()).collect();
    let mut sorted = order.clone();
    sorted.sort_by_key(|r| {
        let p = parts().into_iter().find(|p| p.reference == *r).unwrap();
        (
            p.value.to_ascii_lowercase(),
            p.package.to_ascii_lowercase(),
            p.reference,
        )
    });
    assert_eq!(order, sorted);
    assert!(order.len() >= 2);
}

#[test]
fn a_placement_that_the_table_does_not_have_keeps_the_place_of_the_plan() {
    let j = deferred_job();
    let other: Vec<Part> = parts()
        .into_iter()
        .filter(|p| {
            p.reference != "R1"
                && p.reference != "R2"
                && p.reference != "R3"
                && p.reference != "C1"
                && p.reference != "C2"
                && p.reference != "U1"
        })
        .collect();
    let sheet = build(&j, &other, false);
    assert_eq!(sheet.rows.len(), 1);
    let r = &sheet.rows[0];
    assert!(r.value.is_empty() && r.package.is_empty());
    let placement = j
        .plan()
        .plan
        .placements
        .iter()
        .find(|p| p.reference == r.reference)
        .unwrap();
    assert_eq!((r.x, r.y), (placement.x, placement.y));
}

// ---- aside-sheet

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
fn the_command_says_which_parts_are_left_for_a_person_and_never_writes_over_a_file() {
    let dir = std::env::temp_dir().join(format!("phoenix_pnp_aside_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let cfg: PathBuf = dir.join("m.cfg");
    // half of the picks fail and the policy puts the part aside, from a fixed seed
    std::fs::write(
        &cfg,
        "error_policy = defer\nsim_pick_fail_rate = 0.6\npick_retries = 0\nsim_seed = 7\n",
    )
    .unwrap();
    let (state, out) = (dir.join("s.json"), dir.join("aside.csv"));
    let o = run(&[
        "simulate",
        "examples/board.pos",
        "examples/feeders.csv",
        "--config",
        p(&cfg),
        "--state",
        p(&state),
    ]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let base = [
        "aside-sheet",
        "examples/board.pos",
        "examples/feeders.csv",
        "--config",
        p(&cfg),
        "--state",
        p(&state),
    ];
    let o = run(&base);
    let text = String::from_utf8_lossy(&o.stdout).into_owned();
    assert_eq!(
        o.status.code(),
        Some(2),
        "{text}{}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert!(text.contains("- put aside: "), "{text}");
    assert!(text.contains("parts for a person to place: "), "{text}");
    // the table goes to a new file, once, and never over what was read
    let mut with_out = base.to_vec();
    with_out.extend(["--out", p(&out)]);
    let o = run(&with_out);
    assert_eq!(o.status.code(), Some(2));
    let csv = std::fs::read_to_string(&out).unwrap();
    assert!(csv.starts_with("reference,value,package,side,x,y,rotation,status,reason,feeder\n"));
    assert!(csv.lines().count() >= 2);
    assert!(!run(&with_out).status.success());
    for input in [
        p(&state),
        p(&cfg),
        "examples/board.pos",
        "examples/feeders.csv",
    ] {
        let mut args = base.to_vec();
        args.extend(["--out", input]);
        assert!(!run(&args).status.success(), "{input}");
    }
    // the parts the job did not get to are told with --pending, and a job without any left over exits with 0
    let mut args = base.to_vec();
    args.push("--pending");
    assert_eq!(run(&args).status.code(), Some(2));
    let clean = dir.join("clean.json");
    let o = run(&[
        "simulate",
        "examples/board.pos",
        "examples/feeders.csv",
        "--state",
        p(&clean),
    ]);
    assert!(o.status.success());
    let o = run(&[
        "aside-sheet",
        "examples/board.pos",
        "examples/feeders.csv",
        "--state",
        p(&clean),
    ]);
    assert!(o.status.success());
    assert_eq!(
        String::from_utf8_lossy(&o.stdout),
        "no part is left for a person to place\n"
    );
    // a state is needed and has to be of this job
    assert!(
        !run(&["aside-sheet", "examples/board.pos", "examples/feeders.csv"])
            .status
            .success()
    );
    let o = run(&[
        "aside-sheet",
        "examples/board.pos",
        "examples/feeders_kinds.csv",
        "--state",
        p(&state),
    ]);
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("PHOENIX-PNP-"));
    let _ = std::fs::remove_dir_all(&dir);
}
