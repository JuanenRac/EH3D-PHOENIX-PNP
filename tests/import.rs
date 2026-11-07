//! The import of a position file: a centroid CSV with a map of its columns, the text of EAGLE and the `.pos` of KiCad; units, the bottom side, parts not to be mounted, ranges of references,
//! the errors with their code and line, the settings, and the command `import-centroid`. The samples are small ones made up here; the expected numbers were worked out by hand.

use eh3d_phoenix_pnp::config::{MachineConfig, KEYS};
use eh3d_phoenix_pnp::error::{Code, Lang, PnpError};
use eh3d_phoenix_pnp::import::{
    detect, expand_references, import, ColumnMap, Format, ImportUnit, Imported, RefItem, Settings,
    MAP_KEYS,
};
use eh3d_phoenix_pnp::parts::{parse_all, BottomRotation, Profile, Side};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const MACHINE: &str = include_str!("../examples/machine.cfg");
const CENTROID: &str = include_str!("../examples/centroid.csv");
const CUSTOM: &str = include_str!("../examples/centroid_custom.csv");
const MAP: &str = include_str!("../examples/centroid.map");
const EAGLE: &str = include_str!("../examples/board.mnt");
const KICAD: &str = include_str!("../examples/board.pos");

const DEFAULT_CSV: &str = "Designator,Comment,Footprint,Mid X,Mid Y,Layer,Rotation
R1,10k,R_0603,12.5,7.25,Top,-90
R2,\"10k, 1%\",R_0603,0,0,Bottom,450
";

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

fn csv(text: &str) -> Result<Imported, PnpError> {
    import(text, Format::Csv, None, &Settings::default())
}

fn with_map(text: &str, map: &str) -> Result<Imported, PnpError> {
    let map = ColumnMap::parse(map).unwrap();
    import(text, Format::Csv, Some(&map), &Settings::default())
}

fn ok(r: Result<Imported, PnpError>) -> Imported {
    r.unwrap_or_else(|e| panic!("{e}"))
}

fn bad(r: Result<Imported, PnpError>) -> PnpError {
    r.expect_err("an error was expected")
}

#[test]
fn a_csv_with_the_usual_names_is_read_in_millimetres_with_the_angle_from_0_to_360() {
    let got = ok(csv(DEFAULT_CSV));
    assert_eq!(got.format, Format::Csv);
    assert_eq!(got.rows.len(), 2);
    let r1 = &got.rows[0].part;
    assert_eq!((r1.reference.as_str(), r1.value.as_str()), ("R1", "10k"));
    assert_eq!(r1.package, "R_0603");
    assert!(near(r1.x, 12.5) && near(r1.y, 7.25));
    // -90 degrees is 270
    assert!(near(r1.rotation, 270.0), "{}", r1.rotation);
    assert_eq!(r1.side, Side::Top);
    let r2 = &got.rows[1].part;
    assert_eq!(r2.value, "10k, 1%");
    // 450 degrees is 90, and a value with a comma stays whole because it was in quotes
    assert!(near(r2.rotation, 90.0));
    assert_eq!(r2.side, Side::Bottom);
    assert_eq!((got.rows[0].line, got.rows[1].line), (2, 3));
}

#[test]
fn the_angles_are_normalised_and_a_degree_sign_is_accepted() {
    let text = "ref,val,package,x,y,rot,side
A,1,P,0,0,360,top
B,1,P,0,0,-360,top
C,1,P,0,0,-0.5,top
D,1,P,0,0,720.25,top
E,1,P,0,0,90deg,top
";
    let got = ok(csv(text));
    let angles: Vec<f64> = got.rows.iter().map(|r| r.part.rotation).collect();
    for (a, want) in angles.iter().zip([0.0, 0.0, 359.5, 0.25, 90.0]) {
        assert!(near(*a, want), "{a} is not {want}");
    }
}

#[test]
fn the_unit_comes_from_the_header_the_cell_the_map_or_the_settings() {
    // a unit in the header: 1000 mil is 25.4 mm and 492 mil is 12.4968 mm
    let text = "ref,val,package,x (mil),y (mil),rot,side
A,1,P,1000,492,0,top
";
    let a = &ok(csv(text)).rows[0].part;
    assert!(near(a.x, 25.4) && near(a.y, 12.4968), "{} {}", a.x, a.y);
    // a unit in the cell wins over the unit of the settings, which applies to a bare number
    let text = "ref,val,package,x,y,rot,side
A,1,P,0.5in,10mm,0,top
B,1,P,100,200,0,top
";
    let settings = Settings {
        unit: ImportUnit::Mil,
        ..Settings::default()
    };
    let got = ok(import(text, Format::Csv, None, &settings));
    assert!(near(got.rows[0].part.x, 12.7) && near(got.rows[0].part.y, 10.0));
    // 100 mil is 2.54 mm and 200 mil is 5.08 mm
    assert!(near(got.rows[1].part.x, 2.54) && near(got.rows[1].part.y, 5.08));
    // the unit of the map takes the place of the one of the settings
    let map = ColumnMap::parse("unit = inch").unwrap();
    let got = ok(import(text, Format::Csv, Some(&map), &Settings::default()));
    assert!(near(got.rows[1].part.x, 2540.0) && near(got.rows[1].part.y, 5080.0));
}

