//! The job: steps, pauses, retries, policies, feeders that fail, saved state, and whole jobs from a board and from a panel.

use eh3d_phoenix_pnp::catalog::Catalog;
use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::error::Code;
use eh3d_phoenix_pnp::feeding::{self, assign};
use eh3d_phoenix_pnp::job::{Cursor, Job, Phase, Status};
use eh3d_phoenix_pnp::model::{FeederDevice, Machine, Nozzle};
use eh3d_phoenix_pnp::panel::Panel;
use eh3d_phoenix_pnp::parts::{from_csv, Part, Side};
use eh3d_phoenix_pnp::planner::{plan_advanced, Options};
use eh3d_phoenix_pnp::sim::SimOp;
use serde_json::{json, Value};

const PARTS: &str = "ref,val,package,x,y,rot,side\nR1,10k,R,10,10,0,top\nR2,10k,R,20,10,90,top\nR3,10k,R,30,10,180,top\nC1,100n,C,10,20,0,top\nC2,100n,C,20,20,0,top\nU1,MCU,Q,40,15,0,top\n";
const FEEDERS: &str =
    "id,value,package,x,y,count\nF1,10k,R,-20,10,10\nF2,100n,C,-20,20,10\nF3,MCU,Q,-20,40,2\n";
const SIX_RESISTORS: &str = "ref,val,package,x,y,rot,side\nR1,10k,R,10,10,0,top\nR2,10k,R,20,10,0,top\nR3,10k,R,30,10,0,top\nR4,10k,R,40,10,0,top\nR5,10k,R,50,10,0,top\nR6,10k,R,60,10,0,top\n";
const FAILING: &str = "sim_pick_fail_rate = 0.3\nsim_feed_fail_rate = 0.2\nsim_place_fail_rate = 0.2\nsim_seed = 11\nerror_policy = defer\npick_retries = 1\nplace_retries = 1\nfeed_retries = 1";

fn job_of(parts: &str, feeders: &str, packages: &str, cfg: &str) -> Job {
    let cfg = MachineConfig::parse(cfg).unwrap();
    let parts = from_csv(parts).unwrap();
    let feeders = feeding::from_csv(feeders).unwrap();
    let picks = assign(&parts, &feeders, Some(Side::Top));
    let mut catalog = Catalog::new();
    if !packages.is_empty() {
        catalog.add_packages(packages).unwrap();
    }
    let plan = plan_advanced(
        &parts,
        &feeders,
        &picks,
        &catalog,
        &cfg,
        &Options::default(),
    )
    .unwrap();
    Job::new(plan, feeders, cfg).unwrap()
}

fn job(cfg: &str) -> Job {
    job_of(PARTS, FEEDERS, "", cfg)
}

fn stats(j: &Job) -> Value {
    serde_json::from_str(&j.stats_json()).unwrap()
}

fn count(j: &Job, status: Status) -> usize {
    j.status().iter().filter(|s| **s == status).count()
}

#[test]
fn a_job_carries_the_whole_plan_out() {
    let mut j = job("");
    assert_eq!(j.phase(), Phase::Ready);
    assert_eq!(j.run(None).unwrap(), Phase::Finished);
    assert_eq!(j.stats().placed, 6);
    assert_eq!(count(&j, Status::Placed), 6);
    assert!(j.machine().holding(0).is_none());
    let s = stats(&j);
    assert_eq!(s["placed"], 6);
    assert_eq!(s["of"], 6);
    assert_eq!(s["deferred"], 0);
    let picked: Vec<u64> = s["feeders"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["picked"].as_u64().unwrap())
        .collect();
    assert_eq!(picked, vec![3, 2, 1]);
    // the machine time is at least the time of the picks and the places
    assert!(j.machine().now_us() as f64 / 1e6 >= 6.0 * 0.8);
    assert!(s["seconds_per_part"].as_f64().unwrap() > 0.8);
}

