//! Teaching a strip or a tray from places measured on the machine: the pitch, the angle, the skew, the checks, and that the result is the feeder the planner then walks along.

use eh3d_phoenix_pnp::calibrate::{teach_strip, teach_tray};
use eh3d_phoenix_pnp::error::Code;
use eh3d_phoenix_pnp::feeding::{from_csv, FeederKind};
use std::path::PathBuf;
use std::process::{Command, Output};

fn near(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

#[test]
fn a_strip_along_x_has_the_pitch_of_the_distance_over_the_gaps_and_no_turn() {
    let t = teach_strip((10.0, 5.0), (10.0 + 4.0 * 49.0, 5.0), 50).unwrap();
    assert!(
        near(t.pitch_mm, 4.0, 1e-9) && near(t.rotation_deg, 0.0, 1e-9),
        "{t:?}"
    );
    assert_eq!((t.x_mm, t.y_mm, t.count), (10.0, 5.0, 50));
}

#[test]
fn the_angle_of_a_strip_is_found_in_every_direction() {
    for deg in [0.0, 30.0, 90.0, 135.0, 180.0, 225.0, 270.0, 359.0] {
        let a = f64::to_radians(deg);
        let last = (-20.0 + 8.0 * 9.0 * a.cos(), 15.0 + 8.0 * 9.0 * a.sin());
        let t = teach_strip((-20.0, 15.0), last, 10).unwrap();
        assert!(near(t.pitch_mm, 8.0, 1e-9), "{deg}: {}", t.pitch_mm);
        assert!(near(t.rotation_deg, deg, 1e-7), "{deg}: {}", t.rotation_deg);
        assert!((0.0..360.0).contains(&t.rotation_deg));
    }
}

#[test]
fn what_is_taught_is_the_strip_the_feeder_walks_along() {
    let t = teach_strip((10.0, 5.0), (10.0 + 4.0 * 7.0, 5.0 + 4.0 * 7.0), 8).unwrap();
    // the taught columns join the ones that identify the feeder
    let (head, row) = (
        t.csv().lines().next().unwrap().to_string(),
        t.csv().lines().nth(1).unwrap().to_string(),
    );
    let feeders = from_csv(&format!("id,value,package,{head}\nF1,10k,R,{row}\n")).unwrap();
    let f = &feeders[0];
    match f.kind {
        FeederKind::Strip { pitch, rotation } => {
            assert!(
                near(pitch, t.pitch_mm, 1e-3) && near(rotation, 45.0, 1e-3),
                "{pitch} {rotation}"
            );
        }
        other => panic!("{other:?}"),
    }
    let last = f.pick_point(7);
    assert!(
        near(last.0, 38.0, 0.01) && near(last.1, 33.0, 0.01),
        "{last:?}"
    );
    assert_eq!((f.base.x, f.base.y), (10.0, 5.0));
}

#[test]
fn a_tray_gives_two_pitches_and_the_angle_of_its_rows() {
    // 4 by 3 cells, 12.5 and 10 apart, turned 20 degrees
    let a = f64::to_radians(20.0);
    let at = |col: f64, row: f64| {
        (
            100.0 + 12.5 * col * a.cos() - 10.0 * row * a.sin(),
            50.0 + 12.5 * col * a.sin() + 10.0 * row * a.cos(),
        )
    };
    let t = teach_tray((100.0, 50.0), at(3.0, 0.0), at(0.0, 2.0), 4, 3).unwrap();
    assert!(
        near(t.pitch_x_mm, 12.5, 1e-9) && near(t.pitch_y_mm, 10.0, 1e-9),
        "{t:?}"
    );
    assert!(near(t.rotation_deg, 20.0, 1e-7));
    assert!(t.skew_deg.abs() < 1e-7);
    assert_eq!((t.cols, t.rows), (4, 3));
}

#[test]
fn rows_that_go_the_other_way_give_a_negative_pitch_and_a_tray_that_is_not_square_says_so() {
    let t = teach_tray((0.0, 0.0), (30.0, 0.0), (0.0, -20.0), 4, 3).unwrap();
    assert!(near(t.pitch_y_mm, -10.0, 1e-9), "{}", t.pitch_y_mm);
    assert!(near(t.rotation_deg, 0.0, 1e-9));
    // the last row is 1 mm to the side
    let t = teach_tray((0.0, 0.0), (30.0, 0.0), (1.0, 20.0), 4, 3).unwrap();
    assert!(near(t.pitch_y_mm, 10.0, 1e-9));
    assert!(
        near(t.skew_deg, (1.0f64).atan2(20.0).to_degrees(), 1e-9),
        "{}",
        t.skew_deg
    );
}

#[test]
fn what_is_taught_is_the_tray_the_feeder_empties() {
    let t = teach_tray((100.0, 50.0), (137.5, 50.0), (100.0, 80.0), 4, 3).unwrap();
    let (head, row) = (
        t.csv().lines().next().unwrap().to_string(),
        t.csv().lines().nth(1).unwrap().to_string(),
    );
    // a tray holds as many parts as it has cells, but the file asks for the count too
    let feeders = from_csv(&format!(
        "id,value,package,count,{head}\nF1,MCU,Q,12,{row}\n"
    ))
    .unwrap();
    let f = &feeders[0];
    match f.kind {
        FeederKind::Tray {
            cols,
            rows,
            pitch_x,
            pitch_y,
            ..
        } => {
            assert_eq!((cols, rows), (4, 3));
            assert!(near(pitch_x, 12.5, 1e-3) && near(pitch_y, 15.0, 1e-3));
        }
        other => panic!("{other:?}"),
    }
    let last = f.pick_point(11);
    assert!(
        near(last.0, 137.5, 0.01) && near(last.1, 80.0, 0.01),
        "{last:?}"
    );
}

#[test]
fn what_cannot_be_taught_is_an_error_with_the_code() {
    let mut cases: Vec<(Result<(), eh3d_phoenix_pnp::error::PnpError>, &str)> = Vec::new();
    cases.push((
        teach_strip((0.0, 0.0), (10.0, 0.0), 1).map(|_| ()),
        "2 to 1000000",
    ));
    cases.push((
        teach_strip((0.0, 0.0), (10.0, 0.0), 1_000_001).map(|_| ()),
        "2 to 1000000",
    ));
    cases.push((
        teach_strip((0.0, 0.0), (0.0, 0.0), 5).map(|_| ()),
        "outside 0.01 to 1000",
    ));
    cases.push((
        teach_strip((0.0, 0.0), (5000.0, 0.0), 2).map(|_| ()),
        "outside 0.01 to 1000",
    ));
    cases.push((
        teach_strip((f64::NAN, 0.0), (1.0, 0.0), 2).map(|_| ()),
        "first part",
    ));
    cases.push((
        teach_strip((0.0, 0.0), (20_000.0, 0.0), 2000).map(|_| ()),
        "last part",
    ));
    cases.push((
        teach_tray((0.0, 0.0), (10.0, 0.0), (0.0, 10.0), 1, 3).map(|_| ()),
        "2 to 1000 cells",
    ));
    cases.push((
        teach_tray((0.0, 0.0), (10.0, 0.0), (0.0, 10.0), 3, 1001).map(|_| ()),
        "2 to 1000 cells",
    ));
    cases.push((
        teach_tray((0.0, 0.0), (0.0, 0.0), (0.0, 10.0), 3, 3).map(|_| ()),
        "along the rows",
    ));
    cases.push((
        teach_tray((0.0, 0.0), (10.0, 0.0), (10.0, 0.0), 3, 3).map(|_| ()),
        "along the columns",
    ));
    cases.push((
        teach_tray((0.0, 0.0), (10.0, 0.0), (f64::INFINITY, 0.0), 3, 3).map(|_| ()),
        "last cell of the first column",
    ));
    for (result, needle) in cases {
        let e = result.unwrap_err();
        assert_eq!(e.code, Code::CalibrationBad, "{needle}");
        assert!(e.detail.contains(needle), "{needle}: {}", e.detail);
    }
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_phoenix-pnp"))
        .args(args)
        .current_dir(PathBuf::from(env!("CARGO_MANIFEST_DIR")))
        .output()
        .unwrap()
}

fn text(o: &[u8]) -> String {
    String::from_utf8_lossy(o).into_owned()
}

#[test]
fn feeder_teach_prints_the_summary_and_the_columns_for_a_strip_and_for_a_tray() {
    let o = run(&[
        "feeder-teach",
        "strip",
        "--first",
        "10,5",
        "--last",
        "206,5",
        "--count",
        "50",
    ]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    let out = text(&o.stdout);
    assert!(
        out.starts_with(
            "a strip of 50 parts from X 10.000 Y 5.000: a pitch of 4.0000 mm along 0.000 degrees"
        ),
        "{out}"
    );
    assert!(
        out.ends_with("kind,x,y,count,pitch,rotation\nstrip,10.000,5.000,50,4.0000,0.000\n"),
        "{out}"
    );
    let o = run(&[
        "feeder-teach",
        "tray",
        "--origin",
        "100,50",
        "--col-end",
        "137.5,50",
        "--row-end",
        "100,80",
        "--cols",
        "4",
        "--rows",
        "3",
    ]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    let out = text(&o.stdout);
    assert!(
        out.starts_with(
            "a tray of 4 by 3 cells from X 100.000 Y 50.000: pitches of 12.5000 and 15.0000 mm"
        ),
        "{out}"
    );
    assert!(out.ends_with("kind,x,y,cols,rows,pitch_x,pitch_y,rotation\ntray,100.000,50.000,4,3,12.5000,15.0000,0.000\n"), "{out}");
}

#[test]
fn feeder_teach_is_strict_and_reports_its_errors_in_the_language_asked() {
    for bad in [
        vec!["feeder-teach"],
        vec!["feeder-teach", "reel"],
        vec!["feeder-teach", "strip", "--first", "10,5", "--last", "20,5"],
        vec![
            "feeder-teach",
            "strip",
            "--first",
            "10;5",
            "--last",
            "20,5",
            "--count",
            "3",
        ],
        vec![
            "feeder-teach",
            "strip",
            "--first",
            "a,5",
            "--last",
            "20,5",
            "--count",
            "3",
        ],
        vec![
            "feeder-teach",
            "strip",
            "--first",
            "10,5",
            "--last",
            "20,5",
            "--count",
            "x",
        ],
        vec![
            "feeder-teach",
            "strip",
            "--first",
            "10,5",
            "--last",
            "20,5",
            "--count",
            "3",
            "--oops",
        ],
        vec![
            "feeder-teach",
            "tray",
            "--origin",
            "0,0",
            "--col-end",
            "1,0",
            "--row-end",
            "0,1",
            "--cols",
            "2",
        ],
    ] {
        assert!(!run(&bad).status.success(), "{bad:?}");
    }
    let o = run(&[
        "feeder-teach",
        "strip",
        "--first",
        "0,0",
        "--last",
        "0,0",
        "--count",
        "3",
        "--lang",
        "it",
    ]);
    assert!(
        text(&o.stderr).contains("PHOENIX-PNP-058: I punti di calibrazione non sono validi"),
        "{}",
        text(&o.stderr)
    );
}
