//! The combined calibration of the camera: a pattern fixed to the bed while the head moves. The file of sightings, the fit of the camera, the turn of the sheet and the bending of the lens,
//! the refusal while the setting is off, the settings, the command and the tool of the window. The expected numbers are the ones of the model that made the sightings.

use eh3d_phoenix_pnp::camera::CameraModel;
use eh3d_phoenix_pnp::combined::{
    allowed, calibrate_combined, parse_combined, parse_combined_touch, resolve_origin,
    CombinedPoint, MAX_TURN_DEG, MIN_POINTS,
};
use eh3d_phoenix_pnp::config::{MachineConfig, KEYS};
use eh3d_phoenix_pnp::error::{Code, Lang};
use eh3d_phoenix_pnp::gui::toolbox::{apply_combined, combined_text, Tool, Toolbox};
use eh3d_phoenix_pnp::gui::workspace::Workspace;
use eh3d_phoenix_pnp::units::rotate_xy;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const MACHINE: &str = include_str!("../examples/machine.cfg");
const EXAMPLE: &str = include_str!("../examples/camera_combined.csv");
/// The settings that the example needs: the sheet lies at 30,20 and the mode is on.
const ON: &str =
    "camera_combined = on\ncamera_pattern_origin_x_mm = 30\ncamera_pattern_origin_y_mm = 20\n";

