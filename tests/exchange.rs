//! A job in a file for other programs: the plan and the state of each part, written from a plan or from a job, read back with every check, damaged files, and the commands `job-export`
//! and `job-import`.

use eh3d_phoenix_pnp::catalog::Catalog;
use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::error::Code;
use eh3d_phoenix_pnp::exchange::{from_job, from_plan, parse, phase_name, KIND, MAX_PLACEMENTS};
use eh3d_phoenix_pnp::feeding::{self, assign};
use eh3d_phoenix_pnp::job::{Job, Phase, Status};
use eh3d_phoenix_pnp::parts::{from_csv, Side};
use eh3d_phoenix_pnp::planner::{plan_advanced, AdvancedPlan, Options};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const PARTS: &str = "ref,val,package,x,y,rot,side\nR1,10k,R,10,10,0,top\nR2,10k,R,20,10,90,top\nR3,10k,R,30,10,180,top\nC1,100n,C,10,20,0,top\nC2,100n,C,20,20,0,top\nU1,MCU,Q,40,15,0,top\n";
const FEEDERS: &str =
    "id,value,package,x,y,count\nF1,10k,R,-20,10,10\nF2,100n,C,-20,20,10\nF3,MCU,Q,-20,40,2\n";

fn plan_of(cfg: &MachineConfig) -> (AdvancedPlan, Vec<eh3d_phoenix_pnp::feeding::FeederSpec>) {
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

fn job(cfg_text: &str) -> Job {
    let cfg = MachineConfig::parse(cfg_text).unwrap();
    let (plan, feeders) = plan_of(&cfg);
    Job::new(plan, feeders, cfg).unwrap()
}

fn refused(text: &str) -> String {
    let e = parse(text).unwrap_err();
    assert_eq!(e.code, Code::JobStateBad);
    e.detail
}

fn doc(job: &Job) -> Value {
    serde_json::from_str(&from_job(job)).unwrap()
}

// ---- the plan alone

#[test]
fn a_plan_is_written_with_every_part_pending_and_read_back() {
    let cfg = MachineConfig::default();
    let (plan, _) = plan_of(&cfg);
    let text = from_plan(&plan);
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["schema_version"], 1);
    assert_eq!(v["kind"], KIND);
    assert_eq!(v["state"]["phase"], "ready");
    // the plan is the document of schedule, without its version
    let schedule: Value = serde_json::from_str(&plan.to_json()).unwrap();
    for key in ["order", "batches", "travel_mm", "seconds", "placements"] {
        assert_eq!(v["plan"][key], schedule[key], "{key}");
    }
    assert!(v["plan"].get("schema_version").is_none());
    let e = parse(&text).unwrap();
    assert_eq!(e.phase, Phase::Ready);
    assert_eq!(e.parts.len(), 6);
    assert_eq!(e.counts(), [6, 0, 0, 0]);
    for (part, placed) in e.parts.iter().zip(&plan.plan.placements) {
        assert_eq!(part.reference, placed.reference);
        assert_eq!(part.feeder, placed.feeder);
        assert_eq!(part.place, (placed.x, placed.y));
        assert_eq!(part.rotation, placed.rotation);
        assert_eq!(part.pick, placed.pick);
        assert_eq!((part.nozzle, part.batch), (placed.nozzle, placed.batch));
        assert_eq!(part.status, Status::Pending);
        assert!(part.reason.is_empty());
    }
    assert_eq!(e.batches, plan.plan.batches);
    assert!((e.travel_mm - plan.plan.travel_mm).abs() < 1e-9);
    assert_eq!(e.order, plan.order.name());
    assert!(e.summary().starts_with("6 parts in "));
    assert!(e.summary().contains("6 pending"));
}

#[test]
fn the_same_plan_gives_the_same_file() {
    let (plan, _) = plan_of(&MachineConfig::default());
    assert_eq!(from_plan(&plan), from_plan(&plan));
    // and the file of a job that has not started is the file of the plan
    let j = job("");
    assert_eq!(from_job(&j), from_plan(j.plan()));
}

