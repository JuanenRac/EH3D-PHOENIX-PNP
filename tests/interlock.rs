//! Interlocks of the axes: a move is refused while the signal a rule asks for is not in the state it asks for, an unknown signal does not satisfy a rule, a refusal stops the session the way
//! a move outside the limits does, and none of it needs a controller beyond the simulated one.

use eh3d_phoenix_pnp::driver::{Kind, Session};
use eh3d_phoenix_pnp::error::Lang;
use eh3d_phoenix_pnp::gcodedriver::{DriverProfile, Fault, Target, KEYS};
use eh3d_phoenix_pnp::gcodesim::{Faults, GcodeSim, SimLink};
use eh3d_phoenix_pnp::interlock::{check, Axes, Interlock, Signals, MAX_SIGNAL};
use eh3d_phoenix_pnp::safety::Level;

const MARLIN: &str = include_str!("../examples/driver_marlin.cfg");
const KLIPPER: &str = include_str!("../examples/driver_klipper.cfg");

fn profile_with(extra: &str) -> DriverProfile {
    DriverProfile::parse(&format!("{MARLIN}\n{extra}\n")).unwrap()
}

fn ready(p: &DriverProfile) -> Session<SimLink> {
    let mut s = Session::new(p, SimLink::new(GcodeSim::new(p, Faults::default(), 1)));
    s.connect().unwrap();
    s.home().unwrap();
    s
}

fn xy(x: f64, y: f64) -> Target {
    Target {
        x: Some(x),
        y: Some(y),
        z: None,
        rot: None,
    }
}

fn z_only(z: f64) -> Target {
    Target {
        x: None,
        y: None,
        z: Some(z),
        rot: None,
    }
}

fn rule(text: &str) -> Interlock {
    Interlock::parse(text).unwrap()
}

#[test]
fn a_rule_is_a_signal_a_state_and_the_axes_it_guards() {
    let r = rule("door | on | all");
    assert_eq!(r.signal, "door");
    assert!(r.required);
    assert_eq!(r.axes, Axes::All);
    assert_eq!(rule("  Nozzles_Up|OFF|XY ").axes, Axes::Xy);
    assert_eq!(rule("blow | off | rot").axes, Axes::Rot);
    assert_eq!(rule("a | on | z").axes, Axes::Z);
    for line in ["door | on | all", "blow | off | xy", "a_1 | on | rot"] {
        assert_eq!(rule(line).to_line(), line);
    }
}

#[test]
fn a_rule_that_is_not_understood_is_refused() {
    for bad in [
        "",
        "door",
        "door | on",
        "door | on | all | more",
        "door | maybe | all",
        "door | on | sideways",
        "| on | all",
        "do or | on | all",
        "door;rm | on | all",
        "porta ñ | on | all",
    ] {
        assert!(Interlock::parse(bad).is_err(), "{bad}");
    }
    let long = "x".repeat(MAX_SIGNAL + 1);
    assert!(Interlock::parse(&format!("{long} | on | all")).is_err());
    assert!(Interlock::parse(&format!("{} | on | all", "x".repeat(MAX_SIGNAL))).is_ok());
}

#[test]
fn the_axes_a_move_touches_are_those_it_names() {
    assert!(Axes::Xy.touched_by(&xy(1.0, 2.0)));
    assert!(!Axes::Xy.touched_by(&z_only(5.0)));
    assert!(Axes::Z.touched_by(&z_only(5.0)));
    assert!(!Axes::Z.touched_by(&xy(1.0, 2.0)));
    let turn = Target {
        x: None,
        y: None,
        z: None,
        rot: Some(90.0),
    };
    assert!(Axes::Rot.touched_by(&turn));
    assert!(Axes::All.touched_by(&turn));
    let nothing = Target {
        x: None,
        y: None,
        z: None,
        rot: None,
    };
    assert!(!Axes::All.touched_by(&nothing));
    let only_x = Target {
        x: Some(1.0),
        ..nothing
    };
    assert!(Axes::Xy.touched_by(&only_x));
}

