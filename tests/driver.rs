//! The G-code driver: the profile file and its checks, the lines it writes, how it reads the answers of a controller, the Moonraker requests, the simulated controller and a session that
//! has timeouts, retries and a stop that fails closed. Everything is played against the simulated controller; none of it has been tried on a machine.

use eh3d_phoenix_pnp::driver::{Kind, Session};
use eh3d_phoenix_pnp::drivertest;
use eh3d_phoenix_pnp::error::Lang;
use eh3d_phoenix_pnp::gcodedriver::{
    DriverProfile, Fault, Op, Pos, Reply, Target, Transport, KEYS,
};
use eh3d_phoenix_pnp::gcodesim::{Faults, GcodeSim, Link, MoonrakerLink, SimLink, SimMoonraker};
use eh3d_phoenix_pnp::plan::Placement;

const MARLIN: &str = include_str!("../examples/driver_marlin.cfg");
const KLIPPER: &str = include_str!("../examples/driver_klipper.cfg");
const T_MARLIN: &str = include_str!("../examples/marlin_answers.txt");
const T_KLIPPER: &str = include_str!("../examples/klipper_answers.txt");

fn marlin() -> DriverProfile {
    DriverProfile::parse(MARLIN).unwrap()
}

fn klipper() -> DriverProfile {
    DriverProfile::parse(KLIPPER).unwrap()
}

fn placement(reference: &str, pick: (f64, f64), at: (f64, f64), rot: f64) -> Placement {
    Placement {
        reference: reference.into(),
        feeder: "F1".into(),
        pick,
        x: at.0,
        y: at.1,
        rotation: rot,
        nozzle: 0,
        batch: 0,
    }
}

fn session(p: &DriverProfile, faults: Faults) -> Session<SimLink> {
    Session::new(p, SimLink::new(GcodeSim::new(p, faults, 1)))
}

fn ready(p: &DriverProfile, faults: Faults) -> Session<SimLink> {
    let mut s = session(p, faults);
    s.connect().unwrap();
    s.home().unwrap();
    s
}

fn error_of(text: &str) -> eh3d_phoenix_pnp::gcodedriver::ProfileError {
    DriverProfile::parse(text).unwrap_err()
}

// ---- the profile file

#[test]
fn both_examples_are_valid_profiles_of_their_dialect_and_transport() {
    let m = marlin();
    assert_eq!(m.transport, Transport::Serial);
    assert_eq!(m.dialect.name(), "marlin");
    let k = klipper();
    assert_eq!(k.transport, Transport::Moonraker);
    assert_eq!(k.dialect.name(), "klipper");
    assert_eq!(m.actuators.len(), 2);
    assert_eq!(m.safe_stop.len(), 3);
}

#[test]
fn the_default_profile_is_the_one_the_marlin_example_gives_in_what_a_machine_runs() {
    let d = DriverProfile::default();
    let m = marlin();
    assert_eq!(d.connect, m.connect);
    assert_eq!(d.home, m.home);
    assert_eq!(d.actuators, m.actuators);
    assert_eq!(d.feed_travel, m.feed_travel);
    assert_eq!(d.retries, m.retries);
    assert_eq!(d.position.source(), m.position.source());
}

#[test]
fn every_key_is_documented_in_the_examples_and_is_a_name_in_lower_case_once() {
    let mut seen = std::collections::BTreeSet::new();
    for k in KEYS {
        assert!(seen.insert(k), "{k} twice");
        assert!(
            k.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
            "{k}"
        );
        let line = |t: &str| {
            t.lines()
                .any(|l| l.trim_start().starts_with(&format!("{k} =")))
        };
        assert!(line(MARLIN) || line(KLIPPER), "{k} is not in an example");
    }
    for l in MARLIN
        .lines()
        .chain(KLIPPER.lines())
        .filter(|l| !l.trim_start().starts_with('#') && l.contains('='))
    {
        let key = l.split('=').next().unwrap().trim();
        assert!(KEYS.contains(&key), "{key} is not a key");
    }
}

