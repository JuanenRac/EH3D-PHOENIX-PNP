//! The calibration of the camera tried on a camera that is made up: the sightings, the fit, how far it is from the truth, and `camera-selftest`.

use eh3d_phoenix_pnp::calibrate::{calibrate_camera, parse_camera_points};
use eh3d_phoenix_pnp::calibsim::{selftest, sightings, Plan, DEFAULT_UNITS_MM, MAX_POINTS};
use eh3d_phoenix_pnp::camera::CameraModel;
use std::path::PathBuf;
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_phoenix-pnp");

fn run(args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap()
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "phoenix_pnp_calibsim_{name}_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn camera() -> CameraModel {
    CameraModel {
        units_per_px: 0.015,
        offset_x_mm: 12.5,
        offset_y_mm: -4.0,
        rotation_deg: 30.0,
        ..CameraModel::default()
    }
}

#[test]
fn a_reading_with_no_noise_gives_back_the_camera_that_was_made_up() {
    let plan = Plan {
        noise_px: 0.0,
        ..Plan::default()
    };
    let t = selftest(&camera(), &plan).unwrap();
    assert!(t.max_mm < 1e-6, "{}", t.max_mm);
    let f = &t.found.model;
    assert!((f.units_per_px - 0.015).abs() < 1e-9);
    assert!((f.offset_x_mm - 12.5).abs() < 1e-6 && (f.offset_y_mm + 4.0).abs() < 1e-6);
    assert!((f.rotation_deg - 30.0).abs() < 1e-6);
    assert!(t.within(1e-6));
    assert_eq!(t.units_mm, 0.015);
}

#[test]
fn a_mirrored_camera_is_found_as_the_same_camera_whichever_mirror_is_said() {
    for (fx, fy) in [(true, false), (false, true), (true, true)] {
        let truth = CameraModel {
            flip_x: fx,
            flip_y: fy,
            ..camera()
        };
        let t = selftest(
            &truth,
            &Plan {
                noise_px: 0.0,
                ..Plan::default()
            },
        )
        .unwrap();
        // a mirror left to right with a half turn is a mirror upside down: the places agree, the words do not have to
        assert!(t.max_mm < 1e-6, "{fx} {fy}: {}", t.max_mm);
    }
}

#[test]
fn a_camera_with_no_size_of_pixel_is_tried_with_the_size_of_the_pipeline() {
    let t = selftest(&CameraModel::default(), &Plan::default()).unwrap();
    assert_eq!(t.units_mm, DEFAULT_UNITS_MM);
    assert!(t.max_mm < 0.05);
}

#[test]
fn the_same_seed_gives_the_same_sightings_and_another_seed_other_ones() {
    let plan = Plan::default();
    let a = sightings(&camera(), 0.015, &plan);
    let b = sightings(&camera(), 0.015, &plan);
    let c = sightings(
        &camera(),
        0.015,
        &Plan {
            seed: 2,
            ..plan.clone()
        },
    );
    assert_eq!(a, b);
    assert_ne!(a, c);
    assert_eq!(a.len(), 12);
    // the marks are seen within the picture that was asked for
    for p in &a {
        assert!(p.x_mm.abs() <= 300.0 * 0.015 && p.y_mm.abs() <= 300.0 * 0.015);
    }
}

#[test]
fn more_sightings_do_not_make_the_camera_worse() {
    let noisy = |points: usize, seed: u64| {
        selftest(
            &camera(),
            &Plan {
                points,
                noise_px: 1.5,
                seed,
                ..Plan::default()
            },
        )
        .unwrap()
        .rms_mm
    };
    // the average of many seeds: a lot of sightings beat a few
    let few: f64 = (3..9).map(|seed| noisy(4, seed)).sum::<f64>();
    let many: f64 = (3..9).map(|seed| noisy(100, seed)).sum::<f64>();
    assert!(many < few, "{many} {few}");
}

#[test]
fn fewer_than_three_sightings_or_too_many_are_not_a_calibration() {
    let few = selftest(
        &camera(),
        &Plan {
            points: 2,
            ..Plan::default()
        },
    );
    assert!(few.is_err());
    assert_eq!(MAX_POINTS, 100);
    assert!(selftest(
        &camera(),
        &Plan {
            points: MAX_POINTS,
            ..Plan::default()
        }
    )
    .is_ok());
    assert!(selftest(
        &camera(),
        &Plan {
            points: MAX_POINTS + 1,
            ..Plan::default()
        }
    )
    .is_err());
}

