//! A table of parts turned, flipped or moved as a whole: the places, the angles, the sides, what does not change, and `pos-transform`.

use eh3d_phoenix_pnp::asmexport::{write, Format, Sides, Unit};
use eh3d_phoenix_pnp::parts::{parse, Part, Side};
use eh3d_phoenix_pnp::reorient::{apply, summary, Transform};
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
        "phoenix_pnp_reorient_{name}_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn board() -> Vec<Part> {
    parse(
        &std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/board.pos"),
        )
        .unwrap(),
    )
    .unwrap()
}

fn t(rotate_deg: f64, flip: bool, shift: (f64, f64), zero: bool) -> Transform {
    Transform {
        rotate_deg,
        flip,
        shift,
        zero,
    }
}

fn same(a: &[Part], b: &[Part]) {
    near(a, b, 1e-9);
}

fn near(a: &[Part], b: &[Part], tol: f64) {
    assert_eq!(a.len(), b.len());
    for (p, q) in a.iter().zip(b) {
        assert_eq!((&p.reference, p.side), (&q.reference, q.side));
        assert!(
            (p.x - q.x).abs() < tol && (p.y - q.y).abs() < tol,
            "{}: {} {} / {} {}",
            p.reference,
            p.x,
            p.y,
            q.x,
            q.y
        );
        let d = (p.rotation - q.rotation).rem_euclid(360.0);
        assert!(
            d < tol || 360.0 - d < tol,
            "{}: {} {}",
            p.reference,
            p.rotation,
            q.rotation
        );
    }
}

fn by<'a>(parts: &'a [Part], r: &str) -> &'a Part {
    parts.iter().find(|p| p.reference == r).unwrap()
}

#[test]
fn nothing_to_do_gives_the_table_back() {
    let parts = board();
    same(&apply(&parts, &Transform::default()).unwrap(), &parts);
}

#[test]
fn a_quarter_turn_moves_the_place_counter_clockwise_about_the_origin_and_adds_to_the_angle() {
    let parts = board();
    let turned = apply(&parts, &t(90.0, false, (0.0, 0.0), false)).unwrap();
    let r1 = by(&turned, "R1"); // was X10 Y20, 0 degrees
    assert!(
        (r1.x + 20.0).abs() < 1e-9 && (r1.y - 10.0).abs() < 1e-9,
        "{} {}",
        r1.x,
        r1.y
    );
    assert!((r1.rotation - 90.0).abs() < 1e-9);
    let r3 = by(&turned, "R3"); // 180 degrees becomes 270
    assert!((r3.rotation - 270.0).abs() < 1e-9);
    let c2 = by(&turned, "C2"); // 270 degrees becomes 0
    assert!(c2.rotation.abs() < 1e-9);
    assert_eq!(by(&turned, "D1").side, Side::Bottom);
}

#[test]
fn four_quarter_turns_or_a_turn_and_its_opposite_give_the_table_back() {
    let parts = board();
    let mut p = parts.clone();
    for _ in 0..4 {
        p = apply(&p, &t(90.0, false, (0.0, 0.0), false)).unwrap();
    }
    same(&p, &parts);
    for deg in [-360.0, -123.4, 17.0, 45.0, 359.9] {
        let there = apply(&parts, &t(deg, false, (0.0, 0.0), false)).unwrap();
        let back = apply(&there, &t(-deg, false, (0.0, 0.0), false)).unwrap();
        same(&back, &parts);
    }
}

#[test]
fn a_flip_mirrors_left_to_right_swaps_the_sides_and_mirrors_the_angle_and_twice_gives_the_table_back(
) {
    let parts = board();
    let flipped = apply(&parts, &t(0.0, true, (0.0, 0.0), false)).unwrap();
    let r2 = by(&flipped, "R2"); // X15 Y20 at 90 degrees, top
    assert_eq!((r2.x, r2.y, r2.side), (-15.0, 20.0, Side::Bottom));
    assert!((r2.rotation - 90.0).abs() < 1e-9, "180 - 90 = 90");
    let c2 = by(&flipped, "C2"); // 270 -> -90 -> 270
    assert!((c2.rotation - 270.0).abs() < 1e-9);
    let r1 = by(&flipped, "R1"); // 0 -> 180
    assert!((r1.rotation - 180.0).abs() < 1e-9);
    assert_eq!(by(&flipped, "D1").side, Side::Top);
    same(
        &apply(&flipped, &t(0.0, true, (0.0, 0.0), false)).unwrap(),
        &parts,
    );
}