#[test]
fn a_move_is_allowed_only_when_every_rule_that_guards_it_is_satisfied() {
    let rules = vec![rule("door | on | all"), rule("blow | off | xy")];
    let mut s = Signals::new();
    // nothing is known: nothing moves
    let v = check(&rules, &s, &xy(1.0, 1.0)).unwrap_err();
    assert_eq!((v.signal.as_str(), v.found), ("door", None));
    s.insert("door".into(), true);
    // the door is fine, the blow valve is still unknown
    let v = check(&rules, &s, &xy(1.0, 1.0)).unwrap_err();
    assert_eq!((v.signal.as_str(), v.found), ("blow", None));
    // the rule of the blow valve does not guard the axis of the nozzle
    assert!(check(&rules, &s, &z_only(3.0)).is_ok());
    s.insert("blow".into(), false);
    assert!(check(&rules, &s, &xy(1.0, 1.0)).is_ok());
    s.insert("blow".into(), true);
    let v = check(&rules, &s, &xy(1.0, 1.0)).unwrap_err();
    assert_eq!(
        (v.signal.as_str(), v.found, v.required),
        ("blow", Some(true), false)
    );
    s.insert("door".into(), false);
    // the door guards every axis, even the nozzle
    assert!(check(&rules, &s, &z_only(3.0)).is_err());
}

#[test]
fn no_rule_means_no_refusal_and_a_move_that_names_no_axis_is_never_refused() {
    let none: Vec<Interlock> = Vec::new();
    assert!(check(&none, &Signals::new(), &xy(1.0, 1.0)).is_ok());
    let rules = vec![rule("door | on | all")];
    let nothing = Target {
        x: None,
        y: None,
        z: None,
        rot: None,
    };
    assert!(check(&rules, &Signals::new(), &nothing).is_ok());
}

#[test]
fn the_refusal_is_told_in_the_eight_languages_with_the_signal_and_the_state() {
    let v = check(&[rule("door | on | all")], &Signals::new(), &xy(0.0, 0.0)).unwrap_err();
    for lang in Lang::ALL {
        let text = v.localized(lang);
        assert!(text.contains("door"), "{lang:?}: {text}");
        assert!(!text.contains('{'), "{lang:?}: {text}");
        assert!(text.contains("all"), "{lang:?}: {text}");
    }
    assert!(v.localized(Lang::En).contains("unknown"));
    assert!(v.localized(Lang::Es).contains("desconocida"));
    assert_ne!(v.localized(Lang::De), v.localized(Lang::Fr));
}

#[test]
fn the_profile_lists_its_rules_and_both_examples_carry_one() {
    assert!(KEYS.contains(&"interlock"));
    for (name, text) in [("marlin", MARLIN), ("klipper", KLIPPER)] {
        let p = DriverProfile::parse(text).unwrap();
        assert_eq!(p.interlocks, vec![rule("blow | off | xy")], "{name}");
    }
    let p = profile_with("interlock = door | on | all\ninterlock = nozzles_up | on | xy");
    assert_eq!(p.interlocks.len(), 3);
    // a profile that gives none has none
    assert!(DriverProfile::default().interlocks.is_empty());
}

#[test]
fn a_bad_rule_in_a_profile_names_its_line_and_its_key() {
    let e = DriverProfile::parse(&format!("{MARLIN}\ninterlock = door | sometimes | all\n"))
        .unwrap_err();
    assert_eq!(e.key, "interlock");
    assert!(matches!(e.fault, Fault::BadValue(_)));
    assert!(e.line > 1);
    for lang in Lang::ALL {
        assert!(e.localized(lang).contains("interlock"), "{lang:?}");
    }
    let e = DriverProfile::parse(&format!("{MARLIN}\ninterlock = blow | off | xy\n")).unwrap_err();
    assert_eq!(e.fault, Fault::Duplicate);
    let many: String = (0..40)
        .map(|i| format!("interlock = s{i} | on | xy\n"))
        .collect();
    let e = DriverProfile::parse(&many).unwrap_err();
    assert!(matches!(e.fault, Fault::TooMany(_)), "{:?}", e.fault);
}

#[test]
fn the_state_of_an_actuator_is_known_to_the_session_from_what_it_switched() {
    let p = profile_with("");
    let mut s = ready(&p);
    // the actuators start off
    assert_eq!(s.signals.get("blow"), Some(&false));
    assert_eq!(s.signals.get("pump"), Some(&false));
    s.set_actuator("pump", true).unwrap();
    assert_eq!(s.signals.get("pump"), Some(&true));
    s.set_actuator("pump", false).unwrap();
    assert_eq!(s.signals.get("pump"), Some(&false));
    // the carriage moves while the blow valve is off
    s.move_to(&xy(10.0, 10.0), 6000.0).unwrap();
}

