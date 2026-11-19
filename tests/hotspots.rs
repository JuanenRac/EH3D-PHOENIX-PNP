//! Which feeders give the trouble when the same job is run many times on the simulated machine: the picks, the failed picks, the runs in which a feeder was switched off, the parts put aside, the
//! order of the feeders, the text, the table and `simulate-hotspots`.

use eh3d_phoenix_pnp::catalog::Catalog;
use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::error::Code;
use eh3d_phoenix_pnp::feeding::{self, assign};
use eh3d_phoenix_pnp::hotspots::{run_many, Hotspots, Spot};
use eh3d_phoenix_pnp::parts::{from_csv, Side};
use eh3d_phoenix_pnp::planner::{plan_advanced, Options};
use eh3d_phoenix_pnp::runs::MAX_RUNS;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const PARTS: &str = "ref,val,package,x,y,rot,side\nR1,10k,R,10,10,0,top\nR2,10k,R,20,10,90,top\nR3,10k,R,30,10,180,top\nC1,100n,C,10,20,0,top\nC2,100n,C,20,20,0,top\nU1,MCU,Q,40,15,0,top\n";
const FEEDERS: &str = "id,value,package,x,y,count\nF1,10k,R,-20,10,10\nF2,100n,C,-20,20,10\nF3,MCU,Q,-20,40,2\nF4,LED,L,-20,50,5\n";
const NO_RETRIES: &str = "error_policy = defer\npick_retries = 0\nplace_retries = 0\n";

fn many(cfg_text: &str, runs: usize, seed: u64) -> Hotspots {
    let cfg = MachineConfig::parse(cfg_text).unwrap();
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
    run_many(&plan, &feeders, &cfg, runs, seed).unwrap()
}

fn spot<'a>(h: &'a Hotspots, id: &str) -> &'a Spot {
    h.spots.iter().find(|s| s.id == id).unwrap()
}

#[test]
fn a_machine_that_does_not_fail_gives_no_trouble_and_every_pick_is_counted() {
    let h = many("", 4, 1);
    assert_eq!((h.runs, h.first_seed), (4, 1));
    assert!(h.troubled().is_empty());
    // 4 runs of 3 resistors, 2 capacitors and 1 chip; a feeder nobody picks from is listed with nothing
    assert_eq!(spot(&h, "F1").picks, 12);
    assert_eq!(spot(&h, "F2").picks, 8);
    assert_eq!(spot(&h, "F3").picks, 4);
    assert_eq!(spot(&h, "F4").picks, 0);
    assert!(h
        .spots
        .iter()
        .all(|s| s.failures == 0 && s.put_aside == 0 && s.switched_off == 0));
    // none of them troubled: the order is by the id
    let ids: Vec<&str> = h.spots.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["F1", "F2", "F3", "F4"]);
    assert_eq!(
        h.text(),
        "no feeder gave trouble in 4 runs from the seed 1\n"
    );
}

#[test]
fn a_feeder_that_always_fails_is_the_worst_and_its_parts_are_put_aside() {
    let h = many(
        &format!("{NO_RETRIES}fault_limit = 0\nsim_feeder_fail = F1 : 1\n"),
        5,
        7,
    );
    let f1 = spot(&h, "F1");
    // 3 resistors in each run, none picked up, all put aside
    assert_eq!(
        (f1.picks, f1.failures, f1.put_aside, f1.switched_off),
        (0, 15, 15, 0)
    );
    assert_eq!(h.spots[0].id, "F1");
    assert_eq!(h.troubled().len(), 1);
    let t = h.text();
    assert_eq!(
        t,
        "F1: 15 of 15 tries to pick failed (100.00 %), switched off in 0 of 5 runs, 15 parts put aside\n1 of 4 feeders gave trouble in 5 runs from the seed 7; F1 gave the most\n"
    );
}

#[test]
fn a_feeder_that_is_switched_off_by_its_faults_is_counted_for_each_run() {
    let h = many(
        &format!("{NO_RETRIES}fault_limit = 2\nfault_window = 3\nsim_feeder_fail = F1 : 1\n"),
        6,
        1,
    );
    let f1 = spot(&h, "F1");
    assert_eq!(f1.switched_off, 6);
    assert!(f1.failures >= 12, "{f1:?}");
    assert_eq!(h.spots[0].id, "F1");
}

#[test]
fn the_worst_feeder_comes_first_whatever_the_order_of_the_table() {
    // C fails more often than R: the capacitors' feeder is above the resistors'
    let h = many(
        &format!(
            "{NO_RETRIES}fault_limit = 0\nsim_part_fail = C : 1 : 0\nsim_part_fail = R : 0.3 : 0\n"
        ),
        30,
        5,
    );
    let order: Vec<&str> = h.spots.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(order[0], "F2", "{h:?}");
    let f2 = spot(&h, "F2");
    assert_eq!(f2.failures, 60);
    assert_eq!(f2.put_aside, 60);
    // the share of the picks that failed
    let f1 = spot(&h, "F1");
    assert_eq!(f1.tries(), f1.picks + f1.failures);
    assert!((f1.failure_percent() - f1.failures as f64 * 100.0 / f1.tries() as f64).abs() < 1e-9);
    assert_eq!(spot(&h, "F4").failure_percent(), 0.0);
    // the ones that were not troubled come last, by the id
    let calm: Vec<&str> = h
        .spots
        .iter()
        .filter(|s| !s.troubled())
        .map(|s| s.id.as_str())
        .collect();
    assert!(order.ends_with(&calm), "{order:?} {calm:?}");
}

