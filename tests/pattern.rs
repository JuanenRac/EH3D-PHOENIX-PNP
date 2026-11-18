//! The calibration of the camera from a pattern on a grid: the file of points, the fit of the size of a pixel and of the bending of the lens, the check of the residual, the settings, the
//! command and the tool of the window. The expected numbers are the ones of the model that made the points.

use eh3d_phoenix_pnp::camera::CameraModel;
use eh3d_phoenix_pnp::config::{MachineConfig, KEYS};
use eh3d_phoenix_pnp::error::{Code, Lang};
use eh3d_phoenix_pnp::gui::toolbox::{apply_pattern, pattern_text, Tool, Toolbox};
use eh3d_phoenix_pnp::gui::workspace::Workspace;
use eh3d_phoenix_pnp::pattern::{calibrate_pattern, parse_pattern, PatternPoint, MIN_POINTS};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const MACHINE: &str = include_str!("../examples/machine.cfg");
const EXAMPLE: &str = include_str!("../examples/camera_pattern.csv");

fn near(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

struct Truth {
    size: f64,
    k1: f64,
    turn_deg: f64,
    shift: (f64, f64),
    mirrored: bool,
    noise_px: f64,
}

impl Default for Truth {
    fn default() -> Self {
        Truth {
            size: 0.02,
            k1: 0.002,
            turn_deg: 3.0,
            shift: (0.1, -0.05),
            mirrored: false,
            noise_px: 0.0,
        }
    }
}

/// The points that a camera with this lens sees of a grid of `cols` by `rows` points centred on the middle, a millimetre apart.
fn grid(t: &Truth, cols: i32, rows: i32, seed: u64) -> Vec<PatternPoint> {
    let mut state = seed;
    let mut noise = move || {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((state >> 33) as f64 / (1u64 << 31) as f64) * 2.0 - 1.0
    };
    let (s, c) = t.turn_deg.to_radians().sin_cos();
    let mut out = Vec::new();
    for r in -(rows / 2)..=(rows / 2) {
        for q in -(cols / 2)..=(cols / 2) {
            let (gx, gy) = (q as f64, if t.mirrored { -(r as f64) } else { r as f64 });
            let (dx, dy) = (c * gx - s * gy + t.shift.0, s * gx + c * gy + t.shift.1);
            let bend = 1.0 + t.k1 * (dx * dx + dy * dy);
            out.push(PatternPoint {
                col: q as f64,
                row: r as f64,
                px: dx * bend / t.size + noise() * t.noise_px,
                py: dy * bend / t.size + noise() * t.noise_px,
            });
        }
    }
    out
}

fn to_file(points: &[PatternPoint]) -> String {
    let mut s = String::from("col,row,px,py\n");
    for p in points {
        s.push_str(&format!("{},{},{},{}\n", p.col, p.row, p.px, p.py));
    }
    s
}

// ---- the file

#[test]
fn a_file_of_points_is_read_with_its_header_comments_and_line_endings() {
    let p = parse_pattern("# a note\r\ncol,row,px,py\r\n0,0,1.5,-2\r\n1,0,10,-2\r\n").unwrap();
    assert_eq!(p.len(), 2);
    assert_eq!(
        p[1],
        PatternPoint {
            col: 1.0,
            row: 0.0,
            px: 10.0,
            py: -2.0
        }
    );
    // no header, a byte order mark, semicolons
    assert_eq!(parse_pattern("\u{feff}0;0;1;2\n").unwrap().len(), 1);
    // the example of the repository, whatever the line ending of the checkout
    let e = parse_pattern(EXAMPLE).unwrap();
    assert_eq!(e.len(), 35);
    assert!(near(e[0].col, -3.0, 0.0) && near(e[0].row, -2.0, 0.0));
}

#[test]
fn a_file_that_is_not_one_is_refused_with_its_code_and_its_line() {
    for (text, part) in [
        ("0,0,1\n", "needs 4 numbers"),
        ("0,0,1,x\n", "'x' is not a number"),
        ("0,0,1,2\n0,0,3,4\n", "listed twice"),
        ("0,0,1,99999999\n", "not a usable number"),
        ("0,0,1,inf\n", "not a usable number"),
    ] {
        let e = parse_pattern(text).unwrap_err();
        assert_eq!(e.code, Code::CalibrationBad, "{text}");
        assert!(
            e.detail.contains(part) && e.detail.contains("line"),
            "{}",
            e.detail
        );
    }
    let mut many = String::new();
    for i in 0..=100 {
        many.push_str(&format!("{i},0,{i},0\n"));
    }
    assert!(parse_pattern(&many)
        .unwrap_err()
        .detail
        .contains("more than"));
    assert!(parse_pattern("").unwrap().is_empty());
    assert!(parse_pattern("col,row,px,py\n").unwrap().is_empty());
}

// ---- the fit

#[test]
fn the_example_gives_the_size_of_a_pixel_and_the_bending_that_made_it() {
    let f = calibrate_pattern(&parse_pattern(EXAMPLE).unwrap(), 1.0).unwrap();
    assert!(near(f.units_per_px, 0.02, 1e-4), "{}", f.units_per_px);
    assert!(near(f.lens_k1, 0.002, 2e-4), "{}", f.lens_k1);
    assert!(
        near(f.pattern_turn_deg, 3.0, 0.05),
        "{}",
        f.pattern_turn_deg
    );
    assert!(!f.mirrored && f.skew_deg.abs() < 0.1, "{}", f.skew_deg);
    assert_eq!(f.points, 35);
    // the bending is worth something: without it the points are farther from the model
    assert!(f.rms_mm < 0.005 && f.rms_flat_mm > 3.0 * f.rms_mm, "{f:?}");
    assert!(f.max_mm <= f.max_flat_mm);
    assert!(f.within(0.05) && f.warning(0.05).is_none());
    assert!(f.radius_mm > 3.0);
    let lines = f.config_lines();
    assert!(lines.starts_with("camera_units_per_px = 0.0"), "{lines}");
    let cfg = MachineConfig::parse(&lines).unwrap();
    assert!(near(cfg.camera.lens_k1, f.lens_k1, 1e-6));
    let s = f.summary();
    assert!(s.starts_with("35 points: a pixel is 0.0"), "{s}");
}

#[test]
fn the_fit_finds_the_truth_for_other_lenses_turns_and_sizes() {
    for (i, t) in [
        Truth {
            k1: 0.0,
            ..Truth::default()
        },
        Truth {
            k1: -0.003,
            turn_deg: -40.0,
            ..Truth::default()
        },
        Truth {
            size: 0.011,
            k1: 0.0015,
            turn_deg: 90.0,
            shift: (-0.4, 0.3),
            ..Truth::default()
        },
        Truth {
            size: 0.04,
            k1: 0.004,
            turn_deg: 170.0,
            shift: (0.0, 0.0),
            ..Truth::default()
        },
    ]
    .iter()
    .enumerate()
    {
        let f = calibrate_pattern(&grid(t, 9, 7, 1), 1.0).unwrap();
        assert!(
            near(f.units_per_px, t.size, t.size * 1e-3),
            "{i}: {}",
            f.units_per_px
        );
        assert!(near(f.lens_k1, t.k1, 1e-4), "{i}: {}", f.lens_k1);
        assert!(f.rms_mm < 1e-4, "{i}: {}", f.rms_mm);
    }
}

#[test]
fn a_pitch_other_than_one_scales_the_size_of_a_pixel_and_not_the_bending_per_square_mm() {
    let t = Truth::default();
    let points = grid(&t, 7, 5, 2);
    let one = calibrate_pattern(&points, 1.0).unwrap();
    let half = calibrate_pattern(&points, 0.5).unwrap();
    // half the pitch: the same pixels cover half the distance
    assert!(
        near(half.units_per_px, one.units_per_px / 2.0, 1e-5),
        "{half:?}"
    );
}

#[test]
fn noise_in_the_points_is_averaged_out_and_stays_under_the_limit() {
    let t = Truth {
        noise_px: 0.3,
        ..Truth::default()
    };
    let f = calibrate_pattern(&grid(&t, 11, 9, 7), 1.0).unwrap();
    assert!(near(f.units_per_px, 0.02, 2e-4), "{}", f.units_per_px);
    assert!(near(f.lens_k1, 0.002, 5e-4), "{}", f.lens_k1);
    // a reading is 0.3 px off at most, 0.006 mm: the residual says so
    assert!(f.rms_px > 0.05 && f.rms_px < 0.4, "{}", f.rms_px);
    assert!(f.max_mm < 0.02);
}

#[test]
fn a_mirrored_grid_is_told_and_fitted_all_the_same() {
    let t = Truth {
        mirrored: true,
        ..Truth::default()
    };
    let f = calibrate_pattern(&grid(&t, 7, 5, 3), 1.0).unwrap();
    assert!(f.mirrored);
    assert!(near(f.units_per_px, 0.02, 1e-4) && near(f.lens_k1, 0.002, 2e-4));
    assert!(f.rms_mm < 1e-4);
    assert!(f.summary().contains("and mirrored"));
}

#[test]
fn one_point_read_in_the_wrong_place_is_found_and_the_calibration_is_warned_about() {
    let mut points = grid(&Truth::default(), 9, 7, 4);
    let i = 20;
    points[i].px += 12.0;
    points[i].py -= 7.0;
    let f = calibrate_pattern(&points, 1.0).unwrap();
    assert!(f.max_mm > 0.05, "{}", f.max_mm);
    let w = f.warning(0.05).unwrap();
    assert!(
        w.contains(&format!("{},{}", points[i].col, points[i].row))
            && w.contains("camera_pattern_max_residual_mm"),
        "{w}"
    );
    assert!(!f.within(0.05));
    assert_eq!(f.worst, (points[i].col, points[i].row));
}

#[test]
fn a_pattern_that_fixes_nothing_is_refused() {
    let t = Truth::default();
    let all = grid(&t, 7, 5, 5);
    // too few points
    let few = &all[..MIN_POINTS - 1];
    let e = calibrate_pattern(few, 1.0).unwrap_err();
    assert_eq!(e.code, Code::CalibrationBad);
    assert!(e.detail.contains("at least 6"), "{}", e.detail);
    // all on a line
    let line: Vec<PatternPoint> = all.iter().filter(|p| p.row == 0.0).cloned().collect();
    assert!(line.len() >= MIN_POINTS);
    let e = calibrate_pattern(&line, 1.0).unwrap_err();
    assert!(e.detail.contains("do not fix a pattern"), "{}", e.detail);
    // a pitch outside the range
    for pitch in [0.0, 0.009, 100.1, f64::NAN, f64::INFINITY] {
        let e = calibrate_pattern(&all, pitch).unwrap_err();
        assert!(
            e.detail.contains("camera_pattern_pitch_mm"),
            "{pitch}: {}",
            e.detail
        );
    }
    // a lens that bends the picture more than the setting takes
    let wild = Truth {
        k1: 0.05,
        ..Truth::default()
    };
    let e = calibrate_pattern(&grid(&wild, 7, 5, 5), 1.0).unwrap_err();
    assert!(e.detail.contains("camera_lens_k1"), "{}", e.detail);
    // a pixel as large as a metre
    let huge = Truth {
        size: 50.0,
        k1: 0.0,
        shift: (0.0, 0.0),
        ..Truth::default()
    };
    let e = calibrate_pattern(&grid(&huge, 7, 5, 5), 1.0).unwrap_err();
    assert!(e.detail.contains("camera_units_per_px"), "{}", e.detail);
}

#[test]
fn damaged_points_never_panic() {
    let valid = grid(&Truth::default(), 7, 5, 6);
    let text = to_file(&valid);
    for cut in (0..text.len()).step_by(7) {
        if text.is_char_boundary(cut) {
            if let Ok(points) = parse_pattern(&text[..cut]) {
                let _ = calibrate_pattern(&points, 1.0);
            }
        }
    }
    // the same point many times at different places, and points that coincide on the picture
    let mut crowded: Vec<PatternPoint> = (0..8)
        .map(|i| PatternPoint {
            col: i as f64,
            row: 0.0,
            px: 1.0,
            py: 1.0,
        })
        .collect();
    let _ = calibrate_pattern(&crowded, 1.0);
    crowded[0].px = f64::MAX;
    let _ = calibrate_pattern(&crowded, 1.0);
    let _ = calibrate_pattern(&[], 1.0);
}

// ---- the settings

#[test]
fn the_settings_of_the_pattern_have_defaults_ranges_and_a_line_in_the_example() {
    let d = MachineConfig::default();
    assert_eq!(d.camera_pattern_pitch_mm, 1.0);
    assert_eq!(d.camera_pattern_max_residual_mm, 0.05);
    let e = MachineConfig::parse(MACHINE).unwrap();
    assert_eq!(e.camera_pattern_pitch_mm, d.camera_pattern_pitch_mm);
    assert_eq!(
        e.camera_pattern_max_residual_mm,
        d.camera_pattern_max_residual_mm
    );
    for key in ["camera_pattern_pitch_mm", "camera_pattern_max_residual_mm"] {
        assert!(KEYS.contains(&key));
        assert!(MACHINE.contains(&format!("\n{key} = ")));
    }
    for (key, ok, bad) in [
        (
            "camera_pattern_pitch_mm",
            ["0.01", "100"],
            ["0.009", "100.1"],
        ),
        (
            "camera_pattern_max_residual_mm",
            ["0.0001", "5"],
            ["0.00009", "5.1"],
        ),
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

// ---- the command

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
    let dir =
        std::env::temp_dir().join(format!("phoenix_pnp_pattern_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn p(path: &Path) -> &str {
    path.to_str().unwrap()
}

#[test]
fn camera_pattern_prints_the_fit_and_the_lines_to_use() {
    let o = run(&["camera-pattern", "examples/camera_pattern.csv"]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    let out = text(&o.stdout);
    assert!(out.starts_with("35 points: a pixel is 0.0"), "{out}");
    assert!(out.contains("camera_units_per_px = 0.0"), "{out}");
    let cfg = MachineConfig::parse(&out[out.find("camera_units").unwrap()..]).unwrap();
    assert!(near(cfg.camera.units_per_px, 0.02, 1e-4));
    assert!(near(cfg.camera.lens_k1, 0.002, 2e-4));
}

#[test]
fn camera_pattern_takes_the_pitch_and_the_limit_from_the_options_and_the_configuration() {
    let dir = scratch("cli");
    // the pitch of the option wins over the one of the configuration
    let cfg = dir.join("machine.cfg");
    std::fs::write(&cfg, "camera_pattern_pitch_mm = 2\n").unwrap();
    let a = run(&[
        "camera-pattern",
        "examples/camera_pattern.csv",
        "--config",
        p(&cfg),
    ]);
    let b = run(&[
        "camera-pattern",
        "examples/camera_pattern.csv",
        "--pitch",
        "1",
    ]);
    let c = run(&[
        "camera-pattern",
        "examples/camera_pattern.csv",
        "--config",
        p(&cfg),
        "--pitch",
        "1",
    ]);
    assert!(a.status.success() && b.status.success() && c.status.success());
    assert!(
        text(&a.stdout).contains("a pixel is 0.039"),
        "{}",
        text(&a.stdout)
    );
    assert_eq!(text(&b.stdout), text(&c.stdout));
    // a limit that the example cannot keep: the lines are printed and the exit code is 1
    std::fs::write(&cfg, "camera_pattern_max_residual_mm = 0.0001\n").unwrap();
    let o = run(&[
        "camera-pattern",
        "examples/camera_pattern.csv",
        "--config",
        p(&cfg),
    ]);
    assert!(!o.status.success());
    assert!(text(&o.stdout).contains("camera_lens_k1 = "));
    assert!(
        text(&o.stderr).contains("camera_pattern_max_residual_mm"),
        "{}",
        text(&o.stderr)
    );
    // mistakes: no file, an option that does not exist, a pitch that is not a number, a file that is not one, in Spanish
    assert!(!run(&["camera-pattern"]).status.success());
    assert!(
        !run(&["camera-pattern", "examples/camera_pattern.csv", "--oops"])
            .status
            .success()
    );
    assert!(!run(&[
        "camera-pattern",
        "examples/camera_pattern.csv",
        "--pitch",
        "x"
    ])
    .status
    .success());
    let bad = dir.join("bad.csv");
    std::fs::write(&bad, "1,2,3,4\n").unwrap();
    let o = run(&["camera-pattern", p(&bad), "--lang", "es"]);
    assert!(!o.status.success());
    assert!(
        text(&o.stderr).contains("PHOENIX-PNP-058: Los puntos de calibración no son válidos"),
        "{}",
        text(&o.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- the tool of the window

#[test]
fn the_tool_of_the_window_fits_applies_and_writes_the_lines() {
    let cfg = MachineConfig::default();
    let (found, info, lines) = pattern_text(EXAMPLE, &cfg, Lang::En).unwrap();
    assert!(
        info.starts_with("35 points:") && !info.contains("warning"),
        "{info}"
    );
    assert!(lines.contains("camera_lens_k1"));
    // applying puts the size of a pixel and the bending in the camera and leaves the rest
    let mut machine = MachineConfig {
        camera: CameraModel {
            rotation_deg: 12.0,
            offset_x_mm: 1.5,
            ..CameraModel::default()
        },
        ..MachineConfig::default()
    };
    apply_pattern(&mut machine, &found);
    assert!(near(machine.camera.units_per_px, 0.02, 1e-4));
    assert!(near(machine.camera.lens_k1, 0.002, 2e-4));
    assert_eq!(
        (machine.camera.rotation_deg, machine.camera.offset_x_mm),
        (12.0, 1.5)
    );
    // the tool
    let ws = Workspace::new(Lang::En);
    let dir = scratch("tool");
    let mut tb = Toolbox::new(&cfg);
    tb.tool = Tool::CameraPattern;
    assert!(Tool::CameraPattern.reads_text());
    assert_eq!(Tool::from_key("pattern"), Some(Tool::CameraPattern));
    tb.pattern_text = EXAMPLE.to_string();
    tb.run(&ws, Lang::En);
    assert!(tb.error.is_empty(), "{}", tb.error);
    assert_eq!(tb.made_by, Some(Tool::CameraPattern));
    let mut applied = MachineConfig::default();
    assert!(tb.apply(&mut applied, Lang::En));
    assert!(near(applied.camera.lens_k1, 0.002, 2e-4));
    let path = tb.save(p(&dir), Lang::En).unwrap();
    assert!(path.ends_with("camera_pattern.cfg"), "{path}");
    assert!(tb.save(p(&dir), Lang::En).is_none());
    // a wrong point is warned about on the tab and refused when it is applied
    let mut points = parse_pattern(EXAMPLE).unwrap();
    points[10].px += 15.0;
    tb.pattern_text = to_file(&points);
    tb.run(&ws, Lang::En);
    assert!(tb.info.contains("warning: the point"), "{}", tb.info);
    let mut untouched = MachineConfig::default();
    assert!(!tb.apply(&mut untouched, Lang::En));
    assert!(
        tb.error.contains("camera_pattern_max_residual_mm"),
        "{}",
        tb.error
    );
    assert_eq!(untouched.camera, MachineConfig::default().camera);
    // text that is not a pattern
    tb.pattern_text = "nothing".to_string();
    tb.run(&ws, Lang::Es);
    assert!(tb.error.contains("PHOENIX-PNP-058"), "{}", tb.error);
    let _ = std::fs::remove_dir_all(&dir);
}