#[test]
fn an_error_names_its_line_and_its_key() {
    let e = error_of("dialect = marlin\nbogus = 1\n");
    assert_eq!(
        (e.line, e.key.as_str(), e.fault.clone()),
        (2, "bogus", Fault::UnknownKey)
    );
    let e = error_of("retries = 2\nretries = 3\n");
    assert_eq!((e.line, e.fault), (2, Fault::Duplicate));
    let e = error_of("retries = 11\n");
    assert!(matches!(e.fault, Fault::OutOfRange(..)), "{e:?}");
    let e = error_of("feed_travel_mm_min = fast\n");
    assert!(matches!(e.fault, Fault::BadValue(_)));
    let e = error_of("just some words\n");
    assert_eq!((e.line, e.fault), (1, Fault::Syntax));
    let e = error_of("ok_pattern = (\n");
    assert!(matches!(e.fault, Fault::BadPattern(_)));
    let e = error_of("position_pattern = X:(\\d+)\n");
    assert!(matches!(e.fault, Fault::BadPattern(_)), "{e:?}");
    let e = error_of("schema_version = 2\n");
    assert!(matches!(e.fault, Fault::BadValue(_)));
}

#[test]
fn values_that_depend_on_each_other_are_checked_as_a_whole() {
    for (text, key) in [
        ("feed_max_mm_min = 1000\n", "feed_max_mm_min"),
        ("z_travel_mm = 0.1\n", "z_travel_mm"),
        ("z_pick_mm = 99\n", "z_pick_mm"),
        ("limit_z_max_mm = -1\n", "limit_z_min_mm"),
        ("limit_x_min_mm = 500\n", "limit_x_min_mm"),
        ("safety_slow_events = 1\n", "safety_slow_events"),
        ("safety_slow_speed_pct = 90\n", "safety_slow_speed_pct"),
    ] {
        let e = error_of(text);
        assert_eq!(e.key, key, "{text}");
        assert!(matches!(e.fault, Fault::Order(_)), "{text}: {e:?}");
    }
    let e = error_of("pump = nothing\n");
    assert!(matches!(e.fault, Fault::NoActuator(_)));
    let e = error_of("home = G28\nactuator = a | M42 P1 S255\n");
    assert!(matches!(e.fault, Fault::BadValue(_)));
}

#[test]
fn the_lists_start_fresh_when_a_file_gives_them_and_have_a_limit() {
    let p = DriverProfile::parse("connect = M115\nconnect = M111 S0\nhome = G28 X\nhome = G28 Y\n")
        .unwrap();
    assert_eq!(p.connect, vec!["M115", "M111 S0"]);
    assert_eq!(p.home, vec!["G28 X", "G28 Y"]);
    let many: String = (0..40).map(|i| format!("home = G{i}\n")).collect();
    assert!(matches!(error_of(&many).fault, Fault::TooMany(_)));
    assert!(matches!(
        error_of(&format!("connect = {}\n", "M".repeat(300))).fault,
        Fault::LineTooLong(_)
    ));
    assert!(matches!(
        error_of("home = G28 ; comment\n").fault,
        Fault::BadValue(_)
    ));
}

#[test]
fn a_big_file_and_a_file_with_a_bom_and_odd_line_ends_are_read_or_refused() {
    let big = "# x\n".repeat(20_000);
    assert!(matches!(error_of(&big).fault, Fault::TooBig(_)));
    let crlf = MARLIN.replace('\n', "\r\n");
    assert!(DriverProfile::parse(&format!("\u{feff}{crlf}")).is_ok());
}

#[test]
fn the_message_of_every_kind_of_error_is_in_the_eight_languages_and_has_its_line() {
    let cases = [
        error_of("bogus = 1\n"),
        error_of("retries = 2\nretries = 3\n"),
        error_of("retries = 11\n"),
        error_of("retries = x\n"),
        error_of("ok_pattern = (\n"),
        error_of("junk\n"),
        error_of("pump = nothing\n"),
        error_of("feed_max_mm_min = 1000\n"),
        error_of(&"# x\n".repeat(20_000)),
        error_of(&format!("connect = {}\n", "M".repeat(300))),
    ];
    for e in &cases {
        let mut texts = std::collections::BTreeSet::new();
        for l in Lang::ALL {
            let t = e.localized(l);
            assert!(!t.is_empty() && !t.contains('{'), "{l:?}: {t}");
            texts.insert(t);
        }
        // the detail of an error stays in English, as in every error of the program; what is around it is translated
        assert!(texts.len() >= 4, "{e:?}: {texts:?}");
    }
    assert!(error_of("bogus = 1\n")
        .localized(Lang::De)
        .contains("Zeile 1"));
    assert!(error_of("bogus = 1\n")
        .localized(Lang::Pt)
        .contains("linha 1"));
}

