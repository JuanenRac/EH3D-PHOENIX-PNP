//! The bill of materials: how it is read, how it is checked against the parts of a board, its code and the command `bom-check`.

use eh3d_phoenix_pnp::bom::{compare, normalize_value, Bom, MAX_BOM_ROWS};
use eh3d_phoenix_pnp::error::{Code, Lang};
use eh3d_phoenix_pnp::parts::{parse_all, Parsed, Profile};
use std::path::PathBuf;
use std::process::{Command, Output};

const BOARD: &str = include_str!("../examples/board.pos");
const BOARD_DNP: &str = include_str!("../examples/board_dnp.csv");
const EXAMPLE: &str = include_str!("../examples/bom.csv");

fn parsed(text: &str) -> Parsed {
    parse_all(text, &Profile::default()).unwrap()
}

fn bom(text: &str) -> Bom {
    Bom::parse(text, MAX_BOM_ROWS).unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn the_example_agrees_with_the_example_board() {
    let board = parsed(BOARD);
    let report = compare(&bom(EXAMPLE), &board.parts, &board.dnp);
    assert!(report.is_clean(), "{}", report.text());
    assert_eq!(report.matched, 7);
    assert!(report.text().starts_with("ok:"));
}

#[test]
fn a_row_lists_references_in_a_cell_a_range_or_one_by_one() {
    let b = bom("Designator,Qty\n\"R1, R2\",2\nC1-C3,3\nU1;U2,2\nD1,1\n");
    let refs: Vec<&str> = b
        .lines
        .iter()
        .flat_map(|l| l.references.iter().map(String::as_str))
        .collect();
    assert_eq!(refs, ["R1", "R2", "C1", "C2", "C3", "U1", "U2", "D1"]);
    assert_eq!(b.reference_count(), 8);
    assert_eq!(b.lines[1].line, 3);
}

#[test]
fn the_columns_are_found_by_name_in_any_order_with_a_semicolon_and_a_byte_order_mark() {
    let b = bom("\u{feff}Qty;Footprint;Comment;REFDES\n2;R_0603;10k;R1 R2\n");
    let l = &b.lines[0];
    assert_eq!(l.references, ["R1", "R2"]);
    assert_eq!((l.value.as_str(), l.package.as_str()), ("10k", "R_0603"));
    assert_eq!(l.quantity, Some(2));
}

#[test]
fn a_bom_with_only_references_is_enough() {
    let b = bom("Ref\nR1\nR2\n");
    assert_eq!(b.lines.len(), 2);
    assert_eq!(b.lines[0].quantity, None);
    assert!(b.lines[0].value.is_empty());
}

#[test]
fn what_is_wrong_names_the_line_and_has_the_code() {
    for (text, needle) in [
        ("", "empty"),
        ("Comment,Qty\n10k,1\n", "no column of references"),
        ("Designator,Qty\n,1\n", "no reference"),
        ("Designator,Qty\nR1,two\n", "not a whole number"),
        ("Designator\nR5-R1\n", "goes down"),
        ("Designator\n\"R1\n", "quote"),
    ] {
        let e = Bom::parse(text, MAX_BOM_ROWS).unwrap_err();
        assert_eq!(e.code, Code::BomBad, "{text:?}");
        assert!(e.detail.contains(needle), "{text:?}: {}", e.detail);
    }
    let e = Bom::parse("Designator,Qty\nR1,two\n", MAX_BOM_ROWS).unwrap_err();
    assert!(e.detail.starts_with("line 2:"), "{}", e.detail);
}

#[test]
fn a_bom_over_the_limit_of_rows_is_refused() {
    let e = Bom::parse("Designator\nR1\nR2\nR3\n", 2).unwrap_err();
    assert_eq!(e.code, Code::BomBad);
    assert!(e.detail.contains("more than 2 rows"));
    assert!(Bom::parse("Designator\nR1\nR2\n", 2).is_ok());
}

#[test]
fn the_check_finds_what_is_only_on_one_side() {
    let board = parsed(BOARD);
    let report = compare(
        &bom("Designator\nR1\nR2\nR9\nX1\n"),
        &board.parts,
        &board.dnp,
    );
    assert_eq!(report.only_in_bom, ["R9", "X1"]);
    assert_eq!(report.only_on_board, ["C1", "C2", "D1", "R3", "U1"]);
    assert_eq!(report.problems(), 7);
    assert!(!report.is_clean());
}

#[test]
fn a_repeated_reference_is_reported_once_and_counted_once() {
    let board = parsed(BOARD);
    let report = compare(
        &bom("Designator\n\"R1,R2,R3\"\nR1\nR1\n\"C1,C2,U1,D1\"\n"),
        &board.parts,
        &board.dnp,
    );
    assert_eq!(report.repeated, ["R1"]);
    assert_eq!(report.matched, 7);
}

#[test]
fn values_and_packages_are_compared_without_the_way_they_are_written() {
    let board = parsed(BOARD);
    let report = compare(
        &bom("Designator,Value,Footprint\nR1,10K,r_0603\nR2,10 kΩ,R_0603\nR3,22k,R_0402\nC1,0.1u,C_0603\nC2,,\nU1,MCU,QFN-32\nD1,LED,LED_0603\n"),
        &board.parts,
        &board.dnp,
    );
    // 10 kΩ and 10k are the same value; 22k is not; 0.1u is not written as 100n
    assert_eq!(
        report.value_differs,
        [
            ("C1".to_string(), "0.1u".to_string(), "100n".to_string()),
            ("R3".to_string(), "22k".to_string(), "10k".to_string())
        ]
    );
    assert_eq!(
        report.package_differs,
        [("R3".to_string(), "R_0402".to_string(), "R_0603".to_string())]
    );
    assert!(report
        .text()
        .contains("R3: the value is 22k in the BOM and 10k on the board"));
}

#[test]
fn the_value_is_normalized() {
    assert_eq!(normalize_value("10 kΩ"), "10k");
    assert_eq!(normalize_value("4.7 Ohm"), "4.7");
    assert_eq!(normalize_value("1µF"), "1uf");
    assert_eq!(normalize_value("1μF"), "1uf");
    assert_eq!(normalize_value(" 100N "), "100n");
}

#[test]
fn the_quantity_of_a_row_must_be_the_number_of_its_references() {
    let board = parsed(BOARD);
    let report = compare(
        &bom("Designator,Qty\n\"R1,R2,R3\",2\n\"C1,C2\",2\nU1,1\nD1,1\n"),
        &board.parts,
        &board.dnp,
    );
    assert_eq!(report.quantity_differs, [(2, 2, 3)]);
    assert!(report
        .text()
        .contains("line 2: the quantity is 2 and the row lists 3 references"));
}

#[test]
fn a_part_not_to_be_mounted_may_be_in_the_bom_or_not() {
    let board = parsed(BOARD_DNP);
    // R2 and C2 are marked as not to be mounted
    assert_eq!(board.dnp.len(), 2);
    let listed = compare(
        &bom("Designator\nR1\nR2\nR3\nC1\nC2\nU1\nD1\n"),
        &board.parts,
        &board.dnp,
    );
    assert!(listed.is_clean(), "{}", listed.text());
    assert_eq!(listed.listed_not_mounted, 2);
    let left_out = compare(
        &bom("Designator\nR1\nR3\nC1\nU1\nD1\n"),
        &board.parts,
        &board.dnp,
    );
    assert!(left_out.is_clean(), "{}", left_out.text());
    assert_eq!(left_out.listed_not_mounted, 0);
}

#[test]
fn the_code_has_its_meaning_in_the_eight_languages() {
    assert_eq!(Code::BomBad.number(), 57);
    assert_eq!(Code::BomBad.label(), "PHOENIX-PNP-057");
    let mut seen = Vec::new();
    for lang in Lang::ALL {
        let m = Code::BomBad.message(lang);
        assert!(!m.is_empty());
        assert!(!seen.contains(&m), "{m}");
        seen.push(m);
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
fn bom_check_says_ok_for_the_examples_and_names_what_disagrees() {
    let ok = run(&["bom-check", "examples/board.pos", "examples/bom.csv"]);
    assert!(ok.status.success(), "{}", text(&ok.stderr));
    assert!(text(&ok.stdout).starts_with("ok: the BOM and the board agree"));
    let dir = std::env::temp_dir().join(format!("phoenix_pnp_bom_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let wrong = dir.join("wrong.csv");
    std::fs::write(&wrong, "Designator,Comment\nR1,10k\nR2,22k\nR9,1k\n").unwrap();
    let o = run(&["bom-check", "examples/board.pos", wrong.to_str().unwrap()]);
    assert!(!o.status.success());
    let out = text(&o.stdout);
    assert!(
        out.contains("in the BOM but not on the board (1): R9"),
        "{out}"
    );
    assert!(
        out.contains("R2: the value is 22k in the BOM and 10k on the board"),
        "{out}"
    );
    assert!(text(&o.stderr).contains("disagree"), "{}", text(&o.stderr));
    let bad = dir.join("bad.csv");
    std::fs::write(&bad, "Comment\n10k\n").unwrap();
    let o = run(&[
        "bom-check",
        "examples/board.pos",
        bad.to_str().unwrap(),
        "--lang",
        "es",
    ]);
    assert!(!o.status.success());
    assert!(
        text(&o.stderr).contains("PHOENIX-PNP-057: Una línea de la lista de materiales"),
        "{}",
        text(&o.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn bom_check_is_strict_about_its_arguments() {
    assert!(!run(&["bom-check", "examples/board.pos"]).status.success());
    assert!(!run(&[
        "bom-check",
        "examples/board.pos",
        "examples/bom.csv",
        "extra"
    ])
    .status
    .success());
    assert!(!run(&[
        "bom-check",
        "examples/board.pos",
        "examples/bom.csv",
        "--oops"
    ])
    .status
    .success());
}
