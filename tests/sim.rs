//! The simulated machine: the virtual clock, the invariants of the nozzles, the failures that can be injected and the other parts of the model.

use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::error::Code;
use eh3d_phoenix_pnp::model::{Actuator, Camera, Driver, FeederDevice, Machine, Nozzle, Offset};
use eh3d_phoenix_pnp::motion::AxisMotion;
use eh3d_phoenix_pnp::sim::{Rng, SimActuator, SimCamera, SimDriver, SimFeeder, SimMachine, SimOp};

fn machine(cfg: &str) -> SimMachine {
    SimMachine::from_config(&MachineConfig::parse(cfg).unwrap()).unwrap()
}

#[test]
fn the_generator_gives_the_same_numbers_for_the_same_seed() {
    let (mut a, mut b, mut c) = (Rng::new(5), Rng::new(5), Rng::new(6));
    let xs: Vec<u64> = (0..50).map(|_| a.next_u64()).collect();
    let ys: Vec<u64> = (0..50).map(|_| b.next_u64()).collect();
    let zs: Vec<u64> = (0..50).map(|_| c.next_u64()).collect();
    assert_eq!(xs, ys);
    assert_ne!(xs, zs);
    // the state can be saved and put back
    let mut d = Rng::new(9);
    d.next_u64();
    let mut e = Rng::from_state(d.state());
    assert_eq!(d.next_u64(), e.next_u64());
    // a state of 0 would stay 0 for ever
    assert_ne!(Rng::from_state(0).state(), 0);
    assert_ne!(Rng::new(0).state(), 0);
    assert_ne!(Rng::new(0).next_u64(), Rng::new(1).next_u64());
}

#[test]
fn the_numbers_of_the_generator_are_spread_evenly_between_0_and_1() {
    let mut r = Rng::new(123);
    let mut buckets = [0u32; 10];
    let mut sum = 0.0;
    for _ in 0..20_000 {
        let x = r.next_f64();
        assert!((0.0..1.0).contains(&x));
        buckets[(x * 10.0) as usize] += 1;
        sum += x;
    }
    assert!((sum / 20_000.0 - 0.5).abs() < 0.02);
    for (i, n) in buckets.iter().enumerate() {
        assert!(*n > 1700 && *n < 2300, "bucket {i} has {n}");
    }
}

#[test]
fn the_virtual_clock_moves_only_with_what_takes_time() {
    let mut m = machine("");
    assert_eq!(m.now_us(), 0);
    m.wait(0.0);
    m.wait(-5.0);
    m.wait(f64::NAN);
    assert_eq!(m.now_us(), 0, "nothing takes time");
    m.wait(1.5);
    assert_eq!(m.now_us(), 1_500_000);
    let t0 = m.now_us();
    m.move_to((100.0, 0.0)).unwrap();
    let motion = AxisMotion::new(150.0, 2000.0, 0.0).unwrap();
    assert_eq!(m.now_us() - t0, (motion.time(100.0) * 1e6).round() as u64);
    assert_eq!(m.position(), (100.0, 0.0));
    // an hour of waiting is simulated at once
    m.wait(3600.0);
    assert!(m.now_us() >= 3_600_000_000);
    // the clock never goes back
    let mut last = m.now_us();
    for i in 0..20 {
        m.move_to((f64::from(i) * 7.0, f64::from(i) * 3.0)).unwrap();
        assert!(m.now_us() >= last);
        last = m.now_us();
    }
}

#[test]
fn a_move_outside_the_limits_is_refused_and_leaves_the_head_where_it_was() {
    let mut m = machine("limit_x_max_mm = 100\nlimit_y_max_mm = 100");
    m.move_to((50.0, 50.0)).unwrap();
    for to in [(101.0, 0.0), (0.0, 101.0), (-3000.0, 0.0), (f64::NAN, 0.0)] {
        let e = m.move_to(to).unwrap_err();
        assert_eq!(e.code, Code::UnsafeSetting, "{to:?}");
    }
    assert_eq!(m.position(), (50.0, 50.0));
}