#[test]
fn the_delimiter_is_told_from_the_header_or_given_by_the_map() {
    let semicolon = "ref;val;package;x;y;rot;side\nA;1;P;1.5;2.5;0;top\n";
    let tab = "ref\tval\tpackage\tx\ty\trot\tside\nA\t1\tP\t1.5\t2.5\t0\ttop\n";
    for text in [semicolon, tab] {
        let a = &ok(csv(text)).rows[0].part;
        assert!(near(a.x, 1.5) && near(a.y, 2.5));
    }
    // a decimal comma in a semicolon file would be a field of its own: the map can fix the delimiter
    let odd = "ref;val;package;x;y;rot;side\nA;1;P;1;2;0;top\n";
    let got = ok(with_map(odd, "delimiter = semicolon"));
    assert_eq!(got.rows.len(), 1);
    assert!(bad(with_map(odd, "delimiter = tab")).code == Code::ImportMap);
}

#[test]
fn the_map_names_the_columns_and_the_words_of_the_sides() {
    let got = ok(with_map(CUSTOM, MAP));
    assert_eq!(got.rows.len(), 5);
    let p = |i: usize| got.rows[i].part.clone();
    // 400 mil is 10.16 mm, 800 mil is 20.32 mm, 600 mil is 15.24 mm and 1200 mil is 30.48 mm
    assert!(near(p(0).x, 10.16) && near(p(0).y, 20.32));
    assert!(near(p(1).x, 15.24) && near(p(1).y, 20.32));
    assert!(near(p(2).x, 10.16) && near(p(2).y, 30.48) && near(p(2).rotation, 270.0));
    assert!(near(p(3).x, 40.64) && near(p(3).y, 25.4));
    assert!(near(p(4).x, 30.48) && near(p(4).y, 10.16) && near(p(4).rotation, 180.0));
    assert_eq!(p(4).side, Side::Bottom);
    // a no in the column of the parts that are placed leaves R2 out
    let dnp: Vec<bool> = got.rows.iter().map(|r| r.dnp).collect();
    assert_eq!(dnp, [false, true, false, false, false]);
    // without the map, the names of the columns of this file are not known
    let e = bad(csv(CUSTOM));
    assert_eq!(e.code, Code::ImportMap);
    assert!(e.detail.starts_with("line 1:"), "{}", e.detail);
}

#[test]
fn the_exact_table_of_parts_of_the_custom_example_is_written() {
    let want = "ref,val,package,x,y,rot,side,dnp
R1,10k,R_0603,10.16,20.32,0,top,
R2,10k,R_0603,15.24,20.32,90,top,yes
C1,100n,C_0603,10.16,30.48,270,top,
U1,MCU,QFN-32,40.64,25.4,0,top,
D1,LED,LED_0603,30.48,10.16,180,bottom,
";
    assert_eq!(ok(with_map(CUSTOM, MAP)).to_csv(), want);
}

#[test]
fn the_default_example_has_a_dnp_column_and_a_value_with_a_comma() {
    let got = ok(csv(CENTROID));
    let dnp: Vec<&str> = got
        .rows
        .iter()
        .filter(|r| r.dnp)
        .map(|r| r.part.reference.as_str())
        .collect();
    assert_eq!(dnp, ["R2"]);
    assert_eq!(got.rows[2].part.reference, "R3");
    // -180 is 180
    assert!(near(got.rows[2].part.rotation, 180.0));
    assert_eq!(got.rows[4].part.value, "100n, X7R");
    assert!(got
        .to_csv()
        .contains("C2,\"100n, X7R\",C_0603,15,30,270,top,"));
    assert_eq!(
        got.summary(),
        "7 parts read as csv: 6 to mount, 1 not to be mounted, 1 on the bottom side"
    );
}

#[test]
fn the_side_is_optional_and_means_top_when_the_file_has_none() {
    let text = "ref,val,package,x,y,rot
A,1,P,1,2,0
";
    let got = ok(csv(text));
    assert_eq!(got.rows[0].part.side, Side::Top);
    let e = bad(csv("ref,val,package,x,y,rot,side\nA,1,P,1,2,0,middle\n"));
    assert_eq!(e.code, Code::ImportBad);
    assert!(e.detail.starts_with("line 2:"), "{}", e.detail);
}