#[test]
fn through_moonraker_the_patterns_must_read_what_the_link_writes() {
    let bad = KLIPPER.replace("ok_pattern = ^ok", "ok_pattern = ^done");
    assert!(DriverProfile::parse(&bad).is_err());
    let bad = KLIPPER.replace("error_pattern = ^(!!|error)", "error_pattern = ^never");
    assert!(DriverProfile::parse(&bad).is_err());
}

// ---- the lines it writes

#[test]
fn a_move_is_a_line_and_the_line_that_waits_for_it() {
    let p = marlin();
    let t = Target {
        x: Some(12.5),
        y: Some(-3.0),
        z: Some(10.0),
        rot: Some(90.0),
    };
    assert_eq!(
        p.lines(&Op::Move(t, 6000.0)).unwrap(),
        vec!["G1 X12.500 Y-3.000 Z10.000 E90.000 F6000", "M400"]
    );
    assert_eq!(p.lines(&Op::Query).unwrap(), vec!["M114"]);
    assert_eq!(p.lines(&Op::Dwell(150)).unwrap(), vec!["G4 P150"]);
    assert_eq!(
        p.lines(&Op::Actuator("pump".into(), true)).unwrap(),
        vec!["M42 P5 S255"]
    );
    assert_eq!(
        p.lines(&Op::Actuator("blow".into(), false)).unwrap(),
        vec!["M42 P6 S0"]
    );
    assert!(p.lines(&Op::Actuator("nothing".into(), true)).is_err());
}

#[test]
fn a_machine_without_a_turning_axis_refuses_a_turn_and_the_placement_leaves_it_out() {
    let lf = MARLIN.replace("\r\n", "\n");
    let p = DriverProfile::parse(&format!("{lf}\naxis_rot = none\n").replace("axis_rot = E\n", ""))
        .unwrap();
    let t = Target {
        rot: Some(10.0),
        ..Target::default()
    };
    assert!(p.move_line(&t, 100.0).is_err());
    let ops = p
        .placement_ops(&placement("R1", (-20.0, 10.0), (10.0, 20.0), 90.0))
        .unwrap();
    assert!(ops.iter().all(|o| match o {
        Op::Move(t, _) => t.rot.is_none(),
        _ => true,
    }));
}

#[test]
fn a_move_is_refused_outside_the_limits_over_the_feed_or_with_a_number_that_is_not_one() {
    let p = marlin();
    for t in [
        Target {
            x: Some(-51.0),
            ..Target::default()
        },
        Target {
            y: Some(301.0),
            ..Target::default()
        },
        Target {
            z: Some(-0.1),
            ..Target::default()
        },
        Target {
            z: Some(41.0),
            ..Target::default()
        },
        Target {
            x: Some(f64::INFINITY),
            ..Target::default()
        },
        Target {
            rot: Some(f64::NAN),
            ..Target::default()
        },
    ] {
        assert!(p.move_line(&t, 1000.0).is_err(), "{t:?}");
    }
    let t = Target {
        x: Some(1.0),
        ..Target::default()
    };
    assert!(p.move_line(&t, 0.0).is_err());
    assert!(p.move_line(&t, -5.0).is_err());
    assert!(p.move_line(&t, p.feed_max + 1.0).is_err());
    assert!(p.move_line(&t, p.feed_max).is_ok());
}

#[test]
fn a_placement_goes_up_over_the_feeder_down_pump_on_up_over_the_place_down_pump_off_blow_up() {
    let p = marlin();
    let ops = p
        .placement_ops(&placement("R1", (-20.0, 10.0), (10.0, 20.0), 450.0))
        .unwrap();
    let text: Vec<String> = ops.iter().flat_map(|o| p.lines(o).unwrap()).collect();
    let at = |needle: &str| text.iter().position(|l| l.starts_with(needle)).unwrap();
    assert!(at("M42 P5 S255") < at("G1 X10.000"));
    assert!(text.iter().filter(|l| l.as_str() == "M42 P5 S255").count() == 1);
    assert!(
        text.iter().any(|l| l.contains("E90.000")),
        "the turn is brought into 0 to 360: {text:?}"
    );
    // the pump comes off at the place, and the blow is switched on and off
    let off = text.iter().rposition(|l| l == "M42 P5 S0").unwrap();
    assert!(off > at("G1 X10.000"));
    assert!(text.iter().any(|l| l == "M42 P6 S255") && text.iter().any(|l| l == "M42 P6 S0"));
    // every move is followed by the wait
    for (i, l) in text.iter().enumerate() {
        if l.starts_with("G1 ") {
            assert_eq!(text[i + 1], "M400");
        }
    }
    // the last step is a lift
    assert!(text[text.len() - 2].contains("Z10.000"));
}

