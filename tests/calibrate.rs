//! The calibrations of the camera and of the nozzle: the fits, the files they read, the lines of the configuration they give, the G-code that follows the runout and the two commands.
//! The expected numbers are the ones of the model that made the points.

use eh3d_phoenix_pnp::calibrate::{
    calibrate_camera, fit_runout, offset_of, parse_camera_points, parse_runout_samples,
    CameraPoint, RunoutSample, MAX_POINTS,
};
use eh3d_phoenix_pnp::camera::CameraModel;
use eh3d_phoenix_pnp::config::{MachineConfig, KEYS};
use eh3d_phoenix_pnp::error::{Code, Lang};
use eh3d_phoenix_pnp::sim::Rng;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const MACHINE: &str = include_str!("../examples/machine.cfg");

fn near(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

fn model(units: f64, rot: f64, off: (f64, f64), fx: bool, fy: bool) -> CameraModel {
    CameraModel {
        units_per_px: units,
        offset_x_mm: off.0,
        offset_y_mm: off.1,
        rotation_deg: rot,
        flip_x: fx,
        flip_y: fy,
        lens_scale: 1.0,
        lens_k1: 0.0,
    }
}

/// The sightings a camera with this model gives of a mark seen at nine places of the head.
fn sightings(m: &CameraModel, noise_px: f64, seed: u64) -> Vec<CameraPoint> {
    let mut rng = Rng::new(seed);
    let units = m.units_per_px;
    let mut out = Vec::new();
    for x in [-2.0, 0.0, 2.0] {
        for y in [-1.5, 0.0, 1.5] {
            let (px, py) = m.to_pixels((x, y), units);
            let mut n = || (rng.next_f64() - 0.5) * 2.0 * noise_px;
            out.push(CameraPoint {
                px: px + n(),
                py: py + n(),
                x_mm: x,
                y_mm: y,
            });
        }
    }
    out
}

fn same_model(a: &CameraModel, b: &CameraModel, units: f64) {
    for px in [(0.0, 0.0), (120.0, -40.0), (-300.0, 210.0), (17.0, 99.0)] {
        let (p, q) = (a.to_machine(px, units), b.to_machine(px, units));
        assert!(
            near(p.0, q.0, 1e-6) && near(p.1, q.1, 1e-6),
            "{px:?}: {p:?} {q:?}"
        );
    }
}

#[test]
fn a_camera_with_a_turn_an_offset_and_a_pixel_size_is_found_exactly() {
    let truth = model(0.02, 12.0, (3.5, -2.0), false, false);
    let got = calibrate_camera(&sightings(&truth, 0.0, 1)).unwrap();
    assert_eq!(got.points, 9);
    assert!(
        near(got.model.units_per_px, 0.02, 1e-9),
        "{}",
        got.model.units_per_px
    );
    assert!(near(got.model.rotation_deg, 12.0, 1e-6));
    assert!(near(got.model.offset_x_mm, 3.5, 1e-6) && near(got.model.offset_y_mm, -2.0, 1e-6));
    assert!(!got.model.flip_x && !got.model.flip_y);
    assert!(got.rms_mm < 1e-9 && got.max_mm < 1e-9);
    assert!(got.skew_deg.abs() < 1e-6);
    assert!(near(got.size_x_mm, got.size_y_mm, 1e-9));
    assert_eq!(got.model.lens_scale, 1.0);
    assert_eq!(got.model.lens_k1, 0.0);
}

#[test]
fn a_mirrored_picture_gives_a_model_that_reads_the_same_places() {
    for (fx, fy) in [(true, false), (false, true), (true, true)] {
        for rot in [0.0, 33.0, -120.0, 179.0] {
            let truth = model(0.015, rot, (-1.0, 4.0), fx, fy);
            let got = calibrate_camera(&sightings(&truth, 0.0, 2)).unwrap();
            same_model(&got.model, &truth, 0.015);
            // a mirror on both axes is a turn of half a circle, and a model has at most one mirror
            assert!(!got.model.flip_x, "{fx} {fy}");
            assert_eq!(got.model.flip_y, fx != fy, "{fx} {fy} {rot}");
        }
    }
}

#[test]
fn the_angle_comes_out_in_the_range_minus_180_to_180() {
    for rot in [-179.0, -90.0, 0.0, 90.0, 180.0] {
        let got = calibrate_camera(&sightings(
            &model(0.02, rot, (0.0, 0.0), false, false),
            0.0,
            3,
        ))
        .unwrap();
        let a = got.model.rotation_deg;
        assert!(a > -180.0 && a <= 180.0, "{a}");
        assert!(
            near(a, rot, 1e-6) || near(a.abs(), 180.0, 1e-6),
            "{rot} {a}"
        );
    }
}

#[test]
fn the_lens_scale_is_part_of_the_size_of_a_pixel() {
    let mut truth = model(0.02, 0.0, (0.0, 0.0), false, false);
    truth.lens_scale = 1.02;
    let got = calibrate_camera(&sightings(&truth, 0.0, 4)).unwrap();
    assert!(
        near(got.model.units_per_px, 0.02 * 1.02, 1e-9),
        "{}",
        got.model.units_per_px
    );
}

#[test]
fn small_errors_of_the_sightings_give_small_errors_of_the_model_and_the_fit_says_how_big() {
    let truth = model(0.02, 5.0, (1.0, 1.0), false, false);
    let got = calibrate_camera(&sightings(&truth, 0.5, 5)).unwrap();
    assert!(
        near(got.model.units_per_px, 0.02, 2e-4),
        "{}",
        got.model.units_per_px
    );
    assert!(
        near(got.model.rotation_deg, 5.0, 0.5),
        "{}",
        got.model.rotation_deg
    );
    // half a pixel of error is 0.01 mm at most
    assert!(
        got.rms_mm > 0.0005 && got.max_mm <= 0.02,
        "{} {}",
        got.rms_mm,
        got.max_mm
    );
    assert!(got.max_mm >= got.rms_mm);
    let again = calibrate_camera(&sightings(&truth, 0.5, 5)).unwrap();
    assert_eq!(got, again);
}

#[test]
fn a_pixel_that_is_not_square_is_told_by_the_two_sizes_and_the_skew() {
    // a pixel 2% longer along Y and the second axis of the picture 1 degree away from square
    let (sx, sy, skew) = (0.02, 0.0204, 1.0f64.to_radians());
    let pts: Vec<CameraPoint> = [
        (0.0, 0.0),
        (100.0, 0.0),
        (0.0, 100.0),
        (100.0, 100.0),
        (50.0, 30.0),
    ]
    .iter()
    .map(|&(px, py)| CameraPoint {
        px,
        py,
        x_mm: px * sx + py * sy * skew.sin(),
        y_mm: py * sy * skew.cos(),
    })
    .collect();
    let got = calibrate_camera(&pts).unwrap();
    assert!(near(got.size_x_mm, 0.02, 1e-9), "{}", got.size_x_mm);
    assert!(near(got.size_y_mm, 0.0204, 1e-9), "{}", got.size_y_mm);
    assert!(near(got.skew_deg, -1.0, 1e-6), "{}", got.skew_deg);
    // the model has a square pixel, so the points are not exactly where it puts them
    assert!(got.rms_mm > 0.0);
}

#[test]
fn the_lines_of_the_configuration_are_valid_and_give_the_model_back() {
    let truth = model(0.0173, -37.5, (-8.25, 12.5), false, true);
    let got = calibrate_camera(&sightings(&truth, 0.0, 6)).unwrap();
    let lines = got.config_lines();
    for key in [
        "camera_units_per_px",
        "camera_offset_x_mm",
        "camera_offset_y_mm",
        "camera_rotation_deg",
        "camera_flip_x",
        "camera_flip_y",
    ] {
        assert!(lines.contains(&format!("{key} = ")), "{key}");
        assert!(KEYS.contains(&key));
    }
    let cfg = MachineConfig::parse(&lines).unwrap();
    assert!(near(cfg.camera.units_per_px, 0.0173, 1e-6));
    assert!(near(cfg.camera.rotation_deg, -37.5, 1e-3));
    assert!(near(cfg.camera.offset_x_mm, -8.25, 1e-3));
    assert!(cfg.camera.flip_y && !cfg.camera.flip_x);
    same_model(&cfg.camera, &truth, 0.0173);
}

#[test]
fn what_cannot_be_fitted_is_an_error_with_the_code() {
    let one = CameraPoint {
        px: 1.0,
        py: 2.0,
        x_mm: 0.1,
        y_mm: 0.2,
    };
    let e = calibrate_camera(&[one, one]).unwrap_err();
    assert_eq!(e.code, Code::CalibrationBad);
    assert!(e.detail.contains("at least three"));
    // on one line
    let line: Vec<CameraPoint> = (0..5)
        .map(|i| CameraPoint {
            px: f64::from(i) * 10.0,
            py: f64::from(i) * 5.0,
            x_mm: f64::from(i) * 0.2,
            y_mm: f64::from(i) * 0.1,
        })
        .collect();
    let e = calibrate_camera(&line).unwrap_err();
    assert_eq!(e.code, Code::CalibrationBad);
    assert!(e.detail.contains("do not fix a camera"), "{}", e.detail);
    // a pixel of 20 millimetres, and a nozzle axis 900 millimetres from the middle
    let big = model(20.0, 0.0, (0.0, 0.0), false, false);
    let mut pts = sightings(&model(0.02, 0.0, (0.0, 0.0), false, false), 0.0, 7);
    for p in &mut pts {
        p.x_mm *= 1000.0;
        p.y_mm *= 1000.0;
    }
    let _ = big;
    assert!(calibrate_camera(&pts)
        .unwrap_err()
        .detail
        .contains("camera_units_per_px"));
    let far = sightings(&model(0.02, 0.0, (900.0, 0.0), false, false), 0.0, 8);
    assert!(calibrate_camera(&far)
        .unwrap_err()
        .detail
        .contains("offsets"));
}

#[test]
fn the_file_of_sightings_has_a_header_comments_and_either_delimiter() {
    let a = parse_camera_points("px,py,x_mm,y_mm\n# a comment\n10,20,0.2,0.4\n-10,5,-0.2,0.1\n")
        .unwrap();
    assert_eq!(a.len(), 2);
    assert_eq!((a[1].px, a[1].y_mm), (-10.0, 0.1));
    let b = parse_camera_points("\u{feff}10;20;0.2;0.4\n").unwrap();
    assert_eq!(b.len(), 1);
    assert!(parse_camera_points("").unwrap().is_empty());
}

#[test]
fn a_bad_row_of_a_file_names_its_line() {
    for (text, needle, line) in [
        ("px,py,x,y\n1,2,3\n", "needs 4 numbers", 2),
        ("1,2,3,4\n1,2,x,4\n", "'x' is not a number", 2),
        ("1,2,3,inf\n", "not a usable number", 1),
        ("1,2,3,99999\n", "not a usable number", 1),
        ("1,2,3,4\n\"1,2,3\n", "quote", 2),
    ] {
        let e = parse_camera_points(text).unwrap_err();
        assert_eq!(e.code, Code::CalibrationBad, "{text:?}");
        assert!(e.detail.contains(needle), "{text:?}: {}", e.detail);
        assert!(
            e.detail.starts_with(&format!("line {line}:")),
            "{text:?}: {}",
            e.detail
        );
    }
    let many: String = (0..=MAX_POINTS).map(|i| format!("{i},0,0,0\n")).collect();
    assert!(parse_camera_points(&many)
        .unwrap_err()
        .detail
        .contains("more than 100"));
    let ok: String = (0..MAX_POINTS).map(|i| format!("{i},0,0,0\n")).collect();
    assert_eq!(parse_camera_points(&ok).unwrap().len(), MAX_POINTS);
}

fn readings(
    radius: f64,
    phase: f64,
    axis: (f64, f64),
    angles: &[f64],
    noise: f64,
    seed: u64,
) -> Vec<RunoutSample> {
    let mut rng = Rng::new(seed);
    angles
        .iter()
        .map(|&a| {
            let o = offset_of(radius, phase, a);
            RunoutSample {
                angle_deg: a,
                x_mm: axis.0 + o.0 + (rng.next_f64() - 0.5) * 2.0 * noise,
                y_mm: axis.1 + o.1 + (rng.next_f64() - 0.5) * 2.0 * noise,
            }
        })
        .collect()
}

const SIX: [f64; 6] = [0.0, 60.0, 120.0, 180.0, 240.0, 300.0];

#[test]
fn the_runout_of_a_tip_is_found_exactly() {
    let got = fit_runout(&readings(0.12, 35.0, (100.0, 50.0), &SIX, 0.0, 1)).unwrap();
    assert!(
        near(got.axis.0, 100.0, 1e-9) && near(got.axis.1, 50.0, 1e-9),
        "{:?}",
        got.axis
    );
    assert!(near(got.radius_mm, 0.12, 1e-9), "{}", got.radius_mm);
    assert!(near(got.phase_deg, 35.0, 1e-6), "{}", got.phase_deg);
    assert!(got.rms_mm < 1e-9 && got.max_mm < 1e-9);
    assert_eq!(got.samples, 6);
    // the offset follows the angle
    let o = got.offset(90.0);
    assert!(near(o.0, -0.12 * 35.0f64.to_radians().sin(), 1e-9));
    assert!(near(o.1, 0.12 * 35.0f64.to_radians().cos(), 1e-9));
}

#[test]
fn the_phase_is_found_for_every_quadrant_and_in_the_range() {
    for phase in [-170.0, -90.0, -10.0, 0.0, 45.0, 135.0, 179.0] {
        let got = fit_runout(&readings(0.3, phase, (-20.0, 7.0), &SIX, 0.0, 2)).unwrap();
        assert!(
            near(got.phase_deg, phase, 1e-6),
            "{phase}: {}",
            got.phase_deg
        );
    }
}

#[test]
fn three_readings_that_cover_a_quarter_of_a_turn_are_enough_and_less_is_not() {
    let got = fit_runout(&readings(0.2, 10.0, (0.0, 0.0), &[0.0, 45.0, 90.0], 0.0, 3)).unwrap();
    assert!(near(got.radius_mm, 0.2, 1e-9) && near(got.phase_deg, 10.0, 1e-6));
    let e = fit_runout(&readings(0.2, 10.0, (0.0, 0.0), &[0.0, 30.0, 60.0], 0.0, 3)).unwrap_err();
    assert_eq!(e.code, Code::CalibrationBad);
    assert!(
        e.detail.contains("cover at least 90 degrees"),
        "{}",
        e.detail
    );
    let e = fit_runout(&readings(0.2, 10.0, (0.0, 0.0), &[0.0, 120.0], 0.0, 3)).unwrap_err();
    assert!(e.detail.contains("at least three"));
    // an angle of 370 is the same as 10
    let wrap = fit_runout(&readings(
        0.2,
        10.0,
        (0.0, 0.0),
        &[360.0, 405.0, 450.0, -270.0],
        0.0,
        3,
    ))
    .unwrap();
    assert!(near(wrap.radius_mm, 0.2, 1e-9));
}

#[test]
fn readings_that_do_not_draw_a_circle_are_refused() {
    let same: Vec<RunoutSample> = SIX
        .iter()
        .map(|&a| RunoutSample {
            angle_deg: a,
            x_mm: 5.0,
            y_mm: 5.0,
        })
        .collect();
    assert_eq!(fit_runout(&same).unwrap_err().code, Code::CalibrationBad);
    let line: Vec<RunoutSample> = SIX
        .iter()
        .map(|&a| RunoutSample {
            angle_deg: a,
            x_mm: a / 100.0,
            y_mm: a / 50.0,
        })
        .collect();
    let e = fit_runout(&line).unwrap_err();
    assert!(e.detail.contains("one line or one point"), "{}", e.detail);
}

#[test]
fn noise_in_the_readings_shows_in_the_residuals() {
    let got = fit_runout(&readings(
        0.15,
        80.0,
        (10.0, 10.0),
        &[0.0, 30.0, 70.0, 120.0, 170.0, 220.0, 260.0, 300.0, 340.0],
        0.01,
        9,
    ))
    .unwrap();
    assert!(near(got.radius_mm, 0.15, 0.02), "{}", got.radius_mm);
    assert!(near(got.phase_deg, 80.0, 8.0), "{}", got.phase_deg);
    assert!(got.rms_mm > 0.0 && got.max_mm >= got.rms_mm && got.max_mm < 0.03);
}

#[test]
fn the_file_of_readings_and_its_lines_of_the_configuration() {
    let r = parse_runout_samples("angle_deg,x_mm,y_mm\n0,100.1,50\n90,100,50.1\n180,99.9,50\n")
        .unwrap();
    assert_eq!(r.len(), 3);
    let got = fit_runout(&r).unwrap();
    let lines = got.config_lines();
    for key in ["nozzle_runout_mm", "nozzle_runout_deg"] {
        assert!(KEYS.contains(&key));
        assert!(lines.contains(&format!("{key} = ")));
    }
    let cfg = MachineConfig::parse(&lines).unwrap();
    assert!(near(cfg.nozzle_runout_mm, got.radius_mm, 1e-4));
    assert!(near(cfg.nozzle_runout_deg, got.phase_deg, 1e-3));
    assert_eq!(
        parse_runout_samples("1,2\n").unwrap_err().code,
        Code::CalibrationBad
    );
}

#[test]
fn the_settings_of_the_runout_have_their_ranges_and_the_example_is_the_default() {
    let e = MachineConfig::parse(MACHINE).unwrap();
    let d = MachineConfig::default();
    assert_eq!(
        (e.nozzle_runout_mm, e.nozzle_runout_deg),
        (d.nozzle_runout_mm, d.nozzle_runout_deg)
    );
    assert_eq!((d.nozzle_runout_mm, d.nozzle_runout_deg), (0.0, 0.0));
    for (key, ok, bad) in [
        ("nozzle_runout_mm", ["0", "5"], ["-0.1", "5.1"]),
        ("nozzle_runout_deg", ["-180", "180"], ["-180.1", "180.1"]),
    ] {
        for v in ok {
            assert!(
                MachineConfig::parse(&format!("{key} = {v}")).is_ok(),
                "{key} {v}"
            );
        }
        for v in bad {
            let err = MachineConfig::parse(&format!("{key} = {v}")).unwrap_err();
            assert_eq!(err.code, Code::ConfigOutOfRange, "{key} {v}");
        }
    }
}

#[test]
fn the_code_has_its_meaning_in_the_eight_languages() {
    assert_eq!(Code::CalibrationBad.number(), 58);
    assert_eq!(Code::CalibrationBad.label(), "PHOENIX-PNP-058");
    let mut seen = Vec::new();
    for lang in Lang::ALL {
        let m = Code::CalibrationBad.message(lang);
        assert!(!m.is_empty() && !seen.contains(&m), "{m}");
        seen.push(m);
    }
}

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

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "phoenix_pnp_calibrate_{name}_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn p(path: &Path) -> &str {
    path.to_str().unwrap()
}