#[test]
fn the_bottom_side_is_mirrored_about_the_width_and_turned_by_the_rule() {
    let text = "ref,val,package,x,y,rot,side
T,1,P,30,5,30,top
B,1,P,30,5,30,bottom
";
    let mut settings = Settings {
        mirror_width_mm: 100.0,
        bottom_rotation: BottomRotation::Mirror,
        ..Settings::default()
    };
    let got = ok(import(text, Format::Csv, None, &settings));
    let (t, b) = (&got.rows[0].part, &got.rows[1].part);
    // the top side is left as it is
    assert!(near(t.x, 30.0) && near(t.rotation, 30.0));
    // X becomes 100 - 30 = 70 and the angle 360 - 30 = 330
    assert!(near(b.x, 70.0) && near(b.y, 5.0) && near(b.rotation, 330.0));
    // with the half turn the angle is 30 + 180 = 210, and a width of 0 leaves X alone
    settings.bottom_rotation = BottomRotation::HalfTurn;
    settings.mirror_width_mm = 0.0;
    let b = &ok(import(text, Format::Csv, None, &settings)).rows[1].part;
    assert!(near(b.x, 30.0) && near(b.rotation, 210.0));
}

#[test]
fn the_column_of_mounting_and_the_words_of_the_map() {
    let text = "ref,val,package,x,y,rot,side,skip
R1,1,P,1,2,0,top,si
R2,1,P,1,2,0,top,no
R3,1,P,1,2,0,top,
";
    let map = "dnp = skip\nyes_words = si\n";
    let got = ok(with_map(text, map));
    let dnp: Vec<bool> = got.rows.iter().map(|r| r.dnp).collect();
    assert_eq!(dnp, [true, false, false]);
    // a word that is neither yes nor no is an error with the code of the mounting and the line
    let e = bad(with_map(&text.replace(",si\n", ",perhaps\n"), map));
    assert_eq!(e.code, Code::MountingBad);
    assert!(e.detail.starts_with("line 2:"), "{}", e.detail);
    // the column of the parts that are placed works the other way round
    let fitted = "ref,val,package,x,y,rot,side,fitted\nR1,1,P,1,2,0,top,no\nR2,1,P,1,2,0,top,yes\n";
    let dnp: Vec<bool> = ok(csv(fitted)).rows.iter().map(|r| r.dnp).collect();
    assert_eq!(dnp, [true, false]);
}

#[test]
fn the_errors_of_a_csv_name_the_code_and_the_line() {
    let head = "ref,val,package,x,y,rot,side\n";
    let e = bad(csv(&format!("{head}A,1,P,1,2,0,top\nB,1,P,abc,2,0,top\n")));
    assert_eq!(e.code, Code::ImportBad);
    assert!(e.detail.starts_with("line 3:"), "{}", e.detail);
    assert!(e.to_string().starts_with("PHOENIX-PNP-053: line 3:"));
    // a field with a line break inside quotes counts its lines: the bad row is on line 5
    let e = bad(csv(&format!(
        "{head}A,\"one\ntwo\",P,1,2,0,top\nB,1,P,1,2,0,top\nC,1,P,1,x,0,top\n"
    )));
    assert!(e.detail.starts_with("line 5:"), "{}", e.detail);
    // a row too short
    let e = bad(csv(&format!("{head}A,1,P,1\n")));
    assert!(e.detail.starts_with("line 2:"), "{}", e.detail);
    // an angle that is not a number, an empty reference, a repeated reference
    let e = bad(csv(&format!("{head}A,1,P,1,2,up,top\n")));
    assert!(e.detail.starts_with("line 2:"));
    let e = bad(csv(&format!("{head},1,P,1,2,0,top\n")));
    assert!(e.detail.contains("empty"), "{}", e.detail);
    let e = bad(csv(&format!("{head}A,1,P,1,2,0,top\nA,1,P,3,4,0,top\n")));
    assert_eq!(
        e.detail,
        "line 3: the reference A is used twice (first on line 2)"
    );
    // a quote that is never closed is reported on the line where it opened
    let e = bad(csv(&format!("{head}A,\"1,P,1,2,0,top\nB,1,P,1,2,0,top\n")));
    assert!(e.detail.starts_with("line 2:"), "{}", e.detail);
    // beyond the size of any board
    let e = bad(csv(&format!("{head}A,1,P,99999,2,0,top\n")));
    assert!(e.detail.starts_with("line 2:"), "{}", e.detail);
}

#[test]
fn a_header_without_a_needed_column_is_an_error_of_the_map() {
    let e = bad(csv("ref,val,package,x,y,side\nA,1,P,1,2,top\n"));
    assert_eq!(e.code, Code::ImportMap);
    assert!(e.detail.contains("rotation"), "{}", e.detail);
    assert!(e.detail.starts_with("line 1:"));
    let e = bad(csv(""));
    assert_eq!(e.code, Code::ImportBad);
}

#[test]
fn lines_before_the_header_are_skipped_and_counted() {
    let text = "Exported by a program
Board: demo
ref,val,package,x,y,rot,side
A,1,P,zz,2,0,top
";
    let e = bad(with_map(text, "skip_lines = 2"));
    assert_eq!(e.code, Code::ImportBad);
    assert!(e.detail.starts_with("line 4:"), "{}", e.detail);
    let got = ok(with_map(&text.replace("zz", "7"), "skip_lines = 2"));
    assert!(near(got.rows[0].part.x, 7.0));
    // without skipping, the first line is taken for the header
    assert_eq!(bad(csv(text)).code, Code::ImportMap);
}