#[test]
fn the_same_first_seed_gives_the_same_table_and_another_one_another() {
    let cfg = format!("{NO_RETRIES}fault_limit = 0\nsim_pick_fail_rate = 0.3\n");
    let a = many(&cfg, 40, 11);
    assert_eq!(a, many(&cfg, 40, 11));
    assert_ne!(a, many(&cfg, 40, 12));
    // the runs add up: two halves of the seeds are the whole
    let (x, y) = (many(&cfg, 20, 11), many(&cfg, 20, 31));
    for id in ["F1", "F2", "F3", "F4"] {
        assert_eq!(
            spot(&a, id).failures,
            spot(&x, id).failures + spot(&y, id).failures,
            "{id}"
        );
        assert_eq!(
            spot(&a, id).picks,
            spot(&x, id).picks + spot(&y, id).picks,
            "{id}"
        );
    }
}

#[test]
fn the_table_has_a_row_for_each_feeder() {
    let h = many(
        &format!("{NO_RETRIES}fault_limit = 0\nsim_feeder_fail = F1 : 1\n"),
        2,
        1,
    );
    let t = h.csv();
    let lines: Vec<&str> = t.lines().collect();
    assert_eq!(
        lines[0],
        "feeder,picks,failures,failure_percent,switched_off_runs,put_aside"
    );
    assert_eq!(lines[1], "F1,0,6,100.00,0,6");
    assert_eq!(lines.len(), 5);
    assert!(lines.contains(&"F4,0,0,0.00,0,0"));
}

#[test]
fn the_number_of_runs_is_held_to_its_range() {
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
    for n in [0, MAX_RUNS + 1] {
        assert_eq!(
            run_many(&plan, &feeders, &cfg, n, 1).unwrap_err().code,
            Code::PlanSetting,
            "{n}"
        );
    }
    assert!(run_many(&plan, &feeders, &cfg, 1, u64::MAX).is_ok());
}

// ---- simulate-hotspots

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

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn p(path: &Path) -> &str {
    path.to_str().unwrap()
}

#[test]
fn the_command_exits_with_0_when_no_feeder_gives_trouble_and_with_1_when_one_does() {
    let base = [
        "simulate-hotspots",
        "examples/board.pos",
        "examples/feeders.csv",
    ];
    let mut args = base.to_vec();
    args.extend(["--runs", "3"]);
    let o = run(&args);
    assert!(
        o.status.success(),
        "{}{}",
        out(&o),
        String::from_utf8_lossy(&o.stderr)
    );
    assert_eq!(
        out(&o),
        "no feeder gave trouble in 3 runs from the seed 1\n"
    );
    let dir = std::env::temp_dir().join(format!("phoenix_pnp_hotspots_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("fail.cfg");
    std::fs::write(
        &cfg,
        format!("{NO_RETRIES}fault_limit = 0\nsim_feeder_fail = F2 : 1\nsimulate_runs_count = 4\n"),
    )
    .unwrap();
    let file = dir.join("hot.csv");
    let o = run(&[
        "simulate-hotspots",
        "examples/board.pos",
        "examples/feeders.csv",
        "--config",
        p(&cfg),
        "--out",
        p(&file),
        "--seed",
        "9",
    ]);
    let t = out(&o);
    assert!(!o.status.success(), "{t}");
    assert!(
        t.contains("in 4 runs from the seed 9; F2 gave the most"),
        "{t}"
    );
    assert!(String::from_utf8_lossy(&o.stderr).contains("1 feeders gave trouble in the runs"));
    let csv = std::fs::read_to_string(&file).unwrap();
    assert!(csv.starts_with("feeder,picks,failures,failure_percent,switched_off_runs,put_aside\nF2,0,8,100.00,0,8\n"), "{csv}");
    // not over a file that is there, nor over an input
    assert!(!run(&[
        "simulate-hotspots",
        "examples/board.pos",
        "examples/feeders.csv",
        "--config",
        p(&cfg),
        "--out",
        p(&file)
    ])
    .status
    .success());
    assert!(!run(&[
        "simulate-hotspots",
        "examples/board.pos",
        "examples/feeders.csv",
        "--out",
        "examples/feeders.csv"
    ])
    .status
    .success());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_command_refuses_bad_words() {
    let base = [
        "simulate-hotspots",
        "examples/board.pos",
        "examples/feeders.csv",
    ];
    for (flag, bad) in [
        ("--runs", "0"),
        ("--runs", "10001"),
        ("--runs", "2.5"),
        ("--runs", "x"),
        ("--seed", "-1"),
        ("--seed", "x"),
    ] {
        let mut a = base.to_vec();
        a.extend([flag, bad]);
        assert!(!run(&a).status.success(), "{flag} {bad}");
    }
    assert!(!run(&["simulate-hotspots", "examples/board.pos"])
        .status
        .success());
    assert!(out(&run(&["help"])).contains("phoenix-pnp simulate-hotspots PARTS FEEDERS [--runs N]"));
}