#[test]
fn the_same_job_gives_the_same_result_and_the_same_events() {
    let (mut a, mut b) = (job(FAILING), job(FAILING));
    a.run(None).unwrap();
    b.run(None).unwrap();
    assert_eq!(a.stats_json(), b.stats_json());
    assert_eq!(a.machine().now_us(), b.machine().now_us());
    assert_eq!(a.machine().events, b.machine().events);
    assert_eq!(a.save_state(), b.save_state());
    let mut other = job(&FAILING.replace("sim_seed = 11", "sim_seed = 12"));
    other.run(None).unwrap();
    assert_ne!(a.machine().events, other.machine().events);
}

#[test]
fn a_job_goes_one_step_at_a_time() {
    let mut j = job("");
    assert_eq!(j.cursor(), Cursor::Pick { batch: 0, k: 0 });
    j.step().unwrap();
    assert_eq!(j.cursor(), Cursor::Place { batch: 0, k: 0 });
    assert!(j.machine().holding(0).is_some());
    j.step().unwrap();
    assert_eq!(j.cursor(), Cursor::Pick { batch: 1, k: 0 });
    assert_eq!(j.machine().holding(0), None);
    let mut steps = 2;
    while j.phase() != Phase::Finished {
        j.step().unwrap();
        steps += 1;
    }
    assert_eq!(steps, 12, "a pick and a place for each of the six parts");
    let e = j.step().unwrap_err();
    assert_eq!(e.code, Code::JobNotRunnable);
    assert_eq!(j.run(None).unwrap(), Phase::Finished);
}

#[test]
fn a_job_can_be_paused_stepped_and_resumed() {
    let mut j = job("");
    assert_eq!(j.run(Some(3)).unwrap(), Phase::Paused);
    assert_eq!(j.cursor(), Cursor::Place { batch: 1, k: 0 });
    // a paused job does not run, but it does go one step
    assert_eq!(j.run(None).unwrap(), Phase::Paused);
    assert_eq!(j.cursor(), Cursor::Place { batch: 1, k: 0 });
    j.step().unwrap();
    assert_eq!(j.cursor(), Cursor::Pick { batch: 2, k: 0 });
    assert_eq!(j.phase(), Phase::Paused);
    j.resume();
    assert_eq!(j.phase(), Phase::Ready);
    assert_eq!(j.run(None).unwrap(), Phase::Finished);
    assert_eq!(j.stats().placed, 6);
    // a pause asked for before the start
    let mut k = job("");
    k.pause();
    assert_eq!(k.run(None).unwrap(), Phase::Paused);
    assert_eq!(k.stats().placed, 0);
}

#[test]
fn a_saved_job_resumed_anywhere_ends_as_the_job_that_never_stopped() {
    let mut whole = job(FAILING);
    whole.run(None).unwrap();
    for stop in 0..=12 {
        let mut first = job(FAILING);
        first.run(Some(stop)).unwrap();
        let state = first.save_state();
        let mut second = job(FAILING);
        second.restore_state(&state).unwrap();
        second.resume();
        assert_eq!(second.run(None).unwrap(), Phase::Finished, "stop {stop}");
        assert_eq!(second.stats_json(), whole.stats_json(), "stop {stop}");
        assert_eq!(
            second.machine().now_us(),
            whole.machine().now_us(),
            "stop {stop}"
        );
        assert_eq!(second.status(), whole.status(), "stop {stop}");
    }
}