#[test]
fn comment_rows_and_a_byte_order_mark_are_left_out() {
    let text = "\u{feff}// made by a program
ref,val,package,x,y,rot,side
// a comment between rows
A,1,P,1,2,0,top
";
    let got = ok(csv(text));
    assert_eq!(got.rows.len(), 1);
    assert_eq!(got.rows[0].line, 4);
}

#[test]
fn the_map_file_is_checked_line_by_line() {
    let m = ColumnMap::parse(MAP).unwrap();
    assert_eq!(m.reference, ["part name"]);
    assert_eq!(m.x, ["pos x"]);
    assert_eq!(m.unit, Some(ImportUnit::Mil));
    assert_eq!(m.delimiter, Some(','));
    assert_eq!(m.fitted, ["placed"]);
    assert_eq!(
        (m.top_words, m.bottom_words),
        (vec!["up".to_string()], vec!["down".to_string()])
    );
    for (text, line) in [
        ("x = a\nx = b\n", 2),
        ("\n\ncolour = red\n", 3),
        ("reference\n", 1),
        ("unit = cm\n", 1),
        ("delimiter = pipe\n", 1),
        ("skip_lines = 1001\n", 1),
        ("skip_lines = -1\n", 1),
    ] {
        let e = ColumnMap::parse(text).unwrap_err();
        assert_eq!(e.code, Code::ImportMap, "{text}");
        assert!(
            e.detail.starts_with(&format!("line {line}:")),
            "{text}: {}",
            e.detail
        );
    }
    // a list of too many names
    let many = (0..51)
        .map(|i| format!("n{i}"))
        .collect::<Vec<_>>()
        .join(",");
    assert_eq!(
        ColumnMap::parse(&format!("x = {many}")).unwrap_err().code,
        Code::ImportMap
    );
    // a comment after the value is left out, and the unit of the list of keys below is not a unit
    let all: String = MAP_KEYS
        .iter()
        .map(|k| format!("{k} = auto # note\n"))
        .collect();
    assert!(ColumnMap::parse(&all).is_err(), "unit = auto is not a unit");
    assert_eq!(MAP_KEYS.len(), 16);
    assert!(ColumnMap::parse("x = a # the x column\n").is_ok());
}

#[test]
fn the_text_of_eagle_gives_the_side_by_the_mirror_mark() {
    let got = ok(import(EAGLE, Format::Auto, None, &Settings::default()));
    assert_eq!(got.format, Format::Eagle);
    assert_eq!(got.rows.len(), 5);
    let c1 = &got.rows[2].part;
    assert!(near(c1.x, 10.0) && near(c1.y, 30.0) && near(c1.rotation, 270.0));
    assert_eq!((c1.value.as_str(), c1.package.as_str()), ("100n", "C_0603"));
    let d1 = &got.rows[4].part;
    assert_eq!(d1.side, Side::Bottom);
    assert!(near(d1.rotation, 180.0));
    assert!(got.rows[..4].iter().all(|r| r.part.side == Side::Top));
    // the lines of the rows count the comment lines before them
    assert_eq!(got.rows[0].line, 4);
}

#[test]
fn eagle_declares_its_unit_in_a_comment_and_accepts_a_plain_angle_when_the_format_is_given() {
    let text = "; unit: mil
R1 1000 2000 R-90 10k R_0603
R2 500 500 45 10k \"R 0603\"
";
    let got = ok(import(text, Format::Eagle, None, &Settings::default()));
    // 1000 mil is 25.4 mm and 2000 mil is 50.8 mm; R-90 is 270 degrees
    let r1 = &got.rows[0].part;
    assert!(near(r1.x, 25.4) && near(r1.y, 50.8) && near(r1.rotation, 270.0));
    // 500 mil is 12.7 mm; the package in quotes keeps its space
    let r2 = &got.rows[1].part;
    assert!(near(r2.x, 12.7) && near(r2.rotation, 45.0));
    assert_eq!(r2.package, "R 0603");
    // the plain angle is not enough to tell the format by itself
    assert_eq!(
        detect("R2 500 500 45 10k R\n").unwrap_err().code,
        Code::ImportFormat
    );
}

#[test]
fn the_errors_of_eagle_name_the_line() {
    for (text, line) in [
        ("# a\nR1 1 2 R0 10k\n", 2),
        ("R1 1 2 Q0 10k R\n", 1),
        ("# Unit = furlong\nR1 1 2 R0 10k R\n", 1),
        ("R1 1 2 R0 10k R\nR1 3 4 R0 10k R\n", 2),
        ("R1 1 2 R0 \"10k R\n", 1),
        ("R1 1 two R0 10k R\n", 1),
    ] {
        let e = bad(import(text, Format::Eagle, None, &Settings::default()));
        assert_eq!(e.code, Code::ImportBad, "{text}");
        assert!(
            e.detail.starts_with(&format!("line {line}:")),
            "{text}: {}",
            e.detail
        );
    }
}