#[test]
fn a_placement_of_a_nozzle_the_profile_does_not_have_is_refused() {
    // the nozzles of a plan are counted from 0
    let mut pl = placement("R1", (0.0, 0.0), (1.0, 1.0), 0.0);
    assert!(marlin().placement_ops(&pl).is_ok());
    pl.nozzle = 1;
    assert!(marlin()
        .placement_ops(&pl)
        .unwrap_err()
        .contains("nozzle 2"));
    let two = DriverProfile::parse(&MARLIN.replace("nozzles = 1", "nozzles = 2")).unwrap();
    assert!(two.placement_ops(&pl).is_ok());
    pl.nozzle = 2;
    assert!(two.placement_ops(&pl).is_err());
}

// ---- how it reads a controller

#[test]
fn the_answers_of_the_two_dialects_that_the_examples_write_down_are_read_as_written() {
    let (rows, bad) = drivertest::check(&marlin(), T_MARLIN);
    assert!(rows >= 14, "{rows}");
    assert!(bad.is_empty(), "{bad:?}");
    let (rows, bad) = drivertest::check(&klipper(), T_KLIPPER);
    assert!(rows >= 7, "{rows}");
    assert!(bad.is_empty(), "{bad:?}");
}

#[test]
fn a_pattern_that_confuses_two_answers_is_found_by_the_transcript() {
    // an `ok` that is not tied to the start of the line takes `echo:SD card ok` for an answer
    let loose =
        DriverProfile::parse(&MARLIN.replace("ok_pattern = ^ok", "ok_pattern = ok")).unwrap();
    let (_, bad) = drivertest::check(&loose, T_MARLIN);
    assert!(bad.iter().any(|m| m.text.contains("SD card ok")), "{bad:?}");
    assert!(bad.iter().all(|m| !m.expected.is_empty()));
    // a position pattern without the E group reads the position without the turn
    let no_e = DriverProfile::parse(&MARLIN.replace(r"(?: E:(-?\d+\.?\d*))?", "")).unwrap();
    let (_, bad) = drivertest::check(&no_e, T_MARLIN);
    assert!(
        bad.iter()
            .any(|m| m.expected.starts_with("position 10 20 5 90")),
        "{bad:?}"
    );
    for m in &bad {
        for l in Lang::ALL {
            assert!(drivertest::message(m, l).contains(&m.line.to_string()));
        }
    }
}

#[test]
fn a_transcript_that_is_not_one_is_told_with_its_line() {
    let p = marlin();
    for text in [
        "no bar here\n",
        "maybe | ok\n",
        "position 1 2 | X:1 Y:2\n",
        "position a b c | x\n",
    ] {
        let (_, bad) = drivertest::check(&p, text);
        assert_eq!(bad.len(), 1, "{text}");
        assert_eq!(bad[0].line, 1);
        assert!(bad[0].expected.is_empty());
    }
    let (rows, bad) = drivertest::check(&p, "# only a comment\n\n");
    assert_eq!((rows, bad.len()), (0, 0));
}

#[test]
fn the_cases_of_the_firmware_that_confuse_a_pattern_are_each_read_right() {
    let p = marlin();
    assert!(matches!(p.classify("ok"), Reply::Ok));
    assert!(matches!(p.classify("echo:busy: processing"), Reply::Busy));
    assert!(matches!(
        p.classify("Error:Printer halted. kill() called!"),
        Reply::Error(_)
    ));
    assert!(matches!(p.classify("echo:  M92 X80.00"), Reply::Echo(_)));
    match p.classify("X:1.50 Y:2.00 Z:3.00 E:4.00 Count X:120 Y:160 Z:1200") {
        Reply::Position(Pos { x, y, z, rot }) => {
            assert_eq!((x, y, z, rot), (1.5, 2.0, 3.0, Some(4.0)))
        }
        other => panic!("{other:?}"),
    }
    // the position of the count block is not taken for the position
    assert!(matches!(
        p.classify("Count X:120 Y:160 Z:1200"),
        Reply::Other(_) | Reply::Position(_)
    ));
    // a position with no E at all
    match p.classify("X:0.00 Y:0.00 Z:10.00") {
        Reply::Position(pos) => assert_eq!(pos.rot, None),
        other => panic!("{other:?}"),
    }
}

// ---- Moonraker

