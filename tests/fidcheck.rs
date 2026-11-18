//! What the fiducials say about the panel: the transformation taken apart, the limits, the mark that does not agree and the levels of a panel.

use eh3d_phoenix_pnp::error::Code;
use eh3d_phoenix_pnp::fidcheck::{
    analyze_points, check, check_levels, decompose, refusal, report, solve_robust, Finding, Limits,
    Policy, Reject,
};
use eh3d_phoenix_pnp::fiducial::{FitMode, Transform};
use eh3d_phoenix_pnp::marks::{fit_levels, fit_levels_with};

const EXAMPLE: &str = include_str!("../examples/machine.cfg");

fn near(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

/// A turn by `deg` degrees, a scale along x and along y before the turn, and a shift.
fn transform(deg: f64, sx: f64, sy: f64, tx: f64, ty: f64) -> Transform {
    let (s, c) = deg.to_radians().sin_cos();
    Transform {
        a: c * sx,
        b: -s * sy,
        c: s * sx,
        d: c * sy,
        tx,
        ty,
    }
}

const DESIGN: [(f64, f64); 6] = [
    (0.0, 0.0),
    (100.0, 0.0),
    (0.0, 60.0),
    (100.0, 60.0),
    (50.0, 0.0),
    (50.0, 60.0),
];

fn moved(t: &Transform, points: &[(f64, f64)]) -> Vec<(f64, f64)> {
    points.iter().map(|p| t.apply(p.0, p.1)).collect()
}

fn named(points: &[(f64, f64)]) -> Vec<(String, f64, f64)> {
    points
        .iter()
        .enumerate()
        .map(|(i, p)| (format!("f{}", i + 1), p.0, p.1))
        .collect()
}

#[test]
fn a_transformation_is_taken_apart_into_its_turn_scales_and_shear() {
    let p = decompose(&transform(12.5, 1.0, 1.0, 3.0, -4.0));
    assert!(
        near(p.rotation_deg, 12.5, 1e-9)
            && near(p.scale_x, 1.0, 1e-12)
            && near(p.scale_y, 1.0, 1e-12)
    );
    assert!(near(p.shear_deg, 0.0, 1e-9) && !p.mirrored);
    let p = decompose(&transform(-30.0, 1.02, 0.99, 0.0, 0.0));
    assert!(
        near(p.rotation_deg, -30.0, 1e-9)
            && near(p.scale_x, 1.02, 1e-12)
            && near(p.scale_y, 0.99, 1e-12)
    );
    // the y axis leaning over the x axis by 2 degrees
    let sheared = Transform {
        a: 1.0,
        b: 2f64.to_radians().tan(),
        c: 0.0,
        d: 1.0,
        tx: 0.0,
        ty: 0.0,
    };
    let p = decompose(&sheared);
    assert!(near(p.shear_deg, 2.0, 1e-9), "{p:?}");
    assert!(
        near(p.scale_y, 1.0, 1e-12),
        "the height of the shear is the scale"
    );
    // a flipped plane
    let flipped = Transform {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: -1.0,
        tx: 0.0,
        ty: 0.0,
    };
    assert!(decompose(&flipped).mirrored);
    assert_eq!(decompose(&Transform::IDENTITY).rotation_deg, 0.0);
}

#[test]
fn each_limit_gives_its_own_finding() {
    let lim = Limits::default();
    assert!(check(&Transform::IDENTITY, None, &lim).ok());
    assert!(matches!(
        check(&transform(7.0, 1.0, 1.0, 0.0, 0.0), None, &lim).findings.as_slice(),
        [Finding::Rotation(d)] if near(*d, 7.0, 1e-9)
    ));
    assert!(matches!(
        check(&transform(0.0, 1.03, 1.0, 0.0, 0.0), None, &lim).findings.as_slice(),
        [Finding::ScaleX(p)] if near(*p, 3.0, 1e-9)
    ));
    assert!(matches!(
        check(&transform(0.0, 1.0, 0.97, 0.0, 0.0), None, &lim).findings.as_slice(),
        [Finding::ScaleY(p)] if near(*p, -3.0, 1e-9)
    ));
    let sheared = Transform {
        b: 0.02,
        ..Transform::IDENTITY
    };
    assert!(matches!(
        check(&sheared, None, &lim).findings.as_slice(),
        [Finding::Shear(_)]
    ));
    let flipped = Transform {
        d: -1.0,
        ..Transform::IDENTITY
    };
    assert_eq!(check(&flipped, None, &lim).findings, [Finding::Mirrored]);
    // the shift is the move of the middle of the marks, and is only judged when the design is known
    let shifted = transform(0.0, 1.0, 1.0, 6.0, 0.0);
    assert!(check(&shifted, None, &lim).ok());
    let c = check(&shifted, Some((50.0, 30.0)), &lim);
    assert!(
        matches!(c.findings.as_slice(), [Finding::Shift(s)] if near(*s, 6.0, 1e-9)),
        "{c:?}"
    );
    assert!(c.text().contains("moved 6.00 mm"));
    // a turn about the middle moves the middle by nothing
    let about_middle = {
        let t = transform(3.0, 1.0, 1.0, 0.0, 0.0);
        let (u, v) = t.apply(50.0, 30.0);
        Transform {
            tx: 50.0 - u,
            ty: 30.0 - v,
            ..t
        }
    };
    assert!(check(&about_middle, Some((50.0, 30.0)), &lim).ok());
}

#[test]
fn the_edge_of_a_limit_is_still_accepted() {
    let lim = Limits {
        max_rotation_deg: 5.0,
        scale_tol_percent: 1.0,
        ..Limits::default()
    };
    assert!(check(&transform(4.999, 1.0, 1.0, 0.0, 0.0), None, &lim).ok());
    assert!(check(&transform(0.0, 1.0099, 1.0, 0.0, 0.0), None, &lim).ok());
    assert!(!check(&transform(5.01, 1.0, 1.0, 0.0, 0.0), None, &lim).ok());
}

#[test]
fn marks_that_were_moved_rigidly_are_analyzed_without_findings_and_a_rigid_fit_leaves_nothing() {
    let t = transform(0.4, 1.0, 1.0, 1.0, -0.5);
    let a = analyze_points(
        &DESIGN,
        &moved(&t, &DESIGN),
        FitMode::Auto,
        0.5,
        &Limits::default(),
    )
    .unwrap();
    assert!(a.check.ok(), "{:?}", a.check);
    assert!(near(a.check.parts.rotation_deg, 0.4, 1e-7));
    assert!(a.similarity_max_mm.unwrap() < 1e-9 && a.fit.max < 1e-9);
    assert!(a.suspect.is_none());
    assert_eq!(a.without.len(), 6);
    assert!(a.without.iter().all(|w| w.is_some_and(|v| v < 1e-9)));
    let text = report(&a, &["f1", "f2", "f3", "f4", "f5", "f6"].map(String::from));
    assert!(
        text.contains("check: ok") && text.contains("mark f3: 0.000 mm"),
        "{text}"
    );
}

#[test]
fn a_warped_panel_is_told_from_a_rigid_one_by_what_a_rigid_fit_leaves() {
    let t = transform(0.0, 1.002, 0.998, 0.0, 0.0);
    let a = analyze_points(
        &DESIGN,
        &moved(&t, &DESIGN),
        FitMode::Affine,
        0.5,
        &Limits::default(),
    )
    .unwrap();
    assert!(a.fit.max < 1e-9, "an affine fit takes the deformation in");
    assert!(
        a.similarity_max_mm.unwrap() > 0.05,
        "{:?}",
        a.similarity_max_mm
    );
    assert!(a.check.ok(), "0.2 per cent is inside the default limit");
    let strict = Limits {
        scale_tol_percent: 0.1,
        ..Limits::default()
    };
    let a = analyze_points(&DESIGN, &moved(&t, &DESIGN), FitMode::Affine, 0.5, &strict).unwrap();
    assert_eq!(a.check.findings.len(), 2, "{:?}", a.check);
}

fn with_bad_mark(index: usize, dx: f64) -> Vec<(f64, f64)> {
    let mut m = DESIGN.to_vec();
    m[index].0 += dx;
    m
}

#[test]
fn the_one_mark_that_does_not_agree_is_found() {
    let a = analyze_points(
        &DESIGN,
        &with_bad_mark(3, 3.0),
        FitMode::Auto,
        0.5,
        &Limits::default(),
    )
    .unwrap();
    assert!(a.fit.max > 0.5);
    assert_eq!(a.suspect, Some(3), "{:?}", a.without);
    assert!(a.without[3].unwrap() < 1e-9);
    assert!(report(&a, &[]).contains("the only one that does not agree"));
    // inside the limit nobody is suspected
    let a = analyze_points(
        &DESIGN,
        &with_bad_mark(3, 0.1),
        FitMode::Auto,
        0.5,
        &Limits::default(),
    )
    .unwrap();
    assert!(a.suspect.is_none());
}

#[test]
fn with_just_enough_marks_nobody_can_be_blamed() {
    // four marks make a fit of three when one is left out: that fit has no residual, so it cannot say which one was wrong
    let four = &DESIGN[..4];
    let mut m = four.to_vec();
    m[1].0 += 3.0;
    let a = analyze_points(four, &m, FitMode::Auto, 0.5, &Limits::default()).unwrap();
    assert!(a.fit.max > 0.5 && a.suspect.is_none());
    assert!(a.without.iter().all(Option::is_none));
    // three marks as a similarity are not enough to tell either (two are left)
    let three = &DESIGN[..3];
    let mut m = three.to_vec();
    m[1].0 += 3.0;
    let a = analyze_points(three, &m, FitMode::Similarity, 0.5, &Limits::default()).unwrap();
    assert!(a.suspect.is_none() && a.without.iter().all(Option::is_none));
    // four marks as a similarity can
    let mut m = four.to_vec();
    m[1].0 += 3.0;
    let a = analyze_points(four, &m, FitMode::Similarity, 0.5, &Limits::default()).unwrap();
    assert_eq!(a.suspect, Some(1), "{:?}", a.without);
}

#[test]
fn a_bad_set_of_marks_is_refused() {
    let e = analyze_points(
        &DESIGN[..1],
        &DESIGN[..1],
        FitMode::Auto,
        0.5,
        &Limits::default(),
    )
    .unwrap_err();
    assert_eq!(e.code, Code::FiducialBad);
}

#[test]
fn dropping_the_worst_mark_needs_a_choice_and_leaves_the_others_to_fit() {
    let design = named(&DESIGN);
    let measured = named(&with_bad_mark(3, 3.0));
    let refused = solve_robust(&design, &measured, FitMode::Auto, 0.5, Reject::Refuse).unwrap_err();
    assert_eq!(refused.code, Code::FiducialBad);
    assert!(
        refused.detail.contains("from where the fit puts it"),
        "{}",
        refused.detail
    );
    let (fit, dropped) =
        solve_robust(&design, &measured, FitMode::Auto, 0.5, Reject::DropWorst).unwrap();
    assert_eq!(dropped.as_deref(), Some("f4"));
    assert_eq!(fit.ids, ["f1", "f2", "f3", "f5", "f6"]);
    assert!(fit.max < 1e-9);
    // a fit that is good needs no mark dropped
    let (_, none) = solve_robust(
        &design,
        &named(&DESIGN),
        FitMode::Auto,
        0.5,
        Reject::DropWorst,
    )
    .unwrap();
    assert!(none.is_none());
}

#[test]
fn nothing_is_dropped_when_it_cannot_be_told_which_mark_is_wrong() {
    // two marks off: no single mark explains it
    let mut m = DESIGN.to_vec();
    m[1].0 += 3.0;
    m[3].1 -= 3.0;
    let e = solve_robust(
        &named(&DESIGN),
        &named(&m),
        FitMode::Auto,
        0.5,
        Reject::DropWorst,
    )
    .unwrap_err();
    assert_eq!(e.code, Code::FiducialBad);
    // four marks: the three that are left would fit anything
    let four = &DESIGN[..4];
    let mut m = four.to_vec();
    m[1].0 += 3.0;
    let e = solve_robust(
        &named(four),
        &named(&m),
        FitMode::Auto,
        0.5,
        Reject::DropWorst,
    )
    .unwrap_err();
    assert_eq!(e.code, Code::FiducialBad);
    // a mark that the design does not have is still an error with either choice
    let mut extra = named(&DESIGN);
    extra.push(("zz".into(), 1.0, 1.0));
    for r in [Reject::Refuse, Reject::DropWorst] {
        assert!(solve_robust(&named(&DESIGN), &extra, FitMode::Auto, 0.5, r).is_err());
    }
}

type Marks = Vec<(String, f64, f64)>;

fn level_marks() -> (Marks, Marks) {
    let panel = vec![
        ("p1".to_string(), 0.0, 0.0),
        ("p2".to_string(), 200.0, 0.0),
        ("p3".to_string(), 0.0, 100.0),
    ];
    let boards = vec![
        ("A/FID1".to_string(), 10.0, 10.0),
        ("A/FID2".to_string(), 90.0, 10.0),
        ("A/FID3".to_string(), 10.0, 70.0),
    ];
    (panel, boards)
}

#[test]
fn a_board_turned_in_the_panel_is_a_finding_of_its_own() {
    let (panel, boards) = level_marks();
    let t = transform(3.0, 1.0, 1.0, 0.0, 0.0);
    let mut measured: Vec<(String, f64, f64)> = panel.clone();
    for b in &boards {
        let (u, v) = t.apply(b.1, b.2);
        measured.push((b.0.clone(), u, v));
    }
    let levels = fit_levels(&panel, &boards, &measured, FitMode::Auto, 0.5).unwrap();
    let mut design = panel.clone();
    design.extend(boards.iter().cloned());
    let found = check_levels(&levels, &design, &Limits::default());
    assert_eq!(found.len(), 2);
    assert!(found[0].0.is_empty() && found[0].1.ok(), "{:?}", found[0]);
    assert_eq!(found[1].0, "A");
    assert!(
        matches!(found[1].1.findings.as_slice(), [Finding::LevelTurn(d)] if near(*d, 3.0, 1e-6)),
        "{:?}",
        found[1].1
    );
    // a looser limit for the turn of a board lets it pass
    let loose = Limits {
        level_turn_deg: 4.0,
        ..Limits::default()
    };
    assert!(check_levels(&levels, &design, &loose)
        .iter()
        .all(|(_, c)| c.ok()));
    // the same turn on the panel and on its boards is not a turn of the board in the panel
    let turned_panel: Vec<(String, f64, f64)> = panel
        .iter()
        .chain(boards.iter())
        .map(|m| {
            let (u, v) = t.apply(m.1, m.2);
            (m.0.clone(), u, v)
        })
        .collect();
    let levels = fit_levels(&panel, &boards, &turned_panel, FitMode::Auto, 0.5).unwrap();
    assert!(check_levels(&levels, &design, &Limits::default())
        .iter()
        .all(|(_, c)| c.ok()));
    let e = refusal("A", &found[1].1);
    assert_eq!(e.code, Code::FiducialBad);
    assert!(
        e.detail.contains("the board A") && e.detail.contains("turned +3.00"),
        "{}",
        e.detail
    );
    assert!(refusal("", &found[1].1).detail.contains("the panel"));
}

#[test]
fn a_level_fit_can_leave_out_the_mark_that_does_not_agree() {
    let panel: Vec<(String, f64, f64)> = named(&DESIGN);
    let measured = named(&with_bad_mark(2, 4.0));
    let e = fit_levels(&panel, &[], &measured, FitMode::Auto, 0.5).unwrap_err();
    assert_eq!(e.code, Code::FiducialBad);
    let fits = fit_levels_with(
        &panel,
        &[],
        &measured,
        FitMode::Auto,
        0.5,
        Reject::DropWorst,
    )
    .unwrap();
    assert_eq!(fits[0].fit.ids, ["f1", "f2", "f4", "f5", "f6"]);
}

#[test]
fn the_policy_and_the_choice_have_their_names() {
    for (text, p) in [
        ("off", Policy::Off),
        ("warn", Policy::Warn),
        ("refuse", Policy::Refuse),
    ] {
        assert_eq!(Policy::parse(text), Some(p));
        assert_eq!(p.name(), text);
    }
    for (text, r) in [
        ("refuse", Reject::Refuse),
        ("drop_worst", Reject::DropWorst),
    ] {
        assert_eq!(Reject::parse(text), Some(r));
        assert_eq!(r.name(), text);
    }
    assert!(Policy::parse("on").is_none() && Reject::parse("drop").is_none());
    assert_eq!(
        (Policy::default(), Reject::default()),
        (Policy::Warn, Reject::Refuse)
    );
}

#[test]
fn the_settings_of_the_check_are_in_the_example_with_the_defaults_and_their_ranges() {
    use eh3d_phoenix_pnp::config::{MachineConfig, KEYS};
    let keys = [
        "fiducial_check",
        "fiducial_max_rotation_deg",
        "fiducial_scale_tol_percent",
        "fiducial_shear_tol_deg",
        "fiducial_max_shift_mm",
        "fiducial_level_turn_deg",
        "fiducial_reject",
    ];
    for k in keys {
        assert!(KEYS.contains(&k), "{k}");
        assert!(EXAMPLE.contains(&format!("\n{k} = ")), "{k}");
    }
    let e = MachineConfig::parse(EXAMPLE).unwrap();
    let d = MachineConfig::default();
    assert_eq!(e.fiducial_check, d.fiducial_check);
    assert_eq!(e.fiducial_reject, d.fiducial_reject);
    assert_eq!(
        (
            e.fiducial_max_rotation_deg,
            e.fiducial_scale_tol_percent,
            e.fiducial_shear_tol_deg,
            e.fiducial_max_shift_mm,
            e.fiducial_level_turn_deg
        ),
        (5.0, 1.0, 0.5, 5.0, 2.0)
    );
    let lim = Limits::default();
    assert_eq!(
        (
            lim.max_rotation_deg,
            lim.scale_tol_percent,
            lim.shear_tol_deg,
            lim.max_shift_mm,
            lim.level_turn_deg
        ),
        (
            d.fiducial_max_rotation_deg,
            d.fiducial_scale_tol_percent,
            d.fiducial_shear_tol_deg,
            d.fiducial_max_shift_mm,
            d.fiducial_level_turn_deg
        )
    );
    for (key, ok, bad) in [
        ("fiducial_max_rotation_deg", ["0.1", "180"], ["0", "181"]),
        ("fiducial_scale_tol_percent", ["0.01", "50"], ["0", "51"]),
        ("fiducial_shear_tol_deg", ["0.01", "45"], ["0", "46"]),
        ("fiducial_max_shift_mm", ["0.1", "1000"], ["0", "1001"]),
        ("fiducial_level_turn_deg", ["0.1", "180"], ["0", "181"]),
    ] {
        for v in ok {
            assert!(
                MachineConfig::parse(&format!("{key} = {v}")).is_ok(),
                "{key} {v}"
            );
        }
        for v in bad {
            assert_eq!(
                MachineConfig::parse(&format!("{key} = {v}"))
                    .unwrap_err()
                    .code,
                Code::ConfigOutOfRange,
                "{key} {v}"
            );
        }
    }
    for line in ["fiducial_check = maybe", "fiducial_reject = drop"] {
        assert_eq!(
            MachineConfig::parse(line).unwrap_err().code,
            Code::ConfigBadValue,
            "{line}"
        );
    }
    assert_eq!(
        MachineConfig::parse("fiducial_check = refuse")
            .unwrap()
            .fiducial_check,
        Policy::Refuse
    );
    assert_eq!(
        MachineConfig::parse("fiducial_reject = drop_worst")
            .unwrap()
            .fiducial_reject,
        Reject::DropWorst
    );
}

fn mk(id: &str, x: f64, y: f64) -> (String, f64, f64) {
    (id.to_string(), x, y)
}

#[test]
fn the_marks_are_paired_by_name_and_the_limits_come_from_the_configuration() {
    use eh3d_phoenix_pnp::config::MachineConfig;
    use eh3d_phoenix_pnp::fidcheck::{analyze_marks, pair_marks, residual_table, warnings};
    let design = vec![
        mk("a", 0.0, 0.0),
        mk("b", 100.0, 0.0),
        mk("c", 0.0, 50.0),
        mk("d", 100.0, 50.0),
    ];
    // a mark that is not measured is left out; one that the design does not have is an error
    let (ids, from, to) = pair_marks(&design, &[mk("c", 1.0, 51.0), mk("a", 1.0, 1.0)]).unwrap();
    assert_eq!(ids, ["a", "c"]);
    assert_eq!((from[1], to[1]), ((0.0, 50.0), (1.0, 51.0)));
    let e = pair_marks(&design, &[mk("z", 0.0, 0.0)]).unwrap_err();
    assert_eq!(e.code, Code::FiducialBad);
    let e = pair_marks(&design, &[mk("a", 0.0, 0.0), mk("a", 1.0, 1.0)]).unwrap_err();
    assert!(e.detail.contains("more than once"), "{}", e.detail);
    // the limits are the ones of the configuration
    let cfg = MachineConfig {
        fiducial_max_shift_mm: 1.5,
        ..MachineConfig::default()
    };
    assert_eq!(Limits::from_config(&cfg).max_shift_mm, 1.5);
    assert_eq!(
        Limits::from_config(&MachineConfig::default()),
        Limits::default()
    );
    // marks that are only shifted: all well, a table of ok marks and no warning
    let shifted: Vec<_> = design
        .iter()
        .map(|d| mk(&d.0, d.1 + 0.5, d.2 - 0.25))
        .collect();
    let (a, ids) = analyze_marks(&design, &shifted, &MachineConfig::default()).unwrap();
    assert!(warnings(&a, &ids, 0.5).is_empty());
    let table = residual_table(&a, &ids, 0.5);
    assert_eq!(table.lines().count(), 5);
    assert!(table.lines().skip(1).all(|l| l.ends_with(",ok")), "{table}");
    // one mark taken far from where it is, among enough marks to tell which: it is the suspect, it is over the limit and it is said so
    let mut wide = design.clone();
    wide.push(mk("e", 50.0, 0.0));
    wide.push(mk("f", 50.0, 50.0));
    let mut bad: Vec<_> = wide
        .iter()
        .map(|d| mk(&d.0, d.1 + 0.5, d.2 - 0.25))
        .collect();
    bad[3].1 += 8.0;
    bad[3].2 += 6.0;
    let (a, ids) = analyze_marks(&wide, &bad, &MachineConfig::default()).unwrap();
    let w = warnings(&a, &ids, 0.5);
    assert!(
        w.iter()
            .any(|l| l.contains("the only one that does not agree")),
        "{w:?}"
    );
    assert!(w.iter().any(|l| l.starts_with("mark d is")), "{w:?}");
    assert!(residual_table(&a, &ids, 0.5).contains("d,"));
    assert!(residual_table(&a, &ids, 0.5).contains(
        ",suspect
"
    ));
}