#[test]
fn the_sightings_are_the_file_that_camera_calibrate_reads() {
    let t = selftest(&camera(), &Plan::default()).unwrap();
    let again = calibrate_camera(&parse_camera_points(&t.csv()).unwrap()).unwrap();
    // the file keeps four digits of the pixels, so the fit is the same to the digits written
    assert!((again.model.units_per_px - t.found.model.units_per_px).abs() < 1e-6);
    assert!((again.model.offset_x_mm - t.found.model.offset_x_mm).abs() < 1e-3);
    assert!(t.csv().starts_with("px,py,x_mm,y_mm\n"));
    assert_eq!(t.csv().lines().count(), 13);
}

#[test]
fn the_report_says_what_was_made_up_what_was_found_and_the_error() {
    let t = selftest(&camera(), &Plan::default()).unwrap();
    let text = t.text(0.05);
    assert!(
        text.contains("made up: pixel 0.015000 mm, offset 12.5000 -4.0000 mm, turn 30.000 deg"),
        "{text}"
    );
    assert!(text.contains("found:   pixel 0.0149"), "{text}");
    assert!(text.contains("within 0.05 mm"), "{text}");
    assert!(t.text(1e-9).contains("over 0.000000001 mm"));
}

#[test]
fn the_command_uses_the_camera_of_the_settings_and_the_seed_of_the_settings() {
    let dir = temp("settings");
    let cfg = dir.join("machine.cfg");
    std::fs::write(
        &cfg,
        "camera_units_per_px = 0.015\ncamera_offset_x_mm = 12.5\ncamera_offset_y_mm = -4\ncamera_rotation_deg = 30\nsim_seed = 7\n",
    )
    .unwrap();
    let c = cfg.to_string_lossy().into_owned();
    let o = run(&["camera-selftest", "--config", &c]);
    assert!(o.status.success(), "{}", err(&o));
    let text = out(&o);
    assert!(
        text.contains("made up: pixel 0.015000 mm, offset 12.5000 -4.0000 mm, turn 30.000 deg"),
        "{text}"
    );
    // the seed of the settings is the default, and --seed takes its place
    let same = out(&run(&["camera-selftest", "--config", &c, "--seed", "7"]));
    assert_eq!(same, text);
    let other = out(&run(&["camera-selftest", "--config", &c, "--seed", "8"]));
    assert_ne!(other, text);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_command_fails_when_the_error_is_over_the_tolerance() {
    let o = run(&[
        "camera-selftest",
        "--noise",
        "20",
        "--points",
        "4",
        "--tol",
        "0.0001",
    ]);
    assert_eq!(o.status.code(), Some(2));
    assert!(err(&o).contains("over 0.0001 mm"), "{}", err(&o));
    assert!(out(&o).contains("over 0.0001 mm"));
    let ok = run(&["camera-selftest", "--noise", "0", "--tol", "0.0001"]);
    assert!(ok.status.success(), "{}", err(&ok));
}

#[test]
fn the_command_writes_the_sightings_and_never_over_a_file() {
    let dir = temp("out");
    let file = dir.join("sightings.csv").to_string_lossy().into_owned();
    let o = run(&["camera-selftest", "--out", &file, "--points", "20"]);
    assert!(o.status.success(), "{}", err(&o));
    assert!(out(&o).contains(&format!("written to {file}")));
    let again = run(&["camera-selftest", "--out", &file]);
    assert_eq!(again.status.code(), Some(1));
    assert!(err(&again).contains("PHOENIX-PNP-040"), "{}", err(&again));
    // the file is what camera-calibrate reads
    let fit = run(&["camera-calibrate", &file]);
    assert!(fit.status.success(), "{}", err(&fit));
    assert!(out(&fit).contains("20 sightings"), "{}", out(&fit));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_options_of_the_command_are_checked() {
    for args in [
        vec!["--points", "2"],
        vec!["--points", "101"],
        vec!["--points", "x"],
        vec!["--noise", "-1"],
        vec!["--noise", "51"],
        vec!["--field", "9"],
        vec!["--field", "5001"],
        vec!["--seed", "-1"],
        vec!["--seed", "1.5"],
        vec!["--tol", "0"],
        vec!["--tol", "11"],
        vec!["extra"],
        vec!["--nope"],
    ] {
        let mut all = vec!["camera-selftest"];
        all.extend(args.iter());
        let o = run(&all);
        assert_eq!(o.status.code(), Some(1), "{args:?}");
        assert!(err(&o).starts_with("error:"), "{args:?}: {}", err(&o));
    }
}