#[test]
fn the_order_is_flip_then_turn_then_move_then_zero() {
    let parts = board();
    let all = apply(&parts, &t(90.0, true, (100.0, 50.0), false)).unwrap();
    let step1 = apply(&parts, &t(0.0, true, (0.0, 0.0), false)).unwrap();
    let step2 = apply(&step1, &t(90.0, false, (0.0, 0.0), false)).unwrap();
    let step3 = apply(&step2, &t(0.0, false, (100.0, 50.0), false)).unwrap();
    same(&all, &step3);
    let zeroed = apply(&parts, &t(90.0, false, (100.0, 50.0), true)).unwrap();
    let lowest_x = zeroed.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
    let lowest_y = zeroed.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
    assert!(lowest_x.abs() < 1e-9 && lowest_y.abs() < 1e-9);
    // the shift before the zero changes nothing in the end
    same(
        &zeroed,
        &apply(&parts, &t(90.0, false, (0.0, 0.0), true)).unwrap(),
    );
}

#[test]
fn the_distances_between_the_parts_do_not_change() {
    let parts = board();
    let moved = apply(&parts, &t(37.5, true, (12.0, -7.0), true)).unwrap();
    for i in 0..parts.len() {
        for j in 0..i {
            let d = |p: &Part, q: &Part| (p.x - q.x).hypot(p.y - q.y);
            assert!((d(&parts[i], &parts[j]) - d(&moved[i], &moved[j])).abs() < 1e-9);
        }
    }
}

#[test]
fn a_number_that_is_not_one_or_a_place_beyond_reach_is_an_error_and_an_empty_table_is_fine() {
    let parts = board();
    assert!(apply(&parts, &t(f64::NAN, false, (0.0, 0.0), false)).is_err());
    assert!(apply(&parts, &t(0.0, false, (f64::INFINITY, 0.0), false)).is_err());
    let e = apply(&parts, &t(0.0, false, (2.0e6, 0.0), false)).unwrap_err();
    assert!(e.contains("beyond"), "{e}");
    assert!(apply(&[], &t(10.0, true, (1.0, 1.0), true))
        .unwrap()
        .is_empty());
    assert_eq!(summary(&[]), "no parts");
}

#[test]
fn the_summary_gives_the_box_of_the_centres() {
    assert_eq!(
        summary(&board()),
        "7 parts, centres from X10.000 Y10.000 to X40.000 Y30.000"
    );
}

#[test]
fn the_command_prints_a_table_that_the_program_reads_back_and_writes_it_to_a_new_file() {
    let o = run(&[
        "pos-transform",
        "examples/board.pos",
        "--rotate",
        "90",
        "--zero",
    ]);
    assert!(o.status.success(), "{}", err(&o));
    let expected = write(
        &apply(&board(), &t(90.0, false, (0.0, 0.0), true)).unwrap(),
        Format::KicadPos,
        Unit::Mm,
        Sides::All,
    );
    assert_eq!(out(&o), expected);
    let dir = temp("out");
    let file = dir.join("turned.pos").to_string_lossy().into_owned();
    let o = run(&[
        "pos-transform",
        "examples/board.pos",
        "--flip",
        "--shift",
        "5,-2.5",
        "--format",
        "generic",
        "--unit",
        "inch",
        "--out",
        &file,
    ]);
    assert!(o.status.success(), "{}", err(&o));
    assert!(out(&o).contains("7 parts, centres from X"), "{}", out(&o));
    let back = parse(&std::fs::read_to_string(&file).unwrap()).unwrap();
    // inches are written to six digits
    near(
        &back_sorted(&back),
        &back_sorted(&apply(&board(), &t(0.0, true, (5.0, -2.5), false)).unwrap()),
        1e-4,
    );
    let again = run(&["pos-transform", "examples/board.pos", "--out", &file]);
    assert_eq!(again.status.code(), Some(1));
    assert!(err(&again).contains("PHOENIX-PNP-040"));
    let _ = std::fs::remove_dir_all(dir);
}

/// The parts in the order of their references (the files are written in that order).
fn back_sorted(parts: &[Part]) -> Vec<Part> {
    let mut p = parts.to_vec();
    p.sort_by(|a, b| a.reference.cmp(&b.reference));
    p
}

#[test]
fn the_command_checks_its_options() {
    for args in [
        vec!["pos-transform"],
        vec!["pos-transform", "examples/board.pos", "--rotate", "361"],
        vec!["pos-transform", "examples/board.pos", "--rotate", "x"],
        vec!["pos-transform", "examples/board.pos", "--shift", "5"],
        vec!["pos-transform", "examples/board.pos", "--shift", "a,b"],
        vec!["pos-transform", "examples/board.pos", "--format", "xml"],
        vec!["pos-transform", "examples/board.pos", "--unit", "cm"],
        vec![
            "pos-transform",
            "examples/board.pos",
            "--shift",
            "2000000,0",
        ],
        vec!["pos-transform", "examples/nope.pos"],
        vec!["pos-transform", "examples/board.pos", "extra"],
    ] {
        let o = run(&args);
        assert_eq!(o.status.code(), Some(1), "{args:?}");
        assert!(err(&o).starts_with("error:"), "{args:?}");
    }
}