fn near(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

struct Truth {
    camera: CameraModel,
    units: f64,
    origin: (f64, f64),
    pitch: f64,
    sheet_turn: f64,
    noise_px: f64,
}

impl Default for Truth {
    fn default() -> Self {
        Truth {
            camera: CameraModel {
                units_per_px: 0.0,
                offset_x_mm: 1.2,
                offset_y_mm: -0.8,
                rotation_deg: 3.0,
                flip_x: false,
                flip_y: true,
                lens_scale: 1.0,
                lens_k1: 0.0008,
            },
            units: 0.02,
            origin: (30.0, 20.0),
            pitch: 1.0,
            sheet_turn: 2.0,
            noise_px: 0.0,
        }
    }
}

/// What the camera sees of a 7 by 7 sheet from the places of the head: the points that are in the picture.
fn sightings(t: &Truth, heads: &[(f64, f64)], seed: u64) -> Vec<CombinedPoint> {
    let mut state = seed;
    let mut noise = move || {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((state >> 33) as f64 / (1u64 << 31) as f64) * 2.0 - 1.0
    };
    let mut out = Vec::new();
    for &(hx, hy) in heads {
        for col in 0..7 {
            for row in 0..7 {
                let g = rotate_xy(
                    f64::from(col) * t.pitch,
                    f64::from(row) * t.pitch,
                    t.sheet_turn,
                );
                let place = (t.origin.0 + g.0 - hx, t.origin.1 + g.1 - hy);
                let (px, py) = t.camera.to_pixels(place, t.units);
                if px.abs() <= 320.0 && py.abs() <= 240.0 && (col + row) % 3 == 0 {
                    out.push(CombinedPoint {
                        head_x: hx,
                        head_y: hy,
                        col: f64::from(col),
                        row: f64::from(row),
                        px: px + noise() * t.noise_px,
                        py: py + noise() * t.noise_px,
                    });
                }
            }
        }
    }
    out
}

const HEADS: [(f64, f64); 4] = [(31.5, 21.5), (34.5, 21.5), (31.5, 24.5), (34.5, 24.5)];

fn to_file(rows: &[CombinedPoint]) -> String {
    let mut s = String::from("head_x,head_y,col,row,px,py\n");
    for r in rows {
        s.push_str(&format!(
            "{},{},{},{},{},{}\n",
            r.head_x, r.head_y, r.col, r.row, r.px, r.py
        ));
    }
    s
}

// ---- the file

#[test]
fn a_file_of_sightings_is_read_with_its_header_comments_and_line_endings() {
    let p = parse_combined(
        "# a note\r\nhead_x,head_y,col,row,px,py\r\n1,2,0,0,1.5,-2\r\n1,2,1,0,10,-2\r\n",
    )
    .unwrap();
    assert_eq!(p.len(), 2);
    assert_eq!(
        p[1],
        CombinedPoint {
            head_x: 1.0,
            head_y: 2.0,
            col: 1.0,
            row: 0.0,
            px: 10.0,
            py: -2.0
        }
    );
    // no header, a byte order mark, semicolons
    assert_eq!(parse_combined("\u{feff}1;2;0;0;1;2\n").unwrap().len(), 1);
    // the same point may be seen from two places of the head, not twice from the same one
    assert_eq!(
        parse_combined("1,2,0,0,1,2\n3,2,0,0,5,2\n").unwrap().len(),
        2
    );
    let e = parse_combined("1,2,0,0,1,2\n1,2,0,0,5,2\n").unwrap_err();
    assert!(e.detail.contains("listed twice"), "{}", e.detail);
    // the example of the repository, whatever the line ending of the checkout
    let e = parse_combined(EXAMPLE).unwrap();
    assert_eq!(e.len(), 62);
    assert!(near(e[0].head_x, 31.5, 0.0) && near(e[0].py, 114.55, 1e-9));
}

#[test]
fn a_damaged_file_is_refused_with_its_line_and_never_panics() {
    for (text, why) in [
        ("1,2,3\n", "6 numbers"),
        ("1,2,0,0,1,x\n", "not a number"),
        ("1,2,0,0,1,1e9\n", "not a usable number"),
        ("1,2,0,0,1,NaN\n", "not a usable number"),
        ("head_x,head_y,col,row,px,py\n1,2,0,0,1,2,3\n", "6 numbers"),
    ] {
        let e = parse_combined(text).unwrap_err();
        assert_eq!(e.code, Code::CalibrationBad);
        assert!(
            e.detail.contains("line") && e.detail.contains(why),
            "{}",
            e.detail
        );
    }
    let many: String = (0..101).map(|i| format!("{i},0,0,0,1,1\n")).collect();
    assert!(parse_combined(&many)
        .unwrap_err()
        .detail
        .contains("more than"));
    for junk in [
        "",
        "\n\n",
        "#only a note\n",
        "\u{0}\u{1}\u{2}",
        "1,2,3,4,5,6,7,8",
    ] {
        let _ = parse_combined(junk);
    }
    assert!(parse_combined("").unwrap().is_empty());
}

// ---- the fit

#[test]
fn the_fit_finds_the_camera_the_turn_of_the_sheet_and_the_bending_of_the_lens() {
    let t = Truth::default();
    let rows = sightings(&t, &HEADS, 1);
    assert!(rows.len() >= 30, "{}", rows.len());
    let c = calibrate_combined(&rows, t.pitch, t.origin).unwrap();
    let m = &c.camera.model;
    assert!(near(m.units_per_px, 0.02, 2e-5), "{}", m.units_per_px);
    assert!(
        near(m.offset_x_mm, 1.2, 0.01) && near(m.offset_y_mm, -0.8, 0.01),
        "{m:?}"
    );
    assert!(near(m.rotation_deg, 3.0, 0.05), "{}", m.rotation_deg);
    assert!(m.flip_y && !m.flip_x);
    assert!(near(c.lens_k1, 0.0008, 5e-5), "{}", c.lens_k1);
    assert!(near(m.lens_k1, c.lens_k1, 0.0));
    assert!(near(c.sheet_turn_deg, 2.0, 0.05), "{}", c.sheet_turn_deg);
    assert!(
        c.rms_mm < 0.002 && c.max_mm < 0.005,
        "{} {}",
        c.rms_mm,
        c.max_mm
    );
    // with a perfect lens the points are farther from the model: the bending is worth something
    assert!(
        c.rms_flat_mm > 5.0 * c.rms_mm,
        "{} {}",
        c.rms_flat_mm,
        c.rms_mm
    );
    assert_eq!((c.points, c.heads), (rows.len(), 4));
    assert!(c.within(0.05) && c.warning(0.05).is_none());
    assert!(c
        .warning(1e-9)
        .unwrap()
        .contains("camera_pattern_max_residual_mm"));
    let lines = c.config_lines();
    for key in [
        "camera_units_per_px = 0.020000",
        "camera_flip_y = on",
        "camera_lens_k1 = 0.0008",
    ] {
        assert!(lines.contains(key), "{lines}");
    }
    assert!(!lines.contains("camera_correct_lens"));
    let s = c.summary();
    assert!(
        s.contains("with the head at 4 places") && s.contains("turned +2.0"),
        "{s}"
    );
}

#[test]
fn a_mirrored_picture_a_lens_that_bends_the_other_way_and_a_sheet_turned_the_other_way_are_found() {
    let mut t = Truth::default();
    t.camera.flip_x = true;
    t.camera.flip_y = false;
    t.camera.rotation_deg = -7.5;
    t.camera.lens_k1 = -0.0006;
    t.camera.offset_x_mm = -2.0;
    t.camera.offset_y_mm = 1.5;
    t.sheet_turn = -3.5;
    let heads = [
        (31.0, 21.0),
        (35.0, 21.0),
        (31.0, 25.0),
        (35.0, 25.0),
        (33.0, 23.0),
    ];
    let rows = sightings(&t, &heads, 2);
    let c = calibrate_combined(&rows, t.pitch, t.origin).unwrap();
    let m = &c.camera.model;
    // a mirror left to right is the same picture as a mirror upside down turned half a turn, which is how the fit says it
    assert!(m.flip_y && !m.flip_x);
    assert!(near(m.rotation_deg, 172.5, 0.1), "{}", m.rotation_deg);
    assert!(
        near(m.offset_x_mm, -2.0, 0.02) && near(m.offset_y_mm, 1.5, 0.02),
        "{m:?}"
    );
    assert!(near(c.lens_k1, -0.0006, 5e-5), "{}", c.lens_k1);
    assert!(near(c.sheet_turn_deg, -3.5, 0.1), "{}", c.sheet_turn_deg);
}

#[test]
fn a_lens_with_no_bending_gives_none_and_noise_in_the_pixels_gives_a_fit_a_little_off() {
    let mut t = Truth::default();
    t.camera.lens_k1 = 0.0;
    t.noise_px = 0.3;
    let rows = sightings(&t, &HEADS, 3);
    let c = calibrate_combined(&rows, t.pitch, t.origin).unwrap();
    assert!(c.lens_k1.abs() < 4e-4, "{}", c.lens_k1);
    assert!(near(c.camera.model.units_per_px, 0.02, 1e-4));
    assert!(near(c.camera.model.offset_x_mm, 1.2, 0.05));
    assert!(near(c.sheet_turn_deg, 2.0, 0.3), "{}", c.sheet_turn_deg);
    // the noise is seen as a residual of the size of the noise (0.3 px of 0.02 mm), not more
    assert!(c.rms_mm > 0.001 && c.rms_mm < 0.02, "{}", c.rms_mm);
    // the same noise with a limit under it is warned about
    assert!(c.warning(0.0001).is_some());
}

#[test]
fn a_fit_that_cannot_be_told_is_refused_with_its_reason() {
    let t = Truth::default();
    let rows = sightings(&t, &HEADS, 4);
    let code =
        |r: &[CombinedPoint], pitch: f64| calibrate_combined(r, pitch, t.origin).unwrap_err();
    // the head at one place: the turn of the sheet cannot be told from the turn of the camera
    let one = sightings(&t, &HEADS[..1], 4);
    let e = code(&one, 1.0);
    assert_eq!(e.code, Code::CalibrationBad);
    assert!(e.detail.contains("one place only"), "{}", e.detail);
    // two places that are the same place within a hair
    let mut same = sightings(&t, &HEADS[..1], 4);
    let more: Vec<CombinedPoint> = same
        .iter()
        .map(|p| CombinedPoint {
            head_x: p.head_x + 0.1,
            ..*p
        })
        .collect();
    same.extend(more);
    assert!(code(&same, 1.0).detail.contains("one place only"));
    // too few sightings
    let few = &rows[..MIN_POINTS - 1];
    assert!(code(few, 1.0).detail.contains("at least"));
    // a pitch that is not one
    for pitch in [0.0, -1.0, f64::NAN, 1000.0] {
        assert!(code(&rows, pitch)
            .detail
            .contains("camera_pattern_pitch_mm"));
    }
    // a sheet that is turned more than the search allows
    let far = Truth {
        sheet_turn: MAX_TURN_DEG + 8.0,
        ..Truth::default()
    };
    let rows = sightings(&far, &HEADS, 5);
    let e = calibrate_combined(&rows, far.pitch, far.origin);
    assert!(
        e.is_err() || e.unwrap().max_mm > 0.05,
        "a turn beyond the search was believed"
    );
}

// ---- the settings

#[test]
fn the_mode_is_off_by_default_and_the_origin_is_a_setting() {
    let d = MachineConfig::default();
    assert!(!d.camera_combined);
    assert_eq!(
        (d.camera_pattern_origin_x_mm, d.camera_pattern_origin_y_mm),
        (0.0, 0.0)
    );
    assert!(allowed(&d).unwrap_err().contains("camera_combined = on"));
    let e = MachineConfig::parse(MACHINE).unwrap();
    assert_eq!(e.camera_combined, d.camera_combined);
    for key in [
        "camera_combined",
        "camera_pattern_origin_x_mm",
        "camera_pattern_origin_y_mm",
    ] {
        assert!(KEYS.contains(&key));
    }
    assert!(MACHINE.contains("\ncamera_combined = off"));
    let c = MachineConfig::parse(ON).unwrap();
    assert!(c.camera_combined && allowed(&c).is_ok());
    assert_eq!(
        (c.camera_pattern_origin_x_mm, c.camera_pattern_origin_y_mm),
        (30.0, 20.0)
    );
    for bad in [
        "camera_combined = maybe",
        "camera_pattern_origin_x_mm = 10001",
        "camera_pattern_origin_y_mm = x",
    ] {
        assert!(MachineConfig::parse(bad).is_err(), "{bad}");
    }
    // the separate calibrations do not depend on it
    assert!(eh3d_phoenix_pnp::pattern::parse_pattern("0,0,1,1\n").is_ok());
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

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("phoenix_pnp_combined_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn p(path: &Path) -> &str {
    path.to_str().unwrap()
}

#[test]
fn the_command_is_refused_while_off_and_fits_the_example_when_on() {
    let dir = scratch("cmd");
    // off: it says what to turn on, and nothing is printed as a result
    let o = run(&["camera-combined", "examples/camera_combined.csv"]);
    assert!(!o.status.success());
    assert!(
        text(&o.stderr).contains("camera_combined = on"),
        "{}",
        text(&o.stderr)
    );
    assert!(text(&o.stdout).is_empty());
    let cfg = dir.join("on.cfg");
    std::fs::write(&cfg, ON).unwrap();
    let o = run(&[
        "camera-combined",
        "examples/camera_combined.csv",
        "--config",
        p(&cfg),
    ]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    let out = text(&o.stdout);
    assert!(
        out.starts_with("62 sightings with the head at 4 places:"),
        "{out}"
    );
    assert!(out.contains("camera_units_per_px = 0.020000"), "{out}");
    assert!(
        out.contains("camera_flip_y = on") && out.contains("camera_lens_k1 = 0.0008"),
        "{out}"
    );
    // the origin and the pitch can be given on the command line instead of the file
    let bare = dir.join("bare.cfg");
    std::fs::write(&bare, "camera_combined = on\n").unwrap();
    let o = run(&[
        "camera-combined",
        "examples/camera_combined.csv",
        "--config",
        p(&bare),
        "--origin",
        "30,20",
        "--pitch",
        "1",
    ]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert_eq!(text(&o.stdout), out);
    // a pitch that is not the one of the sheet gives a fit that is far from the sightings: a message and exit 1 (a wrong origin only moves the offset of the camera, which cannot be told)
    let o = run(&[
        "camera-combined",
        "examples/camera_combined.csv",
        "--config",
        p(&bare),
        "--origin",
        "30,20",
        "--pitch",
        "1.3",
    ]);
    assert!(!o.status.success());
    assert!(
        text(&o.stderr).contains("a sighting is "),
        "{}",
        text(&o.stderr)
    );
    // a limit under the noise of the example
    let tight = dir.join("tight.cfg");
    std::fs::write(
        &tight,
        format!("{ON}camera_pattern_max_residual_mm = 0.0001\n"),
    )
    .unwrap();
    let o = run(&[
        "camera-combined",
        "examples/camera_combined.csv",
        "--config",
        p(&tight),
    ]);
    assert!(!o.status.success());
    assert!(text(&o.stdout).contains("camera_lens_k1 = "));
    assert!(
        text(&o.stderr).contains("camera_pattern_max_residual_mm"),
        "{}",
        text(&o.stderr)
    );
    // mistakes: no file, an option that does not exist, an origin that is not X,Y, a file that is not one, in Spanish
    assert!(!run(&["camera-combined", "--config", p(&cfg)])
        .status
        .success());
    for extra in [
        vec!["--oops"],
        vec!["--origin", "7"],
        vec!["--origin", "a,b"],
        vec!["--pitch", "x"],
    ] {
        let mut args = vec![
            "camera-combined",
            "examples/camera_combined.csv",
            "--config",
            p(&cfg),
        ];
        args.extend(extra);
        assert!(!run(&args).status.success());
    }
    let bad = dir.join("bad.csv");
    std::fs::write(&bad, "1,2,3,4\n").unwrap();
    let o = run(&[
        "camera-combined",
        p(&bad),
        "--config",
        p(&cfg),
        "--lang",
        "es",
    ]);
    assert!(!o.status.success());
    assert!(
        text(&o.stderr).contains("PHOENIX-PNP-058: Los puntos de calibración no son válidos"),
        "{}",
        text(&o.stderr)
    );
    // the help names it
    assert!(text(&run(&["help"]).stdout).contains("camera-combined"));
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- the tool of the window

#[test]
fn the_tool_of_the_window_is_off_until_the_setting_is_on_then_fits_applies_and_writes() {
    let off = MachineConfig::default();
    assert!(combined_text(EXAMPLE, &off, Lang::En)
        .unwrap_err()
        .contains("camera_combined"));
    let on = MachineConfig::parse(ON).unwrap();
    let (found, info, lines) = combined_text(EXAMPLE, &on, Lang::En).unwrap();
    assert!(
        info.starts_with("62 sightings") && !info.contains("warning"),
        "{info}"
    );
    assert!(lines.contains("camera_lens_k1"));
    // applying puts the camera and the bending in the machine and leaves the rest, among it whether the camera corrects the lens
    let mut machine = MachineConfig {
        camera: CameraModel {
            rotation_deg: 12.0,
            ..CameraModel::default()
        },
        ..MachineConfig::default()
    };
    assert!(!machine.camera_correct_lens);
    apply_combined(&mut machine, &found);
    assert!(near(machine.camera.units_per_px, 0.02, 2e-5));
    assert!(near(machine.camera.rotation_deg, 3.0, 0.05));
    assert!(near(machine.camera.offset_x_mm, 1.2, 0.01));
    assert!(near(machine.camera.lens_k1, 0.0008, 5e-5));
    assert!(machine.camera.flip_y);
    assert!(
        !machine.camera_correct_lens,
        "the lens is not corrected unless the person says so"
    );
    // the tool
    let ws = Workspace::new(Lang::En);
    let dir = scratch("tool");
    let mut tb = Toolbox::new(&on);
    tb.tool = Tool::CameraCombined;
    assert!(Tool::CameraCombined.reads_text());
    assert_eq!(Tool::from_key("combined"), Some(Tool::CameraCombined));
    assert_eq!(
        Tool::CameraCombined.output_name(),
        Some("camera_combined.cfg")
    );
    tb.combined_text = EXAMPLE.to_string();
    // the workspace has the default settings: off
    tb.run(&ws, Lang::En);
    assert!(tb.error.contains("camera_combined"), "{}", tb.error);
    assert!(tb.made_by.is_none());
    // with the setting on in the workspace
    let mut ws = Workspace::new(Lang::En);
    ws.cfg = on.clone();
    tb.run(&ws, Lang::En);
    assert!(tb.error.is_empty(), "{}", tb.error);
    assert_eq!(tb.made_by, Some(Tool::CameraCombined));
    let mut applied = on.clone();
    assert!(tb.apply(&mut applied, Lang::En));
    assert!(near(applied.camera.lens_k1, 0.0008, 5e-5));
    assert!(!applied.camera_correct_lens);
    let path = tb.save(p(&dir), Lang::En).unwrap();
    assert!(path.ends_with("camera_combined.cfg"), "{path}");
    assert!(tb.save(p(&dir), Lang::En).is_none());
    // a wrong sighting is warned about on the tab and refused when it is applied
    let mut rows = parse_combined(EXAMPLE).unwrap();
    rows[10].px += 40.0;
    tb.combined_text = to_file(&rows);
    tb.run(&ws, Lang::En);
    assert!(tb.info.contains("warning:"), "{}", tb.info);
    let mut untouched = on.clone();
    assert!(!tb.apply(&mut untouched, Lang::En));
    assert!(
        tb.error.contains("camera_pattern_max_residual_mm"),
        "{}",
        tb.error
    );
    assert_eq!(untouched.camera, on.camera);
    // text that is not sightings
    tb.combined_text = "nothing".to_string();
    tb.run(&ws, Lang::Es);
    assert!(tb.error.contains("PHOENIX-PNP-058"), "{}", tb.error);
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- the touch mode: the origin of the sheet from a row of the file

const TOUCH: &str = "camera_combined = on\ncamera_combined_touch = on\n";

#[test]
fn the_touch_mode_is_off_by_default_and_is_a_setting() {
    let d = MachineConfig::default();
    assert!(!d.camera_combined_touch);
    assert!(KEYS.contains(&"camera_combined_touch"));
    assert!(MACHINE.contains("\ncamera_combined_touch = off"));
    assert!(!MachineConfig::parse(MACHINE).unwrap().camera_combined_touch);
    assert!(MachineConfig::parse(TOUCH).unwrap().camera_combined_touch);
    assert!(MachineConfig::parse("camera_combined_touch = sometimes").is_err());
}

#[test]
fn a_row_origin_is_read_among_the_sightings_and_refused_where_it_is_not_wanted() {
    let text = format!("# the nozzle touched the corner\norigin,30.25,-20\n{EXAMPLE}");
    let (rows, origin) = parse_combined_touch(&text).unwrap();
    assert_eq!(origin, Some((30.25, -20.0)));
    assert_eq!(rows.len(), parse_combined(EXAMPLE).unwrap().len());
    // the row may come at the end, in capitals
    let after = format!("{EXAMPLE}ORIGIN,1,2\n");
    assert_eq!(parse_combined_touch(&after).unwrap().1, Some((1.0, 2.0)));
    // the header may come after the row
    let head = "origin,3.5,4\nhead_x,head_y,col,row,px,py\n1,2,0,0,5,6\n";
    let (r, o) = parse_combined_touch(head).unwrap();
    assert_eq!((r.len(), o), (1, Some((3.5, 4.0))));
    // no row, no origin
    assert_eq!(parse_combined_touch(EXAMPLE).unwrap().1, None);
    // the plain reader does not take it quietly
    let e = parse_combined(&text).unwrap_err();
    assert_eq!(e.code, Code::CalibrationBad);
    assert!(e.detail.contains("camera_combined_touch"), "{}", e.detail);
    // damaged rows are refused with their line
    for (bad, want) in [
        (
            "origin,1\n",
            "line 1: the origin of the sheet needs 2 numbers",
        ),
        ("origin,1,2,3\n", "needs 2 numbers"),
        ("origin,a,2\n", "'a' is not a number"),
        ("origin,1,20000\n", "not a usable number"),
        (
            "origin,1,2\nhead_x,y\norigin,3,4\n",
            "line 3: the origin of the sheet is given twice",
        ),
    ] {
        let e = parse_combined_touch(bad).unwrap_err();
        assert!(e.detail.contains(want), "{bad}: {}", e.detail);
    }
}

#[test]
fn the_origin_is_the_one_of_the_file_only_in_the_touch_mode() {
    let cfg =
        MachineConfig::parse("camera_pattern_origin_x_mm = 7\ncamera_pattern_origin_y_mm = 8\n")
            .unwrap();
    // off: the configuration, or the one given on the command line; a row of the file is an error
    assert_eq!(resolve_origin(&cfg, false, None, None).unwrap(), (7.0, 8.0));
    assert_eq!(
        resolve_origin(&cfg, false, None, Some((1.0, 2.0))).unwrap(),
        (1.0, 2.0)
    );
    let e = resolve_origin(&cfg, false, Some((1.0, 2.0)), None).unwrap_err();
    assert!(e.detail.contains("--touch"), "{}", e.detail);
    // on: the file, nothing else; a missing row or a second origin is an error
    assert_eq!(
        resolve_origin(&cfg, true, Some((1.0, 2.0)), None).unwrap(),
        (1.0, 2.0)
    );
    let e = resolve_origin(&cfg, true, None, None).unwrap_err();
    assert!(
        e.detail.contains("needs a row 'origin,X,Y'"),
        "{}",
        e.detail
    );
    let e = resolve_origin(&cfg, true, Some((1.0, 2.0)), Some((3.0, 4.0))).unwrap_err();
    assert!(e.detail.contains("given twice"), "{}", e.detail);
}

#[test]
fn the_touch_mode_gives_the_same_fit_as_the_origin_of_the_configuration() {
    let t = Truth::default();
    let rows = sightings(&t, &HEADS, 3);
    let by_key = calibrate_combined(&rows, t.pitch, t.origin).unwrap();
    let text = format!("origin,{},{}\n{}", t.origin.0, t.origin.1, to_file(&rows));
    let (read, file_origin) = parse_combined_touch(&text).unwrap();
    let origin = resolve_origin(&MachineConfig::default(), true, file_origin, None).unwrap();
    let by_touch = calibrate_combined(&read, t.pitch, origin).unwrap();
    assert_eq!(by_key, by_touch);
    // a file that touched somewhere else gives a camera whose nozzle is somewhere else
    let moved = calibrate_combined(&read, t.pitch, (t.origin.0 + 0.5, t.origin.1)).unwrap();
    assert!(
        (moved.camera.model.offset_x_mm - by_key.camera.model.offset_x_mm).abs() > 0.1,
        "{:?}",
        moved.camera.model
    );
}

#[test]
fn the_command_takes_the_origin_from_the_file_with_touch_or_the_setting() {
    let dir = scratch("touch");
    let key = dir.join("key.cfg");
    std::fs::write(&key, ON).unwrap();
    let touch = dir.join("touch.cfg");
    std::fs::write(&touch, TOUCH).unwrap();
    let file = dir.join("with_origin.csv");
    std::fs::write(&file, format!("origin,30,20\n{EXAMPLE}")).unwrap();
    // by the configuration the row is refused with the way out
    let o = run(&["camera-combined", p(&file), "--config", p(&key)]);
    assert!(!o.status.success());
    assert!(text(&o.stderr).contains("--touch"), "{}", text(&o.stderr));
    // --touch takes it from the file; the result is that of the origin in the configuration
    let plain = "examples/camera_combined.csv";
    let by_key = run(&["camera-combined", plain, "--config", p(&key)]);
    assert!(by_key.status.success(), "{}", text(&by_key.stderr));
    let o = run(&["camera-combined", p(&file), "--touch", "--config", p(&key)]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert_eq!(text(&o.stdout), text(&by_key.stdout));
    // the setting does the same without the switch
    let o = run(&["camera-combined", p(&file), "--config", p(&touch)]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert_eq!(text(&o.stdout), text(&by_key.stdout));
    // a file without the row, or with --origin as well, is refused
    let o = run(&["camera-combined", plain, "--touch", "--config", p(&key)]);
    assert!(!o.status.success());
    assert!(text(&o.stderr).contains("needs a row 'origin,X,Y'"));
    let o = run(&[
        "camera-combined",
        p(&file),
        "--touch",
        "--origin",
        "1,2",
        "--config",
        p(&key),
    ]);
    assert!(!o.status.success());
    assert!(text(&o.stderr).contains("given twice"));
    // touch does not turn the combined mode on
    let off = dir.join("off.cfg");
    std::fs::write(&off, "camera_combined_touch = on\n").unwrap();
    let o = run(&["camera-combined", p(&file), "--config", p(&off)]);
    assert!(!o.status.success());
    assert!(text(&o.stderr).contains("camera_combined = on"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_tool_of_the_window_takes_the_origin_from_the_text_in_the_touch_mode() {
    let on = MachineConfig::parse(TOUCH).unwrap();
    let text = format!("origin,30,20\n{EXAMPLE}");
    let (_, info, _) = combined_text(&text, &on, Lang::En).unwrap();
    assert!(info.starts_with("62 sightings"), "{info}");
    // the text without the row is refused with its reason
    let e = combined_text(EXAMPLE, &on, Lang::En).unwrap_err();
    assert!(
        e.contains("origin,X,Y") && e.contains("PHOENIX-PNP-058"),
        "{e}"
    );
    // and the row is refused when the mode is off
    let key = MachineConfig::parse(ON).unwrap();
    let e = combined_text(&text, &key, Lang::En).unwrap_err();
    assert!(e.contains("camera_combined_touch"), "{e}");
}