#[test]
fn the_pos_text_of_kicad_in_inches_with_quoted_fields() {
    let text = "## Footprint positions - created on a board
## Unit = inch
## Side : All
# Ref     Val       Package           PosX       PosY       Rot  Side
R1   10k   R_0603   1.0   0.5   90   top
U2   \"10k 1%\"   \"SOIC 8\"   2.0  -1.0  -90  bottom
";
    let got = ok(import(text, Format::Auto, None, &Settings::default()));
    assert_eq!(got.format, Format::KicadPos);
    let (r1, u2) = (&got.rows[0].part, &got.rows[1].part);
    // 1 inch is 25.4 mm and half an inch 12.7 mm
    assert!(near(r1.x, 25.4) && near(r1.y, 12.7) && near(r1.rotation, 90.0));
    assert!(near(u2.x, 50.8) && near(u2.y, -25.4) && near(u2.rotation, 270.0));
    assert_eq!(
        (u2.value.as_str(), u2.package.as_str()),
        ("10k 1%", "SOIC 8")
    );
    assert_eq!(u2.side, Side::Bottom);
    assert_eq!((got.rows[0].line, got.rows[1].line), (5, 6));
}

#[test]
fn the_kicad_example_is_read_with_its_unit_in_millimetres() {
    let got = ok(import(KICAD, Format::KicadPos, None, &Settings::default()));
    assert_eq!(got.rows.len(), 7);
    assert!(near(got.rows[5].part.x, 40.0) && near(got.rows[5].part.y, 25.0));
    assert_eq!(got.rows[6].part.side, Side::Bottom);
}

#[test]
fn the_errors_of_the_pos_text_of_kicad_name_the_line() {
    for (text, line) in [
        ("R1 10k R 1 2 0\n", 1),
        ("## Unit = mm\nR1 10k R 1 2 0 up\n", 2),
        ("R1 10k R 1 2 north top\n", 1),
        ("R1 10k R 1 2 0 top\nR1 10k R 3 4 0 top\n", 2),
        ("R1 10k R 1 2 0 top extra\n", 1),
    ] {
        let e = bad(import(text, Format::KicadPos, None, &Settings::default()));
        assert_eq!(e.code, Code::ImportBad, "{text}");
        assert!(
            e.detail.starts_with(&format!("line {line}:")),
            "{text}: {}",
            e.detail
        );
    }
}

#[test]
fn the_format_is_told_from_the_first_row_or_refused_with_a_code() {
    assert_eq!(detect(CENTROID).unwrap(), Format::Csv);
    assert_eq!(detect(CUSTOM).unwrap(), Format::Csv);
    assert_eq!(detect(EAGLE).unwrap(), Format::Eagle);
    assert_eq!(detect(KICAD).unwrap(), Format::KicadPos);
    for text in ["", "# only a comment\n", "R1 10 20\n", "just words here\n"] {
        let e = detect(text).unwrap_err();
        assert_eq!(e.code, Code::ImportFormat, "{text:?}");
    }
    // a column map is for a csv: with the text of EAGLE it is refused
    let map = ColumnMap::default();
    let e = bad(import(
        EAGLE,
        Format::Auto,
        Some(&map),
        &Settings::default(),
    ));
    assert_eq!(e.code, Code::ImportFormat);
    // the names of the formats
    for f in [Format::Auto, Format::Csv, Format::Eagle, Format::KicadPos] {
        assert_eq!(Format::parse(f.name()), Some(f));
    }
    assert_eq!(Format::parse("KiCad"), Some(Format::KicadPos));
    assert_eq!(Format::parse("gerber"), None);
}

#[test]
fn ranges_of_references_are_expanded_with_the_width_of_their_zeros() {
    let items = expand_references("R1-R3, C7; FID-A TP-1 U1-4 R08-R10").unwrap();
    let names = |r: &[&str]| RefItem::Range(r.iter().map(|s| s.to_string()).collect());
    let one = |s: &str| RefItem::Single(s.to_string());
    assert_eq!(
        items,
        vec![
            names(&["R1", "R2", "R3"]),
            one("C7"),
            one("FID-A"),
            one("TP-1"),
            names(&["U1", "U2", "U3", "U4"]),
            names(&["R08", "R09", "R10"]),
        ]
    );
    assert!(expand_references("  ").unwrap().is_empty());
    for bad_range in ["R5-R1", "R1-C5", "R1-R2000000", "R1-R9999,C1-C9999"] {
        let e = expand_references(bad_range).unwrap_err();
        assert_eq!(e.code, Code::ImportRange, "{bad_range}");
    }
}

