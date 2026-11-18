//! The slack of the axes of the simulated machine: the error of a move that reverses an axis, the approach from the positive side, the limits, the saved state and the settings.

use eh3d_phoenix_pnp::catalog::Catalog;
use eh3d_phoenix_pnp::config::{MachineConfig, KEYS};
use eh3d_phoenix_pnp::error::Code;
use eh3d_phoenix_pnp::feeding::{self, assign};
use eh3d_phoenix_pnp::job::{Job, Phase};
use eh3d_phoenix_pnp::model::Machine;
use eh3d_phoenix_pnp::motion::{move_time, AxisMotion, BacklashMode};
use eh3d_phoenix_pnp::parts::{from_csv, Side};
use eh3d_phoenix_pnp::planner::{plan_advanced, Options};
use eh3d_phoenix_pnp::sim::SimMachine;
use std::path::PathBuf;
use std::process::{Command, Output};

const MACHINE: &str = include_str!("../examples/machine.cfg");
const PARTS: &str = "ref,val,package,x,y,rot,side\nR1,10k,R,10,10,0,top\nR2,10k,R,20,10,90,top\nR3,10k,R,30,10,180,top\nC1,100n,C,10,20,0,top\nC2,100n,C,20,20,0,top\nU1,MCU,Q,40,15,0,top\n";
const FEEDERS: &str =
    "id,value,package,x,y,count\nF1,10k,R,-20,10,10\nF2,100n,C,-20,20,10\nF3,MCU,Q,-20,40,2\n";