#[test]
fn a_nozzle_never_picks_while_it_holds_a_part_and_never_places_while_it_holds_none() {
    let mut m = machine("nozzles = 2");
    assert_eq!(m.nozzle_count(), 2);
    assert_eq!(m.place(0).unwrap_err().code, Code::NozzleEmpty);
    m.pick(0, "R1").unwrap();
    assert_eq!(m.holding(0), Some("R1"));
    assert_eq!(m.holding(1), None);
    let busy = m.pick(0, "R2").unwrap_err();
    assert_eq!(busy.code, Code::NozzleBusy);
    assert!(busy.detail.contains("R1"));
    // the other nozzle is free
    m.pick(1, "R2").unwrap();
    assert_eq!(m.place(1).unwrap(), "R2");
    assert_eq!(m.place(1).unwrap_err().code, Code::NozzleEmpty);
    assert_eq!(m.place(0).unwrap(), "R1");
    // there is no third nozzle
    assert_eq!(m.pick(2, "X").unwrap_err().code, Code::PlanSetting);
    assert_eq!(m.place(2).unwrap_err().code, Code::PlanSetting);
    assert_eq!(m.holding(2), None);
    // discarding gets rid of the part, and of nothing, without a fuss
    m.pick(0, "R3").unwrap();
    m.discard(0);
    m.discard(0);
    m.discard(7);
    assert_eq!(m.holding(0), None);
    m.pick(0, "R4").unwrap();
}

#[test]
fn a_pick_and_a_place_take_their_time_and_a_tip_change_takes_its_own() {
    let mut m = machine("pick_s = 0.5\nplace_s = 0.25\ntip_change_s = 4");
    m.pick(0, "R1").unwrap();
    assert_eq!(m.now_us(), 500_000);
    m.place(0).unwrap();
    assert_eq!(m.now_us(), 750_000);
    m.change_tip("N75");
    assert_eq!(m.now_us(), 4_750_000);
    assert_eq!(m.tip(), "N75");
    m.mount_tip("N45");
    assert_eq!(m.tip(), "N45");
    assert_eq!(m.now_us(), 4_750_000, "mounting at the start is free");
    assert!(m.events.iter().any(|e| e.text.contains("picked R1")));
    assert!(m.events.windows(2).all(|w| w[0].t_us <= w[1].t_us));
}

#[test]
fn a_failure_can_be_forced_at_a_chosen_pick_or_place() {
    let mut m = machine("");
    m.force_fail(SimOp::Pick, 2);
    m.force_fail(SimOp::Place, 1);
    m.pick(0, "R1").unwrap();
    let e = m.place(0).unwrap_err();
    assert_eq!(e.code, Code::SimFault);
    assert_eq!(m.holding(0), Some("R1"), "a failed place keeps the part");
    m.place(0).unwrap();
    let e = m.pick(0, "R2").unwrap_err();
    assert_eq!(e.code, Code::SimFault);
    assert_eq!(m.holding(0), None, "a failed pick holds nothing");
    m.pick(0, "R2").unwrap();
}

#[test]
fn the_chance_of_failing_is_followed_and_the_same_seed_fails_in_the_same_places() {
    let run = |cfg: &str| -> Vec<bool> {
        let mut m = machine(cfg);
        (0..2000)
            .map(|_| {
                let failed = m.pick(0, "R").is_err();
                m.discard(0);
                failed
            })
            .collect()
    };
    let never = run("sim_pick_fail_rate = 0");
    assert!(never.iter().all(|f| !f));
    let always = run("sim_pick_fail_rate = 1");
    assert!(always.iter().all(|f| *f));
    let some = run("sim_pick_fail_rate = 0.3\nsim_seed = 4");
    let rate = some.iter().filter(|f| **f).count() as f64 / 2000.0;
    assert!((rate - 0.3).abs() < 0.05, "{rate}");
    assert_eq!(some, run("sim_pick_fail_rate = 0.3\nsim_seed = 4"));
    assert_ne!(some, run("sim_pick_fail_rate = 0.3\nsim_seed = 5"));
}

#[test]
fn the_chance_of_failing_to_place_does_not_move_the_failures_of_the_picks() {
    let run = |cfg: &str| -> Vec<bool> {
        let mut m = machine(cfg);
        let mut picks = Vec::new();
        for _ in 0..300 {
            let failed = m.pick(0, "R").is_err();
            picks.push(failed);
            if !failed {
                let _ = m.place(0);
            }
            m.discard(0);
        }
        picks
    };
    let base = run("sim_pick_fail_rate = 0.4\nsim_seed = 9\nsim_place_fail_rate = 0");
    let other = run("sim_pick_fail_rate = 0.4\nsim_seed = 9\nsim_place_fail_rate = 0.7");
    assert_eq!(base, other);
}