#[test]
fn a_script_is_a_post_with_the_lines_joined_and_the_query_is_a_get() {
    let k = klipper();
    let (path, body) = k.script_request(&["G1 X1".to_string(), "M400".to_string()]);
    assert_eq!(path, "/printer/gcode/script");
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["script"], "G1 X1\nM400");
    assert_eq!(k.query_request(), "/printer/objects/query?toolhead");
}

#[test]
fn the_answers_of_moonraker_are_read_status_and_message_and_position() {
    let k = klipper();
    assert_eq!(k.read_script_answer(200, "{\"result\":\"ok\"}"), Reply::Ok);
    assert_eq!(
        k.read_script_answer(
            400,
            "{\"error\":{\"code\":400,\"message\":\"Must home axis first\"}}"
        ),
        Reply::Error("Must home axis first".into())
    );
    assert_eq!(
        k.read_script_answer(503, "<html>"),
        Reply::Error("HTTP 503".into())
    );
    let ok = "{\"result\":{\"status\":{\"toolhead\":{\"position\":[1.5,2,3,4]}}}}";
    assert_eq!(
        k.read_position_answer(ok).unwrap(),
        Pos {
            x: 1.5,
            y: 2.0,
            z: 3.0,
            rot: Some(4.0)
        }
    );
    for bad in [
        "",
        "[]",
        "{\"result\":{}}",
        "{\"result\":{\"status\":{\"toolhead\":{\"position\":[1,2]}}}}",
        "{\"result\":{\"status\":{\"toolhead\":{\"position\":[\"a\",2,3]}}}}",
    ] {
        assert!(k.read_position_answer(bad).is_err(), "{bad}");
    }
}

// ---- the simulated controller

#[test]
fn the_simulated_marlin_answers_like_one() {
    let p = marlin();
    let mut s = GcodeSim::new(&p, Faults::default(), 1);
    assert_eq!(s.handle("G28").unwrap().0, vec!["ok"]);
    assert_eq!(s.handle("G1 X100 Y50 Z5 E45 F6000").unwrap().0, vec!["ok"]);
    let (_, delay) = s.handle("M400").unwrap();
    assert!(delay > 0.5, "{delay}");
    let m114 = s.handle("M114").unwrap().0;
    assert_eq!(
        p.classify(&m114[0]),
        Reply::Position(Pos {
            x: 100.0,
            y: 50.0,
            z: 5.0,
            rot: Some(45.0)
        })
    );
    assert_eq!(s.handle("M42 P5 S255").unwrap().0, vec!["ok"]);
    assert_eq!(s.pin(5), Some(255));
    assert!(s.handle("M42 P5 S999").unwrap().0[0].starts_with("Error:"));
    let unknown = s.handle("G999").unwrap().0;
    assert!(unknown[0].starts_with("echo:Unknown command") && unknown[1] == "ok");
    assert!(s.handle("G1 X5000 F1000").unwrap().0[0].starts_with("Error:"));
}

#[test]
fn the_simulated_klipper_refuses_what_it_does_not_know_and_names_its_pins() {
    let p = klipper();
    let mut s = GcodeSim::new(&p, Faults::default(), 1);
    assert!(s.handle("G999").unwrap().0[0].starts_with("!!"));
    assert_eq!(s.handle("SET_PIN PIN=pump VALUE=1").unwrap().0, vec!["ok"]);
    assert_eq!(s.named_pin("pump"), Some(1.0));
    assert!(s.handle("SET_PIN PIN=pump").unwrap().0[0].starts_with("!!"));
}

#[test]
fn the_faults_do_what_they_say() {
    let p = marlin();
    let mut s = GcodeSim::new(
        &p,
        Faults {
            error_at: Some((2, "boom".into())),
            ..Faults::default()
        },
        1,
    );
    assert_eq!(
        s.handle("M115").unwrap().0.last().map(String::as_str),
        Some("ok")
    );
    assert_eq!(s.handle("M115").unwrap().0, vec!["Error:boom"]);
    let mut s = GcodeSim::new(
        &p,
        Faults {
            silent_at: Some(1),
            ..Faults::default()
        },
        1,
    );
    assert!(s.handle("G28").is_none());
    assert_eq!(
        s.position().z,
        40.0,
        "a silent controller still did the line"
    );
    let mut s = GcodeSim::new(
        &p,
        Faults {
            halt_at: Some(2),
            ..Faults::default()
        },
        1,
    );
    assert!(s.handle("M115").is_some());
    assert!(s.handle("M115").unwrap().0[0].starts_with("Error:Printer halted"));
    assert!(s.is_halted());
    let mut s = GcodeSim::new(
        &p,
        Faults {
            busy_lines: 2,
            ..Faults::default()
        },
        1,
    );
    assert_eq!(s.handle("G90").unwrap().0.len(), 3);
    let mut s = GcodeSim::new(
        &p,
        Faults {
            homing_jitter: 0.5,
            ..Faults::default()
        },
        3,
    );
    s.handle("G28");
    let off = s.true_offset;
    assert!(off != (0.0, 0.0) && off.0.abs() <= 0.5 && off.1.abs() <= 0.5);
}

