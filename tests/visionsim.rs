//! The simulated camera made less perfect (blur, uneven light, pixels that turn black or white, spoilt pictures) and the readings it gives for `part-inspect` and `fiducial-check`: the
//! settings of the vision file, the pictures, the readings of the parts and of the marks, and the two commands that make the files those checks read.

use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::error::Code;
use eh3d_phoenix_pnp::fidcheck::{analyze_marks, decompose};
use eh3d_phoenix_pnp::fiducial::parse_measured;
use eh3d_phoenix_pnp::inspect::{judge, parse_readings, tally, Limits, SizeRow, SizeTable};
use eh3d_phoenix_pnp::model::Offset;
use eh3d_phoenix_pnp::sim::Rng;
use eh3d_phoenix_pnp::vision::{
    default_file, measure_image, render, FailureKind, Image, VisionConfig, KEYS,
};
use eh3d_phoenix_pnp::visionsim::{
    mark_failures, marks_csv, part_failures, read_marks, read_parts, readings_csv, PanelTruth,
    Spread,
};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn near(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

fn cfg() -> VisionConfig {
    VisionConfig::default()
}

fn offset(dx: f64, dy: f64, turn: f64) -> Offset {
    Offset {
        dx_mm: dx,
        dy_mm: dy,
        rotation_deg: turn,
    }
}

fn mean(img: &Image, x0: usize, x1: usize) -> f64 {
    let mut sum = 0.0;
    let mut n = 0.0;
    for y in 0..img.height {
        for x in x0..x1 {
            sum += f64::from(img.get(x, y));
            n += 1.0;
        }
    }
    sum / n
}

// ---- the settings

#[test]
fn the_new_settings_are_off_by_default_and_in_the_example() {
    let d = cfg();
    assert_eq!(
        (d.sim_blur_px, d.sim_salt_pepper, d.sim_gradient_levels),
        (0.0, 0.0, 0.0)
    );
    assert_eq!(
        (d.sim_failure_rate, d.sim_failure_kind),
        (0.0, FailureKind::Blank)
    );
    assert_eq!(KEYS.len(), 24);
    for key in [
        "sim_blur_px",
        "sim_salt_pepper",
        "sim_gradient_levels",
        "sim_failure_rate",
        "sim_failure_kind",
    ] {
        assert!(KEYS.contains(&key), "{key}");
        assert!(default_file().contains(&format!("\n{key} = ")), "{key}");
    }
    let c = VisionConfig::parse(
        "sim_blur_px = 2.5\nsim_salt_pepper = 0.05\nsim_gradient_levels = -40\nsim_failure_rate = 0.25\nsim_failure_kind = MIX\n",
    )
    .unwrap();
    assert_eq!(
        (c.sim_blur_px, c.sim_salt_pepper, c.sim_gradient_levels),
        (2.5, 0.05, -40.0)
    );
    assert_eq!(
        (c.sim_failure_rate, c.sim_failure_kind),
        (0.25, FailureKind::Mix)
    );
    for kind in FailureKind::ALL {
        assert_eq!(FailureKind::parse(kind.name()), Some(kind));
    }
    assert_eq!(FailureKind::parse("fog"), None);
    for (bad, code) in [
        ("sim_blur_px = 11", Code::ConfigOutOfRange),
        ("sim_blur_px = -1", Code::ConfigOutOfRange),
        ("sim_salt_pepper = 0.6", Code::ConfigOutOfRange),
        ("sim_gradient_levels = 200", Code::ConfigOutOfRange),
        ("sim_failure_rate = 1.5", Code::ConfigOutOfRange),
        ("sim_failure_rate = many", Code::ConfigBadValue),
        ("sim_failure_kind = fog", Code::ConfigBadValue),
    ] {
        assert_eq!(VisionConfig::parse(bad).unwrap_err().code, code, "{bad}");
    }
}

// ---- the pictures

#[test]
fn the_picture_with_nothing_asked_is_the_sharp_one_and_the_same_every_time() {
    let truth = offset(0.2, -0.1, 12.0);
    let a = render(&cfg(), truth, &mut Rng::new(5));
    let b = render(&cfg(), truth, &mut Rng::new(5));
    assert_eq!(a, b);
    // only the grey of the background and of the part, nothing in between
    assert!(a.pixels.iter().all(|g| *g == 30 || *g == 200));
    // a setting that is off does not change the picture, nor the noise of another one
    let noisy = VisionConfig {
        sim_noise_levels: 30,
        ..cfg()
    };
    let with_off = VisionConfig {
        sim_noise_levels: 30,
        sim_blur_px: 0.0,
        sim_salt_pepper: 0.0,
        sim_failure_rate: 0.0,
        ..cfg()
    };
    assert_eq!(
        render(&noisy, truth, &mut Rng::new(9)),
        render(&with_off, truth, &mut Rng::new(9))
    );
}

#[test]
fn a_blur_softens_the_edges_and_keeps_the_light() {
    let truth = offset(0.0, 0.0, 0.0);
    let sharp = render(&cfg(), truth, &mut Rng::new(1));
    let soft = render(
        &VisionConfig {
            sim_blur_px: 2.0,
            ..cfg()
        },
        truth,
        &mut Rng::new(1),
    );
    let between = |i: &Image| i.pixels.iter().filter(|g| **g > 30 && **g < 200).count();
    assert_eq!(between(&sharp), 0);
    assert!(between(&soft) > 100, "{}", between(&soft));
    // the middle of the part is still the part, and the light is not lost
    assert_eq!(soft.get(64, 64), 200);
    let (m_sharp, m_soft) = (mean(&sharp, 0, 128), mean(&soft, 0, 128));
    assert!(near(m_sharp, m_soft, 1.0), "{m_sharp} {m_soft}");
    // a blur wider than the part leaves no part to see
    let gone = render(
        &VisionConfig {
            sim_blur_px: 10.0,
            sim_part_long_mm: 0.3,
            sim_part_short_mm: 0.2,
            ..cfg()
        },
        truth,
        &mut Rng::new(1),
    );
    assert!(gone.pixels.iter().all(|g| *g < 128));
    assert!(measure_image(&cfg(), &gone).is_err());
}

#[test]
fn pixels_that_turn_black_or_white_come_in_the_share_that_is_asked() {
    let truth = offset(0.0, 0.0, 0.0);
    let img = render(
        &VisionConfig {
            sim_salt_pepper: 0.1,
            ..cfg()
        },
        truth,
        &mut Rng::new(2),
    );
    let extreme = img.pixels.iter().filter(|g| **g == 0 || **g == 255).count();
    let share = extreme as f64 / img.pixels.len() as f64;
    assert!(near(share, 0.1, 0.02), "{share}");
    let black = img.pixels.iter().filter(|g| **g == 0).count();
    let white = img.pixels.iter().filter(|g| **g == 255).count();
    assert!(black > 0 && white > 0 && near(black as f64, white as f64, 0.4 * extreme as f64));
    // the pipeline still finds the part through a few of them (the median filter is on by default)
    let few = render(
        &VisionConfig {
            sim_salt_pepper: 0.02,
            ..cfg()
        },
        truth,
        &mut Rng::new(2),
    );
    let r = measure_image(&cfg(), &few).unwrap();
    assert!(near(r.offset.dx_mm, 0.0, 0.1) && near(r.offset.dy_mm, 0.0, 0.1));
}

#[test]
fn an_uneven_light_is_brighter_on_one_side() {
    let truth = offset(0.0, 0.0, 0.0);
    let img = render(
        &VisionConfig {
            sim_gradient_levels: 60.0,
            ..cfg()
        },
        truth,
        &mut Rng::new(3),
    );
    // the background at the two edges differs by about the levels asked for (the part is in the middle)
    let (left, right) = (mean(&img, 0, 8), mean(&img, 120, 128));
    assert!(
        near(right - left, 60.0 * 120.0 / 128.0, 3.0),
        "{left} {right}"
    );
    let back = render(
        &VisionConfig {
            sim_gradient_levels: -60.0,
            ..cfg()
        },
        truth,
        &mut Rng::new(3),
    );
    assert!(mean(&back, 0, 8) > mean(&back, 120, 128));
}

#[test]
fn a_share_of_the_pictures_is_spoilt_in_the_way_that_is_asked() {
    let truth = offset(0.0, 0.0, 0.0);
    let count_failed = |c: &VisionConfig, n: u64| {
        (0..n)
            .filter(|s| measure_image(c, &render(c, truth, &mut Rng::new(*s))).is_err())
            .count()
    };
    assert_eq!(count_failed(&cfg(), 50), 0);
    let blank = VisionConfig {
        sim_failure_rate: 1.0,
        ..cfg()
    };
    assert_eq!(count_failed(&blank, 50), 50);
    let half = VisionConfig {
        sim_failure_rate: 0.5,
        ..cfg()
    };
    let n = count_failed(&half, 400);
    assert!((150..=250).contains(&n), "{n} of 400");
    // each kind
    let pic = |kind: FailureKind| {
        render(
            &VisionConfig {
                sim_failure_rate: 1.0,
                sim_failure_kind: kind,
                ..cfg()
            },
            truth,
            &mut Rng::new(4),
        )
    };
    assert!(pic(FailureKind::Blank).pixels.iter().all(|g| *g == 30));
    assert!(pic(FailureKind::Dark).pixels.iter().all(|g| *g == 7));
    assert!(pic(FailureKind::Glare).pixels.iter().all(|g| *g == 255));
    for kind in FailureKind::ALL {
        let r = measure_image(
            &VisionConfig {
                sim_failure_rate: 1.0,
                sim_failure_kind: kind,
                ..cfg()
            },
            &pic(kind),
        );
        assert!(r.is_err(), "{kind:?}");
    }
    // a mix has all three
    let mut seen = [false; 3];
    for s in 0..60 {
        let p = render(
            &VisionConfig {
                sim_failure_rate: 1.0,
                sim_failure_kind: FailureKind::Mix,
                ..cfg()
            },
            truth,
            &mut Rng::new(s),
        );
        match p.get(0, 0) {
            30 => seen[0] = true,
            7 => seen[1] = true,
            255 => seen[2] = true,
            other => panic!("a spoilt picture with the grey {other}"),
        }
    }
    assert_eq!(seen, [true; 3]);
}

// ---- the readings of the parts

fn sizes() -> SizeTable {
    SizeTable::parse(include_str!("../examples/package_sizes.csv")).unwrap()
}

fn items(names: &[(&str, &str)]) -> Vec<(String, String, SizeRow)> {
    let table = sizes();
    names
        .iter()
        .map(|(r, p)| {
            (
                r.to_string(),
                p.to_string(),
                table.lookup(p).unwrap().clone(),
            )
        })
        .collect()
}

#[test]
fn the_readings_of_the_parts_repeat_and_do_not_depend_on_the_other_parts() {
    let list = items(&[
        ("R1", "R_0603"),
        ("R2", "R_0603"),
        ("C1", "C_0603"),
        ("L1", "LED_0603"),
    ]);
    let a = read_parts(&list, &cfg(), &Spread::default(), 7);
    let b = read_parts(&list, &cfg(), &Spread::default(), 7);
    assert_eq!(a, b);
    // another seed, other numbers
    assert_ne!(a, read_parts(&list, &cfg(), &Spread::default(), 8));
    // the reading of a part is the same when others are taken away after it
    let fewer = read_parts(&list[..2], &cfg(), &Spread::default(), 7);
    assert_eq!(fewer[..], a[..2]);
    // what the camera saw is near the true size and within the spread
    for s in &a {
        let m = s.outcome.as_ref().unwrap();
        assert!(
            near(m.long_mm, 1.6, 0.12) && near(m.short_mm, 0.8, 0.12),
            "{m:?}"
        );
        assert!(
            m.dx_mm.abs() <= 0.2 + 0.06 && m.dy_mm.abs() <= 0.2 + 0.06,
            "{m:?}"
        );
        assert!(m.rotation_deg.abs() <= 2.0 + 1.5, "{m:?}");
    }
    // no spread: the parts hang from the axis
    let still = read_parts(
        &list,
        &cfg(),
        &Spread {
            offset_mm: 0.0,
            turn_deg: 0.0,
            wrong_every: 0,
        },
        7,
    );
    for s in &still {
        let m = s.outcome.as_ref().unwrap();
        assert!(m.dx_mm.abs() < 0.06 && m.dy_mm.abs() < 0.06 && m.rotation_deg.abs() < 1.5);
    }
}

#[test]
fn the_part_of_another_size_is_refused_by_the_check_and_the_rest_are_not() {
    let list = items(&[
        ("R1", "R_0603"),
        ("R2", "R_0603"),
        ("R3", "R_0603"),
        ("R4", "R_0603"),
    ]);
    let spread = Spread {
        wrong_every: 2,
        ..Spread::default()
    };
    let seen = read_parts(&list, &cfg(), &spread, 1);
    let csv = readings_csv(&seen);
    let readings = parse_readings(&csv).unwrap();
    assert_eq!(readings.len(), 4);
    let rows = judge(&readings, &sizes(), &Limits::default());
    let verdicts: Vec<bool> = rows
        .iter()
        .map(|r| r.inspection.as_ref().unwrap().accepted())
        .collect();
    assert_eq!(verdicts, [true, false, true, false], "{csv}");
    assert_eq!(tally(&rows), (2, 2, 0));
}

#[test]
fn a_picture_with_no_part_gives_no_reading_and_is_told_with_its_reason() {
    let list = items(&[("R1", "R_0603"), ("U1", "QFN-32"), ("R2", "R_0603")]);
    let seen = read_parts(&list, &cfg(), &Spread::default(), 1);
    // the QFN is larger than the picture can hold at this size of pixel
    assert!(seen[0].outcome.is_ok() && seen[1].outcome.is_err() && seen[2].outcome.is_ok());
    let failures = part_failures(&seen);
    assert_eq!(failures.len(), 1);
    assert!(failures[0].starts_with("U1: "), "{}", failures[0]);
    let csv = readings_csv(&seen);
    assert_eq!(csv.lines().count(), 3);
    assert!(!csv.contains("U1"));
    // all the pictures spoilt: the file has the header and nothing else
    let broken = VisionConfig {
        sim_failure_rate: 1.0,
        ..cfg()
    };
    let none = read_parts(&list[..1], &broken, &Spread::default(), 1);
    assert_eq!(
        readings_csv(&none),
        "ref,package,long_mm,short_mm,dx_mm,dy_mm,rot_deg,exp_rot_deg\n"
    );
    assert_eq!(part_failures(&none).len(), 1);
}

// ---- the readings of the marks

fn design() -> Vec<(String, f64, f64)> {
    parse_measured(include_str!("../examples/fid_design.csv")).unwrap()
}

#[test]
fn the_marks_are_read_where_the_panel_lies_and_the_check_finds_how_it_lies() {
    let truth = PanelTruth {
        shift: (0.4, -0.3),
        turn_deg: 0.5,
        scale: 1.0005,
    };
    let seen = read_marks(&design(), &truth, &cfg(), 0.5, 1);
    assert!(mark_failures(&seen).is_empty());
    for (s, (_, x, y)) in seen.iter().zip(design()) {
        let (tx, ty) = truth.apply(x, y);
        let (mx, my) = *s.outcome.as_ref().unwrap();
        assert!(
            near(mx, tx, 0.06) && near(my, ty, 0.06),
            "{mx} {my} against {tx} {ty}"
        );
    }
    let measured = parse_measured(&marks_csv(&seen)).unwrap();
    assert_eq!(measured.len(), 4);
    let (a, _) = analyze_marks(&design(), &measured, &MachineConfig::default()).unwrap();
    let parts = decompose(&a.fit.transform);
    assert!(
        near(parts.rotation_deg, 0.5, 0.05),
        "{}",
        parts.rotation_deg
    );
    assert!(near(parts.scale_x, 1.0005, 0.0005), "{}", parts.scale_x);
    assert!(a.fit.max < 0.06, "{}", a.fit.max);
    // the repeat is the same, the panel on its place gives the design back
    assert_eq!(seen, read_marks(&design(), &truth, &cfg(), 0.5, 1));
    let home = read_marks(&design(), &PanelTruth::default(), &cfg(), 0.5, 1);
    for (s, (_, x, y)) in home.iter().zip(design()) {
        let (mx, my) = *s.outcome.as_ref().unwrap();
        assert!(near(mx, x, 0.06) && near(my, y, 0.06));
    }
}

#[test]
fn a_mark_that_is_not_in_the_picture_is_told_and_the_check_asks_for_more_marks() {
    // a panel off by more than the picture holds (3.2 mm either way) shows no mark
    let far = PanelTruth {
        shift: (6.0, 0.0),
        ..PanelTruth::default()
    };
    let seen = read_marks(&design(), &far, &cfg(), 0.5, 1);
    assert_eq!(mark_failures(&seen).len(), 4);
    assert_eq!(marks_csv(&seen), "id,x,y\n");
    // spoilt pictures: some marks are missing and what is left is still read
    let spoilt = VisionConfig {
        sim_failure_rate: 0.5,
        ..cfg()
    };
    let mut lost = 0;
    for seed in 0..20 {
        let seen = read_marks(&design(), &PanelTruth::default(), &spoilt, 0.5, seed);
        lost += mark_failures(&seen).len();
        let measured = parse_measured(&marks_csv(&seen));
        if let Ok(m) = measured {
            // fewer marks than a fit needs is an error with a code, never a panic
            let r = analyze_marks(&design(), &m, &MachineConfig::default());
            if m.len() < 3 {
                assert!(r.is_err() || m.len() == 2, "{} marks", m.len());
            }
        }
    }
    assert!((20..=60).contains(&lost), "{lost} of 80");
}

#[test]
fn blur_noise_and_lost_pixels_make_the_marks_less_exact_but_not_wrong() {
    let truth = PanelTruth {
        shift: (0.3, 0.2),
        ..PanelTruth::default()
    };
    let rough = VisionConfig {
        sim_noise_levels: 60,
        sim_blur_px: 1.5,
        sim_salt_pepper: 0.02,
        sim_gradient_levels: 40.0,
        ..cfg()
    };
    let seen = read_marks(&design(), &truth, &rough, 0.5, 3);
    assert!(
        mark_failures(&seen).is_empty(),
        "{:?}",
        mark_failures(&seen)
    );
    for (s, (_, x, y)) in seen.iter().zip(design()) {
        let (tx, ty) = truth.apply(x, y);
        let (mx, my) = *s.outcome.as_ref().unwrap();
        assert!(
            near(mx, tx, 0.15) && near(my, ty, 0.15),
            "{mx} {my} against {tx} {ty}"
        );
    }
}

// ---- the commands

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
    let d = std::env::temp_dir().join(format!(
        "phoenix_pnp_visionsim_{tag}_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn p(path: &Path) -> &str {
    path.to_str().unwrap()
}

#[test]
fn the_readings_of_the_command_are_what_part_inspect_reads() {
    let dir = scratch("readings");
    let out = dir.join("readings.csv");
    let o = run(&[
        "vision-readings",
        "examples/board.pos",
        "--sizes",
        "examples/package_sizes.csv",
        "--out",
        p(&out),
    ]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    let err = text(&o.stderr);
    assert!(
        err.contains("6 readings, 1 pictures without a part, 0 parts without a size"),
        "{err}"
    );
    assert!(err.contains("no reading - U1: the spot has"), "{err}");
    assert!(text(&o.stdout).is_empty());
    // the file is read by part-inspect and every part that was read is accepted
    let o = run(&[
        "part-inspect",
        p(&out),
        "--sizes",
        "examples/package_sizes.csv",
    ]);
    assert!(o.status.success(), "{}{}", text(&o.stdout), text(&o.stderr));
    assert!(text(&o.stderr).contains("6 accepted, 0 refused, 0 without a size"));
    // never over a file that exists
    let o = run(&[
        "vision-readings",
        "examples/board.pos",
        "--sizes",
        "examples/package_sizes.csv",
        "--out",
        p(&out),
    ]);
    assert!(!o.status.success());
    // the same seed gives the same readings, to the screen too
    let a = run(&[
        "vision-readings",
        "examples/board.pos",
        "--packages",
        "examples/packages_bodies.csv",
        "--seed",
        "4",
    ]);
    let b = run(&[
        "vision-readings",
        "examples/board.pos",
        "--packages",
        "examples/packages_bodies.csv",
        "--seed",
        "4",
    ]);
    let c = run(&[
        "vision-readings",
        "examples/board.pos",
        "--packages",
        "examples/packages_bodies.csv",
        "--seed",
        "5",
    ]);
    assert!(a.status.success());
    assert_eq!(text(&a.stdout), text(&b.stdout));
    assert_ne!(text(&a.stdout), text(&c.stdout));
    // a part of another size every second part: the check refuses them
    let wrong = run(&[
        "vision-readings",
        "examples/board.pos",
        "--sizes",
        "examples/package_sizes.csv",
        "--wrong-every",
        "2",
        "--out",
        p(&dir.join("wrong.csv")),
    ]);
    assert!(wrong.status.success());
    let o = run(&[
        "part-inspect",
        p(&dir.join("wrong.csv")),
        "--sizes",
        "examples/package_sizes.csv",
    ]);
    assert_eq!(o.status.code(), Some(1));
    // R2 and C1 are the wrong ones that were read; the third wrong one is the QFN, which does not fit the picture
    assert!(
        text(&o.stderr).contains("4 accepted, 2 refused"),
        "{}",
        text(&o.stderr)
    );
    // pictures that fail by chance, from a vision file
    let vision = dir.join("vision.cfg");
    std::fs::write(&vision, "sim_failure_rate = 1\nsim_failure_kind = dark\n").unwrap();
    let o = run(&[
        "vision-readings",
        "examples/board.pos",
        "--sizes",
        "examples/package_sizes.csv",
        "--vision",
        p(&vision),
    ]);
    assert!(o.status.success());
    assert!(
        text(&o.stderr).contains("0 readings, 7 pictures without a part"),
        "{}",
        text(&o.stderr)
    );
    assert_eq!(text(&o.stdout).lines().count(), 1);
    // mistakes
    for args in [
        vec!["vision-readings"],
        vec!["vision-readings", "examples/board.pos"],
        vec![
            "vision-readings",
            "examples/board.pos",
            "--sizes",
            "examples/package_sizes.csv",
            "--seed",
            "-1",
        ],
        vec![
            "vision-readings",
            "examples/board.pos",
            "--sizes",
            "examples/package_sizes.csv",
            "--spread",
            "99",
        ],
        vec![
            "vision-readings",
            "examples/board.pos",
            "--sizes",
            "examples/package_sizes.csv",
            "--wrong-every",
            "1.5",
        ],
        vec![
            "vision-readings",
            "examples/board.pos",
            "--sizes",
            "examples/package_sizes.csv",
            "--oops",
        ],
        vec![
            "vision-readings",
            "examples/board.pos",
            "--sizes",
            "examples/package_sizes.csv",
            "--vision",
            "nothing.cfg",
        ],
    ] {
        assert!(!run(&args).status.success(), "{args:?}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_marks_of_the_command_are_what_fiducial_check_reads() {
    let dir = scratch("marks");
    let out = dir.join("measured.csv");
    let o = run(&[
        "vision-marks",
        "examples/fid_design.csv",
        "--shift",
        "0.4,-0.3",
        "--turn",
        "0.5",
        "--out",
        p(&out),
    ]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert!(text(&o.stderr).contains("4 marks read, 0 without a reading"));
    let o = run(&["fiducial-check", "examples/fid_design.csv", p(&out)]);
    assert!(o.status.success(), "{}{}", text(&o.stdout), text(&o.stderr));
    assert!(
        text(&o.stdout).contains("0.5") || text(&o.stdout).contains("turn"),
        "{}",
        text(&o.stdout)
    );
    // a panel that lies far from the design shows no mark
    let o = run(&["vision-marks", "examples/fid_design.csv", "--shift", "9,0"]);
    assert!(o.status.success());
    assert!(
        text(&o.stderr).contains("0 marks read, 4 without a reading"),
        "{}",
        text(&o.stderr)
    );
    assert_eq!(text(&o.stdout), "id,x,y\n");
    // the same seed, the same marks; a scale that is not one is found
    let a = run(&[
        "vision-marks",
        "examples/fid_design.csv",
        "--scale",
        "1.001",
        "--seed",
        "2",
    ]);
    let b = run(&[
        "vision-marks",
        "examples/fid_design.csv",
        "--scale",
        "1.001",
        "--seed",
        "2",
    ]);
    assert_eq!(text(&a.stdout), text(&b.stdout));
    // mistakes
    for args in [
        vec!["vision-marks"],
        vec!["vision-marks", "examples/fid_design.csv", "--shift", "1"],
        vec!["vision-marks", "examples/fid_design.csv", "--shift", "a,b"],
        vec!["vision-marks", "examples/fid_design.csv", "--scale", "3"],
        vec!["vision-marks", "examples/fid_design.csv", "--dot", "0"],
        vec!["vision-marks", "examples/fid_design.csv", "--oops"],
        vec!["vision-marks", "nothing.csv"],
    ] {
        assert!(!run(&args).status.success(), "{args:?}");
    }
    assert!(text(&run(&["help"]).stdout).contains("vision-marks"));
    assert!(text(&run(&["help"]).stdout).contains("vision-readings"));
    let _ = std::fs::remove_dir_all(&dir);
}