// ---- a job

#[test]
fn a_job_is_written_with_what_became_of_each_part() {
    let mut j = job("");
    j.run(Some(6)).unwrap();
    let paused = parse(&from_job(&j)).unwrap();
    assert_ne!(paused.phase, Phase::Ready);
    let placed = paused.counts()[1];
    assert!(placed > 0 && placed < 6, "{placed}");
    j.resume();
    j.run(None).unwrap();
    let e = parse(&from_job(&j)).unwrap();
    assert_eq!(e.phase, Phase::Finished);
    assert_eq!(e.counts(), [0, 6, 0, 0]);
    // the statuses are those of the job, part by part
    for (i, part) in e.parts.iter().enumerate() {
        assert_eq!(part.status, j.status()[i]);
    }
    // parts put aside, with the reason
    let mut lost =
        job("error_policy = defer\nsim_pick_fail_rate = 1\npick_retries = 0\nfault_limit = 0\n");
    lost.run(None).unwrap();
    let e = parse(&from_job(&lost)).unwrap();
    assert_eq!(e.counts(), [0, 0, 6, 0]);
    assert!(e.parts.iter().all(|p| !p.reason.is_empty()));
    assert_eq!(e.parts[0].reason, lost.reason(0));
    assert_eq!(phase_name(Phase::Stopped), "stopped");
}

#[test]
fn a_job_stopped_by_an_error_says_so() {
    let mut j =
        job("error_policy = alert\nsim_pick_fail_rate = 1\npick_retries = 0\nfault_limit = 0\n");
    assert!(j.run(None).is_err());
    let e = parse(&from_job(&j)).unwrap();
    assert_eq!(e.phase, Phase::Stopped);
    assert_eq!(e.counts()[0], 6);
}

// ---- the table

#[test]
fn the_placements_are_a_table_with_their_status() {
    let mut j =
        job("error_policy = defer\nsim_pick_fail_rate = 1\npick_retries = 0\nfault_limit = 0\n");
    j.run(None).unwrap();
    let csv = parse(&from_job(&j)).unwrap().csv();
    let mut lines = csv.lines();
    assert_eq!(
        lines.next().unwrap(),
        "reference,feeder,x,y,rotation,nozzle,batch,tip,status,reason"
    );
    assert_eq!(csv.lines().count(), 7);
    let first = lines.next().unwrap();
    assert!(first.contains(",deferred,"), "{first}");
    // a reason with a comma is quoted
    let mut v = doc(&j);
    v["state"]["parts"][0]["reason"] = "a, b".into();
    let e = parse(&v.to_string()).unwrap();
    assert!(e.csv().contains("\"a, b\""));
}

// ---- damaged files