#[test]
fn parts_are_left_out_by_reference_and_by_range() {
    let text = "ref,val,package,x,y,rot,side
R1,1,P,1,1,0,top
R2,1,P,1,1,0,top
R3,1,P,1,1,0,top
R5,1,P,1,1,0,top
";
    let mut got = ok(csv(text));
    // R4 does not exist: a range may cross a gap
    got.apply_dnp(&expand_references("R2-R4").unwrap()).unwrap();
    let dnp: Vec<&str> = got
        .rows
        .iter()
        .filter(|r| r.dnp)
        .map(|r| r.part.reference.as_str())
        .collect();
    assert_eq!(dnp, ["R2", "R3"]);
    assert_eq!(got.parts().len(), 2);
    assert_eq!(got.dnp_parts().len(), 2);
    // a reference that no part has is an error, and so is a range that matches nothing
    let e = got
        .apply_dnp(&expand_references("R9").unwrap())
        .unwrap_err();
    assert_eq!(e.code, Code::MountingBad);
    let e = got
        .apply_dnp(&expand_references("C1-C3").unwrap())
        .unwrap_err();
    assert_eq!(e.code, Code::ImportRange);
}

#[test]
fn what_is_written_is_read_back_by_the_rest_of_the_program() {
    for (text, map) in [(CENTROID, ""), (CUSTOM, MAP), (EAGLE, ""), (KICAD, "")] {
        let format = detect(text).unwrap();
        let map = (format == Format::Csv).then(|| ColumnMap::parse(map).unwrap());
        let got = ok(import(text, format, map.as_ref(), &Settings::default()));
        let back = parse_all(&got.to_csv(), &Profile::default()).unwrap();
        let (want, want_dnp) = (got.parts(), got.dnp_parts());
        assert_eq!(
            (back.parts.len(), back.dnp.len()),
            (want.len(), want_dnp.len())
        );
        for (a, b) in back
            .parts
            .iter()
            .chain(&back.dnp)
            .zip(want.iter().chain(&want_dnp))
        {
            assert_eq!(
                (&a.reference, &a.value, &a.package),
                (&b.reference, &b.value, &b.package)
            );
            assert_eq!(a.side, b.side);
            assert!((a.x - b.x).abs() < 1e-4 && (a.y - b.y).abs() < 1e-4);
            assert!((a.rotation - b.rotation).abs() < 1e-4);
        }
    }
}

#[test]
fn the_number_in_the_table_has_at_most_four_decimals_and_no_zeros_at_the_end() {
    let text = "ref,val,package,x,y,rot,side
A,1,P,1.23456,-0.00001,0,top
B,1,P,100,2.50,0,top
C,1,P,0.00004,-3,0,top
";
    let table = ok(csv(text)).to_csv();
    assert!(table.contains("A,1,P,1.2346,0,0,top,"), "{table}");
    assert!(table.contains("B,1,P,100,2.5,0,top,"), "{table}");
    assert!(table.contains("C,1,P,0,-3,0,top,"), "{table}");
}

#[test]
fn a_cut_or_damaged_file_never_panics() {
    let garbage = [
        "\0\0\0",
        "\"",
        "\"\"\"",
        ",,,,,,\n,,,,,,\n",
        "R1 1e999 2 R0 a b\n",
        "ref,val,package,x,y,rot,side\nA,1,P,1e999,2,0,top\n",
        "ref,val,package,x,y,rot,side\nA,1,P,NaN,inf,nan,top\n",
        "# unit = \n R1 1 2 R0 a b",
        "ü,ö,ä,ß,é\né,à,ç,ñ,ø\n",
    ];
    let map = ColumnMap::parse(MAP).unwrap();
    for sample in [CENTROID, CUSTOM, EAGLE, KICAD].into_iter().chain(garbage) {
        for end in 0..=sample.len() {
            if !sample.is_char_boundary(end) {
                continue;
            }
            let cut = &sample[..end];
            let _ = detect(cut);
            for format in [Format::Auto, Format::Csv, Format::Eagle, Format::KicadPos] {
                let _ = import(cut, format, None, &Settings::default());
                let _ = import(cut, format, Some(&map), &Settings::default());
            }
            let _ = ColumnMap::parse(cut);
            let _ = expand_references(cut);
        }
    }
}

#[test]
fn more_parts_than_the_limit_are_refused_with_the_line() {
    let mut text = String::from("ref,val,package,x,y,rot,side\n");
    for i in 0..50_001 {
        text.push_str(&format!("R{i},1,P,1,1,0,top\n"));
    }
    let e = bad(csv(&text));
    assert_eq!(e.code, Code::TooManyParts);
    assert!(e.detail.starts_with("line 50002:"), "{}", e.detail);
}