#[test]
fn a_saved_machine_goes_on_with_the_same_failures() {
    let mut a = machine("sim_pick_fail_rate = 0.5\nsim_seed = 3");
    for _ in 0..10 {
        let _ = a.pick(0, "R");
        a.discard(0);
    }
    let saved = a.save();
    let mut b = machine("sim_pick_fail_rate = 0.5\nsim_seed = 3");
    b.restore(saved, a.position(), vec![None], a.tip().to_string());
    for _ in 0..50 {
        assert_eq!(a.pick(0, "R").is_err(), b.pick(0, "R").is_err());
        a.discard(0);
        b.discard(0);
    }
    assert_eq!(a.now_us(), b.now_us());
}

#[test]
fn a_feeder_fails_at_its_own_places_and_counts_down() {
    let mut f = SimFeeder::new("F1", 3, 1, 0, 0.0);
    assert_eq!((f.id(), f.remaining(), f.is_enabled()), ("F1", 3, true));
    f.force_fail(2);
    f.feed().unwrap();
    assert_eq!(f.feed().unwrap_err().code, Code::SimFault);
    f.feed().unwrap();
    f.take();
    f.take();
    f.take();
    f.take();
    assert_eq!(f.remaining(), 0, "a feeder cannot go below empty");
    f.set_enabled(false);
    assert!(!f.is_enabled());
    // two feeders with the same chance do not fail together
    let mut a = SimFeeder::new("A", 9, 7, 0, 0.5);
    let mut b = SimFeeder::new("B", 9, 7, 1, 0.5);
    let xs: Vec<bool> = (0..64).map(|_| a.feed().is_err()).collect();
    let ys: Vec<bool> = (0..64).map(|_| b.feed().is_err()).collect();
    assert_ne!(xs, ys);
    // a saved feeder goes on the same
    let mut c = SimFeeder::new("C", 9, 7, 0, 0.5);
    let mut d = SimFeeder::new("C", 9, 7, 0, 0.5);
    for _ in 0..5 {
        let _ = c.feed();
    }
    d.restore(c.save(), 9, true);
    for _ in 0..20 {
        assert_eq!(c.feed().is_err(), d.feed().is_err());
    }
}

#[test]
fn the_machine_can_be_used_through_its_traits() {
    fn go(machine: &mut dyn Machine, to: (f64, f64)) -> u64 {
        machine.move_to(to).unwrap();
        machine.now_us()
    }
    let mut m = machine("");
    assert!(go(&mut m, (10.0, 10.0)) > 0);
    let mut dev: Box<dyn FeederDevice> = Box::new(SimFeeder::new("F", 1, 1, 0, 0.0));
    dev.feed().unwrap();
    dev.take();
    assert_eq!(dev.remaining(), 0);
    let mut nozzle: Box<dyn Nozzle> = Box::new(machine(""));
    nozzle.pick(0, "R1").unwrap();
    assert_eq!(nozzle.holding(0), Some("R1"));
}

#[test]
fn the_actuator_the_camera_and_the_driver_of_the_simulator_behave() {
    let mut a = SimActuator::default();
    assert!(!a.is_on());
    a.set(true).unwrap();
    assert!(a.is_on());
    let mut cam = SimCamera {
        bias: Offset {
            dx_mm: 0.1,
            dy_mm: -0.2,
            rotation_deg: 1.5,
        },
    };
    assert_eq!(cam.measure(0).unwrap().dx_mm, 0.1);
    assert_eq!(SimCamera::default().measure(3).unwrap(), Offset::default());
    let mut d = SimDriver::default();
    assert_eq!(d.send("G0 X1 Y2 ").unwrap(), "ok");
    assert_eq!(d.sent, vec!["G0 X1 Y2".to_string()]);
    let long = "G".repeat(257);
    for bad in ["", "   ", "G0\u{7}", long.as_str()] {
        assert_eq!(d.send(bad).unwrap_err().code, Code::DriverError);
    }
    assert_eq!(d.sent.len(), 1);
}