// ---- a session

#[test]
fn a_whole_placement_runs_on_the_simulated_controller_and_leaves_the_pump_off() {
    let p = marlin();
    let mut s = ready(&p, Faults::default());
    let report = s.run_placements(&[
        placement("R1", (-20.0, 10.0), (10.0, 20.0), 0.0),
        placement("R2", (-20.0, 10.0), (15.0, 20.0), 90.0),
    ]);
    assert_eq!(report.placed, 2);
    assert!(report.failed.is_none());
    assert_eq!(s.link.sim.pin(5), Some(0));
    assert_eq!(s.link.sim.pin(6), Some(0));
    let pos = s.link.sim.position();
    assert_eq!((pos.x, pos.y, pos.z), (15.0, 20.0, 10.0));
    assert!(s.link.now() > 1.0);
    assert!(!s.is_halted());
    s.close().unwrap();
    assert_eq!(s.link.sim.log.last().map(String::as_str), Some("M84"));
}

#[test]
fn a_session_works_through_moonraker_too() {
    let p = klipper();
    let link = MoonrakerLink::new(&p, SimMoonraker::new(&p, Faults::default(), 1));
    let mut s = Session::new(&p, link);
    s.connect().unwrap();
    s.home().unwrap();
    let r = s.run_placements(&[placement("R1", (-20.0, 10.0), (10.0, 20.0), 0.0)]);
    assert_eq!(r.placed, 1, "{r:?}");
    assert_eq!(s.link.http.sim.named_pin("pump"), Some(0.0));
    // an error of the controller comes back as the message of Moonraker and stops the session
    let mut s = Session::new(
        &p,
        MoonrakerLink::new(&p, SimMoonraker::new(&p, Faults::default(), 1)),
    );
    s.connect().unwrap();
    let e = s
        .move_to(
            &Target {
                x: Some(1.0),
                ..Target::default()
            },
            1000.0,
        )
        .unwrap_err();
    assert_eq!(e.kind, Kind::Rejected);
    assert!(e.detail.contains("home axis"), "{}", e.detail);
    assert!(s.is_halted());
}

#[test]
fn an_error_from_the_controller_stops_the_session_after_the_lines_that_leave_it_safe() {
    let p = marlin();
    let mut s = ready(
        &p,
        Faults {
            error_at: Some((12, "stepper fault".into())),
            ..Faults::default()
        },
    );
    let report = s.run_placements(&[placement("R1", (-20.0, 10.0), (10.0, 20.0), 0.0)]);
    let (who, f) = report.failed.unwrap();
    assert_eq!(who, "R1");
    assert_eq!(f.kind, Kind::Rejected);
    assert!(s.is_halted());
    let log = &s.link.sim.log;
    let tail: Vec<&str> = log.iter().rev().take(3).map(String::as_str).collect();
    assert_eq!(tail, vec!["M84", "M42 P6 S0", "M42 P5 S0"]);
    // nothing more is sent: the session fails closed
    let before = s.link.sim.log.len();
    assert_eq!(
        s.move_to(
            &Target {
                x: Some(1.0),
                ..Target::default()
            },
            1000.0
        )
        .unwrap_err()
        .kind,
        Kind::Halted
    );
    assert_eq!(s.set_actuator("pump", true).unwrap_err().kind, Kind::Halted);
    assert_eq!(s.home().unwrap_err().kind, Kind::Halted);
    assert_eq!(s.position().unwrap_err().kind, Kind::Halted);
    assert_eq!(s.link.sim.log.len(), before);
}