#[test]
fn the_settings_of_the_import_are_keys_of_the_configuration() {
    let cfg = MachineConfig::parse(MACHINE).unwrap();
    // the example shows the default of each key of the import
    let default = MachineConfig::default();
    assert_eq!(cfg.import_unit, default.import_unit);
    assert_eq!(
        cfg.import_bottom_mirror_width_mm,
        default.import_bottom_mirror_width_mm
    );
    assert_eq!(cfg.import_settings(), Settings::default());
    for key in ["import_unit", "import_bottom_mirror_width_mm"] {
        assert!(KEYS.contains(&key), "{key}");
        assert!(MACHINE.contains(&format!("\n{key} = ")), "{key}");
    }
    let cfg = MachineConfig::parse(
        "schema_version = 1\nimport_unit = Mil\nimport_bottom_mirror_width_mm = 120.5\nbottom_rotation = half_turn\n",
    )
    .unwrap();
    assert_eq!(
        cfg.import_settings(),
        Settings {
            unit: ImportUnit::Mil,
            mirror_width_mm: 120.5,
            bottom_rotation: BottomRotation::HalfTurn
        }
    );
    assert_eq!(
        MachineConfig::parse("import_unit = cm\n").unwrap_err().code,
        Code::ConfigBadValue
    );
    for text in [
        "import_bottom_mirror_width_mm = -1\n",
        "import_bottom_mirror_width_mm = 10001\n",
    ] {
        assert_eq!(
            MachineConfig::parse(text).unwrap_err().code,
            Code::ConfigOutOfRange
        );
    }
    for unit in ImportUnit::ALL {
        assert_eq!(ImportUnit::parse(unit.name()), Some(unit));
    }
    assert_eq!(ImportUnit::parse("inches"), Some(ImportUnit::Inch));
    assert!(
        near(ImportUnit::Mil.mm_per_unit(), 0.0254) && near(ImportUnit::Inch.mm_per_unit(), 25.4)
    );
}