fn machine(cfg: &str) -> SimMachine {
    SimMachine::from_config(&MachineConfig::parse(cfg).unwrap()).unwrap()
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn a_machine_without_slack_moves_as_it_always_did_whatever_the_mode() {
    let mut a = machine("");
    let mut b = machine("backlash_mode = approach");
    for to in [(10.0, 10.0), (5.0, 2.0), (30.0, 40.0), (0.0, 0.0)] {
        a.move_to(to).unwrap();
        b.move_to(to).unwrap();
    }
    assert_eq!(a.now_us(), b.now_us());
    assert_eq!(a.events, b.events);
    assert!(!a.has_backlash() && !b.has_backlash());
    assert_eq!(a.backlash_stats().short_moves, 0);
    assert_eq!(a.backlash_stats().approaches, 0);
}

#[test]
fn a_move_that_reverses_an_axis_ends_short_by_the_slack_and_one_that_does_not_ends_on_the_place() {
    let mut m = machine("backlash_x_mm = 0.2\nbacklash_y_mm = 0.1");
    // both axes start with the slack taken up on the positive side, so the first moves forward are exact
    m.move_to((10.0, 10.0)).unwrap();
    assert_eq!(m.backlash_stats().short_moves, 0);
    assert_eq!(m.lash(), (1, 1));
    // X reverses: it ends 0.2 above the place (it came from above and lagged)
    m.move_to((5.0, 10.0)).unwrap();
    let s = m.backlash_stats();
    assert_eq!(s.short_moves, 1);
    assert!(near(s.worst_mm, 0.2), "{}", s.worst_mm);
    assert_eq!(m.lash(), (-1, 1));
    assert!(m
        .events
        .last()
        .unwrap()
        .text
        .contains("ended X +0.200 Y +0.000 from the place"));
    // the same direction again: the slack is taken up, no error
    m.move_to((2.0, 10.0)).unwrap();
    assert_eq!(m.backlash_stats().short_moves, 1);
    // both axes reverse: the error is the hypotenuse
    m.move_to((8.0, 4.0)).unwrap();
    m.move_to((8.0, 6.0)).unwrap();
    let s = m.backlash_stats();
    assert_eq!(s.short_moves, 3);
    assert!(
        near(s.worst_mm, 0.2f64.hypot(0.1)) || s.worst_mm >= 0.2,
        "{}",
        s.worst_mm
    );
    // the position the machine reports is the commanded one
    assert_eq!(m.position(), (8.0, 6.0));
}

#[test]
fn a_move_with_no_distance_on_an_axis_does_not_touch_its_slack() {
    let mut m = machine("backlash_x_mm = 0.3\nbacklash_y_mm = 0.3");
    m.move_to((10.0, 10.0)).unwrap();
    m.move_to((10.0, 5.0)).unwrap();
    assert_eq!(m.lash(), (1, -1));
    m.move_to((20.0, 5.0)).unwrap();
    assert_eq!(m.lash(), (1, -1));
    assert_eq!(m.backlash_stats().short_moves, 1);
}

#[test]
fn the_approach_goes_past_the_place_and_comes_back_which_costs_time_and_leaves_no_error() {
    let cfg = "backlash_x_mm = 0.4\nbacklash_y_mm = 0.4\nbacklash_mode = approach";
    let mut m = machine(cfg);
    m.move_to((10.0, 10.0)).unwrap();
    let before = m.now_us();
    assert_eq!(
        m.backlash_stats().approaches,
        0,
        "a move forward has no approach"
    );
    m.move_to((5.0, 5.0)).unwrap();
    let s = m.backlash_stats();
    assert_eq!((s.approaches, s.short_moves), (1, 0));
    assert_eq!(m.lash(), (1, 1));
    // the seconds: the way to 0.4 past the place, then 0.4 back along each axis
    let c = MachineConfig::parse(cfg).unwrap();
    let (ax, ay) = (
        AxisMotion::new(c.x_speed_mm_s, c.x_accel_mm_s2, c.x_jerk_mm_s3).unwrap(),
        AxisMotion::new(c.y_speed_mm_s, c.y_accel_mm_s2, c.y_jerk_mm_s3).unwrap(),
    );
    let first = move_time(-5.4, -5.4, &ax, &ay);
    let second = move_time(0.4, 0.4, &ax, &ay);
    let taken = (m.now_us() - before) as f64 / 1e6;
    assert!(
        (taken - (first + second)).abs() < 2e-6,
        "{taken} {}",
        first + second
    );
    assert!((s.approach_s - second).abs() < 2e-6);
    assert!(m
        .events
        .iter()
        .any(|e| e.text.contains("went past the place by X 0.400 Y 0.400")));
    // the same moves without the approach are faster and end short
    let mut off = machine("backlash_x_mm = 0.4\nbacklash_y_mm = 0.4");
    off.move_to((10.0, 10.0)).unwrap();
    let b = off.now_us();
    off.move_to((5.0, 5.0)).unwrap();
    assert!(off.now_us() - b < m.now_us() - before);
    assert_eq!(off.backlash_stats().short_moves, 1);
}

#[test]
fn an_axis_that_moves_forward_while_the_other_goes_back_is_approached_on_one_axis_only() {
    let mut m = machine("backlash_x_mm = 0.5\nbacklash_y_mm = 0.5\nbacklash_mode = approach");
    m.move_to((10.0, 10.0)).unwrap();
    m.move_to((20.0, 4.0)).unwrap();
    let s = m.backlash_stats();
    assert_eq!((s.approaches, s.short_moves), (1, 0));
    assert!(m.events.iter().any(
        |e| e.text.contains("went past the place by X -0.000 Y 0.500")
            || e.text.contains("went past the place by X 0.000 Y 0.500")
    ));
}

#[test]
fn a_place_too_near_the_limit_to_go_past_it_is_reached_the_plain_way() {
    let mut m = machine(
        "limit_x_min_mm = 0\nbacklash_x_mm = 0.5\nbacklash_y_mm = 0.5\nbacklash_mode = approach",
    );
    m.move_to((10.0, 10.0)).unwrap();
    m.move_to((0.2, 10.0)).unwrap();
    let s = m.backlash_stats();
    assert_eq!((s.approaches, s.short_moves), (0, 1));
    assert!(near(s.worst_mm, 0.5));
    // the axis stayed on the negative side, so the move forward reverses it and ends short; the one after, back, goes past the place again
    m.move_to((30.0, 10.0)).unwrap();
    assert_eq!(m.backlash_stats().short_moves, 2);
    m.move_to((20.0, 10.0)).unwrap();
    assert_eq!(m.backlash_stats().approaches, 1);
}

#[test]
fn the_limits_still_stop_a_move_with_slack() {
    let mut m = machine("backlash_x_mm = 0.5\nbacklash_mode = approach");
    let e = m.move_to((1.0e6, 0.0)).unwrap_err();
    assert_eq!(e.code, Code::UnsafeSetting);
    assert_eq!(m.position(), (0.0, 0.0));
    assert_eq!(m.backlash_stats().approaches, 0);
}

fn job(cfg: &str) -> Job {
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
    Job::new(plan, feeders, cfg).unwrap()
}

#[test]
fn a_job_with_slack_counts_it_and_the_approach_costs_machine_time() {
    let slack = "backlash_x_mm = 0.2\nbacklash_y_mm = 0.2";
    let (mut plain, mut off, mut approach) = (
        job(""),
        job(slack),
        job(&format!("{slack}\nbacklash_mode = approach")),
    );
    for j in [&mut plain, &mut off, &mut approach] {
        assert_eq!(j.run(None).unwrap(), Phase::Finished);
    }
    // the feeders are on the negative side of the board and the head goes back and forth, so axes reverse
    assert!(off.machine().backlash_stats().short_moves > 0);
    assert_eq!(off.machine().backlash_stats().approaches, 0);
    assert_eq!(approach.machine().backlash_stats().short_moves, 0);
    assert!(approach.machine().backlash_stats().approaches > 0);
    assert!(approach.machine().now_us() > off.machine().now_us());
    // the slack that is not simulated is not paid for
    assert!(off.machine().now_us() >= plain.machine().now_us());
    assert_eq!(plain.machine().backlash_stats().short_moves, 0);
    // and nothing else about the job changes
    assert_eq!(plain.stats().placed, approach.stats().placed);
}

#[test]
fn a_saved_job_with_slack_resumed_anywhere_ends_as_the_job_that_never_stopped() {
    for mode in ["off", "approach"] {
        let cfg = format!("backlash_x_mm = 0.2\nbacklash_y_mm = 0.3\nbacklash_mode = {mode}");
        let mut whole = job(&cfg);
        whole.run(None).unwrap();
        for stop in 0..=12 {
            let mut first = job(&cfg);
            first.run(Some(stop)).unwrap();
            let state = first.save_state();
            let mut second = job(&cfg);
            second.restore_state(&state).unwrap();
            second.resume();
            second.run(None).unwrap();
            assert_eq!(
                second.machine().now_us(),
                whole.machine().now_us(),
                "{mode} {stop}"
            );
            assert_eq!(
                second.machine().backlash_stats(),
                whole.machine().backlash_stats(),
                "{mode} {stop}"
            );
            assert_eq!(
                second.machine().lash(),
                whole.machine().lash(),
                "{mode} {stop}"
            );
        }
    }
}

#[test]
fn the_state_of_a_machine_without_slack_has_no_section_for_it_and_one_without_it_is_accepted() {
    let mut plain = job("");
    plain.run(Some(3)).unwrap();
    assert!(!plain.save_state().contains("backlash"));
    // a state saved by a machine without slack loads in one that has it, which starts from the positive side
    let state = plain.save_state();
    let mut with = job("backlash_x_mm = 0.2\nbacklash_mode = approach");
    with.restore_state(&state).unwrap();
    assert_eq!(with.machine().lash(), (1, 1));
    assert_eq!(with.machine().backlash_stats().approaches, 0);
    // a damaged figure is refused
    let mut saved = job("backlash_x_mm = 0.2");
    saved.run(Some(3)).unwrap();
    let bad = saved
        .save_state()
        .replacen("\"short_moves\": \"", "\"short_moves\": \"x", 1);
    assert_eq!(
        job("backlash_x_mm = 0.2")
            .restore_state(&bad)
            .unwrap_err()
            .code,
        Code::JobStateBad
    );
}

#[test]
fn the_settings_have_their_ranges_the_names_of_the_modes_and_the_example_is_the_default() {
    for key in ["backlash_x_mm", "backlash_y_mm", "backlash_mode"] {
        assert!(KEYS.contains(&key), "{key}");
    }
    let e = MachineConfig::parse(MACHINE).unwrap();
    let d = MachineConfig::default();
    assert_eq!(
        (e.backlash_x_mm, e.backlash_y_mm, e.backlash_mode),
        (d.backlash_x_mm, d.backlash_y_mm, d.backlash_mode)
    );
    assert_eq!((d.backlash_x_mm, d.backlash_mode), (0.0, BacklashMode::Off));
    for key in ["backlash_x_mm", "backlash_y_mm"] {
        for v in ["0", "2"] {
            assert!(
                MachineConfig::parse(&format!("{key} = {v}")).is_ok(),
                "{key} {v}"
            );
        }
        for v in ["-0.1", "2.1"] {
            let err = MachineConfig::parse(&format!("{key} = {v}")).unwrap_err();
            assert_eq!(err.code, Code::ConfigOutOfRange, "{key} {v}");
        }
    }
    assert_eq!(
        MachineConfig::parse("backlash_mode = APPROACH")
            .unwrap()
            .backlash_mode,
        BacklashMode::Approach
    );
    assert_eq!(
        MachineConfig::parse("backlash_mode = one_sided")
            .unwrap_err()
            .code,
        Code::ConfigBadValue
    );
    for m in BacklashMode::ALL {
        assert_eq!(BacklashMode::parse(m.name()), Some(m));
    }
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_phoenix-pnp"))
        .args(args)
        .current_dir(PathBuf::from(env!("CARGO_MANIFEST_DIR")))
        .output()
        .unwrap()
}

#[test]
fn simulate_says_what_the_slack_did_only_when_the_machine_has_some() {
    let dir = std::env::temp_dir().join(format!("phoenix_pnp_backlash_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("slack.cfg");
    let base = ["simulate", "examples/board.pos", "examples/feeders.csv"];
    let plain = run(&base);
    assert!(plain.status.success());
    assert!(!String::from_utf8_lossy(&plain.stdout).contains("backlash"));
    std::fs::write(&cfg, "backlash_x_mm = 0.2\nbacklash_y_mm = 0.2\n").unwrap();
    let o = run(&[base[0], base[1], base[2], "--config", cfg.to_str().unwrap()]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let out = String::from_utf8_lossy(&o.stdout).into_owned();
    let line = out
        .lines()
        .find(|l| l.starts_with("backlash:"))
        .unwrap_or_else(|| panic!("{out}"));
    assert!(line.contains("ended short of the place"), "{line}");
    assert!(line.contains("0 approaches"), "{line}");
    std::fs::write(
        &cfg,
        "backlash_x_mm = 0.2\nbacklash_y_mm = 0.2\nbacklash_mode = approach\n",
    )
    .unwrap();
    let o = run(&[base[0], base[1], base[2], "--config", cfg.to_str().unwrap()]);
    let out = String::from_utf8_lossy(&o.stdout).into_owned();
    let line = out
        .lines()
        .find(|l| l.starts_with("backlash:"))
        .unwrap_or_else(|| panic!("{out}"));
    assert!(line.starts_with("backlash: 0 moves ended short"), "{line}");
    assert!(!line.contains(" 0 approaches"), "{line}");
    let _ = std::fs::remove_dir_all(&dir);
}
