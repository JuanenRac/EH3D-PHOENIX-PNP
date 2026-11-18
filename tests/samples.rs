//! The small examples of the calibrations and of the teaching of feeders: they are read, they give the figures they were made with, and the file of measures of several feeders is strict.

use eh3d_phoenix_pnp::calibrate::{
    calibrate_camera, fit_runout, parse_camera_points, parse_runout_samples, taught_csv,
    teach_file, Taught, MAX_TEACH_ROWS,
};
use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::error::Code;
use eh3d_phoenix_pnp::feeding::from_csv;
use std::path::PathBuf;
use std::process::{Command, Output};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn example(name: &str) -> String {
    std::fs::read_to_string(root().join("examples").join(name)).unwrap()
}

fn near(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
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

#[test]
fn the_example_of_the_camera_gives_the_pixel_it_was_made_with() {
    let points = parse_camera_points(&example("camera_points.csv")).unwrap();
    assert_eq!(points.len(), 7);
    let found = calibrate_camera(&points).unwrap();
    assert!(near(found.size_x_mm, 0.02, 1e-4), "{}", found.summary());
    assert!(near(found.size_y_mm, 0.02, 1e-4), "{}", found.summary());
    assert!(near(found.model.offset_x_mm, -0.15, 5e-3));
    assert!(near(found.model.offset_y_mm, 0.08, 5e-3));
    assert!(near(found.model.rotation_deg, 0.0, 0.5));
    assert!(found.rms_mm < 0.005, "{}", found.summary());
    // the lines it prints are lines that the configuration reads
    let cfg = MachineConfig::parse(&found.config_lines()).unwrap();
    assert!(near(cfg.camera.units_per_px, 0.02, 1e-4));
}

#[test]
fn the_example_of_the_nozzle_gives_the_runout_it_was_made_with() {
    let samples = parse_runout_samples(&example("nozzle_readings.csv")).unwrap();
    assert_eq!(samples.len(), 8);
    let found = fit_runout(&samples).unwrap();
    assert!(near(found.radius_mm, 0.12, 5e-3), "{}", found.summary());
    assert!(near(found.phase_deg, 30.0, 3.0), "{}", found.summary());
    assert!(near(found.axis.0, 150.0, 5e-3) && near(found.axis.1, 42.0, 5e-3));
    let cfg = MachineConfig::parse(&found.config_lines()).unwrap();
    assert!(near(cfg.nozzle_runout_mm, found.radius_mm, 1e-3));
}

#[test]
fn the_commands_run_on_the_examples() {
    let o = run(&["camera-calibrate", "examples/camera_points.csv"]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert!(text(&o.stdout).starts_with("7 sightings: a pixel is 0.02"));
    let o = run(&["nozzle-calibrate", "examples/nozzle_readings.csv"]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert!(text(&o.stdout).starts_with("8 readings: the axis is at X 150.0"));
}

#[test]
fn the_example_of_the_measures_teaches_the_feeders_of_the_example_of_kinds() {
    let rows = teach_file(&example("feeder_measures.csv")).unwrap();
    assert_eq!(rows.len(), 2);
    let kinds = from_csv(&example("feeders_kinds.csv")).unwrap();
    for (id, taught) in &rows {
        let feeder = kinds
            .iter()
            .find(|f| &f.base.id == id)
            .unwrap_or_else(|| panic!("{id} is not a feeder of the example of kinds"));
        match taught {
            Taught::Strip(s) => {
                assert!(near(s.pitch_mm, 4.0, 1e-9) && near(s.rotation_deg, 90.0, 1e-9));
                assert_eq!(
                    (s.x_mm, s.y_mm, s.count),
                    (feeder.base.x, feeder.base.y, 40)
                );
            }
            Taught::Tray(t) => {
                assert!(near(t.pitch_x_mm, 12.5, 1e-9) && near(t.pitch_y_mm, 12.5, 1e-9));
                assert!(near(t.rotation_deg, 0.0, 1e-9) && near(t.skew_deg, 0.0, 1e-9));
                assert_eq!((t.cols, t.rows), (3, 2));
            }
        }
    }
    let csv = taught_csv(&rows);
    assert_eq!(
        csv,
        "id,kind,x,y,count,cols,rows,pitch_x,pitch_y,pitch,rotation\n\
         F2,strip,-20.000,20.000,40,,,,,4.0000,90.000\n\
         F3,tray,-60.000,40.000,,3,2,12.5000,12.5000,,0.000\n"
    );
}

#[test]
fn feeder_teach_reads_a_file_of_measures() {
    let o = run(&["feeder-teach", "--measures", "examples/feeder_measures.csv"]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    let out = text(&o.stdout);
    assert!(out.starts_with("F2: a strip of 40 parts from X -20.000 Y 20.000: a pitch of 4.0000 mm along 90.000 degrees\nF3: a tray of 3 by 2 cells"), "{out}");
    assert!(
        out.contains("\nid,kind,x,y,count,cols,rows,pitch_x,pitch_y,pitch,rotation\nF2,strip,"),
        "{out}"
    );
    // in Spanish the error names its code and the line
    let dir = std::env::temp_dir().join(format!("phoenix_pnp_measures_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let bad = dir.join("bad.csv");
    std::fs::write(
        &bad,
        "id,kind,x1,y1,x2,y2,x3,y3,count,rows\nF1,reel,0,0,1,1,,,3,\n",
    )
    .unwrap();
    let o = run(&[
        "feeder-teach",
        "--measures",
        bad.to_str().unwrap(),
        "--lang",
        "es",
    ]);
    assert!(!o.status.success());
    assert!(
        text(&o.stderr).contains("PHOENIX-PNP-058") && text(&o.stderr).contains("line 2"),
        "{}",
        text(&o.stderr)
    );
    // a kind and a --measures file together, or a file that is not there, are refused
    assert!(
        !run(&["feeder-teach", "strip", "--measures", bad.to_str().unwrap()])
            .status
            .success()
    );
    assert!(!run(&["feeder-teach", "--measures", "no/such/file.csv"])
        .status
        .success());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_file_of_measures_that_is_wrong_is_an_error_with_its_line() {
    let head = "id,kind,x1,y1,x2,y2,x3,y3,count,rows\n";
    let cases: [(String, &str); 9] = [
        (String::new(), "no feeder"),
        (head.to_string(), "no feeder"),
        (format!("{head}F1,reel,0,0,1,1,,,3,\n"), "not strip or tray"),
        (format!("{head}F1,strip,0,0,1,1\n"), "needs 10 columns"),
        (
            format!("{head}F1,strip,0,x,1,1,,,3,\n"),
            "y1 'x' is not a number",
        ),
        (
            format!("{head}F1,strip,0,0,1,1,,,3.5,\n"),
            "not a whole number",
        ),
        (
            format!("{head}F1,strip,0,0,3,0,,,4,\nF1,strip,0,5,3,5,,,4,\n"),
            "used twice",
        ),
        (format!("{head}F1,strip,0,0,0,0,,,4,\n"), "outside 0.01"),
        (
            format!("{head}F1,tray,0,0,10,0,0,10,3,1\n"),
            "2 to 1000 cells",
        ),
    ];
    for (file, needle) in cases {
        let e = teach_file(&file).unwrap_err();
        assert_eq!(e.code, Code::CalibrationBad, "{needle}");
        assert!(e.detail.contains(needle), "{needle}: {}", e.detail);
    }
    // the line is the one of the file, a comment and a header included
    let e = teach_file(&format!("# first\n{head}\nF1,reel,0,0,1,1,,,3,\n")).unwrap_err();
    assert!(e.detail.starts_with("line 4: "), "{}", e.detail);
}

#[test]
fn a_file_of_measures_is_read_with_semicolons_a_header_or_none_and_comments() {
    let a = teach_file("F1;strip;0;0;30;0;;;4;\n").unwrap();
    let b = teach_file("# note\nid,kind,x1,y1,x2,y2,x3,y3,count,rows\nF1,STRIP,0,0,30,0,,,4,\n")
        .unwrap();
    assert_eq!(a, b);
    match a[0].1 {
        Taught::Strip(s) => assert!(near(s.pitch_mm, 10.0, 1e-9)),
        Taught::Tray(_) => panic!("a strip"),
    }
    // as many feeders as the limit are read, one more is not
    let row = |n: usize| format!("F{n},strip,0,0,30,0,,,4,\n");
    let many: String = (0..MAX_TEACH_ROWS).map(row).collect();
    assert_eq!(teach_file(&many).unwrap().len(), MAX_TEACH_ROWS);
    let e = teach_file(&(many + &row(MAX_TEACH_ROWS))).unwrap_err();
    assert!(e.detail.contains("more than 1000"), "{}", e.detail);
}

#[test]
fn garbage_and_cut_files_of_measures_never_panic() {
    let good = example("feeder_measures.csv");
    for cut in 0..good.len() {
        if good.is_char_boundary(cut) {
            let _ = teach_file(&good[..cut]);
        }
    }
    for junk in [
        "\"",
        ",,,,,,,,,",
        "\u{feff}",
        "id",
        "#",
        "F1,strip,1e999,0,0,0,,,2,",
        "F1,tray,0,0,0,0,0,0,0,0",
    ] {
        let _ = teach_file(junk);
    }
}