#[test]
fn the_codes_of_the_import_have_their_meaning_in_the_eight_languages() {
    assert_eq!(Code::ALL.len(), 56);
    for (code, number) in [
        (Code::ImportBad, 53),
        (Code::ImportMap, 54),
        (Code::ImportFormat, 55),
        (Code::ImportRange, 56),
    ] {
        assert_eq!(code.number(), number);
        assert_eq!(code.label(), format!("PHOENIX-PNP-0{number}"));
        let mut seen = std::collections::HashSet::new();
        for lang in Lang::ALL {
            assert!(!code.message(lang).is_empty());
            seen.insert(code.message(lang));
        }
        assert_eq!(seen.len(), 8, "{}", code.label());
    }
    assert_eq!(
        Code::ImportBad.message(Lang::Pt),
        "Uma linha do ficheiro de posições não é válida"
    );
    assert_eq!(
        Code::ImportRange.message(Lang::Pt),
        "Um intervalo de referências não é válido"
    );
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn example(name: &str) -> String {
    root()
        .join("examples")
        .join(name)
        .to_string_lossy()
        .into_owned()
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("eh3d_pnp_import_{}_{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn path(dir: &Path, name: &str) -> String {
    dir.join(name).to_string_lossy().into_owned()
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_phoenix-pnp"))
        .args(args)
        .output()
        .unwrap()
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

#[test]
fn import_centroid_writes_the_table_of_parts_that_tables_check_accepts() {
    let dir = scratch("write");
    let table = path(&dir, "parts.csv");
    let o = run(&[
        "import-centroid",
        &example("centroid_custom.csv"),
        "--map",
        &example("centroid.map"),
        "--out",
        &table,
    ]);
    assert_eq!(o.status.code(), Some(0), "{}", err(&o));
    assert!(out(&o).starts_with("ok: 5 parts read as csv: 4 to mount, 1 not to be mounted, 1 on the bottom side; written to "));
    let text = std::fs::read_to_string(&table)
        .unwrap()
        .replace("\r\n", "\n");
    assert_eq!(text, ok(with_map(CUSTOM, MAP)).to_csv());
    let check = run(&["tables-check", "--parts", &table]);
    assert_eq!(check.status.code(), Some(0), "{}", err(&check));
    // a file that exists is never written over
    let again = run(&[
        "import-centroid",
        &example("centroid_custom.csv"),
        "--map",
        &example("centroid.map"),
        "--out",
        &table,
    ]);
    assert_eq!(again.status.code(), Some(1));
    assert!(err(&again).contains("PHOENIX-PNP-040"), "{}", err(&again));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn import_centroid_prints_the_table_to_the_standard_output_without_an_out() {
    for (file, args, first) in [
        ("board.mnt", vec![], "R1,10k,R_0603,10,20,0,top,"),
        (
            "board.pos",
            vec!["--format", "kicad-pos"],
            "R1,10k,R_0603,10,20,0,top,",
        ),
        (
            "centroid.csv",
            vec!["--format", "csv"],
            "R1,10k,R_0603,10,20,0,top,",
        ),
    ] {
        let mut a = vec!["import-centroid".to_string(), example(file)];
        a.extend(args.iter().map(|s| s.to_string()));
        let refs: Vec<&str> = a.iter().map(String::as_str).collect();
        let o = run(&refs);
        assert_eq!(o.status.code(), Some(0), "{file}: {}", err(&o));
        let text = out(&o).replace("\r\n", "\n");
        let mut lines = text.lines();
        assert_eq!(
            lines.next(),
            Some("ref,val,package,x,y,rot,side,dnp"),
            "{file}"
        );
        assert_eq!(lines.next(), Some(first), "{file}");
        assert!(err(&o).starts_with("ok: "), "{file}: {}", err(&o));
    }
}

#[test]
fn import_centroid_marks_references_and_ranges_as_not_to_be_mounted() {
    let o = run(&[
        "import-centroid",
        &example("board.mnt"),
        "--dnp",
        "R1-R2,U1",
    ]);
    assert_eq!(o.status.code(), Some(0), "{}", err(&o));
    let text = out(&o).replace("\r\n", "\n");
    assert!(text.contains("R1,10k,R_0603,10,20,0,top,yes"), "{text}");
    assert!(text.contains("R2,10k,R_0603,15,20,90,top,yes"), "{text}");
    assert!(text.contains("U1,MCU,QFN-32,40,25,0,top,yes"), "{text}");
    assert!(text.contains("C1,100n,C_0603,10,30,270,top,\n"), "{text}");
    assert!(
        err(&o).contains("2 to mount, 3 not to be mounted"),
        "{}",
        err(&o)
    );
    let o = run(&["import-centroid", &example("board.mnt"), "--dnp", "R9"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(err(&o).contains("PHOENIX-PNP-033"), "{}", err(&o));
    let o = run(&["import-centroid", &example("board.mnt"), "--dnp", "R4-R1"]);
    assert!(err(&o).contains("PHOENIX-PNP-056"), "{}", err(&o));
}

#[test]
fn import_centroid_uses_the_settings_of_the_configuration() {
    let dir = scratch("config");
    let cfg = path(&dir, "machine.cfg");
    std::fs::write(
        &cfg,
        "schema_version = 1\nimport_bottom_mirror_width_mm = 100\nbottom_rotation = mirror\n",
    )
    .unwrap();
    let o = run(&["import-centroid", &example("board.mnt"), "--config", &cfg]);
    assert_eq!(o.status.code(), Some(0), "{}", err(&o));
    // the bottom part D1 at X 30 turned 180: X becomes 70 and the angle 360 - 180 = 180
    assert!(
        out(&o).contains("D1,LED,LED_0603,70,10,180,bottom,"),
        "{}",
        out(&o)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn import_centroid_names_the_code_and_the_line_of_an_error_in_the_language_asked() {
    let dir = scratch("errors");
    let bad_file = path(&dir, "bad.csv");
    std::fs::write(
        &bad_file,
        "ref,val,package,x,y,rot,side\nA,1,P,1,2,0,top\nB,1,P,x,2,0,top\n",
    )
    .unwrap();
    let o = run(&["import-centroid", &bad_file]);
    assert_eq!(o.status.code(), Some(1));
    assert!(
        err(&o).contains("PHOENIX-PNP-053") && err(&o).contains("line 3"),
        "{}",
        err(&o)
    );
    assert!(out(&o).is_empty());
    let o = run(&["import-centroid", &bad_file, "--lang", "es"]);
    assert!(
        err(&o).contains("Una línea del archivo de posiciones no es válida"),
        "{}",
        err(&o)
    );
    assert!(err(&o).contains("line 3"));
    // a map for a text of EAGLE, a format that does not exist, an output that is the input and a file that is not there
    let o = run(&[
        "import-centroid",
        &example("board.mnt"),
        "--map",
        &example("centroid.map"),
    ]);
    assert!(err(&o).contains("PHOENIX-PNP-055"), "{}", err(&o));
    let o = run(&[
        "import-centroid",
        &example("board.mnt"),
        "--format",
        "gerber",
    ]);
    assert!(err(&o).contains("--format"), "{}", err(&o));
    let o = run(&["import-centroid", &bad_file, "--out", &bad_file]);
    assert_eq!(o.status.code(), Some(1));
    assert!(err(&o).contains("also an input"), "{}", err(&o));
    let o = run(&["import-centroid", &path(&dir, "missing.csv")]);
    assert_eq!(o.status.code(), Some(1));
    let o = run(&["import-centroid"]);
    assert!(err(&o).contains("needs a file"), "{}", err(&o));
    let o = run(&["import-centroid", &bad_file, "--nonsense"]);
    assert!(err(&o).contains("unknown option"), "{}", err(&o));
    let bad_map = path(&dir, "bad.map");
    std::fs::write(&bad_map, "x = a\nx = b\n").unwrap();
    let o = run(&[
        "import-centroid",
        &example("centroid.csv"),
        "--map",
        &bad_map,
    ]);
    assert!(
        err(&o).contains("PHOENIX-PNP-054") && err(&o).contains("line 2"),
        "{}",
        err(&o)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_help_names_import_centroid_and_its_options() {
    let o = run(&["help"]);
    let text = out(&o);
    for word in [
        "import-centroid",
        "--format auto|csv|eagle|kicad-pos",
        "--map",
        "--dnp",
        "import_unit",
    ] {
        assert!(text.contains(word), "the help does not name {word}");
    }
}