#[test]
fn a_move_in_x_and_y_is_refused_while_the_blow_valve_is_on_and_the_session_stops() {
    let p = profile_with("");
    let mut s = ready(&p);
    s.set_actuator("blow", true).unwrap();
    let before = s.link.sim.true_position();
    let f = s.move_to(&xy(50.0, 50.0), 6000.0).unwrap_err();
    assert_eq!(f.kind, Kind::Interlock);
    assert!(f.detail.contains("blow"), "{}", f.detail);
    // nothing was sent that moves the head
    assert_eq!(s.link.sim.true_position(), before);
    assert!(s.is_halted());
    assert!(s.log.iter().any(|e| e.line == "(halt)"));
    // the lines that leave the machine safe were sent, and nothing more is
    let after = s.move_to(&xy(1.0, 1.0), 6000.0).unwrap_err();
    assert_eq!(after.kind, Kind::Halted);
    // the rules of safety slowed the machine down
    assert!(s.governor.level() >= Level::Careful);
}

#[test]
fn the_axis_of_the_nozzle_still_moves_when_only_x_and_y_are_guarded() {
    let p = profile_with("");
    let mut s = ready(&p);
    s.set_actuator("blow", true).unwrap();
    s.move_to(&z_only(5.0), 3000.0).unwrap();
    assert!(!s.is_halted());
}

#[test]
fn a_sensor_that_nobody_reports_keeps_the_machine_still_and_one_that_does_lets_it_move() {
    let p = profile_with("interlock = door | on | all");
    let mut s = ready(&p);
    let f = s.move_to(&xy(5.0, 5.0), 6000.0).unwrap_err();
    assert_eq!(f.kind, Kind::Interlock);
    assert!(f.detail.contains("door"));
    // a new session, with the door reported closed
    let mut s = ready(&p);
    assert!(s.set_signal("door", true));
    s.move_to(&xy(5.0, 5.0), 6000.0).unwrap();
    s.move_to(&z_only(8.0), 3000.0).unwrap();
    // the door opens in the middle of the job
    assert!(s.set_signal("door", false));
    let f = s.move_to(&xy(9.0, 9.0), 6000.0).unwrap_err();
    assert_eq!(f.kind, Kind::Interlock);
    assert!(s.is_halted());
}

#[test]
fn the_signals_the_session_keeps_are_few() {
    let p = profile_with("");
    let mut s = ready(&p);
    let mut accepted = 0;
    for i in 0..500 {
        if s.set_signal(&format!("sensor_{i}"), true) {
            accepted += 1;
        }
    }
    assert!(accepted < 100, "{accepted}");
    // one that is known can still change
    assert!(s.set_signal("sensor_0", false));
    // and a name that is not a name is not kept
    assert!(!s.set_signal("", true));
    assert!(!s.set_signal("a b", true));
    assert!(!s.set_signal(&"y".repeat(MAX_SIGNAL + 1), true));
}

#[test]
fn what_the_session_does_is_what_the_calculation_says_on_moves_made_by_chance() {
    // a fixed seed, so a failure can be run again
    let mut x: u64 = 0x2545_F491_4F6C_DD1D;
    let mut next = move || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    let p = profile_with("interlock = door | on | all\ninterlock = lift | on | xy");
    for _round in 0..60 {
        let mut s = ready(&p);
        let mut signals: Signals = s.signals.clone();
        for _step in 0..25 {
            match next() % 4 {
                0 => {
                    let on = next() % 2 == 0;
                    assert!(s.set_signal("door", on));
                    signals.insert("door".into(), on);
                }
                1 => {
                    let on = next() % 2 == 0;
                    assert!(s.set_signal("lift", on));
                    signals.insert("lift".into(), on);
                }
                _ => {
                    let target = if next() % 2 == 0 {
                        xy((next() % 300) as f64, (next() % 200) as f64)
                    } else {
                        z_only((next() % 30) as f64)
                    };
                    let expected = check(&p.interlocks, &signals, &target).is_ok();
                    let before = s.link.sim.true_position();
                    match s.move_to(&target, 6000.0) {
                        Ok(()) => assert!(expected, "a move that a rule forbids was made"),
                        Err(f) if f.kind == Kind::Interlock => {
                            assert!(!expected);
                            assert_eq!(s.link.sim.true_position(), before);
                            break;
                        }
                        Err(f) => panic!("{f:?}"),
                    }
                }
            }
        }
    }
}
