//! Panels: boards and panels inside panels, each in its place, turned and named.

use eh3d_phoenix_pnp::error::Code;
use eh3d_phoenix_pnp::fiducial::{
    correct, fit_points, measured_csv, parse_measured, solve, FitMode, Transform,
};
use eh3d_phoenix_pnp::panel::Panel;
use eh3d_phoenix_pnp::parts::{from_csv, Part};
use eh3d_phoenix_pnp::units::rotate_xy;
use std::cell::Cell;

const BOARD: &str = "ref,val,package,x,y,rot,side\nR1,10k,R,10,20,0,top\nR2,10k,R,15,20,90,top\n";

const PANEL: &str = r#"{
  "schema_version": 1,
  "boards": { "main": "board.pos" },
  "panels": {
    "top": { "children": [
               { "id": "A", "board": "main", "x": 0, "y": 0, "rotation": 0 },
               { "id": "B", "panel": "pair", "x": 60, "y": 0, "rotation": 90 } ],
             "fiducials": [ { "id": "f1", "x": 2, "y": 2 } ] },
    "pair": { "children": [ { "id": "C", "board": "main", "x": 0, "y": 0, "rotation": 0 } ],
              "fiducials": [ { "id": "g1", "x": 1, "y": 0 } ] }
  },
  "root": "top"
}"#;

fn loader(file: &str) -> Result<Vec<Part>, String> {
    match file {
        "board.pos" => from_csv(BOARD),
        other => Err(format!("{other}: no such board")),
    }
}

fn find<'a>(parts: &'a [Part], reference: &str) -> &'a Part {
    parts
        .iter()
        .find(|p| p.reference == reference)
        .unwrap_or_else(|| panic!("{reference} is not in the panel"))
}

#[test]
fn the_boards_are_put_in_their_places_and_named_by_their_children() {
    let panel = Panel::parse(PANEL).unwrap();
    assert_eq!(
        panel.boards(),
        vec![("main".to_string(), "board.pos".to_string())]
    );
    let e = panel.expand(&loader).unwrap();
    let names: Vec<&str> = e.parts.iter().map(|p| p.reference.as_str()).collect();
    assert_eq!(names, vec!["A/R1", "A/R2", "B/C/R1", "B/C/R2"]);
    let a1 = find(&e.parts, "A/R1");
    assert_eq!((a1.x, a1.y, a1.rotation), (10.0, 20.0, 0.0));
    // a board inside a panel turned a quarter: (10, 20) becomes (-20, 10) and then moves by (60, 0)
    let c1 = find(&e.parts, "B/C/R1");
    assert!(
        (c1.x - 40.0).abs() < 1e-9 && (c1.y - 10.0).abs() < 1e-9,
        "{} {}",
        c1.x,
        c1.y
    );
    assert!((c1.rotation - 90.0).abs() < 1e-9);
    let c2 = find(&e.parts, "B/C/R2");
    assert!((c2.x - 40.0).abs() < 1e-9 && (c2.y - 15.0).abs() < 1e-9);
    assert!((c2.rotation - 180.0).abs() < 1e-9);
    // the fiducials of the panels follow the same transforms
    assert_eq!(e.fiducials.len(), 2);
    assert_eq!(e.fiducials[0], ("f1".to_string(), 2.0, 2.0));
    let g = &e.fiducials[1];
    assert_eq!(g.0, "B/g1");
    assert!(
        (g.1 - 60.0).abs() < 1e-9 && (g.2 - 1.0).abs() < 1e-9,
        "{g:?}"
    );
}

#[test]
fn a_board_used_many_times_is_read_once() {
    let panel = Panel::parse(PANEL).unwrap();
    let calls = Cell::new(0);
    let counting = |file: &str| {
        calls.set(calls.get() + 1);
        loader(file)
    };
    let e = panel.expand(&counting).unwrap();
    assert_eq!(e.parts.len(), 4);
    assert_eq!(calls.get(), 1);
}

#[test]
fn the_same_panel_gives_the_same_parts_every_time() {
    let panel = Panel::parse(PANEL).unwrap();
    assert_eq!(
        panel.expand(&loader).unwrap(),
        panel.expand(&loader).unwrap()
    );
}

