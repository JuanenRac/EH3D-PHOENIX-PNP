//! The simulated vacuum and the actuators, and the decisions of the person when a feeder reaches its limit of faults: the settings and their ranges, the checks after a pick and after a
//! place, the times, the codes, the saved state and the report.

use eh3d_phoenix_pnp::catalog::Catalog;
use eh3d_phoenix_pnp::config::{FaultAction, MachineConfig};
use eh3d_phoenix_pnp::error::{Code, Lang};
use eh3d_phoenix_pnp::feeding::{self, assign};
use eh3d_phoenix_pnp::job::{Action, Job, Phase, Status};
use eh3d_phoenix_pnp::model::{Actuator, Machine, Nozzle};
use eh3d_phoenix_pnp::parts::{from_csv, Side};
use eh3d_phoenix_pnp::planner::{plan_advanced, Options};
use eh3d_phoenix_pnp::sim::SimOp;
use serde_json::Value;

const EXAMPLE: &str = include_str!("../examples/machine.cfg");
const PARTS: &str = "ref,val,package,x,y,rot,side\nR1,10k,R,10,10,0,top\nR2,10k,R,20,10,90,top\nR3,10k,R,30,10,180,top\nC1,100n,C,10,20,0,top\nC2,100n,C,20,20,0,top\nU1,MCU,Q,40,15,0,top\n";
const FEEDERS: &str =
    "id,value,package,x,y,count\nF1,10k,R,-20,10,10\nF2,100n,C,-20,20,10\nF3,MCU,Q,-20,40,2\n";
const SIX_RESISTORS: &str = "ref,val,package,x,y,rot,side\nR1,10k,R,10,10,0,top\nR2,10k,R,20,10,0,top\nR3,10k,R,30,10,0,top\nR4,10k,R,40,10,0,top\nR5,10k,R,50,10,0,top\nR6,10k,R,60,10,0,top\n";
const TWO_FEEDERS: &str =
    "id,value,package,x,y,count,priority\nFA,10k,R,-20,10,10,high\nFB,10k,R,-20,30,10,normal\n";
const ALERT_WINDOW: &str =
    "pick_retries = 0\nerror_policy = defer\nfault_window = 3\nfault_limit = 2\nfault_action = alert\n";

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