#[test]
fn a_state_of_another_job_or_a_damaged_one_is_refused_and_changes_nothing() {
    let mut a = job("");
    a.run(Some(5)).unwrap();
    let state = a.save_state();
    // a job made with two nozzles has other batches
    let mut other = job("nozzles = 2");
    let before = other.save_state();
    assert_eq!(
        other.restore_state(&state).unwrap_err().code,
        Code::JobStateMismatch
    );
    assert_eq!(other.save_state(), before);
    let mut fresh = job("");
    let fresh_state = fresh.save_state();
    for text in [
        "",
        "not json",
        "{}",
        "[]",
        r#"{"schema_version": 1}"#,
        r#"{"schema_version": 2, "hash": "x"}"#,
    ] {
        let e = fresh.restore_state(text).unwrap_err();
        assert_eq!(e.code, Code::JobStateBad, "{text:?}");
    }
    type Damage = Box<dyn Fn(&mut Value)>;
    let damage: Vec<Damage> = vec![
        Box::new(|v: &mut Value| v["status"].as_array_mut().unwrap().push(json!("pending"))),
        Box::new(|v: &mut Value| v["status"][0] = json!("flying")),
        Box::new(|v: &mut Value| v["cursor"]["kind"] = json!("dance")),
        Box::new(|v: &mut Value| v["cursor"]["batch"] = json!(99)),
        Box::new(|v: &mut Value| v["cursor"]["k"] = json!(5)),
        Box::new(|v: &mut Value| {
            v["devices"].as_array_mut().unwrap().pop();
        }),
        Box::new(|v: &mut Value| v["machine"]["rng"] = json!("many")),
        Box::new(|v: &mut Value| v["machine"]["counts"] = json!(["1", "2"])),
        Box::new(|v: &mut Value| v["error"] = json!({"code": 999, "detail": "x"})),
        Box::new(|v: &mut Value| v["stats"]["placed"] = json!("six")),
        Box::new(|v: &mut Value| v["reasons"] = json!([1, 2, 3, 4, 5, 6])),
    ];
    for (i, f) in damage.iter().enumerate() {
        let mut v: Value = serde_json::from_str(&state).unwrap();
        f(&mut v);
        let e = fresh.restore_state(&v.to_string()).unwrap_err();
        assert_eq!(e.code, Code::JobStateBad, "damage {i}: {}", e.detail);
        assert_eq!(
            fresh.save_state(),
            fresh_state,
            "damage {i} changed the job"
        );
    }
    // the good state still goes in
    fresh.restore_state(&state).unwrap();
    assert_eq!(fresh.stats().placed, a.stats().placed);
}

#[test]
fn a_pick_that_fails_is_tried_again() {
    let mut j = job("");
    j.machine_mut().force_fail(SimOp::Pick, 1);
    assert_eq!(j.run(None).unwrap(), Phase::Finished);
    assert_eq!(j.stats().pick_failures, 1);
    assert_eq!(j.stats().pick_attempts, 7);
    assert_eq!(j.stats().placed, 6);
}

#[test]
fn when_the_attempts_are_used_up_alert_stops_the_job_and_resume_tries_again() {
    let mut j = job("");
    for n in 1..=3 {
        j.machine_mut().force_fail(SimOp::Pick, n);
    }
    let e = j.run(None).unwrap_err();
    assert_eq!(e.code, Code::PickFailed);
    assert_eq!(j.phase(), Phase::Stopped);
    assert_eq!(
        j.cursor(),
        Cursor::Pick { batch: 0, k: 0 },
        "the step that failed is the next one"
    );
    assert_eq!(j.error().map(|e| e.code), Some(Code::PickFailed));
    assert_eq!(j.step().unwrap_err().code, Code::JobNotRunnable);
    // the state of a stopped job is kept, and a job made from it is stopped too
    let state = j.save_state();
    let mut copy = job("");
    copy.restore_state(&state).unwrap();
    assert_eq!(copy.phase(), Phase::Stopped);
    assert_eq!(copy.error().map(|e| e.code), Some(Code::PickFailed));
    for each in [&mut j, &mut copy] {
        each.resume();
        assert_eq!(each.run(None).unwrap(), Phase::Finished);
        assert_eq!(each.stats().placed, 6);
    }
    assert_eq!(j.stats().pick_failures, 3);
    assert_eq!(j.stats().pick_attempts, 9);
}

