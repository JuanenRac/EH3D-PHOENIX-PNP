//! The time the plan counts for the second passes of the approach (`backlash_mode = approach`): the plan goes past the places and comes back as the simulated machine does, so the seconds
//! of the plan are the seconds of the job, and the plan says how many of them are for the second passes.

use eh3d_phoenix_pnp::catalog::Catalog;
use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::export::report;
use eh3d_phoenix_pnp::feeding::{self, assign};
use eh3d_phoenix_pnp::job::{Job, Phase};
use eh3d_phoenix_pnp::model::Machine;
use eh3d_phoenix_pnp::parts::{from_csv, Side};
use eh3d_phoenix_pnp::planner::{plan_advanced, AdvancedPlan, Options};
use serde_json::Value;

const PARTS: &str = "ref,val,package,x,y,rot,side\nR1,10k,R,10,10,0,top\nR2,10k,R,20,10,90,top\nR3,10k,R,30,10,180,top\nC1,100n,C,10,20,0,top\nC2,100n,C,20,20,0,top\nU1,MCU,Q,40,15,0,top\n";
const FEEDERS: &str =
    "id,value,package,x,y,count\nF1,10k,R,-20,10,10\nF2,100n,C,-20,20,10\nF3,MCU,Q,-20,40,2\n";
const SLACK: &str = "backlash_x_mm = 0.2\nbacklash_y_mm = 0.2\n";

fn plan(cfg: &str) -> (AdvancedPlan, MachineConfig) {
    let cfg = MachineConfig::parse(cfg).unwrap();
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
    (plan, cfg)
}

fn simulated_seconds(cfg: &str) -> f64 {
    let (plan, cfg) = plan(cfg);
    let feeders = feeding::from_csv(FEEDERS).unwrap();
    let mut job = Job::new(plan, feeders, cfg).unwrap();
    assert_eq!(job.run(None).unwrap(), Phase::Finished);
    job.machine().now_us() as f64 / 1e6
}

fn near(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

#[test]
fn without_the_approach_the_plan_has_no_second_passes_and_keeps_its_time() {
    for cfg in ["", SLACK, "backlash_mode = approach", "backlash_mode = off"] {
        let (p, _) = plan(cfg);
        assert_eq!(p.approach_s, 0.0, "{cfg:?}");
    }
    // the slack alone changes nothing in the plan: the machine pays for it, the plan did not count it before and still does not
    let (plain, _) = plan("");
    let (slack, _) = plan(SLACK);
    assert_eq!(plain.plan.seconds, slack.plan.seconds);
    assert!(!plain.to_json().contains("approach_s"));
}

#[test]
fn the_plan_counts_the_second_passes_and_says_how_long_they_are() {
    let cfg = format!("{SLACK}backlash_mode = approach");
    let (with, _) = plan(&cfg);
    let (without, _) = plan("");
    assert!(with.approach_s > 0.0, "{}", with.approach_s);
    assert!(with.plan.seconds > without.plan.seconds);
    // the second passes are in the seconds of the plan: what the plan gained is at least what they take less the little the first pass is longer
    assert!(with.plan.seconds - without.plan.seconds >= with.approach_s - 1e-9);
    // the order of the parts is the same: only the time changed
    let order = |p: &AdvancedPlan| {
        p.plan
            .placements
            .iter()
            .map(|x| x.reference.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(order(&with), order(&without));
    // a bigger slack costs more
    let (big, _) = plan("backlash_x_mm = 0.5\nbacklash_y_mm = 0.5\nbacklash_mode = approach");
    assert!(big.approach_s > with.approach_s);
    // the report and the JSON of the plan say it
    let text = report(
        &with,
        &assign(
            &from_csv(PARTS).unwrap(),
            &feeding::from_csv(FEEDERS).unwrap(),
            Some(Side::Top),
        ),
        &[],
    );
    assert!(
        text.contains("for the head to go past the places and come back from the positive side"),
        "{text}"
    );
    let json: Value = serde_json::from_str(&with.to_json()).unwrap();
    assert!(near(
        json["approach_s"].as_f64().unwrap(),
        with.approach_s,
        1e-12
    ));
}

#[test]
fn the_seconds_of_the_plan_are_the_seconds_of_the_job_with_the_approach() {
    for slack in [0.1, 0.2, 0.5] {
        let cfg =
            format!("backlash_x_mm = {slack}\nbacklash_y_mm = {slack}\nbacklash_mode = approach");
        let (p, _) = plan(&cfg);
        let job = simulated_seconds(&cfg);
        assert!(
            near(p.plan.seconds, job, 0.01),
            "slack {slack}: the plan says {} s, the job takes {job} s",
            p.plan.seconds
        );
    }
    // an axis with no slack goes past nothing
    let (x_only, _) = plan("backlash_x_mm = 0.3\nbacklash_mode = approach");
    let (both, _) = plan("backlash_x_mm = 0.3\nbacklash_y_mm = 0.3\nbacklash_mode = approach");
    assert!(x_only.approach_s > 0.0 && both.approach_s > x_only.approach_s);
    let cfg = "backlash_x_mm = 0.3\nbacklash_mode = approach";
    assert!(near(plan(cfg).0.plan.seconds, simulated_seconds(cfg), 0.01));
}

#[test]
fn a_place_too_near_the_limit_has_no_room_to_go_past_and_the_plan_knows() {
    // the head cannot go below the limit, so the places near it are approached without going past; the job and the plan agree on that too
    let cfg =
        "limit_x_min_mm = -20\nbacklash_x_mm = 0.5\nbacklash_y_mm = 0.5\nbacklash_mode = approach";
    let (p, _) = plan(cfg);
    let roomy = "backlash_x_mm = 0.5\nbacklash_y_mm = 0.5\nbacklash_mode = approach";
    let (q, _) = plan(roomy);
    assert!(
        p.approach_s < q.approach_s,
        "{} {}",
        p.approach_s,
        q.approach_s
    );
    assert!(near(p.plan.seconds, simulated_seconds(cfg), 0.01));
}
