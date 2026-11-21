//! The size of the body of a package read from a footprint file of KiCad: the drawing of the body, the courtyard that stands in for it, the old format, the damaged files, the limits, the
//! row of the table of packages and the command `package-kicad`.

use eh3d_phoenix_pnp::catalog::Catalog;
use eh3d_phoenix_pnp::error::Code;
use eh3d_phoenix_pnp::kicadmod::{
    height_hint, package_spec, parse_footprint, BodySource, MAX_BYTES,
};
use eh3d_phoenix_pnp::pkggen::table_csv;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const EXAMPLE: &str = include_str!("../examples/chip_0603.kicad_mod");

fn fp(text: &str) -> eh3d_phoenix_pnp::kicadmod::Footprint {
    parse_footprint(text).unwrap_or_else(|e| panic!("{}", e.detail))
}

fn refused(text: &str) -> String {
    let e = parse_footprint(text).unwrap_err();
    assert_eq!(e.code, Code::CatalogBadRow);
    e.detail
}

// ---- the drawing of the body

#[test]
fn the_body_is_the_box_of_the_drawing_on_the_fabrication_layer() {
    let f = fp(EXAMPLE);
    assert_eq!(f.name, "R_0603_1608Metric");
    assert_eq!((f.body_long_mm, f.body_short_mm), (1.6, 0.825));
    assert_eq!(f.source, BodySource::Fab);
    assert_eq!(f.pads, 2);
    assert!(f.note().is_none());
    // the name of a library in front is taken off, and the file may have Windows line endings and a byte order mark
    let crlf = format!("\u{feff}{}", EXAMPLE.replace('\n', "\r\n")).replace(
        "(footprint \"R_0603_1608Metric\"",
        "(footprint \"Resistor_SMD:R_0603_1608Metric\"",
    );
    assert_eq!(fp(&crlf), f);
    // the bottom side
    let bottom = EXAMPLE.replace("\"F.Fab\"", "\"B.Fab\"");
    assert_eq!(fp(&bottom).source, BodySource::Fab);
}

#[test]
fn every_kind_of_drawing_counts_in_the_box() {
    let wrap = |body: &str| format!("(footprint \"X\" (layer \"F.Cu\") {body} (pad 1 smd rect))");
    // a polygon
    let poly = wrap("(fp_poly (pts (xy 0 0) (xy 4 0) (xy 4 2) (xy 0 2)) (layer \"F.Fab\"))");
    assert_eq!(
        (fp(&poly).body_long_mm, fp(&poly).body_short_mm),
        (4.0, 2.0)
    );
    // a circle, in the old way of writing a layer
    let circle = wrap("(fp_circle (center 1 1) (end 1 4) (layer F.Fab))");
    assert_eq!(
        (fp(&circle).body_long_mm, fp(&circle).body_short_mm),
        (6.0, 6.0)
    );
    // an arc with a middle point, and an old arc with an angle
    let mid = wrap("(fp_arc (start 0 0) (mid 1 1) (end 2 0) (layer \"F.Fab\"))");
    assert_eq!((fp(&mid).body_long_mm, fp(&mid).body_short_mm), (2.0, 1.0));
    let old = wrap("(fp_arc (start 0 0) (end 3 0) (angle 90) (layer F.Fab))");
    let o = fp(&old);
    assert!((o.body_long_mm - 3.0).abs() < 0.02 && (o.body_short_mm - 3.0).abs() < 0.02);
    // a curve counts by its points; lines of other layers and text do not count
    let curve = wrap(
        "(fp_curve (pts (xy 0 0) (xy 1 2) (xy 2 2) (xy 3 0)) (layer \"F.Fab\")) \
         (fp_line (start -50 -50) (end 50 50) (layer \"F.SilkS\")) (fp_text user \"x\" (at 9 9) (layer \"F.Fab\"))",
    );
    assert_eq!(
        (fp(&curve).body_long_mm, fp(&curve).body_short_mm),
        (3.0, 2.0)
    );
    // the old format with a module
    let old_format = "(module \"Lib:OLD_5x5\" (layer F.Cu)\n\
        (fp_line (start -2.5 -2.5) (end 2.5 -2.5) (layer F.Fab))\n\
        (fp_line (start 2.5 -2.5) (end 2.5 2.5) (layer F.Fab))\n\
        (fp_line (start 2.5 2.5) (end -2.5 2.5) (layer F.Fab))\n\
        (fp_line (start -2.5 2.5) (end -2.5 -2.5) (layer F.Fab))\n\
        (pad 1 smd rect (at 0 0) (size 1 1) (layers F.Cu)))";
    let f = fp(old_format);
    assert_eq!(
        (f.name.as_str(), f.body_long_mm, f.body_short_mm),
        ("OLD_5x5", 5.0, 5.0)
    );
}