#[test]
fn a_query_that_gets_no_answer_is_asked_again_and_a_move_is_not_sent_twice() {
    let p = marlin();
    // line 1 is M115, 2 is G90 (connect), 3 is G28; the 4th line is the first thing asked
    let mut s = ready(
        &p,
        Faults {
            silent_at: Some(4),
            ..Faults::default()
        },
    );
    let pos = s.position().unwrap();
    assert_eq!(pos.z, 40.0);
    assert_eq!(
        s.link
            .sim
            .log
            .iter()
            .filter(|l| l.as_str() == "M114")
            .count(),
        2
    );
    // a move whose own line gets no answer is not sent again
    let mut s = ready(
        &p,
        Faults {
            silent_at: Some(4),
            ..Faults::default()
        },
    );
    let e = s
        .move_to(
            &Target {
                x: Some(5.0),
                ..Target::default()
            },
            3000.0,
        )
        .unwrap_err();
    assert_eq!(e.kind, Kind::Timeout);
    assert_eq!(
        s.link
            .sim
            .log
            .iter()
            .filter(|l| l.starts_with("G1 "))
            .count(),
        1
    );
    assert!(s.is_halted());
}

#[test]
fn a_wait_that_times_out_is_settled_by_asking_where_the_head_is() {
    let p = marlin();
    let mut s = ready(&p, Faults::default());
    s.profile.move_timeout_s = 0.5;
    // 100 mm at 3000 mm/min take 2 s: the wait times out, the head is at the target, the session goes on
    let t = Target {
        x: Some(100.0),
        y: Some(0.0),
        z: Some(40.0),
        rot: None,
    };
    s.move_to(&t, 3000.0).unwrap();
    assert!(!s.is_halted());
    assert!(s.log.iter().any(|e| e.line == "(recovered)"));
}

#[test]
fn a_position_that_is_not_the_one_asked_for_stops_the_session() {
    let p = marlin();
    let mut s = ready(
        &p,
        Faults {
            report_offset: 0.5,
            ..Faults::default()
        },
    );
    let e = s
        .move_to(
            &Target {
                x: Some(10.0),
                y: Some(10.0),
                ..Target::default()
            },
            3000.0,
        )
        .unwrap_err();
    assert_eq!(e.kind, Kind::Mismatch);
    assert!(s.is_halted());
    assert_eq!(s.link.sim.pin(5), Some(0), "the pump stays off");
    // with the check off the same machine does not notice (that is why it is on)
    let mut s = ready(
        &p,
        Faults {
            report_offset: 0.5,
            ..Faults::default()
        },
    );
    s.verify_moves = false;
    assert!(s
        .move_to(
            &Target {
                x: Some(10.0),
                y: Some(10.0),
                ..Target::default()
            },
            3000.0
        )
        .is_ok());
}

#[test]
fn a_move_outside_the_limits_is_refused_before_it_is_sent_and_stops_the_session() {
    let p = marlin();
    let mut s = ready(&p, Faults::default());
    let before = s.link.sim.log.len();
    let e = s
        .move_to(
            &Target {
                x: Some(9999.0),
                ..Target::default()
            },
            1000.0,
        )
        .unwrap_err();
    assert_eq!(e.kind, Kind::Refused);
    assert_eq!(
        s.link
            .sim
            .log
            .iter()
            .filter(|l| l.starts_with("G1 X9999"))
            .count(),
        0
    );
    assert!(s.is_halted());
    assert!(
        s.link.sim.log.len() > before,
        "only the lines of safe_stop were sent"
    );
}

#[test]
fn a_session_that_was_not_connected_sends_nothing() {
    let p = marlin();
    let mut s = session(&p, Faults::default());
    assert_eq!(s.home().unwrap_err().kind, Kind::Refused);
    assert_eq!(
        s.move_to(&Target::default(), 100.0).unwrap_err().kind,
        Kind::Refused
    );
    assert!(s.link.sim.log.is_empty());
}

#[test]
fn after_a_trouble_the_moves_go_slower_and_never_faster_again() {
    let p = marlin();
    // two silent answers to queries make retries; the feeds of the moves after them fall and stay down
    let mut s = ready(
        &p,
        Faults {
            silent_at: Some(4),
            ..Faults::default()
        },
    );
    let feed_of = |log: &[String]| -> Vec<f64> {
        log.iter()
            .filter(|l| l.starts_with("G1 "))
            .map(|l| l.rsplit('F').next().unwrap().parse::<f64>().unwrap())
            .collect()
    };
    s.position().unwrap();
    s.move_to(
        &Target {
            x: Some(1.0),
            ..Target::default()
        },
        6000.0,
    )
    .unwrap();
    s.move_to(
        &Target {
            x: Some(2.0),
            ..Target::default()
        },
        6000.0,
    )
    .unwrap();
    s.set_actuator("pump", true).unwrap();
    s.move_to(
        &Target {
            x: Some(3.0),
            ..Target::default()
        },
        6000.0,
    )
    .unwrap();
    let feeds = feed_of(&s.link.sim.log);
    assert_eq!(feeds.len(), 3);
    assert!(feeds.iter().all(|f| *f <= 6000.0));
    assert!(feeds[0] < 6000.0, "{feeds:?}");
    assert!(feeds.windows(2).all(|w| w[1] <= w[0]), "{feeds:?}");
}

