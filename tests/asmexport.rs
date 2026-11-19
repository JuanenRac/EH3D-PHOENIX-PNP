//! The parts in the shapes that assemblers and other programs ask for: the columns, the units, the sides, the order, what is read back, and `export-assembler`.

use eh3d_phoenix_pnp::asmexport::{count, write, Format, Sides, Unit};
use eh3d_phoenix_pnp::parts::{parse, Part, Side};
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
    let dir = std::env::temp_dir().join(format!("phoenix_pnp_asm_{name}_{}", std::process::id()));
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

fn same(a: &[Part], b: &[Part], tol: f64) {
    let mut a = a.to_vec();
    let mut b = b.to_vec();
    a.sort_by(|x, y| x.reference.cmp(&y.reference));
    b.sort_by(|x, y| x.reference.cmp(&y.reference));
    assert_eq!(a.len(), b.len());
    for (p, q) in a.iter().zip(&b) {
        assert_eq!(
            (&p.reference, &p.value, &p.package, p.side),
            (&q.reference, &q.value, &q.package, q.side)
        );
        assert!(
            (p.x - q.x).abs() <= tol && (p.y - q.y).abs() <= tol,
            "{} {} {}",
            p.reference,
            p.x,
            q.x
        );
        let turn = (p.rotation - q.rotation).rem_euclid(360.0);
        assert!(
            turn <= tol || 360.0 - turn <= tol,
            "{} {} {}",
            p.reference,
            p.rotation,
            q.rotation
        );
    }
}

#[test]
fn the_component_placement_list_has_its_columns_the_unit_on_the_place_and_the_layer_in_capitals() {
    let text = write(&board(), Format::Cpl, Unit::Mm, Sides::All);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines[0],
        "Designator,Val,Package,Mid X,Mid Y,Rotation,Layer"
    );
    assert_eq!(lines.len(), 8);
    // in the order of the references
    assert_eq!(lines[1], "C1,100n,C_0603,10.0000mm,30.0000mm,0.00,Top");
    assert_eq!(lines[3], "D1,LED,LED_0603,30.0000mm,10.0000mm,0.00,Bottom");
    assert_eq!(lines[5], "R2,10k,R_0603,15.0000mm,20.0000mm,90.00,Top");
}

#[test]
fn the_sides_are_chosen_and_counted() {
    let parts = board();
    assert_eq!(
        (
            count(&parts, Sides::All),
            count(&parts, Sides::Only(Side::Top)),
            count(&parts, Sides::Only(Side::Bottom))
        ),
        (7, 6, 1)
    );
    let top = write(&parts, Format::Generic, Unit::Mm, Sides::Only(Side::Top));
    assert_eq!(top.lines().count(), 7);
    assert!(!top.contains("D1"));
    let bottom = write(&parts, Format::Generic, Unit::Mm, Sides::Only(Side::Bottom));
    assert_eq!(bottom.lines().count(), 2);
    assert!(bottom.contains("D1,30.0000,10.0000,0.0000,bottom,LED,LED_0603"));
}

#[test]
fn inches_are_the_millimetres_over_25_4_with_the_unit_in_the_header() {
    let parts = board();
    let g = write(&parts, Format::Generic, Unit::Inch, Sides::All);
    assert!(
        g.starts_with("reference,x(in),y(in),rotation,side,value,package\n"),
        "{g}"
    );
    assert!(
        g.contains("C1,0.393701,1.181102,0.0000,top,100n,C_0603"),
        "{g}"
    );
    let k = write(&parts, Format::KicadPos, Unit::Inch, Sides::All);
    assert!(k.contains("## Unit = inch"), "{k}");
    assert!(write(&parts, Format::Cpl, Unit::Inch, Sides::All).contains("0.393701in,1.181102in"));
}

#[test]
fn what_kicad_pos_and_generic_write_is_read_back_to_the_digits_that_are_written() {
    let parts = board();
    for unit in [Unit::Mm, Unit::Inch] {
        let kicad = parse(&write(&parts, Format::KicadPos, unit, Sides::All)).unwrap();
        same(&parts, &kicad, 1e-4);
        // the unit is in the name of the columns of the plain table, and the reader of tables takes it from there
        let generic = parse(&write(&parts, Format::Generic, unit, Sides::All)).unwrap();
        same(&parts, &generic, 1e-4);
    }
}

#[test]
fn the_turn_is_always_from_0_up_to_360_and_a_text_with_spaces_or_nothing_is_one_word() {
    let mut parts = board();
    parts[0].rotation = -90.0;
    parts[1].rotation = 360.0;
    parts[2].rotation = 725.5;
    parts[3].value = "4.7 uF".into();
    parts[4].package = String::new();
    let k = write(&parts, Format::KicadPos, Unit::Mm, Sides::All);
    let back = parse(&k).unwrap();
    let by = |r: &str| back.iter().find(|p| p.reference == r).unwrap();
    assert_eq!(by("R1").rotation, 270.0);
    assert_eq!(by("R2").rotation, 0.0);
    assert!((by("R3").rotation - 5.5).abs() < 1e-9);
    assert_eq!(by("C1").value, "4.7_uF");
    assert_eq!(by("C2").package, "-");
}