fn job_of(parts: &str, feeders: &str, cfg: &str) -> Job {
    let cfg = MachineConfig::parse(cfg).unwrap();
    let parts = from_csv(parts).unwrap();
    let feeders = feeding::from_csv(feeders).unwrap();
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

fn job(cfg: &str) -> Job {
    job_of(PARTS, FEEDERS, cfg)
}

fn stats(j: &Job) -> Value {
    serde_json::from_str(&j.stats_json()).unwrap()
}

fn state(j: &Job) -> Value {
    serde_json::from_str(&j.save_state()).unwrap()
}

fn code_of(text: &str) -> Code {
    MachineConfig::parse(text).unwrap_err().code
}

fn told(j: &Job, what: &str) -> usize {
    j.machine()
        .events
        .iter()
        .filter(|e| e.text.contains(what))
        .count()
}

// ---- the settings

#[test]
fn the_settings_have_their_defaults_and_the_example_is_the_default() {
    let d = MachineConfig::parse("").unwrap();
    assert!(!d.vacuum);
    assert!(near(d.vacuum_suck_s, 0.08) && near(d.vacuum_blow_s, 0.05));
    assert!(near(d.vacuum_free_kpa, 8.0) && near(d.vacuum_held_kpa, 62.0));
    assert!(near(d.vacuum_pick_min_kpa, 40.0) && near(d.vacuum_release_max_kpa, 20.0));
    assert!(near(d.sim_vacuum_noise_kpa, 0.0));
    assert!(near(d.sim_vacuum_miss_rate, 0.0) && near(d.sim_vacuum_stuck_rate, 0.0));
    assert!(d.actuators.is_empty());
    assert_eq!(d.fault_action, FaultAction::Disable);
    // the example holds the default of every one of these keys (it only adds the actuator it shows)
    let e = MachineConfig::parse(EXAMPLE).unwrap();
    assert_eq!(e.vacuum, d.vacuum);
    assert_eq!(e.fault_action, d.fault_action);
    for (a, b) in [
        (e.vacuum_suck_s, d.vacuum_suck_s),
        (e.vacuum_blow_s, d.vacuum_blow_s),
        (e.vacuum_free_kpa, d.vacuum_free_kpa),
        (e.vacuum_held_kpa, d.vacuum_held_kpa),
        (e.vacuum_pick_min_kpa, d.vacuum_pick_min_kpa),
        (e.vacuum_release_max_kpa, d.vacuum_release_max_kpa),
        (e.sim_vacuum_noise_kpa, d.sim_vacuum_noise_kpa),
        (e.sim_vacuum_miss_rate, d.sim_vacuum_miss_rate),
        (e.sim_vacuum_stuck_rate, d.sim_vacuum_stuck_rate),
    ] {
        assert!(near(a, b), "{a} {b}");
    }
    assert_eq!(e.actuators.len(), 1);
    assert_eq!(e.actuators[0].name, "light");
    assert!(near(e.actuators[0].delay_s, 0.02));
}

#[test]
fn the_settings_have_strict_ranges_and_the_levels_are_checked_together() {
    for yes in ["on", "yes", "true", "1"] {
        assert!(
            MachineConfig::parse(&format!("vacuum = {yes}"))
                .unwrap()
                .vacuum
        );
    }
    assert_eq!(code_of("vacuum = maybe"), Code::ConfigBadValue);
    assert_eq!(
        code_of("vacuum = on\nvacuum = off"),
        Code::ConfigDuplicateKey
    );
    for (key, good, bad) in [
        ("vacuum_suck_s", ["0", "10"], ["-0.1", "10.5"]),
        ("vacuum_blow_s", ["0", "10"], ["-0.1", "10.5"]),
        ("sim_vacuum_noise_kpa", ["0", "50"], ["-1", "50.5"]),
        ("sim_vacuum_miss_rate", ["0", "1"], ["-0.1", "1.5"]),
        ("sim_vacuum_stuck_rate", ["0", "1"], ["-0.1", "1.5"]),
        ("vacuum_free_kpa", ["0", "8"], ["-1", "102"]),
        ("vacuum_held_kpa", ["41", "101"], ["-1", "102"]),
    ] {
        for v in good {
            assert!(
                MachineConfig::parse(&format!("{key} = {v}")).is_ok(),
                "{key} {v}"
            );
        }
        for v in bad {
            assert_eq!(
                code_of(&format!("{key} = {v}")),
                Code::ConfigOutOfRange,
                "{key} {v}"
            );
        }
        assert_eq!(code_of(&format!("{key} = many")), Code::ConfigBadValue);
    }
    // free <= release_max < pick_min <= held
    assert!(MachineConfig::parse("vacuum_pick_min_kpa = 62").is_ok());
    assert_eq!(code_of("vacuum_pick_min_kpa = 63"), Code::UnsafeSetting);
    assert_eq!(code_of("vacuum_release_max_kpa = 40"), Code::UnsafeSetting);
    assert_eq!(code_of("vacuum_free_kpa = 21"), Code::UnsafeSetting);
    assert!(MachineConfig::parse("vacuum_free_kpa = 20").is_ok());
    // the action of the window of faults
    assert_eq!(
        MachineConfig::parse("fault_action = Alert")
            .unwrap()
            .fault_action,
        FaultAction::Alert
    );
    assert_eq!(code_of("fault_action = stop"), Code::ConfigBadValue);
}

#[test]
fn the_actuators_are_listed_one_to_a_line_with_a_name_and_a_delay() {
    let c = MachineConfig::parse("actuator = valve : 0\nactuator = light-1 : 0.5").unwrap();
    assert_eq!(c.actuators.len(), 2);
    assert_eq!(c.actuators[1].name, "light-1");
    assert!(near(c.actuators[1].delay_s, 0.5));
    for bad in [
        "actuator = valve",
        "actuator = : 1",
        "actuator = a b : 1",
        "actuator = valve : 1 : 2",
        "actuator = valve : fast",
        "actuator = valve : 1\nactuator = valve : 2",
    ] {
        assert_eq!(code_of(bad), Code::ConfigBadValue, "{bad}");
    }
    assert_eq!(code_of("actuator = valve : 11"), Code::ConfigOutOfRange);
    let many: String = (0..33).map(|n| format!("actuator = a{n} : 0\n")).collect();
    assert_eq!(code_of(&many), Code::ConfigBadValue);
    let enough: String = (0..32).map(|n| format!("actuator = a{n} : 0\n")).collect();
    assert_eq!(MachineConfig::parse(&enough).unwrap().actuators.len(), 32);
}

// ---- the vacuum in the job

#[test]
fn with_the_vacuum_off_nothing_changes_whatever_the_other_settings_say() {
    let mut plain = job("");
    let mut other = job(
        "sim_vacuum_miss_rate = 1\nsim_vacuum_stuck_rate = 1\nvacuum_suck_s = 5\nvacuum_blow_s = 5\nvacuum_pick_min_kpa = 50",
    );
    plain.run(None).unwrap();
    other.run(None).unwrap();
    assert_eq!(plain.save_state(), other.save_state());
    assert_eq!(plain.stats_json(), other.stats_json());
    assert_eq!(plain.stats_csv(), other.stats_csv());
    let s = stats(&plain);
    assert!(s.get("vacuum").is_none() && s.get("actuators").is_none());
    assert!(state(&plain).get("vacuum").is_none());
    assert!(plain.vacuum_level(0).is_none());
    assert_eq!(told(&plain, "vacuum"), 0);
}

#[test]
fn a_clean_job_adds_the_times_of_sucking_and_blowing_to_the_clock() {
    let mut off = job("");
    let mut on = job("vacuum = on");
    off.run(None).unwrap();
    on.run(None).unwrap();
    assert_eq!(on.stats().placed, 6);
    // six parts, each with 0.08 s of sucking and 0.05 s of blowing: 6 x 130000 microseconds
    assert_eq!(on.machine().now_us() - off.machine().now_us(), 780_000);
    let v = &on.stats().vacuum;
    assert_eq!(
        (v.pick_checks, v.pick_misses, v.place_checks, v.place_stuck),
        (6, 0, 6, 0)
    );
    // the levels of a nozzle that holds a part and of one that holds nothing, with no error in the reading
    assert!(near(v.lowest_held_kpa.unwrap(), 62.0));
    assert!(near(v.highest_released_kpa.unwrap(), 8.0));
    assert_eq!(told(&on, "kPa, "), 12);
    assert_eq!(told(&on, "is held"), 6);
    assert_eq!(told(&on, "let go"), 6);
    assert!(told(&on, "vacuum of nozzle 1: 62.0 kPa, ") >= 6);
    // the pump is off and the level is back to the air after the last place
    assert!(near(on.vacuum_level(0).unwrap(), 0.0));
    assert!(!on.vacuum_mut().is_on(0));
    let s = stats(&on);
    assert_eq!(s["vacuum"]["pick_checks"], 6);
    assert_eq!(s["vacuum"]["pick_min_kpa"], 40.0);
    assert_eq!(s["vacuum"]["lowest_held_kpa"], 62.0);
}

#[test]
fn the_reading_has_an_error_that_comes_from_the_seed() {
    let cfg = "vacuum = on\nsim_vacuum_noise_kpa = 5\n";
    let mut j = job(cfg);
    // the first step is the pick of the first part: 62 + (2 x 0.2980982003797039 - 1) x 5, from the second number drawn
    j.step().unwrap();
    assert!(near(j.vacuum_level(0).unwrap(), 59.980_982_003_797_04));
    assert!(j.vacuum_mut().is_on(0));
    // the second step is its place: 8 + (2 x 0.14908116817015904 - 1) x 5, from the fourth number drawn
    j.step().unwrap();
    assert!(near(
        j.stats().vacuum.highest_released_kpa.unwrap(),
        4.490_811_681_701_59
    ));
    j.run(None).unwrap();
    let v = &j.stats().vacuum;
    assert!(v.lowest_held_kpa.unwrap() >= 57.0 && v.lowest_held_kpa.unwrap() <= 62.0 + 1e-9);
    assert!(v.highest_released_kpa.unwrap() <= 13.0 + 1e-9);
    // the same seed, the same readings
    let mut again = job(cfg);
    again.run(None).unwrap();
    assert_eq!(again.stats_json(), j.stats_json());
    // another seed, other readings
    let mut other = job(&format!("{cfg}sim_seed = 7\n"));
    other.step().unwrap();
    assert!(near(other.vacuum_level(0).unwrap(), 64.801_341_285_005_14));
}

#[test]
fn a_pick_that_leaves_no_part_on_the_nozzle_is_tried_again() {
    let mut j = job("vacuum = on");
    j.vacuum_mut().force_miss(1);
    assert_eq!(j.run(None).unwrap(), Phase::Finished);
    let v = &j.stats().vacuum;
    assert_eq!((v.pick_checks, v.pick_misses), (7, 1));
    assert_eq!(j.stats().placed, 6);
    assert_eq!((j.stats().pick_attempts, j.stats().pick_failures), (7, 1));
    assert_eq!(j.trace().codes.get(&44), Some(&1));
    assert_eq!(told(&j, "8.0 kPa, no part on the nozzle"), 1);
    // the part never left the feeder: nothing was taken for the attempt that missed
    assert_eq!(j.stats().feeders[0].picked, 3);
    assert_eq!(j.trace().retried, 1);
}

#[test]
fn a_miss_stops_the_job_with_its_code_and_the_person_decides() {
    let mut j = job("vacuum = on\npick_retries = 0");
    j.vacuum_mut().force_miss(1);
    let e = j.run(None).unwrap_err();
    assert_eq!(e.code, Code::VacuumNoPart);
    assert_eq!(e.code.label(), "PHOENIX-PNP-044");
    assert!(e.detail.contains("8.0 kPa") && e.detail.contains("at least 40 kPa"));
    assert_eq!(j.phase(), Phase::Stopped);
    let f = j.failure().unwrap();
    assert!(f.at_pick);
    assert!(j.machine().holding(f.nozzle).is_none());
    j.act(Action::Skip).unwrap();
    assert_eq!(j.run(None).unwrap(), Phase::Finished);
    assert_eq!((j.stats().placed, j.stats().skipped), (5, 1));
    assert_eq!(
        j.status().iter().filter(|s| **s == Status::Skipped).count(),
        1
    );
}

#[test]
fn a_part_that_stays_on_the_nozzle_after_the_place_is_placed_again() {
    let mut j = job("vacuum = on");
    j.vacuum_mut().force_stuck(1);
    assert_eq!(j.run(None).unwrap(), Phase::Finished);
    let v = &j.stats().vacuum;
    assert_eq!((v.place_checks, v.place_stuck), (7, 1));
    assert_eq!((j.stats().placed, j.stats().place_failures), (6, 1));
    assert_eq!(j.trace().codes.get(&45), Some(&1));
    assert_eq!(told(&j, "62.0 kPa, "), 7);
    assert_eq!(told(&j, "is still on the nozzle"), 1);
    assert_eq!(told(&j, "still holds"), 1);
}

#[test]
fn a_stuck_part_stops_the_job_holding_it_and_the_place_is_tried_again_by_the_person() {
    let mut j = job("vacuum = on\nplace_retries = 0");
    j.vacuum_mut().force_stuck(1);
    let e = j.run(None).unwrap_err();
    assert_eq!(e.code, Code::VacuumStuck);
    assert_eq!(e.code.label(), "PHOENIX-PNP-045");
    assert!(e.detail.contains("62.0 kPa") && e.detail.contains("at most 20 kPa"));
    let f = j.failure().unwrap();
    assert!(!f.at_pick);
    // the part is on the nozzle again, so the step can be tried again after the cause is put right
    assert_eq!(j.machine().holding(f.nozzle), Some(f.reference.as_str()));
    j.act(Action::Retry).unwrap();
    assert_eq!(j.run(None).unwrap(), Phase::Finished);
    assert_eq!(j.stats().placed, 6);
    assert_eq!(j.stats().vacuum.place_checks, 7);
}

#[test]
fn with_the_policy_defer_the_parts_that_never_pick_or_let_go_are_put_aside_with_their_code() {
    let base = "vacuum = on\nerror_policy = defer\nfault_limit = 0\n";
    let mut miss = job(&format!("{base}pick_retries = 0\nsim_vacuum_miss_rate = 1"));
    assert_eq!(miss.run(None).unwrap(), Phase::Finished);
    assert_eq!((miss.stats().placed, miss.stats().deferred), (0, 6));
    assert_eq!(miss.stats().pick_failures, 6);
    assert!(miss
        .deferred()
        .iter()
        .all(|(_, why)| why.contains("PHOENIX-PNP-044")));
    let mut stuck = job(&format!("{base}sim_vacuum_stuck_rate = 1"));
    assert_eq!(stuck.run(None).unwrap(), Phase::Finished);
    assert_eq!((stuck.stats().placed, stuck.stats().deferred), (0, 6));
    // two attempts at each place, all of them stuck
    assert_eq!(stuck.stats().place_failures, 12);
    assert!(stuck
        .deferred()
        .iter()
        .all(|(_, why)| why.contains("PHOENIX-PNP-045")));
    assert!(stuck.machine().holding(0).is_none());
}

#[test]
fn a_job_with_the_vacuum_and_the_camera_does_both() {
    let cfg = "vacuum = on\nalign = on\nsim_align_dx_mm = 0.1\nactuator = light : 0.02\n";
    let mut j = job(cfg);
    j.vacuum_mut().force_miss(2);
    assert_eq!(j.run(None).unwrap(), Phase::Finished);
    assert_eq!(j.stats().placed, 6);
    assert_eq!(j.stats().vacuum.pick_misses, 1);
    assert_eq!(j.stats().align.corrected, 6);
    // the vacuum rows come before the align rows, which stay the last section
    let csv = j.stats_csv();
    let (v, a) = (
        csv.find("\nvacuum,").unwrap(),
        csv.find("\nalign,").unwrap(),
    );
    assert!(v < a);
    assert!(csv.contains("\nvacuum,pick_checks,7,,,,,,\n"));
    assert!(csv.contains("\nvacuum,pick_misses,1,,,,,,\n"));
    assert!(csv.contains("\nvacuum,lowest_held_kpa,62.0,,,,,,\n"));
    assert!(csv.contains("\nactuator,light,off,,,,,0,\n"));
}

// ---- the state saved

#[test]
fn a_job_saved_and_resumed_ends_as_the_one_that_never_stopped() {
    let cfg = "vacuum = on\nsim_vacuum_noise_kpa = 3\nsim_vacuum_miss_rate = 0.3\nsim_vacuum_stuck_rate = 0.2\nsim_seed = 5\nerror_policy = defer\nfault_limit = 0\npick_retries = 1\nplace_retries = 1\nactuator = light : 0.02\n";
    let mut a = job(cfg);
    a.run(Some(7)).unwrap();
    a.set_actuator("light", true).unwrap();
    let saved = a.save_state();
    let mut b = job(cfg);
    b.restore_state(&saved).unwrap();
    assert_eq!(b.save_state(), saved);
    assert!(b.actuators()[0].is_on());
    a.resume();
    b.resume();
    assert_eq!(a.run(None).unwrap(), Phase::Finished);
    assert_eq!(b.run(None).unwrap(), Phase::Finished);
    assert_eq!(a.save_state(), b.save_state());
    assert_eq!(a.stats_json(), b.stats_json());
    assert_eq!(a.stats_csv(), b.stats_csv());
    // it did check something
    assert!(a.stats().vacuum.pick_checks >= 6);
}

#[test]
fn a_state_of_a_job_with_the_vacuum_is_for_that_job_only() {
    let mut on = job("vacuum = on");
    on.run(Some(3)).unwrap();
    let saved = on.save_state();
    let mut off = job("");
    assert_eq!(
        off.restore_state(&saved).unwrap_err().code,
        Code::JobStateMismatch
    );
    // a state saved with the vacuum but without its section starts the checks again from zero
    let mut plain: Value = serde_json::from_str(&saved).unwrap();
    plain.as_object_mut().unwrap().remove("vacuum");
    let mut j = job("vacuum = on");
    j.restore_state(&plain.to_string()).unwrap();
    assert_eq!(j.stats().vacuum.pick_checks, 0);
    // a section that does not fit the nozzles is refused
    let mut bad: Value = serde_json::from_str(&saved).unwrap();
    bad["vacuum"]["on"] = serde_json::json!([true, false]);
    assert_eq!(
        job("vacuum = on")
            .restore_state(&bad.to_string())
            .unwrap_err()
            .code,
        Code::JobStateBad
    );
}

// ---- the actuators

#[test]
fn an_actuator_is_switched_by_name_and_its_delay_goes_on_the_clock() {
    let mut j = job("actuator = light : 0.02\nactuator = valve : 0");
    assert_eq!(j.actuators().len(), 2);
    let t0 = j.machine().now_us();
    j.set_actuator("light", true).unwrap();
    assert_eq!(j.machine().now_us() - t0, 20_000);
    j.set_actuator("light", true).unwrap();
    j.set_actuator("light", false).unwrap();
    j.set_actuator("valve", true).unwrap();
    // 3 times 20000 microseconds for the light and nothing for the valve
    assert_eq!(j.machine().now_us() - t0, 60_000);
    assert_eq!(j.actuators()[0].switches(), 2);
    assert_eq!(j.actuators()[1].switches(), 1);
    assert_eq!(told(&j, "actuator light switched on"), 2);
    assert_eq!(told(&j, "actuator light switched off"), 1);
    let s = stats(&j);
    let list = s["actuators"].as_array().unwrap();
    assert_eq!(list[0]["name"], "light");
    assert_eq!(list[0]["switches"], 2);
    assert_eq!(list[1]["on"], true);
    let csv = j.stats_csv();
    assert!(csv.contains("\nactuator,light,off,,,,,2,\n"));
    assert!(csv.contains("\nactuator,valve,on,,,,,1,\n"));
    // a name that is not in the machine is refused and the clock does not move
    let now = j.machine().now_us();
    let e = j.set_actuator("pump", true).unwrap_err();
    assert_eq!(e.code, Code::ActuatorUnknown);
    assert_eq!(e.code.label(), "PHOENIX-PNP-046");
    assert_eq!(j.machine().now_us(), now);
}

#[test]
fn the_new_codes_have_a_meaning_in_every_language() {
    let all = [
        Code::VacuumNoPart,
        Code::VacuumStuck,
        Code::ActuatorUnknown,
        Code::FeederFaultWindow,
    ];
    for (n, code) in all.iter().enumerate() {
        assert_eq!(code.number() as usize, 44 + n);
        let mut seen = std::collections::HashSet::new();
        for lang in Lang::ALL {
            let m = code.message(lang);
            assert!(!m.is_empty());
            seen.insert(m);
        }
        assert_eq!(seen.len(), 8, "{}", code.label());
        assert!(Code::ALL.contains(code));
    }
    assert_eq!(Code::ALL.len(), 47);
    assert_eq!(
        Code::VacuumNoPart.message(Lang::Pt),
        "O vácuo não mostra nenhuma peça no bico depois da recolha"
    );
    assert_eq!(
        Code::VacuumStuck.message(Lang::Pt),
        "O vácuo mostra que a peça continua no bico depois da colocação"
    );
    assert_eq!(
        Code::ActuatorUnknown.message(Lang::Pt),
        "A máquina não tem nenhum atuador com esse nome"
    );
    assert_eq!(
        Code::FeederFaultWindow.message(Lang::Pt),
        "Um alimentador tem demasiadas falhas nas últimas recolhas: a pessoa decide"
    );
    assert!(Code::VacuumNoPart.message(Lang::Es).contains("boquilla"));
}

// ---- the window of faults of a feeder

fn stopped_by_the_window() -> Job {
    let mut j = job_of(SIX_RESISTORS, TWO_FEEDERS, ALERT_WINDOW);
    j.machine_mut().force_fail(SimOp::Pick, 1);
    j.machine_mut().force_fail(SimOp::Pick, 2);
    let e = j.run(None).unwrap_err();
    assert_eq!(e.code, Code::FeederFaultWindow);
    assert_eq!(e.code.label(), "PHOENIX-PNP-047");
    assert_eq!(e.detail, "feeder 'FA': 2 faults among its last 3 picks");
    j
}

#[test]
fn a_feeder_that_reaches_its_limit_stops_the_job_instead_of_being_switched_off() {
    let j = stopped_by_the_window();
    assert_eq!(j.phase(), Phase::Stopped);
    let f = j.failure().unwrap();
    assert_eq!((f.feeder.as_str(), f.at_pick), ("FA", true));
    assert!(!j.stats().feeders[0].disabled);
    // the first part was put aside by the policy, the second waits for the person
    assert_eq!(j.stats().deferred, 1);
    assert_eq!(j.status()[f.index], Status::Pending);
    assert_eq!(told(&j, "feeder FA has 2 faults among its last 3 picks"), 1);
    // with the default action the same faults switch the feeder off and the job never stops
    let mut auto = job_of(
        SIX_RESISTORS,
        TWO_FEEDERS,
        "pick_retries = 0\nerror_policy = defer\nfault_window = 3\nfault_limit = 2",
    );
    auto.machine_mut().force_fail(SimOp::Pick, 1);
    auto.machine_mut().force_fail(SimOp::Pick, 2);
    assert_eq!(auto.run(None).unwrap(), Phase::Finished);
    assert!(auto.stats().feeders[0].disabled);
}

#[test]
fn the_person_tries_again_and_the_faults_that_stopped_the_job_are_not_counted_again() {
    let mut j = stopped_by_the_window();
    j.act(Action::Retry).unwrap();
    assert_eq!(state(&j)["devices"][0]["window"], serde_json::json!([]));
    assert_eq!(j.run(None).unwrap(), Phase::Finished);
    assert_eq!((j.stats().placed, j.stats().deferred), (5, 1));
    assert!(!j.stats().feeders[0].disabled);
    assert_eq!(j.trace().actions, [1, 0, 0, 0]);
}

#[test]
fn the_person_leaves_the_part_out_or_puts_it_aside() {
    let mut skip = stopped_by_the_window();
    skip.act(Action::Skip).unwrap();
    assert_eq!(skip.run(None).unwrap(), Phase::Finished);
    assert_eq!(
        (
            skip.stats().placed,
            skip.stats().skipped,
            skip.stats().deferred
        ),
        (4, 1, 1)
    );
    assert!(!skip.stats().feeders[0].disabled);
    let mut aside = stopped_by_the_window();
    aside.act(Action::Defer).unwrap();
    assert_eq!(aside.run(None).unwrap(), Phase::Finished);
    assert_eq!((aside.stats().placed, aside.stats().deferred), (4, 2));
    assert!(aside
        .deferred()
        .iter()
        .any(|(_, why)| why.contains("by the person")));
}

#[test]
fn the_person_switches_the_feeder_off_and_the_other_one_takes_over() {
    let mut j = stopped_by_the_window();
    j.act(Action::SwitchOffFeeder).unwrap();
    assert!(j.stats().feeders[0].disabled);
    assert_eq!(j.run(None).unwrap(), Phase::Finished);
    assert_eq!((j.stats().placed, j.stats().deferred), (5, 1));
    assert_eq!(j.stats().feeders[1].picked, 5);
    assert_eq!(told(&j, "feeder FA switched off by the person"), 1);
    // a job that no error stopped refuses every action
    let mut again = stopped_by_the_window();
    again.act(Action::SwitchOffFeeder).unwrap();
    assert_eq!(
        again.act(Action::Retry).unwrap_err().code,
        Code::JobActionBad
    );
}

#[test]
fn a_feed_that_fails_too_often_stops_the_job_the_same_way() {
    let cfg = "feed_retries = 0\nfault_window = 2\nfault_limit = 2\nfault_action = alert\nerror_policy = defer\n";
    let mut j = job_of(SIX_RESISTORS, TWO_FEEDERS, cfg);
    j.devices_mut()[0].force_fail(1);
    j.devices_mut()[0].force_fail(2);
    let e = j.run(None).unwrap_err();
    assert_eq!(e.code, Code::FeederFaultWindow);
    assert_eq!(j.stats().feeders[0].faults, 2);
    j.act(Action::SwitchOffFeeder).unwrap();
    assert_eq!(j.run(None).unwrap(), Phase::Finished);
    assert_eq!(j.stats().placed + j.stats().deferred, 6);
}