#[test]
fn with_defer_the_part_is_put_aside_and_the_job_goes_on() {
    let mut j = job("error_policy = defer");
    for n in 1..=3 {
        j.machine_mut().force_fail(SimOp::Pick, n);
    }
    assert_eq!(j.run(None).unwrap(), Phase::Finished);
    assert_eq!(j.stats().placed, 5);
    assert_eq!(j.stats().deferred, 1);
    assert_eq!(count(&j, Status::Deferred), 1);
    let aside = j.deferred();
    assert_eq!(aside.len(), 1);
    assert!(aside[0].1.contains("PHOENIX-PNP-022"), "{}", aside[0].1);
    assert_eq!(
        stats(&j)["deferred_parts"][0]["reference"],
        aside[0].0.as_str()
    );
    assert!(j.machine().holding(0).is_none());
}

#[test]
fn a_place_that_fails_keeps_the_part_on_the_nozzle_with_alert_and_drops_it_with_defer() {
    let mut j = job("");
    j.machine_mut().force_fail(SimOp::Place, 1);
    j.machine_mut().force_fail(SimOp::Place, 2);
    let e = j.run(None).unwrap_err();
    assert_eq!(e.code, Code::PlaceFailed);
    assert!(
        j.machine().holding(0).is_some(),
        "the part is still on the nozzle"
    );
    let held = j.machine().holding(0).unwrap().to_string();
    assert!(e.detail.contains(&held));
    j.resume();
    assert_eq!(j.run(None).unwrap(), Phase::Finished);
    assert_eq!(j.stats().placed, 6);
    assert_eq!(j.stats().place_failures, 2);

    let mut d = job("error_policy = defer");
    d.machine_mut().force_fail(SimOp::Place, 1);
    d.machine_mut().force_fail(SimOp::Place, 2);
    assert_eq!(d.run(None).unwrap(), Phase::Finished);
    assert_eq!(d.stats().placed, 5);
    assert_eq!(d.stats().deferred, 1);
    assert!(
        d.machine().holding(0).is_none(),
        "a part put aside is dropped"
    );
    assert!(d.deferred()[0].1.contains("PHOENIX-PNP-024"));
}

#[test]
fn a_feed_that_fails_is_tried_again_and_then_the_policy_decides() {
    // the first feed of every feeder fails
    let mut j = job("");
    for d in j.devices_mut() {
        d.force_fail(1);
    }
    assert_eq!(j.run(None).unwrap(), Phase::Finished);
    assert_eq!(j.stats().feed_failures, 3);
    assert_eq!(j.stats().placed, 6);

    // the first two feeds of every feeder fail: with one retry that is a feed that fails, once for each feeder
    let mut a = job("");
    for d in a.devices_mut() {
        d.force_fail(1);
        d.force_fail(2);
    }
    let mut stops = 0;
    loop {
        match a.run(None) {
            Ok(Phase::Finished) => break,
            Ok(other) => panic!("the job is {other:?}"),
            Err(e) => {
                assert_eq!(e.code, Code::FeedFailed);
                stops += 1;
                a.resume();
            }
        }
    }
    assert_eq!(stops, 3);
    assert_eq!(a.stats().placed, 6);

    let mut d = job("error_policy = defer");
    for dev in d.devices_mut() {
        dev.force_fail(1);
        dev.force_fail(2);
    }
    assert_eq!(d.run(None).unwrap(), Phase::Finished);
    assert_eq!(d.stats().deferred, 3);
    assert_eq!(d.stats().placed, 3);
    assert!(d
        .deferred()
        .iter()
        .all(|(_, why)| why.contains("PHOENIX-PNP-023")));
}