#[test]
fn a_panel_turned_as_a_whole_turns_every_part_about_its_origin() {
    for quarter in 0..8 {
        let deg = f64::from(quarter) * 45.0;
        let text = format!(
            r#"{{"boards": {{"main": "board.pos"}}, "panels": {{"p": {{"children": [{{"id": "A", "board": "main", "x": 0, "y": 0, "rotation": {deg}}}]}}}}, "root": "p"}}"#
        );
        let e = Panel::parse(&text).unwrap().expand(&loader).unwrap();
        let plain = from_csv(BOARD).unwrap();
        for (turned, original) in e.parts.iter().zip(&plain) {
            let (x, y) = rotate_xy(original.x, original.y, deg);
            assert!((turned.x - x).abs() < 1e-9 && (turned.y - y).abs() < 1e-9);
            assert!((turned.rotation - (original.rotation + deg).rem_euclid(360.0)).abs() < 1e-9);
            // the distance to the origin does not change
            assert!((turned.x.hypot(turned.y) - original.x.hypot(original.y)).abs() < 1e-9);
        }
    }
}

fn code_of(text: &str) -> Code {
    Panel::parse(text).unwrap_err().code
}

#[test]
fn a_bad_panel_is_refused_with_its_code() {
    let ok = |extra: &str| {
        format!(
            r#"{{"boards": {{"main": "board.pos"}}, "panels": {{"p": {{"children": [{extra}]}}}}, "root": "p"}}"#
        )
    };
    assert_eq!(code_of(""), Code::PanelBad);
    assert_eq!(code_of("[1, 2]"), Code::PanelBad);
    assert_eq!(code_of(r#"{"panels": {}}"#), Code::PanelBad);
    assert_eq!(
        code_of(r#"{"panels": {"p": {"children": []}}, "root": "q"}"#),
        Code::PanelBad
    );
    assert_eq!(
        code_of(r#"{"schema_version": 2, "panels": {"p": {"children": []}}, "root": "p"}"#),
        Code::PanelBad
    );
    // what a child names must exist, and a child names one thing
    assert_eq!(
        code_of(&ok(r#"{"id": "A", "board": "nope"}"#)),
        Code::PanelBad
    );
    assert_eq!(
        code_of(&ok(r#"{"id": "A", "panel": "nope"}"#)),
        Code::PanelBad
    );
    assert_eq!(
        code_of(&ok(r#"{"id": "A", "board": "main", "panel": "p"}"#)),
        Code::PanelBad
    );
    assert_eq!(code_of(&ok(r#"{"id": "A"}"#)), Code::PanelBad);
    // names
    assert_eq!(code_of(&ok(r#"{"board": "main"}"#)), Code::PanelBad);
    assert_eq!(
        code_of(&ok(r#"{"id": "A/B", "board": "main"}"#)),
        Code::PanelBad
    );
    assert_eq!(
        code_of(&ok(r#"{"id": "", "board": "main"}"#)),
        Code::PanelBad
    );
    assert_eq!(
        code_of(&ok(
            r#"{"id": "A", "board": "main"}, {"id": "A", "board": "main"}"#
        )),
        Code::PanelBad
    );
    // numbers
    assert_eq!(
        code_of(&ok(r#"{"id": "A", "board": "main", "x": "far"}"#)),
        Code::PanelBad
    );
    assert_eq!(
        code_of(&ok(r#"{"id": "A", "board": "main", "y": 1e9}"#)),
        Code::PanelBad
    );
    assert!(Panel::parse(&ok(r#"{"id": "A", "board": "main", "rotation": 1000}"#)).is_ok());
}

#[test]
fn a_panel_that_is_too_big_or_holds_itself_is_stopped() {
    let kids: Vec<String> = (0..1001)
        .map(|i| format!(r#"{{"id": "K{i}", "board": "main"}}"#))
        .collect();
    let big = format!(
        r#"{{"boards": {{"main": "board.pos"}}, "panels": {{"p": {{"children": [{}]}}}}, "root": "p"}}"#,
        kids.join(",")
    );
    assert_eq!(code_of(&big), Code::PanelTooBig);
    let selfish = r#"{"panels": {"p": {"children": [{"id": "A", "panel": "p"}, {"id": "B", "panel": "p"}]}}, "root": "p"}"#;
    let panel = Panel::parse(selfish).unwrap();
    assert_eq!(panel.expand(&loader).unwrap_err().code, Code::PanelTooBig);
}

#[test]
fn a_panel_with_too_many_children_in_all_is_stopped() {
    // three levels of 100 children make a million visits
    let many = |panel: &str| -> String {
        let kids: Vec<String> = (0..100)
            .map(|i| format!(r#"{{"id": "K{i}", "panel": "{panel}"}}"#))
            .collect();
        kids.join(",")
    };
    let text = format!(
        r#"{{"panels": {{"a": {{"children": [{}]}}, "b": {{"children": [{}]}}, "c": {{"children": []}}}}, "root": "a"}}"#,
        many("b"),
        many("c")
    );
    let panel = Panel::parse(&text).unwrap();
    // 100 + 100 * 100 = 10100 visits: fine
    assert!(panel.expand(&loader).is_ok());
    let deeper = format!(
        r#"{{"panels": {{"a": {{"children": [{}]}}, "b": {{"children": [{}]}}, "c": {{"children": [{}]}}, "d": {{"children": []}}}}, "root": "a"}}"#,
        many("b"),
        many("c"),
        many("d")
    );
    let e = Panel::parse(&deeper).unwrap().expand(&loader).unwrap_err();
    assert_eq!(e.code, Code::PanelTooBig);
}

#[test]
fn a_board_that_cannot_be_read_is_a_panel_error() {
    let text = r#"{"boards": {"x": "missing.pos"}, "panels": {"p": {"children": [{"id": "A", "board": "x"}]}}, "root": "p"}"#;
    let e = Panel::parse(text).unwrap().expand(&loader).unwrap_err();
    assert_eq!(e.code, Code::PanelBad);
    assert!(e.detail.contains("missing.pos"), "{}", e.detail);
}

#[test]
fn the_order_in_which_the_children_are_listed_gives_the_same_parts() {
    let a = r#"{"boards": {"m": "board.pos"}, "panels": {"p": {"children": [{"id": "A", "board": "m", "x": 0}, {"id": "B", "board": "m", "x": 50}]}}, "root": "p"}"#;
    let b = r#"{"boards": {"m": "board.pos"}, "panels": {"p": {"children": [{"id": "B", "board": "m", "x": 50}, {"id": "A", "board": "m", "x": 0}]}}, "root": "p"}"#;
    let mut pa = Panel::parse(a).unwrap().expand(&loader).unwrap().parts;
    let mut pb = Panel::parse(b).unwrap().expand(&loader).unwrap().parts;
    pa.sort_by(|x, y| x.reference.cmp(&y.reference));
    pb.sort_by(|x, y| x.reference.cmp(&y.reference));
    assert_eq!(pa, pb);
}

// ---- the fiducials of a panel

fn fid_near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

/// Whether two angles are the same angle, whatever the turns they are written with.
fn angle_near(a: f64, b: f64) -> bool {
    ((a - b + 180.0).rem_euclid(360.0) - 180.0).abs() < 1e-9
}

#[test]
fn marks_that_were_only_shifted_give_the_shift_and_no_residual() {
    let design = [(0.0, 0.0), (100.0, 0.0), (0.0, 100.0)];
    let measured = [(1.0, 2.0), (101.0, 2.0), (1.0, 102.0)];
    let fit = fit_points(&design, &measured, FitMode::Auto).unwrap();
    // three marks: the fit is affine
    assert_eq!(fit.mode, FitMode::Affine);
    let t = fit.transform;
    assert!(fid_near(t.a, 1.0) && fid_near(t.b, 0.0) && fid_near(t.c, 0.0) && fid_near(t.d, 1.0));
    assert!(fid_near(t.tx, 1.0) && fid_near(t.ty, 2.0));
    assert!(fid_near(t.rotation_deg(), 0.0));
    let (sx, sy) = t.scale();
    assert!(fid_near(sx, 1.0) && fid_near(sy, 1.0));
    assert_eq!(fit.residuals.len(), 3);
    assert!(fit.max < 1e-9 && fit.rms < 1e-9);
    let moved = t.apply(50.0, 25.0);
    assert!(fid_near(moved.0, 51.0) && fid_near(moved.1, 27.0));
}

#[test]
fn two_marks_fix_a_turn_and_a_shift() {
    // a quarter turn about the origin and a shift of (10, 20): x' = -y + 10, y' = x + 20
    let design = [(0.0, 0.0), (100.0, 0.0)];
    let measured = [(10.0, 20.0), (10.0, 120.0)];
    let fit = fit_points(&design, &measured, FitMode::Auto).unwrap();
    assert_eq!(fit.mode, FitMode::Similarity);
    let t = fit.transform;
    assert!(fid_near(t.a, 0.0) && fid_near(t.b, -1.0) && fid_near(t.c, 1.0) && fid_near(t.d, 0.0));
    assert!(fid_near(t.tx, 10.0) && fid_near(t.ty, 20.0));
    assert!(fid_near(t.rotation_deg(), 90.0));
    let (sx, sy) = t.scale();
    assert!(fid_near(sx, 1.0) && fid_near(sy, 1.0));
    let q = t.apply(100.0, 0.0);
    assert!(fid_near(q.0, 10.0) && fid_near(q.1, 120.0));
    assert!(fit.max < 1e-9);
    // an affine fit needs three marks
    assert_eq!(
        fit_points(&design, &measured, FitMode::Affine)
            .unwrap_err()
            .code,
        Code::FiducialBad
    );
}

#[test]
fn a_similarity_cannot_follow_a_stretch_and_says_how_far_it_misses() {
    // the mark at (100, 0) is measured one per cent further: x' = 1.01 x
    let design = [(0.0, 0.0), (100.0, 0.0), (0.0, 100.0)];
    let measured = [(0.0, 0.0), (101.0, 0.0), (0.0, 100.0)];
    let affine = fit_points(&design, &measured, FitMode::Affine).unwrap();
    assert!(fid_near(affine.transform.a, 1.01) && fid_near(affine.transform.d, 1.0));
    assert!(affine.max < 1e-9);
    let s = fit_points(&design, &measured, FitMode::Similarity).unwrap();
    assert_eq!(s.mode, FitMode::Similarity);
    let t = s.transform;
    // by hand: a = 1.005, the turn is 0.0025, the shift (0.25, -0.25)
    assert!(fid_near(t.a, 1.005) && fid_near(t.d, 1.005));
    assert!(fid_near(t.c, 0.0025) && fid_near(t.b, -0.0025));
    assert!(fid_near(t.tx, 0.25) && fid_near(t.ty, -0.25));
    let diagonal = 0.25 * 2f64.sqrt();
    assert!(fid_near(s.residuals[0], diagonal));
    assert!(fid_near(s.residuals[1], 0.25) && fid_near(s.residuals[2], 0.25));
    assert!(fid_near(s.max, diagonal));
    assert!(fid_near(s.rms, (0.25f64 / 3.0).sqrt()));
}

#[test]
fn the_residual_of_a_mark_that_is_off_is_shared_by_all_four() {
    // a square of marks, the last one measured four millimetres too high: the least squares fit leaves one millimetre on each
    let design = [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)];
    let measured = [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 104.0)];
    let fit = fit_points(&design, &measured, FitMode::Auto).unwrap();
    assert_eq!(fit.mode, FitMode::Affine);
    let t = fit.transform;
    assert!(fid_near(t.a, 1.0) && fid_near(t.b, 0.0) && fid_near(t.tx, 0.0));
    assert!(fid_near(t.c, -0.02) && fid_near(t.d, 1.02) && fid_near(t.ty, 1.0));
    for r in &fit.residuals {
        assert!(fid_near(*r, 1.0), "{r}");
    }
    assert!(fid_near(fit.rms, 1.0) && fid_near(fit.max, 1.0));
}

#[test]
fn marks_that_do_not_fix_a_position_are_refused() {
    let line = [(0.0, 0.0), (50.0, 0.0), (100.0, 0.0)];
    let there = [(1.0, 1.0), (51.0, 1.0), (101.0, 1.0)];
    let bad = |design: &[(f64, f64)], measured: &[(f64, f64)], mode| {
        fit_points(design, measured, mode).unwrap_err().code
    };
    // three marks on a line, one mark, none, one point twice, a different number of marks
    assert_eq!(bad(&line, &there, FitMode::Auto), Code::FiducialBad);
    assert_eq!(bad(&line, &there, FitMode::Affine), Code::FiducialBad);
    assert_eq!(
        bad(&[(1.0, 1.0)], &[(1.0, 1.0)], FitMode::Auto),
        Code::FiducialBad
    );
    assert_eq!(bad(&[], &[], FitMode::Auto), Code::FiducialBad);
    assert_eq!(
        bad(
            &[(5.0, 5.0), (5.0, 5.0)],
            &[(1.0, 1.0), (2.0, 2.0)],
            FitMode::Auto
        ),
        Code::FiducialBad
    );
    assert_eq!(bad(&line, &there[..2], FitMode::Auto), Code::FiducialBad);
    assert_eq!(
        bad(
            &[(0.0, 0.0), (f64::NAN, 1.0)],
            &[(0.0, 0.0), (1.0, 1.0)],
            FitMode::Auto
        ),
        Code::FiducialBad
    );
    // on a line a similarity still fixes the position of the line
    assert!(fit_points(&line, &there, FitMode::Similarity).is_ok());
    assert_eq!(FitMode::parse("affine"), Some(FitMode::Affine));
    assert_eq!(FitMode::parse("loose"), None);
    assert_eq!(FitMode::Similarity.name(), "similarity");
}

fn named(marks: &[(&str, f64, f64)]) -> Vec<(String, f64, f64)> {
    marks
        .iter()
        .map(|(id, x, y)| (id.to_string(), *x, *y))
        .collect()
}

#[test]
fn marks_are_matched_by_name_and_a_fit_that_misses_by_too_much_is_refused() {
    let design = named(&[
        ("f1", 0.0, 0.0),
        ("f2", 100.0, 0.0),
        ("f3", 100.0, 100.0),
        ("f4", 0.0, 100.0),
    ]);
    // measured in another order, and f4 four millimetres off
    let measured = named(&[
        ("f4", 0.0, 104.0),
        ("f2", 100.0, 0.0),
        ("f1", 0.0, 0.0),
        ("f3", 100.0, 100.0),
    ]);
    let fit = solve(&design, &measured, FitMode::Auto, 1.5).unwrap();
    assert_eq!(fit.ids, ["f1", "f2", "f3", "f4"]);
    assert!(fit.residuals.iter().all(|r| fid_near(*r, 1.0)));
    // the limit is 0.5 mm: refused, and the message names the limit
    let e = solve(&design, &measured, FitMode::Auto, 0.5).unwrap_err();
    assert_eq!(e.code, Code::FiducialBad);
    assert!(e.detail.contains("the limit is 0.5 mm"), "{}", e.detail);
    // two marks measured out of four: a similarity of those two
    let two = named(&[("f1", 1.0, 2.0), ("f2", 101.0, 2.0)]);
    let fit = solve(&design, &two, FitMode::Auto, 0.5).unwrap();
    assert_eq!((fit.ids.len(), fit.mode), (2, FitMode::Similarity));
    assert!(fid_near(fit.transform.tx, 1.0) && fid_near(fit.transform.ty, 2.0));
    // a mark the design does not have is a mistake; one mark is too few
    let stray = named(&[("f1", 1.0, 2.0), ("zz", 101.0, 2.0)]);
    let e = solve(&design, &stray, FitMode::Auto, 0.5).unwrap_err();
    assert!(e.detail.contains("'zz'"), "{}", e.detail);
    let one = named(&[("f1", 1.0, 2.0)]);
    assert_eq!(
        solve(&design, &one, FitMode::Auto, 0.5).unwrap_err().code,
        Code::FiducialBad
    );
}

#[test]
fn the_fit_moves_the_parts_and_turns_them() {
    let t = Transform {
        a: 0.0,
        b: -1.0,
        c: 1.0,
        d: 0.0,
        tx: 10.0,
        ty: 20.0,
    };
    let mut parts = [Part {
        reference: "R1".into(),
        value: "10k".into(),
        package: "R".into(),
        x: 50.0,
        y: 50.0,
        rotation: 10.0,
        side: eh3d_phoenix_pnp::parts::Side::Top,
    }];
    correct(&mut parts, &t);
    // (50, 50) becomes (-50 + 10, 50 + 20) and the part is turned a quarter
    assert!(fid_near(parts[0].x, -40.0) && fid_near(parts[0].y, 70.0));
    assert!(angle_near(parts[0].rotation, 100.0));
    // a shift alone leaves the angle as it was
    let mut shifted = [Part {
        rotation: 360.0 - 1e-12,
        ..parts[0].clone()
    }];
    let before = shifted[0].rotation;
    correct(
        &mut shifted,
        &Transform {
            tx: 3.0,
            ..Transform::IDENTITY
        },
    );
    assert_eq!(shifted[0].rotation, before);
    assert!(fid_near(shifted[0].x, -37.0));
}

#[test]
fn the_measured_marks_are_read_from_a_csv_and_written_back() {
    let marks = parse_measured("\u{feff}id;x;y\nf1;1,5;2\n").err();
    // a comma as the decimal mark with a semicolon between fields is not a number
    assert!(marks.is_some());
    let marks = parse_measured("\u{feff}id;x;y\nf1;1.5;2\nf2;-3;4.25\n").unwrap();
    assert_eq!(marks, named(&[("f1", 1.5, 2.0), ("f2", -3.0, 4.25)]));
    assert_eq!(measured_csv(&marks), "id,x,y\nf1,1.5,2\nf2,-3,4.25\n");
    assert_eq!(parse_measured(&measured_csv(&marks)).unwrap(), marks);
    for bad in [
        "",
        "id,x,y\nf1,1\n",
        "id,x\nf1,1\n",
        "id,x,y\nf1,1,2\nf1,3,4\n",
        "id,x,y\n,1,2\n",
        "id,x,y\nf1,one,2\n",
        "id,x,y\nf1,99999,2\n",
        "id,x,y\nf1,NaN,2\n",
    ] {
        assert_eq!(
            parse_measured(bad).unwrap_err().code,
            Code::FiducialBad,
            "{bad:?}"
        );
    }
    // a header only: no marks
    assert!(parse_measured("id,x,y\n").unwrap().is_empty());
}

#[test]
fn the_children_and_the_boards_of_a_panel_can_be_listed() {
    let panel = Panel::parse(PANEL).unwrap();
    assert_eq!(panel.root(), "top");
    let kids = panel.children();
    // the panels in the order of their names, the children as the file has them
    let ids: Vec<&str> = kids.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, ["C", "A", "B"]);
    let panels: Vec<&str> = kids.iter().map(|c| c.panel.as_str()).collect();
    assert_eq!(panels, ["pair", "top", "top"]);
    assert_eq!(
        kids.iter().map(|c| c.is_board).collect::<Vec<_>>(),
        [true, true, false]
    );
    assert_eq!(kids[2].target, "pair");
    assert_eq!((kids[2].x, kids[2].y, kids[2].rotation), (60.0, 0.0, 90.0));
    // the board is placed by A and, inside the panel B, by C
    let counts = panel.board_counts();
    assert_eq!(counts.len(), 1);
    assert_eq!(counts["main"], 2);
}

#[test]
fn the_fiducials_of_a_panel_move_its_parts_to_where_the_panel_really_is() {
    let panel = Panel::parse(PANEL).unwrap();
    let e = panel.expand(&loader).unwrap();
    // f1 of the panel at (2, 2); g1 of the panel inside B: (1, 0) turned a quarter and moved by (60, 0)
    assert_eq!(e.fiducials.len(), 2);
    assert_eq!(e.fiducials[0].0, "f1");
    assert_eq!(e.fiducials[1].0, "B/g1");
    assert!(fid_near(e.fiducials[1].1, 60.0) && fid_near(e.fiducials[1].2, 1.0));
    // the panel sits half a millimetre to the right and a quarter of a millimetre lower
    let measured = named(&[("f1", 2.5, 1.75), ("B/g1", 60.5, 0.75)]);
    let fit = solve(&e.fiducials, &measured, FitMode::Auto, 0.1).unwrap();
    let mut parts = e.parts.clone();
    correct(&mut parts, &fit.transform);
    let a1 = find(&parts, "A/R1");
    assert!(fid_near(a1.x, 10.5) && fid_near(a1.y, 19.75));
    assert!(angle_near(a1.rotation, 0.0));
    let c1 = find(&parts, "B/C/R1");
    assert!(fid_near(c1.x, 40.5) && fid_near(c1.y, 9.75));
    assert!(angle_near(c1.rotation, 90.0));
}
