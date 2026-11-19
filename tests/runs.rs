//! The same job run many times on the simulated machine: one seed for each run, the runs that placed everything, the ones an error stopped, the parts put aside, the times and `simulate-runs`.

use eh3d_phoenix_pnp::catalog::Catalog;
use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::error::Code;
use eh3d_phoenix_pnp::feeding::{self, assign};
use eh3d_phoenix_pnp::job::Phase;
use eh3d_phoenix_pnp::parts::{from_csv, Side};
use eh3d_phoenix_pnp::planner::{plan_advanced, AdvancedPlan, Options};
use eh3d_phoenix_pnp::runs::{run_many, Run, Runs, MAX_RUNS};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const PARTS: &str = "ref,val,package,x,y,rot,side\nR1,10k,R,10,10,0,top\nR2,10k,R,20,10,90,top\nR3,10k,R,30,10,180,top\nC1,100n,C,10,20,0,top\nC2,100n,C,20,20,0,top\nU1,MCU,Q,40,15,0,top\n";
const FEEDERS: &str =
    "id,value,package,x,y,count\nF1,10k,R,-20,10,10\nF2,100n,C,-20,20,10\nF3,MCU,Q,-20,40,2\n";

fn plan(cfg: &MachineConfig) -> (AdvancedPlan, Vec<feeding::FeederSpec>) {
    let parts = from_csv(PARTS).unwrap();
    let feeders = feeding::from_csv(FEEDERS).unwrap();
    let picks = assign(&parts, &feeders, Some(Side::Top));
    let plan = plan_advanced(
        &parts,
        &feeders,
        &picks,
        &Catalog::new(),
        cfg,
        &Options::default(),
    )
    .unwrap();
    (plan, feeders)
}

fn many(cfg_text: &str, runs: usize, seed: u64) -> Runs {
    let cfg = MachineConfig::parse(cfg_text).unwrap();
    let (p, f) = plan(&cfg);
    run_many(&p, &f, &cfg, runs, seed).unwrap()
}

#[test]
fn a_machine_that_does_not_fail_gives_the_same_run_every_time() {
    let r = many("", 5, 1);
    assert_eq!(r.rows.len(), 5);
    assert_eq!(r.parts, 6);
    assert_eq!((r.complete(), r.stopped()), (5, 0));
    assert!(r
        .rows
        .iter()
        .all(|x| x.phase == Phase::Finished && x.placed == 6 && x.error.is_none()));
    let (lo, mid, hi) = r.seconds().unwrap();
    assert_eq!((lo, mid, hi), (lo, lo, lo));
    assert!(lo > 0.0);
    assert_eq!(r.placed(), (6, 6.0, 6));
    assert_eq!(r.mean_deferred(), 0.0);
    assert!(r.errors().is_empty());
    let text = r.text();
    assert!(
        text.starts_with(
            "5 runs from the seed 1: 5 placed every part, 0 were stopped by an error\n"
        ),
        "{text}"
    );
    assert!(text
        .contains("parts placed in a run: 6 at the least, 6.0 on average, 6 at the most, of 6\n"));
    assert!(text.contains("parts put aside in a run: 0.00 on average\n"));
    assert!(
        text.contains("time of a run that went to the end: "),
        "{text}"
    );
    assert!(!text.contains("error PHOENIX"));
}

#[test]
fn the_seeds_follow_one_another_and_the_same_first_seed_gives_the_same_runs() {
    let cfg = "error_policy = defer\nsim_pick_fail_rate = 0.4\npick_retries = 0\n";
    let a = many(cfg, 12, 100);
    let b = many(cfg, 12, 100);
    assert_eq!(a, b);
    assert_eq!(
        a.rows.iter().map(|r| r.seed).collect::<Vec<_>>(),
        (100..112).collect::<Vec<u64>>()
    );
    // another first seed throws the dice again
    let c = many(cfg, 12, 5000);
    assert_ne!(
        a.rows.iter().map(|r| r.deferred).collect::<Vec<_>>(),
        c.rows.iter().map(|r| r.deferred).collect::<Vec<_>>()
    );
    // with the policy defer a run goes to the end and the parts that failed are put aside
    assert!(a
        .rows
        .iter()
        .all(|r| r.error.is_none() && r.phase == Phase::Finished));
    assert!(a.rows.iter().any(|r| r.deferred > 0));
    assert!(a
        .rows
        .iter()
        .all(|r| r.placed + r.deferred + r.skipped == 6));
    assert!(a.mean_deferred() > 0.0);
    assert!(a.complete() < a.rows.len());
    // a run that put parts aside placed fewer
    let (least, mean, most) = a.placed();
    assert!(least < most && mean < 6.0 && most <= 6);
    // the csv has a row for each run, with the seed
    let csv = a.csv();
    assert!(csv.starts_with("run,seed,phase,placed,deferred,skipped,pending,seconds,error\n"));
    assert_eq!(csv.lines().count(), 13);
    assert!(csv.lines().nth(1).unwrap().starts_with("1,100,finished,"));
}