#[test]
fn a_machine_that_is_held_does_not_move_until_a_person_says_so_and_then_only_slowly() {
    use eh3d_phoenix_pnp::safety::{Event, Level};
    let p = marlin();
    let mut s = ready(&p, Faults::default());
    s.event(Event::VacuumLost);
    assert_eq!(s.governor.level(), Level::Hold);
    let e = s
        .move_to(
            &Target {
                x: Some(1.0),
                ..Target::default()
            },
            1000.0,
        )
        .unwrap_err();
    assert_eq!(e.kind, Kind::Held);
    assert!(
        !s.is_halted(),
        "held is not halted: the session can go on once it is released"
    );
    assert_eq!(s.governor.acknowledge().unwrap(), Level::Slow);
    s.move_to(
        &Target {
            x: Some(1.0),
            ..Target::default()
        },
        1000.0,
    )
    .unwrap();
    let f: f64 = s
        .link
        .sim
        .log
        .iter()
        .rev()
        .find(|l| l.starts_with("G1 "))
        .unwrap()
        .rsplit('F')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    assert!(f <= 250.0, "{f}");
}

#[test]
fn a_controller_that_only_says_busy_is_asked_again_and_then_given_up() {
    let p = marlin();
    let mut s = ready(&p, Faults::default());
    // a link that never says ok
    struct Busy(f64);
    impl Link for Busy {
        fn exchange(
            &mut self,
            _: &str,
            _: f64,
        ) -> Result<Vec<String>, eh3d_phoenix_pnp::gcodesim::LinkError> {
            Ok(vec!["echo:busy: processing".into()])
        }
        fn pause(&mut self, s: f64) {
            self.0 += s;
        }
        fn now(&self) -> f64 {
            self.0
        }
    }
    let _ = &mut s;
    let mut b = Session::new(&p, Busy(0.0));
    let e = b.connect().unwrap_err();
    assert_eq!(e.kind, Kind::Timeout);
    assert!(b.link.0 >= 0.5, "it paused between the tries: {}", b.link.0);
}

#[test]
fn a_link_that_closes_stops_the_session() {
    struct Closed;
    impl Link for Closed {
        fn exchange(
            &mut self,
            _: &str,
            _: f64,
        ) -> Result<Vec<String>, eh3d_phoenix_pnp::gcodesim::LinkError> {
            Err(eh3d_phoenix_pnp::gcodesim::LinkError::Closed(
                "unplugged".into(),
            ))
        }
        fn pause(&mut self, _: f64) {}
        fn now(&self) -> f64 {
            0.0
        }
    }
    let mut s = Session::new(&marlin(), Closed);
    assert_eq!(s.connect().unwrap_err().kind, Kind::Link);
    assert!(s.is_halted());
}

#[test]
fn the_same_run_gives_the_same_log_every_time() {
    let run = || {
        let p = marlin();
        let mut s = ready(
            &p,
            Faults {
                homing_jitter: 0.2,
                busy_lines: 1,
                ..Faults::default()
            },
        );
        s.run_placements(&[placement("R1", (-20.0, 10.0), (10.0, 20.0), 0.0)]);
        s.log
            .iter()
            .map(|e| format!("{:.3}|{}|{}", e.at_s, e.line, e.result))
            .collect::<Vec<_>>()
    };
    assert_eq!(run(), run());
}

#[test]
fn strict_mode_stops_at_a_line_it_does_not_know() {
    let strict = DriverProfile::parse(
        &MARLIN.replace("tolerate_unknown_echo = yes", "tolerate_unknown_echo = no"),
    )
    .unwrap();
    let mut s = session(&strict, Faults::default());
    // the answer to M115 holds a FIRMWARE_NAME line that no pattern knows
    assert_eq!(s.connect().unwrap_err().kind, Kind::Link);
    let mut s = session(&marlin(), Faults::default());
    assert!(s.connect().is_ok());
}