#[test]
fn without_a_drawing_of_the_body_the_courtyard_stands_in_and_says_so() {
    let yard = "(footprint \"Y\" (fp_rect (start -1.5 -1) (end 1.5 1) (layer \"F.CrtYd\")))";
    let f = fp(yard);
    assert_eq!((f.body_long_mm, f.body_short_mm), (3.0, 2.0));
    assert_eq!(f.source, BodySource::Courtyard);
    assert!(f.note().unwrap().contains("courtyard"));
    // a drawing of the body wins over the courtyard
    let both = "(footprint \"Y\" (fp_rect (start -1.5 -1) (end 1.5 1) (layer \"F.CrtYd\")) (fp_rect (start -1 -0.5) (end 1 0.5) (layer \"F.Fab\")))";
    assert_eq!(fp(both).source, BodySource::Fab);
    assert_eq!(fp(both).body_long_mm, 2.0);
    // nothing to measure
    let d = refused(
        "(footprint \"Z\" (fp_line (start 0 0) (end 4 0) (layer \"F.SilkS\")) (pad 1 smd rect))",
    );
    assert!(d.contains("no drawing"), "{d}");
    // a line is not the outline of a body
    let d = refused("(footprint \"Z\" (fp_line (start 0 0) (end 4 0) (layer \"F.Fab\")))");
    assert!(d.contains("not the outline of a body"), "{d}");
    let d = refused("(footprint \"Z\" (fp_rect (start 0 0) (end 400 4) (layer \"F.Fab\")))");
    assert!(d.contains("not the outline of a body"), "{d}");
}

// ---- damaged files

#[test]
fn a_file_that_is_not_understood_is_refused_with_its_reason() {
    for (text, want) in [
        ("", "does not start with a bracket"),
        ("hello", "does not start with a bracket"),
        ("(footprint \"A\"", "not closed"),
        ("(footprint \"A", "inside a quoted text"),
        ("(footprint \"A\") junk", "after the end"),
        ("(footprint \"A\"))", "after the end"),
        ("(kicad_pcb (version 1))", "not a footprint"),
        ("(footprint)", "no name"),
        ("(footprint \" \")", "no name"),
        (
            "(footprint \"A\" (fp_line (start nan 0) (end 1 1) (layer \"F.Fab\")))",
            "not a usable number",
        ),
        (
            "(footprint \"A\" (fp_line (start 1e9 0) (end 1 1) (layer \"F.Fab\")))",
            "not a usable number",
        ),
        (
            "(footprint \"A\" (fp_line (start abc 0) (end 1 1) (layer \"F.Fab\")))",
            "not a usable number",
        ),
        (
            "(footprint \"A\" (fp_line (start 1) (end 1 1) (layer \"F.Fab\")))",
            "a number is missing",
        ),
        (
            "(footprint \"A\" (fp_circle (end 1 1) (layer \"F.Fab\")))",
            "has no center",
        ),
        (
            "(footprint \"A\" (fp_arc (end 1 1) (angle 90) (layer \"F.Fab\")))",
            "has no center",
        ),
    ] {
        let d = refused(text);
        assert!(d.contains(want), "{text}: {d}");
    }
    // the lists nest only so deep
    let d = refused(&"(".repeat(100));
    assert!(d.contains("nests lists deeper"), "{d}");
    // the file is only so big and has only so many items
    let big = format!("(footprint \"A\" {})", " ".repeat(MAX_BYTES));
    assert!(refused(&big).contains("the limit is"));
    let many = format!("(footprint \"A\"{})", " x".repeat(200_001));
    assert!(refused(&many).contains("more than 200000 items"));
    // binary rubbish does not panic
    let rubbish: String = (0u8..=255).map(|b| b as char).collect();
    let _ = parse_footprint(&rubbish);
    let _ = parse_footprint("(\"\\");
    let _ = parse_footprint("(footprint \"\\\"");
}