#[test]
fn a_run_that_an_error_stops_says_which_error_and_the_errors_are_counted() {
    let r = many("sim_pick_fail_rate = 0.5\npick_retries = 0\n", 20, 1);
    assert!(r.stopped() > 0, "{}", r.text());
    assert!(r.complete() < 20);
    let errors = r.errors();
    assert!(!errors.is_empty());
    assert_eq!(errors.iter().map(|e| e.1).sum::<usize>(), r.stopped());
    assert!(errors.iter().all(|e| e.0.starts_with("PHOENIX-PNP-")));
    for w in errors.windows(2) {
        assert!(w[0].1 >= w[1].1);
    }
    let stopped: Vec<&Run> = r.rows.iter().filter(|x| x.error.is_some()).collect();
    assert!(stopped
        .iter()
        .all(|x| x.pending > 0 && x.phase == Phase::Stopped));
    let text = r.text();
    assert!(
        text.contains(&format!("error {}: {} runs\n", errors[0].0, errors[0].1)),
        "{text}"
    );
    assert!(r.csv().contains(&errors[0].0));
    // a run that was stopped is not in the times of the runs that went to the end
    let done = r.rows.iter().filter(|x| x.pending == 0).count();
    assert_eq!(r.seconds().is_some(), done > 0);
    // every run an error stops: no run went to the end
    let all = many("sim_pick_fail_rate = 1\npick_retries = 0\n", 4, 1);
    assert_eq!(all.stopped(), 4);
    assert!(all.seconds().is_none());
    assert!(all.text().contains("no run went to the end\n"));
}

#[test]
fn the_middle_time_is_the_middle_of_the_times_and_an_even_number_takes_the_mean_of_the_two() {
    let row = |seconds: f64| Run {
        seed: 1,
        phase: Phase::Finished,
        placed: 3,
        deferred: 0,
        skipped: 0,
        pending: 0,
        seconds,
        error: None,
    };
    let odd = Runs {
        rows: vec![row(9.0), row(1.0), row(5.0)],
        parts: 3,
    };
    assert_eq!(odd.seconds(), Some((1.0, 5.0, 9.0)));
    let even = Runs {
        rows: vec![row(4.0), row(1.0), row(2.0), row(9.0)],
        parts: 3,
    };
    assert_eq!(even.seconds(), Some((1.0, 3.0, 9.0)));
}

#[test]
fn a_number_of_runs_out_of_range_is_refused() {
    let cfg = MachineConfig::parse("").unwrap();
    let (p, f) = plan(&cfg);
    for n in [0, MAX_RUNS + 1] {
        let e = run_many(&p, &f, &cfg, n, 1).unwrap_err();
        assert_eq!(e.code, Code::PlanSetting, "{n}");
    }
    assert!(run_many(&p, &f, &cfg, 1, u64::MAX).is_ok());
}

// ---- simulate-runs

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
fn the_command_runs_the_job_many_times_and_writes_a_row_for_each_run_once() {
    let dir: PathBuf =
        std::env::temp_dir().join(format!("phoenix_pnp_runs_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let base = [
        "simulate-runs",
        "examples/board.pos",
        "examples/feeders.csv",
    ];
    // no failures: exit 0 and every run placed everything
    let mut args = base.to_vec();
    args.extend(["--runs", "4", "--seed", "9"]);
    let o = run(&args);
    let text = String::from_utf8_lossy(&o.stdout).into_owned();
    assert!(
        o.status.success(),
        "{text}{}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert!(
        text.starts_with(
            "4 runs from the seed 9: 4 placed every part, 0 were stopped by an error\n"
        ),
        "{text}"
    );
    // the default is 20 runs from the seed of the settings
    let o = run(&base);
    assert!(String::from_utf8_lossy(&o.stdout).starts_with("20 runs from the seed "));
    // with failures, exit 1, and the table of the runs goes to a new file
    let cfg = dir.join("m.cfg");
    std::fs::write(&cfg, "sim_pick_fail_rate = 0.5\npick_retries = 0\n").unwrap();
    let out = dir.join("runs.csv");
    let mut args = base.to_vec();
    args.extend(["--runs", "10", "--config", p(&cfg), "--out", p(&out)]);
    let o = run(&args);
    assert_eq!(
        o.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    let csv = std::fs::read_to_string(&out).unwrap();
    assert_eq!(csv.lines().count(), 11);
    assert!(csv.contains("PHOENIX-PNP-"));
    // a second time does not go over the file, and tells so before the runs are made
    let o = run(&args);
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("PHOENIX-PNP-040"));
    assert!(String::from_utf8_lossy(&o.stdout).is_empty());
    // never over a file that was read
    for input in [p(&cfg), "examples/board.pos", "examples/feeders.csv"] {
        let mut args = base.to_vec();
        args.extend(["--out", input]);
        assert!(!run(&args).status.success(), "{input}");
    }
    // the numbers are checked
    for (flag, value) in [
        ("--runs", "0"),
        ("--runs", "10001"),
        ("--runs", "2.5"),
        ("--runs", "many"),
        ("--seed", "-1"),
        ("--seed", "x"),
    ] {
        let mut args = base.to_vec();
        args.extend([flag, value]);
        assert!(!run(&args).status.success(), "{flag} {value}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