#[test]
fn a_file_that_is_not_understood_is_refused_with_its_reason() {
    let mut j = job("");
    j.run(None).unwrap();
    let good = doc(&j);
    let with = |f: &dyn Fn(&mut Value)| {
        let mut v = good.clone();
        f(&mut v);
        v.to_string()
    };
    assert!(parse(&good.to_string()).is_ok());
    assert!(refused("").contains("not valid JSON"));
    assert!(refused("{").contains("not valid JSON"));
    assert!(refused("[]").contains("schema_version"));
    assert!(refused(&with(&|v| v["schema_version"] = 2.into())).contains("newer"));
    assert!(refused(&with(&|v| v["schema_version"] = "1".into())).contains("schema_version"));
    assert!(refused(&with(&|v| v["kind"] = "plan".into())).contains("not a job exchange"));
    assert!(refused(&with(&|v| {
        v.as_object_mut().unwrap().remove("plan");
    }))
    .contains("no plan"));
    assert!(refused(&with(&|v| {
        v.as_object_mut().unwrap().remove("state");
    }))
    .contains("no state"));
    assert!(
        refused(&with(&|v| v["plan"]["placements"] = 3.into())).contains("no list of placements")
    );
    assert!(refused(&with(&|v| v["state"]["parts"] = Value::Null)).contains("no list of parts"));
    assert!(refused(&with(&|v| {
        v["state"]["parts"].as_array_mut().unwrap().pop();
    }))
    .contains("5 parts and the plan has 6"));
    assert!(refused(&with(&|v| v["state"]["phase"] = "done".into())).contains("phase"));
    assert!(refused(&with(&|v| v["plan"]["batches"] = (-1).into())).contains("batches"));
    // the plan has batches 0, 1, ...: one fewer than it has leaves its last placement outside
    let batches = good["plan"]["batches"].as_u64().unwrap();
    assert!(batches >= 2, "{batches}");
    assert!(
        refused(&with(&|v| v["plan"]["batches"] = (batches - 1).into())).contains("outside the")
    );
    assert!(refused(&with(&|v| v["plan"]["order"] = 7.into())).contains("order"));
    assert!(refused(&with(&|v| v["plan"]["seconds"] = "x".into())).contains("seconds"));
    assert!(refused(&with(&|v| v["plan"]["travel_mm"] = 1e9.into())).contains("travel_mm"));
    // a placement
    let at = |key: &str, value: Value| {
        let key = key.to_string();
        with(&move |v| v["plan"]["placements"][2][key.as_str()] = value.clone())
    };
    assert!(refused(&at("reference", "".into())).contains("reference is empty"));
    let first = good["plan"]["placements"][0]["reference"].clone();
    assert!(refused(&at("reference", first)).contains("twice"));
    assert!(refused(&at("reference", "x\u{7}y".into())).contains("control character"));
    assert!(refused(&at("reference", "a".repeat(129).into())).contains("longer than 128"));
    assert!(refused(&at("feeder", 3.into())).contains("feeder"));
    assert!(refused(&at("nozzle", 64.into())).contains("nozzle"));
    assert!(refused(&at("nozzle", (-1).into())).contains("nozzle"));
    assert!(refused(&at("batch", 99.into())).contains("batch"));
    assert!(refused(&at("rank", 1.5.into())).contains("rank"));
    assert!(refused(&at("bin", 256.into())).contains("bin"));
    assert!(refused(&at("place", json!({"x": 1, "y": "z", "rotation": 0}))).contains("place y"));
    assert!(refused(&at("place", json!({"x": 1, "y": 2}))).contains("rotation"));
    assert!(refused(&at("pick", Value::Null)).contains("pick"));
    assert!(refused(&at("tip", "t".repeat(200).into())).contains("tip"));
    // a part of the state
    let row = |key: &str, value: Value| {
        let key = key.to_string();
        with(&move |v| v["state"]["parts"][1][key.as_str()] = value.clone())
    };
    assert!(refused(&row("reference", "R9".into())).contains("part 2 of the state is 'R9'"));
    assert!(refused(&row("status", "lost".into())).contains("status"));
    assert!(refused(&row("status", Value::Null)).contains("status"));
    assert!(refused(&row("reason", "x".repeat(513).into())).contains("reason"));
    assert!(refused(&row("reason", "a\nb".into())).contains("control character"));
    // the state and the phase must agree
    assert!(refused(&with(
        &|v| v["state"]["parts"][0]["status"] = "pending".into()
    ))
    .contains("still pending"));
    assert!(refused(&with(&|v| v["state"]["phase"] = "ready".into())).contains("already placed"));
}

#[test]
fn a_file_with_too_many_placements_is_refused_before_it_is_read_part_by_part() {
    let rows: Vec<Value> = (0..=MAX_PLACEMENTS).map(|_| json!({})).collect();
    let v = json!({"schema_version": 1, "kind": KIND, "plan": {"placements": rows}, "state": {"parts": []}});
    assert!(refused(&v.to_string()).contains("more than 100000 placements"));
}