#[test]
fn camera_calibrate_prints_the_fit_and_the_lines_to_use() {
    let dir = scratch("camera");
    let truth = model(0.02, 12.0, (3.5, -2.0), false, false);
    let mut file = String::from("px,py,x_mm,y_mm\n");
    for s in sightings(&truth, 0.0, 1) {
        file.push_str(&format!("{},{},{},{}\n", s.px, s.py, s.x_mm, s.y_mm));
    }
    let path = dir.join("sightings.csv");
    std::fs::write(&path, file).unwrap();
    let o = run(&["camera-calibrate", p(&path)]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    let out = text(&o.stdout);
    assert!(
        out.starts_with("9 sightings: a pixel is 0.020000 mm along X"),
        "{out}"
    );
    assert!(out.contains("camera_units_per_px = 0.020000\n"), "{out}");
    assert!(out.contains("camera_rotation_deg = 12.0000\n"), "{out}");
    assert!(out.contains("camera_flip_y = off\n"), "{out}");
    let cfg = MachineConfig::parse(&out[out.find("camera_units").unwrap()..]).unwrap();
    assert!(near(cfg.camera.offset_x_mm, 3.5, 1e-3));
    // too few points, in Spanish
    std::fs::write(&path, "1,2,3,4\n").unwrap();
    let o = run(&["camera-calibrate", p(&path), "--lang", "es"]);
    assert!(!o.status.success());
    assert!(
        text(&o.stderr).contains("PHOENIX-PNP-058: Los puntos de calibración no son válidos"),
        "{}",
        text(&o.stderr)
    );
    assert!(!run(&["camera-calibrate"]).status.success());
    assert!(!run(&["camera-calibrate", p(&path), "--oops"])
        .status
        .success());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn nozzle_calibrate_prints_the_fit_and_the_lines_to_use() {
    let dir = scratch("nozzle");
    let mut file = String::from("angle_deg,x_mm,y_mm\n");
    for s in readings(0.12, 35.0, (100.0, 50.0), &SIX, 0.0, 1) {
        file.push_str(&format!("{},{},{}\n", s.angle_deg, s.x_mm, s.y_mm));
    }
    let path = dir.join("readings.csv");
    std::fs::write(&path, file).unwrap();
    let o = run(&["nozzle-calibrate", p(&path)]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    let out = text(&o.stdout);
    assert!(
        out.starts_with("6 readings: the axis is at X 100.0000 Y 50.0000"),
        "{out}"
    );
    assert!(out.contains("nozzle_runout_mm = 0.1200\n"), "{out}");
    assert!(out.contains("nozzle_runout_deg = 35.000\n"), "{out}");
    // a runout above what the setting accepts is refused, not written
    let mut far = String::new();
    for s in readings(8.0, 0.0, (0.0, 0.0), &SIX, 0.0, 1) {
        far.push_str(&format!("{},{},{}\n", s.angle_deg, s.x_mm, s.y_mm));
    }
    std::fs::write(&path, far).unwrap();
    let o = run(&["nozzle-calibrate", p(&path)]);
    assert!(!o.status.success());
    assert!(
        text(&o.stderr).contains("above the 5 mm"),
        "{}",
        text(&o.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

fn places(gcode: &str) -> Vec<(String, f64, f64)> {
    let mut out = Vec::new();
    let mut lines = gcode.lines();
    while let Some(l) = lines.next() {
        if let Some(rest) = l.strip_prefix("; place ") {
            let g = lines.next().unwrap();
            let mut it = g.split_whitespace();
            it.next();
            let x: f64 = it.next().unwrap()[1..].parse().unwrap();
            let y: f64 = it.next().unwrap()[1..].parse().unwrap();
            out.push((rest.split(' ').next().unwrap().to_string(), x, y));
        }
    }
    out
}

#[test]
fn the_gcode_moves_each_place_by_the_opposite_of_the_runout_at_its_angle() {
    let dir = scratch("gcode");
    let plain = dir.join("plain.nc");
    let moved = dir.join("moved.nc");
    let cfg = dir.join("runout.cfg");
    std::fs::write(&cfg, "nozzle_runout_mm = 0.2\nnozzle_runout_deg = 90\n").unwrap();
    let base = ["schedule", "examples/board.pos", "examples/feeders.csv"];
    let o = run(&[
        base[0],
        base[1],
        base[2],
        p(&dir.join("a.json")),
        "--gcode",
        p(&plain),
    ]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    let o = run(&[
        base[0],
        base[1],
        base[2],
        p(&dir.join("b.json")),
        "--gcode",
        p(&moved),
        "--config",
        p(&cfg),
    ]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    let (a, b) = (
        std::fs::read_to_string(&plain).unwrap(),
        std::fs::read_to_string(&moved).unwrap(),
    );
    assert!(!a.contains("nozzle tip"), "no runout, no note");
    assert!(b.contains("; the places are moved for a nozzle tip 0.2000 mm from its axis towards 90.000 degrees at 0 degrees"));
    let (pa, pb) = (places(&a), places(&b));
    assert_eq!(pa.len(), pb.len());
    assert!(!pa.is_empty());
    let mut turned = 0;
    for ((ra, xa, ya), (rb, xb, yb)) in pa.iter().zip(&pb) {
        assert_eq!(ra, rb);
        // the tip of a nozzle at angle A is at 0.2 * (cos(90 + A), sin(90 + A)): the place moves by its opposite
        let line = a
            .lines()
            .find(|l| l.starts_with(&format!("; place {ra} turned ")))
            .unwrap();
        let angle: f64 = line.split("turned ").nth(1).unwrap().parse().unwrap();
        let o = offset_of(0.2, 90.0, angle);
        assert!(
            near(xa - xb, o.0, 0.0011) && near(ya - yb, o.1, 0.0011),
            "{ra}: {} {} vs {o:?}",
            xa - xb,
            ya - yb
        );
        if angle != 0.0 {
            turned += 1;
        }
    }
    assert!(turned > 0, "the example has parts that are turned");
    // with the setting at 0 the file is the one it always was
    std::fs::write(&cfg, "nozzle_runout_mm = 0\nnozzle_runout_deg = 45\n").unwrap();
    let same = dir.join("same.nc");
    let o = run(&[
        base[0],
        base[1],
        base[2],
        p(&dir.join("c.json")),
        "--gcode",
        p(&same),
        "--config",
        p(&cfg),
    ]);
    assert!(o.status.success());
    assert_eq!(std::fs::read_to_string(&same).unwrap(), a);
    let _ = std::fs::remove_dir_all(&dir);
}
