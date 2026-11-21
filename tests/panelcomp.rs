//! The composition of a panel: the sheet, its four rails, the outlines of the boards and the units put at a place and turned. The checks (inside the rails, no overlap, a least distance), the
//! panel file that comes out (stable, and read back by the reader of panels) and the command `panel-compose`.

use eh3d_phoenix_pnp::panel::Panel;
use eh3d_phoenix_pnp::panelcomp::{num, Composition, Finding};
use eh3d_phoenix_pnp::parts::{from_csv, Part};
use std::path::PathBuf;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_phoenix-pnp");
const EXAMPLE: &str = include_str!("../examples/panel_compose.json");

fn spec(units: &str) -> String {
    format!(
        r#"{{ "schema_version": 1,
  "panel": {{ "width": 100, "height": 80, "rails": {{ "left": 5, "right": 5, "top": 5, "bottom": 5 }} }},
  "boards": {{ "m": {{ "file": "m.pos", "width": 40, "height": 30 }}, "s": {{ "file": "s.pos", "width": 10, "height": 10 }} }},
  "units": [{units}],
  "fiducials": [ {{ "id": "f1", "x": 2, "y": 2 }} ] }}"#
    )
}

fn unit(id: &str, board: &str, x: f64, y: f64, rot: f64) -> String {
    format!(r#"{{ "id": "{id}", "board": "{board}", "x": {x}, "y": {y}, "rotation": {rot} }}"#)
}

fn comp(units: &[String]) -> Composition {
    Composition::parse(&spec(&units.join(","))).unwrap()
}

#[test]
fn the_example_is_sound_and_the_boxes_of_its_units_are_where_the_turn_puts_them() {
    let c = Composition::parse(EXAMPLE).unwrap();
    assert!(c.check(1.0).is_empty(), "{:?}", c.check(1.0));
    assert_eq!(c.usable(), (5.0, 5.0, 115.0, 95.0));
    let b = c.units.iter().find(|u| u.id == "B").unwrap();
    // 40 by 30 turned a quarter about its origin: it lies to the left of its origin and above it
    let corners = c.corners(b);
    let xs: Vec<f64> = corners.iter().map(|p| p.0).collect();
    let ys: Vec<f64> = corners.iter().map(|p| p.1).collect();
    let near = |a: f64, b: f64| (a - b).abs() < 1e-9;
    assert!(near(xs.iter().cloned().fold(f64::INFINITY, f64::min), 82.0));
    assert!(near(
        xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
        112.0
    ));
    assert!(near(ys.iter().cloned().fold(f64::INFINITY, f64::min), 8.0));
    assert!(near(
        ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
        48.0
    ));
    let text = c.text(1.0);
    assert!(text.contains("the composition is sound"), "{text}");
    assert!(text.contains("4 units cover"), "{text}");
}

#[test]
fn a_unit_must_lie_wholly_between_the_rails_and_touching_the_rail_is_inside() {
    // the board of 40 by 30 at 5,5 touches the rails
    assert!(comp(&[unit("A", "m", 5.0, 5.0, 0.0)]).check(1.0).is_empty());
    // 0.5 mm to the left, and the top over the rail
    let f = comp(&[unit("A", "m", 4.5, 5.0, 0.0)]).check(1.0);
    assert!(
        matches!(&f[..], [Finding::Outside { unit, over_mm }] if unit == "A" && (over_mm - 0.5).abs() < 1e-9),
        "{f:?}"
    );
    let f = comp(&[unit("A", "m", 5.0, 45.5, 0.0)]).check(1.0);
    assert!(
        matches!(&f[..], [Finding::Outside { over_mm, .. }] if (over_mm - 0.5).abs() < 1e-9),
        "{f:?}"
    );
    // turned a half, the board lies to the left and below its origin
    assert!(comp(&[unit("A", "m", 45.0, 35.0, 180.0)])
        .check(1.0)
        .is_empty());
    assert_eq!(
        comp(&[unit("A", "m", 44.0, 35.0, 180.0)]).check(1.0).len(),
        1
    );
    // a turn that is not a quarter makes a larger box
    assert_eq!(comp(&[unit("A", "m", 20.0, 5.0, 45.0)]).check(1.0).len(), 1);
}

#[test]
fn two_units_that_overlap_or_stand_closer_than_the_least_distance_are_found() {
    let a = unit("A", "m", 5.0, 5.0, 0.0);
    let overlapping = comp(&[a.clone(), unit("B", "s", 40.0, 10.0, 0.0)]).check(1.0);
    assert_eq!(
        overlapping,
        [Finding::Overlap {
            a: "A".into(),
            b: "B".into()
        }]
    );
    // 0.4 mm apart: no overlap, but closer than 1 mm
    let close = comp(&[a.clone(), unit("B", "s", 45.4, 10.0, 0.0)]).check(1.0);
    assert!(
        matches!(&close[..], [Finding::TooClose { mm, .. }] if (mm - 0.4).abs() < 1e-6),
        "{close:?}"
    );
    // the same with a least distance of 0.4 or less: sound; edges that touch do not overlap
    assert!(comp(&[a.clone(), unit("B", "s", 45.4, 10.0, 0.0)])
        .check(0.4)
        .is_empty());
    let touching = comp(&[a.clone(), unit("B", "s", 45.0, 10.0, 0.0)]);
    assert!(touching.check(0.0).is_empty());
    assert_eq!(touching.check(1.0).len(), 1);
    // a unit turned 45 degrees: its box meets the box of the board beside it, its shape does not (the corner of A is 1.41 mm from its edge)
    let tilted = comp(&[a.clone(), unit("B", "s", 52.0, 30.0, 45.0)]);
    assert!(tilted.check(1.0).is_empty(), "{:?}", tilted.check(1.0));
    assert!(
        matches!(&tilted.check(2.0)[..], [Finding::TooClose { mm, .. }] if (mm - 2.0_f64.sqrt()).abs() < 1e-6)
    );
    // moved 4 mm to the left, the corner of A is inside it
    let reaching = comp(&[a, unit("B", "s", 48.0, 30.0, 45.0)]).check(0.0);
    assert_eq!(
        reaching,
        [Finding::Overlap {
            a: "A".into(),
            b: "B".into()
        }]
    );
    for f in &overlapping {
        assert!(f.text(1.0).contains("overlap"));
    }
}

fn inside(poly: &[(f64, f64); 4], p: (f64, f64)) -> bool {
    // strictly inside a convex polygon whose corners run counter-clockwise
    (0..4).all(|i| {
        let (a, b) = (poly[i], poly[(i + 1) % 4]);
        (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0) > 1e-9
    })
}

#[test]
fn the_check_agrees_with_a_count_of_points_on_pairs_of_units_chosen_by_a_fixed_seed() {
    let mut x: u64 = 0x1234_5678_9ABC_DEF1;
    let mut rnd = move |n: u64| {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x % n
    };
    let (mut overlaps, mut apart) = (0, 0);
    for _ in 0..300 {
        let ax = 10.0 + rnd(40) as f64;
        let ay = 10.0 + rnd(30) as f64;
        let bx = 10.0 + rnd(60) as f64;
        let by = 10.0 + rnd(40) as f64;
        let (ra, rb) = (rnd(360) as f64, rnd(360) as f64);
        let c = comp(&[unit("A", "s", ax, ay, ra), unit("B", "s", bx, by, rb)]);
        let (pa, pb) = (c.corners(&c.units[0]), c.corners(&c.units[1]));
        // points of A (a grid over its board, turned and moved) that lie strictly inside B
        let mut hit = false;
        for i in 1..20 {
            for j in 1..20 {
                let (u, v) = (f64::from(i) / 20.0 * 10.0, f64::from(j) / 20.0 * 10.0);
                let (s, t) = (ra.to_radians().sin(), ra.to_radians().cos());
                let p = (ax + u * t - v * s, ay + u * s + v * t);
                if inside(&pb, p) {
                    hit = true;
                }
            }
        }
        let found = c
            .check(0.0)
            .iter()
            .any(|f| matches!(f, Finding::Overlap { .. }));
        if hit {
            assert!(
                found,
                "a point of A is inside B but no overlap was found: {ra} {rb} {pa:?} {pb:?}"
            );
            overlaps += 1;
        } else if !found {
            apart += 1;
        }
    }
    assert!(overlaps > 10 && apart > 10, "{overlaps} {apart}");
}

#[test]
fn the_panel_file_is_the_same_whatever_the_order_of_the_units_and_the_numbers_are_written_one_way()
{
    let a = unit("A", "m", 5.0, 5.0, 0.0);
    let b = unit("B", "s", 60.0, 10.0, 450.0 - 360.0);
    let c = unit("C", "m", 50.0, 40.0, -90.0);
    let one = comp(&[a.clone(), b.clone(), c.clone()]).to_panel_json();
    assert_eq!(
        comp(&[c.clone(), a.clone(), b.clone()]).to_panel_json(),
        one
    );
    assert_eq!(comp(&[b, c, a]).to_panel_json(), one);
    // a turn of -90 is 270; a turn of 360 is 0; the numbers have at most four decimals and no -0
    assert!(
        one.contains(r#""id": "C", "board": "m", "x": 50, "y": 40, "rotation": 270"#),
        "{one}"
    );
    assert_eq!(num(-0.00001), "0");
    assert_eq!(num(1.23456), "1.2346");
    assert_eq!(num(2.5), "2.5");
    assert_eq!(num(100.0), "100");
    let r = comp(&[unit("A", "m", 5.0, 5.0, 360.0)]).to_panel_json();
    assert!(r.contains(r#""rotation": 0 }"#), "{r}");
    assert!(Composition::parse(&spec(&unit("A", "m", 5.0, 5.0, 361.0))).is_err());
    assert!(Composition::parse(&spec(&unit("A", "m", 5.0, 5.0, -361.0))).is_err());
    // a board no unit uses is not in the file
    let only_m = comp(&[unit("A", "m", 5.0, 5.0, 0.0)]).to_panel_json();
    assert!(
        only_m.contains("m.pos") && !only_m.contains("s.pos"),
        "{only_m}"
    );
}

fn loader(file: &str) -> Result<Vec<Part>, String> {
    match file {
        "m.pos" => {
            from_csv("ref,val,package,x,y,rot,side\nR1,10k,R,10,20,0,top\nR2,10k,R,15,20,90,top\n")
        }
        "s.pos" => from_csv("ref,val,package,x,y,rot,side\nC1,1u,C,1,2,0,top\n"),
        other => Err(format!("{other}: no such board")),
    }
}

#[test]
fn the_panel_file_is_read_by_the_reader_of_panels_with_a_different_design_in_each_unit() {
    let c = comp(&[
        unit("A", "m", 5.0, 5.0, 0.0),
        unit("B", "s", 60.0, 10.0, 90.0),
        unit("C", "m", 95.0, 75.0, 180.0),
    ]);
    assert!(c.check(1.0).is_empty(), "{:?}", c.check(1.0));
    let panel = Panel::parse(&c.to_panel_json()).unwrap();
    assert_eq!(panel.children().len(), 3);
    let counts = panel.board_counts();
    assert_eq!((counts["m"], counts["s"]), (2, 1));
    let e = panel.expand(&loader).unwrap();
    assert_eq!(e.parts.len(), 2 + 1 + 2);
    assert_eq!(e.fiducials.len(), 1);
    // the second part of board A is where the composition puts it: the origin plus the place
    let r2 = e.parts.iter().find(|p| p.reference == "A/R2").unwrap();
    assert!(
        (r2.x - 20.0).abs() < 1e-9 && (r2.y - 25.0).abs() < 1e-9,
        "{} {}",
        r2.x,
        r2.y
    );
    // board C is turned a half: R1 at 10,20 goes to 95-10, 75-20
    let r1 = e.parts.iter().find(|p| p.reference == "C/R1").unwrap();
    assert!(
        (r1.x - 85.0).abs() < 1e-9 && (r1.y - 55.0).abs() < 1e-9,
        "{} {}",
        r1.x,
        r1.y
    );
}

#[test]
fn a_composition_that_is_not_one_is_refused_with_what_is_wrong() {
    let ok = spec(&unit("A", "m", 5.0, 5.0, 0.0));
    assert!(Composition::parse(&ok).is_ok());
    for (bad, fragment) in [
        ("{", "not JSON"),
        (r#"{"schema_version":2}"#, "schema_version"),
        (r#"{"schema_version":1}"#, "no 'panel'"),
        (&ok.replace("\"width\": 100", "\"width\": 0"), "size"),
        (
            &ok.replace("\"left\": 5, \"right\": 5", "\"left\": 60, \"right\": 50"),
            "no room",
        ),
        (
            &ok.replace("\"board\": \"m\"", "\"board\": \"zz\""),
            "not in 'boards'",
        ),
        (
            &ok.replace("\"id\": \"A\"", "\"id\": \"A/B\""),
            "unfit name",
        ),
        (
            &ok.replace("\"file\": \"m.pos\"", "\"file\": \"\""),
            "no 'file'",
        ),
        (&ok.replace("\"x\": 5", "\"x\": 99999"), "number up to"),
        (&ok.replace("\"width\": 40", "\"width\": 5000"), "size"),
    ] {
        let e = Composition::parse(bad).unwrap_err().to_string();
        assert!(e.contains(fragment), "{fragment}: {e}");
    }
    let twice = spec(&format!(
        "{},{}",
        unit("A", "m", 5.0, 5.0, 0.0),
        unit("A", "s", 60.0, 5.0, 0.0)
    ));
    assert!(Composition::parse(&twice)
        .unwrap_err()
        .to_string()
        .contains("used twice"));
    let many: Vec<String> = (0..1001)
        .map(|i| unit(&format!("U{i}"), "s", 6.0, 6.0, 0.0))
        .collect();
    assert!(Composition::parse(&spec(&many.join(",")))
        .unwrap_err()
        .to_string()
        .contains("1000"));
    // a fiducial off the sheet is a finding
    let off = ok.replace("\"x\": 2, \"y\": 2", "\"x\": 200, \"y\": 2");
    assert_eq!(
        Composition::parse(&off).unwrap().check(1.0),
        [Finding::FiducialOff { id: "f1".into() }]
    );
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn run(args: &[&str]) -> (i32, String, String) {
    let out = Command::new(BIN)
        .arg("panel-compose")
        .args(args)
        .current_dir(root())
        .output()
        .unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn the_command_checks_prints_and_writes_the_panel_file_but_never_over_a_file_or_a_panel_that_does_not_fit(
) {
    let dir = std::env::temp_dir().join(format!("phoenix_pnp_pcomp_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("panel.json");
    let (code, stdout, _) = run(&[
        "examples/panel_compose.json",
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{stdout}");
    assert!(stdout.contains("the composition is sound"));
    let written = std::fs::read_to_string(&out).unwrap();
    assert!(Panel::parse(&written).is_ok());
    // the same input, the same bytes
    let again = dir.join("again.json");
    run(&[
        "examples/panel_compose.json",
        "--out",
        again.to_str().unwrap(),
    ]);
    assert_eq!(std::fs::read_to_string(&again).unwrap(), written);
    // without --out the panel file is printed after the report
    let (code, stdout, _) = run(&["examples/panel_compose.json"]);
    assert_eq!(code, 0);
    assert!(stdout.ends_with(&written), "{stdout}");
    // a file that exists is not written over
    let (code, _, err) = run(&[
        "examples/panel_compose.json",
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code, 1, "{err}");
    // a composition that does not fit ends with 2 and writes nothing
    let bad = dir.join("bad.json");
    std::fs::write(
        &bad,
        spec(&format!(
            "{},{}",
            unit("A", "m", 5.0, 5.0, 0.0),
            unit("B", "m", 20.0, 5.0, 0.0)
        )),
    )
    .unwrap();
    let nothing = dir.join("nothing.json");
    let (code, stdout, err) = run(&[bad.to_str().unwrap(), "--out", nothing.to_str().unwrap()]);
    assert_eq!(code, 2, "{stdout}{err}");
    assert!(stdout.contains("PROBLEM: A and B overlap"), "{stdout}");
    assert!(!nothing.exists());
    // a least distance that is asked for on the line
    let apart = dir.join("apart.json");
    std::fs::write(
        &apart,
        spec(&format!(
            "{},{}",
            unit("A", "m", 5.0, 5.0, 0.0),
            unit("B", "m", 46.5, 5.0, 0.0)
        )),
    )
    .unwrap();
    assert_eq!(run(&[apart.to_str().unwrap()]).0, 0);
    assert_eq!(run(&[apart.to_str().unwrap(), "--clearance", "2"]).0, 2);
    assert_eq!(run(&[apart.to_str().unwrap(), "--clearance", "99"]).0, 1);
    assert_eq!(run(&[]).0, 1);
    assert_eq!(run(&["no_such_file.json"]).0, 1);
    let _ = std::fs::remove_dir_all(&dir);
}