#[test]
fn a_feeder_that_fails_too_often_is_switched_off_and_another_one_takes_over() {
    let feeders =
        "id,value,package,x,y,count,priority\nFA,10k,R,-20,10,10,high\nFB,10k,R,-20,30,10,normal\n";
    let cfg = "pick_retries = 0\nerror_policy = defer\nfault_window = 3\nfault_limit = 2";
    let mut j = job_of(SIX_RESISTORS, feeders, "", cfg);
    j.machine_mut().force_fail(SimOp::Pick, 1);
    j.machine_mut().force_fail(SimOp::Pick, 2);
    assert_eq!(j.run(None).unwrap(), Phase::Finished);
    let f = &j.stats().feeders;
    assert_eq!(
        (f[0].id.as_str(), f[0].disabled, f[0].faults, f[0].picked),
        ("FA", true, 2, 0)
    );
    assert_eq!(
        (f[1].id.as_str(), f[1].disabled, f[1].picked),
        ("FB", false, 4)
    );
    assert_eq!(j.stats().placed, 4);
    assert_eq!(j.stats().deferred, 2);
    assert!(j
        .machine()
        .events
        .iter()
        .any(|e| e.text.contains("feeder FA switched off")));
}

#[test]
fn a_feeder_is_never_switched_off_when_the_limit_is_0() {
    let feeders = "id,value,package,x,y,count\nFA,10k,R,-20,10,10\n";
    let cfg = "pick_retries = 0\nerror_policy = defer\nfault_limit = 0";
    let mut j = job_of(SIX_RESISTORS, feeders, "", cfg);
    for n in 1..=5 {
        j.machine_mut().force_fail(SimOp::Pick, n);
    }
    j.run(None).unwrap();
    assert_eq!(j.stats().placed, 1);
    assert!(!j.stats().feeders[0].disabled);
}

#[test]
fn a_nozzle_that_holds_a_part_does_not_pick_another_and_one_that_holds_the_wrong_part_does_not_place(
) {
    let mut j = job("");
    j.machine_mut().pick(0, "STRAY").unwrap();
    let e = j.run(None).unwrap_err();
    assert_eq!(e.code, Code::NozzleBusy);
    assert!(e.detail.contains("STRAY"));
    j.machine_mut().discard(0);
    j.resume();
    j.step().unwrap();
    // now the nozzle holds the part of the plan; somebody swaps it
    j.machine_mut().discard(0);
    j.machine_mut().pick(0, "OTHER").unwrap();
    let e = j.step().unwrap_err();
    assert_eq!(e.code, Code::NozzleBusy);
    j.machine_mut().discard(0);
    j.resume();
    assert_eq!(j.step().unwrap_err().code, Code::NozzleEmpty);
}

#[test]
fn a_part_without_any_working_feeder_follows_the_policy() {
    let mut a = job("");
    a.devices_mut()[0].set_enabled(false);
    assert_eq!(a.run(None).unwrap_err().code, Code::FeederUnavailable);
    let mut d = job("error_policy = defer");
    d.devices_mut()[0].set_enabled(false);
    assert_eq!(d.run(None).unwrap(), Phase::Finished);
    assert_eq!(d.stats().placed, 3);
    assert_eq!(d.stats().deferred, 3);
}

#[test]
fn the_tip_is_changed_when_the_plan_says_and_the_changes_are_counted() {
    let parts = "ref,val,package,x,y,rot,side\nA1,a,PA,1,0,0,top\nB1,b,PB,2,0,0,top\nA2,a,PA,3,0,0,top\nB2,b,PB,4,0,0,top\n";
    let feeders = "id,value,package,x,y,count\nFA,a,PA,-10,0,5\nFB,b,PB,-10,10,5\n";
    let tables = "package,nozzle_tip\nPA,N1\nPB,N2\n";
    let cfg = "tip_strategy = as_planned\ntip_changer_x_mm = -50\ntip_changer_y_mm = 0";
    let mut j = job_of(
        parts,
        feeders,
        tables,
        &format!(
            "{cfg}
tip_change_s = 3"
        ),
    );
    assert_eq!(j.plan().tip_changes, 3);
    j.run(None).unwrap();
    assert_eq!(j.stats().tip_changes, 3);
    assert_eq!(j.machine().tip(), "N2");
    // the three changes took 9 seconds more than the same job without them
    let mut free = job_of(parts, feeders, tables, &format!("{cfg}\ntip_change_s = 0"));
    free.run(None).unwrap();
    let extra = (j.machine().now_us() - free.machine().now_us()) as f64 / 1e6;
    assert!((extra - 9.0).abs() < 1e-6, "{extra}");
}