#[test]
fn a_cut_file_never_panics_at_any_length() {
    for i in 0..EXAMPLE.len() {
        if EXAMPLE.is_char_boundary(i) {
            if let Ok(f) = parse_footprint(&EXAMPLE[..i]) {
                // a cut file that is read at all has a whole body
                assert!(f.body_short_mm > 0.0);
            }
        }
    }
}

// ---- the row of the table of packages

#[test]
fn the_row_has_the_body_the_tape_and_the_height_of_the_package_or_the_person() {
    let f = fp(EXAMPLE);
    // the name says a chip of 0603: its height
    assert_eq!(height_hint(&f), Some(0.5));
    let s = package_spec(&f, None).unwrap();
    assert_eq!(
        (s.name.as_str(), s.height_mm, s.tape_mm),
        ("R_0603_1608Metric", 0.5, 8.0)
    );
    assert_eq!((s.body_long_mm, s.body_short_mm), (1.6, 0.825));
    assert_eq!(
        table_csv(&[s]),
        "package,height_mm,tape_mm,body_long_mm,body_short_mm\nR_0603_1608Metric,0.5,8,1.6,0.825\n"
    );
    // the height of the person wins
    assert_eq!(package_spec(&f, Some(0.45)).unwrap().height_mm, 0.45);
    // a name that says nothing needs the height
    let odd = fp("(footprint \"Part_X\" (fp_rect (start 0 0) (end 5 3) (layer \"F.Fab\")))");
    assert_eq!(height_hint(&odd), None);
    let d = package_spec(&odd, None).unwrap_err().detail;
    assert!(d.contains("--height"), "{d}");
    let s = package_spec(&odd, Some(2.0)).unwrap();
    assert_eq!((s.tape_mm, s.height_mm), (12.0, 2.0));
    // a name that is a known package but a body that is not its body is not trusted
    let liar = fp("(footprint \"R_0603\" (fp_rect (start 0 0) (end 9 6) (layer \"F.Fab\")))");
    assert_eq!(height_hint(&liar), None);
    // a height that is not possible
    for h in [0.0, -1.0, 51.0, f64::NAN] {
        assert!(package_spec(&f, Some(h)).is_err(), "{h}");
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

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("phoenix_pnp_kicad_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn p(path: &Path) -> &str {
    path.to_str().unwrap()
}

#[test]
fn the_command_writes_the_rows_of_files_and_of_a_folder() {
    let o = run(&["package-kicad", "examples/chip_0603.kicad_mod"]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert_eq!(
        text(&o.stdout),
        "package,height_mm,tape_mm,body_long_mm,body_short_mm\nR_0603_1608Metric,0.5,8,1.6,0.825\n"
    );
    // a folder: the footprints in the order of their names, the files that are not footprints left alone
    let dir = scratch("folder");
    let lib = dir.join("Lib.pretty");
    std::fs::create_dir_all(&lib).unwrap();
    std::fs::write(lib.join("b.kicad_mod"), EXAMPLE).unwrap();
    std::fs::write(
        lib.join("a.KICAD_MOD"),
        "(footprint \"Part_X\" (fp_rect (start 0 0) (end 5 3) (layer \"F.Fab\")))",
    )
    .unwrap();
    std::fs::write(lib.join("notes.txt"), "not a footprint").unwrap();
    let o = run(&["package-kicad", p(&lib), "--height", "1.2"]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    let out = text(&o.stdout);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 3, "{out}");
    assert_eq!(lines[1], "Part_X,1.2,12,5,3");
    assert_eq!(lines[2], "R_0603_1608Metric,1.2,8,1.6,0.825");
    // without --height the footprint that has no known name is told and the exit code is 1, the other is written
    let o = run(&["package-kicad", p(&lib)]);
    assert_eq!(o.status.code(), Some(2));
    assert!(text(&o.stdout).contains("R_0603_1608Metric,0.5,8,1.6,0.825"));
    let err = text(&o.stderr);
    assert!(
        err.contains("a.KICAD_MOD") && err.contains("--height") && err.contains("PHOENIX-PNP-010"),
        "{err}"
    );
    assert!(err.contains("1 footprints were not read"), "{err}");
    // the same footprint twice is written once and told
    let o = run(&[
        "package-kicad",
        "examples/chip_0603.kicad_mod",
        p(&lib.join("b.kicad_mod")),
    ]);
    assert_eq!(o.status.code(), Some(2));
    assert_eq!(text(&o.stdout).lines().count(), 2);
    assert!(text(&o.stderr).contains("listed twice"));
    // a courtyard stands in for the body: a note, and the row is written
    let yard = dir.join("yard.kicad_mod");
    std::fs::write(
        &yard,
        "(footprint \"Yard_1\" (fp_rect (start -1.5 -1) (end 1.5 1) (layer \"F.CrtYd\")))",
    )
    .unwrap();
    let o = run(&["package-kicad", p(&yard), "--height", "1"]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert!(text(&o.stdout).contains("Yard_1,1,8,3,2"));
    assert!(text(&o.stderr).contains("courtyard"), "{}", text(&o.stderr));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_command_writes_a_table_that_part_inspect_reads_and_refuses_what_it_should() {
    let dir = scratch("out");
    let table = dir.join("packages.csv");
    let t = p(&table);
    let o = run(&["package-kicad", "examples/chip_0603.kicad_mod", "--out", t]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert!(text(&o.stdout).is_empty());
    let written = std::fs::read_to_string(&table).unwrap();
    let mut catalog = Catalog::new();
    catalog.add_packages(&written).unwrap();
    assert_eq!(
        catalog.package_body("r_0603_1608metric"),
        Some((1.6, 0.825))
    );
    // never over a file that exists, nor over a footprint it reads
    assert!(
        !run(&["package-kicad", "examples/chip_0603.kicad_mod", "--out", t])
            .status
            .success()
    );
    assert!(!run(&[
        "package-kicad",
        "examples/chip_0603.kicad_mod",
        "--out",
        "examples/chip_0603.kicad_mod"
    ])
    .status
    .success());
    // mistakes
    assert!(!run(&["package-kicad"]).status.success());
    assert!(
        !run(&["package-kicad", "examples/chip_0603.kicad_mod", "--oops"])
            .status
            .success()
    );
    assert!(!run(&["package-kicad", "nothing.kicad_mod"])
        .status
        .success());
    assert!(!run(&[
        "package-kicad",
        "examples/chip_0603.kicad_mod",
        "--height",
        "x"
    ])
    .status
    .success());
    let empty = dir.join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    let o = run(&["package-kicad", p(&empty)]);
    assert!(text(&o.stderr).contains("no .kicad_mod file"));
    // a file of another sort is not a footprint
    let o = run(&["package-kicad", "examples/board.pos"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(
        text(&o.stderr).contains("PHOENIX-PNP-010"),
        "{}",
        text(&o.stderr)
    );
    assert!(text(&run(&["help"]).stdout).contains("package-kicad"));
    let _ = std::fs::remove_dir_all(&dir);
}