#[test]
fn a_cell_with_a_comma_or_a_quote_is_quoted_in_the_tables_with_commas() {
    let mut parts = board();
    parts[0].value = "10k, 1%".into();
    parts[1].package = "R \"0603\"".into();
    for format in [Format::Cpl, Format::Generic] {
        let t = write(&parts, format, Unit::Mm, Sides::All);
        assert!(t.contains("\"10k, 1%\""), "{t}");
        assert!(t.contains("\"R \"\"0603\"\"\""), "{t}");
        for line in t.lines().skip(1) {
            let mut cells = 0;
            let mut quoted = false;
            for c in line.chars() {
                match c {
                    '"' => quoted = !quoted,
                    ',' if !quoted => cells += 1,
                    _ => {}
                }
            }
            assert_eq!(cells, 6, "{line}");
        }
    }
}

#[test]
fn the_names_of_the_shapes_and_units_are_read_in_any_case() {
    for f in Format::ALL {
        assert_eq!(Format::parse(f.name()), Some(f));
        assert_eq!(Format::parse(&f.name().to_ascii_uppercase()), Some(f));
    }
    assert_eq!(Format::parse("kicad_pos"), Some(Format::KicadPos));
    assert_eq!(Format::parse("csv"), Some(Format::Generic));
    assert_eq!(Format::parse("xml"), None);
    assert_eq!(
        (
            Unit::parse("MM"),
            Unit::parse("in"),
            Unit::parse("Inch"),
            Unit::parse("cm")
        ),
        (Some(Unit::Mm), Some(Unit::Inch), Some(Unit::Inch), None)
    );
    assert_eq!(
        (
            Sides::parse("TOP"),
            Sides::parse("all"),
            Sides::parse("both")
        ),
        (Some(Sides::Only(Side::Top)), Some(Sides::All), None)
    );
}

#[test]
fn an_empty_table_is_a_header_and_nothing_else() {
    let none: Vec<Part> = Vec::new();
    assert_eq!(
        write(&none, Format::Cpl, Unit::Mm, Sides::All)
            .lines()
            .count(),
        1
    );
    assert_eq!(
        write(&none, Format::Generic, Unit::Mm, Sides::All)
            .lines()
            .count(),
        1
    );
    assert_eq!(count(&none, Sides::All), 0);
}

#[test]
fn the_command_prints_or_writes_in_the_shape_asked_for() {
    let o = run(&["export-assembler", "examples/board.pos"]);
    assert!(o.status.success(), "{}", err(&o));
    assert_eq!(out(&o), write(&board(), Format::Cpl, Unit::Mm, Sides::All));
    let o = run(&[
        "export-assembler",
        "examples/board.pos",
        "--format",
        "generic",
        "--side",
        "top",
        "--unit",
        "inch",
    ]);
    assert_eq!(
        out(&o),
        write(
            &board(),
            Format::Generic,
            Unit::Inch,
            Sides::Only(Side::Top)
        )
    );
    let dir = temp("cmd");
    let file = dir.join("board_cpl.csv").to_string_lossy().into_owned();
    let o = run(&["export-assembler", "examples/board.pos", "--out", &file]);
    assert!(o.status.success(), "{}", err(&o));
    assert!(
        out(&o).contains(&format!("written to {file}"))
            && out(&o).contains("7 parts in the shape cpl"),
        "{}",
        out(&o)
    );
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        write(&board(), Format::Cpl, Unit::Mm, Sides::All)
    );
    let again = run(&["export-assembler", "examples/board.pos", "--out", &file]);
    assert_eq!(again.status.code(), Some(1));
    assert!(err(&again).contains("PHOENIX-PNP-040"));
    let same_file = run(&[
        "export-assembler",
        "examples/board.pos",
        "--out",
        "examples/board.pos",
    ]);
    assert_eq!(same_file.status.code(), Some(1));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_command_refuses_what_it_does_not_know() {
    for args in [
        vec!["export-assembler"],
        vec!["export-assembler", "examples/board.pos", "--format", "xml"],
        vec!["export-assembler", "examples/board.pos", "--side", "left"],
        vec!["export-assembler", "examples/board.pos", "--unit", "cm"],
        vec!["export-assembler", "examples/nope.pos"],
        vec!["export-assembler", "examples/board.pos", "extra"],
    ] {
        let o = run(&args);
        assert_eq!(o.status.code(), Some(1), "{args:?}");
        assert!(err(&o).starts_with("error:"));
    }
}