const PANEL: &str = r#"{"boards": {"main": "board.pos"}, "panels": {"p": {"children": [
    {"id": "A", "board": "main", "x": 0, "y": 0}, {"id": "B", "board": "main", "x": 60, "y": 0}]}}, "root": "p"}"#;
const PANEL_REVERSED: &str = r#"{"boards": {"main": "board.pos"}, "panels": {"p": {"children": [
    {"id": "B", "board": "main", "x": 60, "y": 0}, {"id": "A", "board": "main", "x": 0, "y": 0}]}}, "root": "p"}"#;

fn panel_parts(text: &str) -> Vec<Part> {
    let load = |_: &str| from_csv(PARTS);
    Panel::parse(text).unwrap().expand(&load).unwrap().parts
}

#[test]
fn a_whole_panel_is_planned_and_carried_out_and_the_order_of_its_children_does_not_matter() {
    let feeders = feeding::from_csv(
        "id,value,package,x,y,count\nF1,10k,R,-20,10,10\nF2,100n,C,-20,20,10\nF3,MCU,Q,-20,40,4\n",
    )
    .unwrap();
    let cfg = MachineConfig::parse("nozzles = 2").unwrap();
    let run = |text: &str| {
        let parts = panel_parts(text);
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
        let json = plan.to_json();
        let mut j = Job::new(plan, feeders.clone(), cfg.clone()).unwrap();
        assert_eq!(j.run(None).unwrap(), Phase::Finished);
        (json, j.stats().placed, j.stats_json())
    };
    let (plan_a, placed, stats_a) = run(PANEL);
    assert_eq!(placed, 12);
    assert!(plan_a.contains("\"A/R1\"") && plan_a.contains("\"B/U1\""));
    let (plan_b, _, stats_b) = run(PANEL_REVERSED);
    assert_eq!(plan_a, plan_b, "the plan of the panel is stable");
    assert_eq!(stats_a, stats_b);
}

#[test]
fn a_board_with_nothing_to_place_is_a_finished_job() {
    let mut j = job_of("ref,val,package,x,y,rot,side\n", FEEDERS, "", "");
    assert_eq!(j.phase(), Phase::Finished);
    assert_eq!(j.run(None).unwrap(), Phase::Finished);
    assert_eq!(j.step().unwrap_err().code, Code::JobNotRunnable);
    assert_eq!(stats(&j)["placed"], 0);
    assert_eq!(stats(&j)["seconds_per_part"], 0.0);
}

#[test]
fn a_job_is_not_made_from_a_plan_that_does_not_fit_the_machine_or_the_feeders() {
    let cfg = MachineConfig::parse("nozzles = 2").unwrap();
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
    // one nozzle for a plan of two
    let one = MachineConfig::default();
    assert_eq!(
        Job::new(plan.clone(), feeders.clone(), one)
            .unwrap_err()
            .code,
        Code::PlanSetting
    );
    // the feeders of the plan are not in the list
    let few = feeders[1..].to_vec();
    assert_eq!(
        Job::new(plan, few, cfg).unwrap_err().code,
        Code::PlanSetting
    );
}

#[test]
fn the_machine_time_never_goes_back_in_a_job_that_fails() {
    let mut j = job(FAILING);
    j.run(None).unwrap();
    let t: Vec<u64> = j.machine().events.iter().map(|e| e.t_us).collect();
    assert!(t.windows(2).all(|w| w[0] <= w[1]));
    assert!(*t.last().unwrap() <= j.machine().now_us());
    assert!(!j.hash().is_empty());
}