#[test]
fn a_cut_file_never_panics_at_any_length() {
    let mut j =
        job("error_policy = defer\nsim_pick_fail_rate = 0.5\nsim_seed = 3\nfault_limit = 0\n");
    j.run(None).unwrap();
    let text = from_job(&j);
    let step = (text.len() / 400).max(1);
    for i in (0..text.trim_end().len()).step_by(step) {
        if text.is_char_boundary(i) {
            assert!(parse(&text[..i]).is_err(), "{i}");
        }
    }
    assert!(parse(&format!("\u{feff}{text}")).is_ok());
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

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("phoenix_pnp_exchange_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn p(path: &Path) -> &str {
    path.to_str().unwrap()
}

#[test]
fn the_commands_write_a_job_and_read_it_back() {
    let dir = scratch("cmd");
    let file = dir.join("job.json");
    let board = ["examples/board.pos", "examples/feeders.csv"];
    let o = run(&["job-export", board[0], board[1], p(&file)]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    let e = parse(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(e.parts.len(), 6);
    assert_eq!(e.phase, Phase::Ready);
    let o = run(&["job-import", p(&file)]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    let out = text(&o.stdout);
    assert!(out.starts_with("6 parts in "), "{out}");
    assert!(out.contains("6 pending"), "{out}");
    // the table, never over a file that exists
    let table = dir.join("parts.csv");
    let o = run(&["job-import", p(&file), "--csv", p(&table)]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert_eq!(std::fs::read_to_string(&table).unwrap().lines().count(), 7);
    assert!(!run(&["job-import", p(&file), "--csv", p(&table)])
        .status
        .success());
    assert!(!run(&["job-import", p(&file), "--csv", p(&file)])
        .status
        .success());
    // the saved state of a job that was run for a while
    let state = dir.join("state.json");
    let o = run(&[
        "simulate",
        board[0],
        board[1],
        "--state",
        p(&state),
        "--steps",
        "6",
    ]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    let half = dir.join("half.json");
    let o = run(&[
        "job-export",
        board[0],
        board[1],
        p(&half),
        "--state",
        p(&state),
    ]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    let e = parse(&std::fs::read_to_string(&half).unwrap()).unwrap();
    assert_eq!(e.phase, Phase::Paused);
    assert!(e.counts()[1] > 0 && e.counts()[0] > 0, "{:?}", e.counts());
    let o = run(&["job-import", p(&half)]);
    assert!(
        text(&o.stdout).contains("the job is paused"),
        "{}",
        text(&o.stdout)
    );
    // mistakes: the output is an input, a state of another board, a file that is not a job
    assert!(!run(&["job-export", board[0], board[1], board[0]])
        .status
        .success());
    assert!(!run(&[
        "job-export",
        board[0],
        board[1],
        p(&state),
        "--state",
        p(&state)
    ])
    .status
    .success());
    assert!(!run(&["job-export", board[0], board[1]]).status.success());
    let other = run(&[
        "job-export",
        "examples/board_dnp.csv",
        board[1],
        p(&dir.join("o.json")),
        "--state",
        p(&state),
    ]);
    assert!(!other.status.success());
    let o = run(&["job-import", board[0]]);
    assert_eq!(o.status.code(), Some(1));
    assert!(
        text(&o.stderr).contains("PHOENIX-PNP-026"),
        "{}",
        text(&o.stderr)
    );
    assert!(!run(&["job-import"]).status.success());
    assert!(!run(&["job-import", p(&file), "--oops"]).status.success());
    assert!(!run(&["job-import", "nothing.json"]).status.success());
    let help = text(&run(&["help"]).stdout);
    assert!(help.contains("job-export") && help.contains("job-import"));
    let _ = std::fs::remove_dir_all(&dir);
}
