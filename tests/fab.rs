//! The files for a board assembly house and the position files of the CAD programs: the order and the writing of designators, the placement list and the bill of materials (a part on each
//! line, surface and through-hole, the parts not to be mounted in a column of their own), what a house would refuse, and a position file of KiCad, Altium, EAGLE or a house brought to another
//! origin and another way of counting the rotation, and taken back.

use eh3d_phoenix_pnp::bom::Bom;
use eh3d_phoenix_pnp::config::{MachineConfig, KEYS};
use eh3d_phoenix_pnp::cplconv::{self, Conversion, Dialect};
use eh3d_phoenix_pnp::designators::{compress, is_single, join, natural_cmp, sort};
use eh3d_phoenix_pnp::error::Code;
use eh3d_phoenix_pnp::import::{expand_references, RefItem};
use eh3d_phoenix_pnp::jlcpcb::{self, DnpMode, Options};
use eh3d_phoenix_pnp::parts::{parse_all, BottomRotation, Part, Side};
use std::cmp::Ordering;
use std::path::PathBuf;

fn example(name: &str) -> String {
    std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("examples")
            .join(name),
    )
    .unwrap()
}

fn part(reference: &str, package: &str, x: f64, y: f64, rotation: f64, side: Side) -> Part {
    Part {
        reference: reference.into(),
        value: "10k".into(),
        package: package.into(),
        x,
        y,
        rotation,
        side,
    }
}

fn strings(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

/// A generator of numbers from a seed (the same on every run).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }

    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

// ---- designators

#[test]
fn designators_are_ordered_by_their_number_and_not_by_their_letters() {
    let mut list = strings(&["R10", "R2", "C1", "R1", "C10", "C9", "U1"]);
    sort(&mut list);
    assert_eq!(list, strings(&["C1", "C9", "C10", "R1", "R2", "R10", "U1"]));
    assert_eq!(natural_cmp("R2", "R10"), Ordering::Less);
    assert_eq!(natural_cmp("R10", "R10"), Ordering::Equal);
}

#[test]
fn a_number_of_any_size_is_compared_without_overflowing() {
    let big = "R99999999999999999999999999";
    let bigger = "R100000000000000000000000000";
    assert_eq!(natural_cmp(big, bigger), Ordering::Less);
    assert_eq!(natural_cmp("R007", "R7"), Ordering::Less);
    assert_eq!(natural_cmp("R7", "R007"), Ordering::Greater);
}

#[test]
fn a_run_of_three_or_more_is_written_as_a_range_and_read_back() {
    let list = strings(&["R3", "R1", "R2", "R5", "C1", "C2", "C3", "C4", "FID-A"]);
    let text = compress(&list, 3);
    assert_eq!(text, "C1-4, FID-A, R1-3, R5");
    let mut back: Vec<String> = Vec::new();
    for item in expand_references(&text).unwrap() {
        match item {
            RefItem::Single(s) => back.push(s),
            RefItem::Range(v) => back.extend(v),
        }
    }
    sort(&mut back);
    let mut want = list.clone();
    sort(&mut want);
    assert_eq!(back, want);
}

#[test]
fn a_pair_is_never_a_range_and_a_zero_in_front_keeps_the_designator_apart() {
    assert_eq!(compress(&strings(&["R1", "R2"]), 1), "R1, R2");
    assert_eq!(
        compress(&strings(&["R01", "R02", "R03"]), 3),
        "R01, R02, R03"
    );
    assert_eq!(compress(&strings(&["R1", "R1", "R2", "R3"]), 3), "R1-3");
}

#[test]
fn a_cell_with_a_list_or_a_range_is_not_one_designator() {
    for ok in ["R1", "FID-A", "TP-1", "U12"] {
        assert!(is_single(ok), "{ok}");
    }
    for bad in ["R1,R2", "R1 R2", "R1;R2", "R1-R5", "R1-5", "", "  "] {
        assert!(!is_single(bad), "{bad:?}");
    }
    assert_eq!(join(&strings(&["R2", "R10", "R1"]), ","), "R1,R2,R10");
}

// ---- the placement list

fn board() -> (Vec<Part>, Vec<Part>) {
    let parsed = parse_all(&example("board_dnp.csv"), &MachineConfig::default().profile).unwrap();
    (parsed.parts, parsed.dnp)
}

#[test]
fn the_placement_list_has_a_part_on_each_line_with_the_places_in_two_columns() {
    let (parts, dnp) = board();
    let text = jlcpcb::cpl(&parts, &dnp, &Options::default());
    let mut lines = text.lines();
    assert_eq!(
        lines.next().unwrap(),
        "Designator,Mid X,Mid Y,Layer,Rotation,Customer Supply"
    );
    assert!(text
        .lines()
        .any(|l| l == "R1,10.0000mm,20.0000mm,Top,0.00,"));
    assert!(text
        .lines()
        .any(|l| l.starts_with("D1,30.0000mm,10.0000mm,Bottom,")));
    assert_eq!(text.lines().count(), 1 + parts.len() + dnp.len());
    assert!(jlcpcb::check_cpl(&text).is_empty());
}

#[test]
fn a_part_that_is_not_to_be_mounted_is_marked_in_the_column_or_left_out() {
    let (parts, dnp) = board();
    assert!(!dnp.is_empty());
    let marked = jlcpcb::cpl(&parts, &dnp, &Options::default());
    for d in &dnp {
        assert!(
            marked
                .lines()
                .any(|l| l.starts_with(&format!("{},", d.reference)) && l.ends_with(",DNP")),
            "{}",
            d.reference
        );
    }
    let omit = Options {
        dnp_mode: DnpMode::Omit,
        ..Options::default()
    };
    let text = jlcpcb::cpl(&parts, &dnp, &omit);
    assert!(!text.contains("DNP"));
    assert!(!text.contains("Customer Supply"));
    for d in &dnp {
        assert!(!text
            .lines()
            .any(|l| l.starts_with(&format!("{},", d.reference))));
    }
    assert_eq!(jlcpcb::count(&parts, &dnp, &omit), parts.len());
}

#[test]
fn the_name_of_the_column_of_the_parts_not_to_be_mounted_is_a_setting() {
    let (parts, dnp) = board();
    let options = Options {
        dnp_column: "Do Not Place".into(),
        ..Options::default()
    };
    let text = jlcpcb::cpl(&parts, &dnp, &options);
    assert!(text.starts_with("Designator,Mid X,Mid Y,Layer,Rotation,Do Not Place\n"));
    let cfg = MachineConfig::parse("fab_dnp_column = Do Not Place\nfab_dnp_mode = omit\n").unwrap();
    assert_eq!(cfg.fab_dnp_column, "Do Not Place");
    assert_eq!(cfg.fab_dnp_mode, DnpMode::Omit);
}

#[test]
fn a_part_that_goes_through_the_board_is_told_by_the_words_of_the_settings() {
    let words = MachineConfig::default().fab_tht_words;
    let p = |pkg: &str| part("J1", pkg, 0.0, 0.0, 0.0, Side::Top);
    for yes in [
        "Resistor_THT:R_Axial_DIN0207",
        "DIP-8",
        "PinHeader_1x04",
        "TO-220",
        "Hole",
    ] {
        assert!(jlcpcb::is_tht(&p(yes), &words), "{yes}");
    }
    for no in ["R_0603", "SOIC-8", "QFN-32", "Dipole_0603", "ADIPOSE", ""] {
        assert!(!jlcpcb::is_tht(&p(no), &words), "{no}");
    }
    let only = vec!["screw".to_string()];
    assert!(jlcpcb::is_tht(&p("Terminal_Screw_2"), &only));
    assert!(!jlcpcb::is_tht(&p("DIP-8"), &only));
}

#[test]
fn the_mount_column_says_surface_or_through_hole() {
    let parts = vec![
        part("R1", "R_0603", 1.0, 2.0, 0.0, Side::Top),
        part("J1", "PinHeader_1x04", 3.0, 4.0, 90.0, Side::Top),
    ];
    let options = Options {
        mount_column: true,
        dnp_mode: DnpMode::Omit,
        ..Options::default()
    };
    let text = jlcpcb::cpl(&parts, &[], &options);
    assert!(text.starts_with("Designator,Mid X,Mid Y,Layer,Rotation,Mount\n"));
    assert!(text.contains("R1,1.0000mm,2.0000mm,Top,0.00,SMD"));
    assert!(text.contains("J1,3.0000mm,4.0000mm,Top,90.00,THT"));
}

#[test]
fn one_side_can_be_asked_for_and_the_order_is_the_natural_one() {
    let parts = vec![
        part("R10", "R_0603", 1.0, 1.0, 0.0, Side::Top),
        part("R2", "R_0603", 2.0, 1.0, 0.0, Side::Bottom),
        part("R1", "R_0603", 3.0, 1.0, 0.0, Side::Top),
    ];
    let all = jlcpcb::cpl(&parts, &[], &Options::default());
    let names: Vec<&str> = all
        .lines()
        .skip(1)
        .map(|l| l.split(',').next().unwrap())
        .collect();
    assert_eq!(names, ["R1", "R2", "R10"]);
    let bottom = Options {
        side: Some(Side::Bottom),
        ..Options::default()
    };
    assert_eq!(jlcpcb::cpl(&parts, &[], &bottom).lines().count(), 2);
}

#[test]
fn a_house_refuses_a_list_in_a_cell_a_designator_twice_and_a_layer_that_is_not_top_or_bottom() {
    let text = "Designator,Mid X,Mid Y,Layer,Rotation\n\
                \"R1,R2\",1mm,2mm,Top,0\n\
                R3,1mm,2mm,Top,0\n\
                R3,1mm,2mm,Top,0\n\
                R4,x,2mm,Middle,north\n\
                ,1mm,2mm,Top,0\n";
    let problems = jlcpcb::check_cpl(text);
    let said = |line: usize, word: &str| {
        problems
            .iter()
            .any(|p| p.line == line && p.what.contains(word))
    };
    assert!(said(2, "one designator on each line"));
    assert!(said(4, "twice"));
    assert!(said(5, "Mid X"));
    assert!(said(5, "Top or Bottom"));
    assert!(said(5, "rotation"));
    assert!(said(6, "empty"));
    assert!(!problems.iter().any(|p| p.line == 3));
}

#[test]
fn a_list_without_the_columns_is_told_by_the_header() {
    let problems = jlcpcb::check_cpl("Ref,X,Y\nR1,1,2\n");
    assert!(problems.iter().any(|p| p.what.contains("Designator")));
    assert!(problems.iter().any(|p| p.what.contains("Layer")));
    assert_eq!(problems[0].line, 1);
    assert_eq!(jlcpcb::check_cpl("")[0].what, "the file is empty");
}

#[test]
fn what_is_written_is_read_back_by_the_reader_of_position_files() {
    let (parts, dnp) = board();
    let text = jlcpcb::cpl(&parts, &dnp, &Options::default());
    assert_eq!(cplconv::detect(&text), Dialect::Jlcpcb);
    let read = cplconv::read(&text, None).unwrap();
    // the parts that are not to be mounted come back apart, from the column that marks them
    assert_eq!(read.parts.len(), parts.len());
    assert_eq!(read.dnp.len(), dnp.len());
    for p in parts.iter().chain(dnp.iter()) {
        let q = read
            .parts
            .iter()
            .chain(read.dnp.iter())
            .find(|q| q.reference == p.reference)
            .unwrap_or_else(|| panic!("{}", p.reference));
        assert!(
            (q.x - p.x).abs() < 1e-3 && (q.y - p.y).abs() < 1e-3,
            "{}",
            p.reference
        );
        assert!((q.rotation - p.rotation).abs() < 0.01, "{}", p.reference);
        assert_eq!(q.side, p.side);
    }
}

// ---- the bill of materials

#[test]
fn the_bill_groups_the_parts_and_takes_the_part_numbers_from_the_bill_of_the_cad_program() {
    let parts = vec![
        part("R1", "R_0603", 1.0, 1.0, 0.0, Side::Top),
        part("R3", "R_0603", 2.0, 1.0, 0.0, Side::Top),
        part("R2", "R_0603", 3.0, 1.0, 0.0, Side::Top),
        part("C1", "C_0603", 4.0, 1.0, 0.0, Side::Top),
    ];
    let mut parts = parts;
    parts[3].value = "100n".into();
    let source = Bom::parse(&example("bom.csv"), 1000).unwrap();
    let rows = jlcpcb::bom_rows(&parts, &[], Some(&source), &Options::default());
    assert_eq!(rows.len(), 2);
    let c = &rows[0];
    assert_eq!(c.designators, strings(&["C1"]));
    assert_eq!(c.mpn, "CC0603KRX7R9BB104");
    let r = &rows[1];
    assert_eq!(r.designators, strings(&["R1", "R2", "R3"]));
    assert_eq!(r.quantity, 3);
    assert_eq!(r.mpn, "RC0603FR-0710KL");
    assert_eq!(r.description, "10k R_0603");
    assert_eq!(r.footprint, "R_0603");
}

#[test]
fn the_bill_as_a_csv_has_the_columns_in_order_and_the_designators_in_one_cell() {
    let (parts, dnp) = board();
    let options = Options::default();
    let rows = jlcpcb::bom_rows(&parts, &dnp, None, &options);
    let text = jlcpcb::bom_csv(&rows, &options);
    assert_eq!(
        text.lines().next().unwrap(),
        "MPN,Designator,Quantity,Description,Footprint,Customer Supply"
    );
    let total: usize = rows.iter().map(|r| r.quantity).sum();
    assert_eq!(total, parts.len() + dnp.len());
    assert!(text.contains("\"R1,R3\"") || text.contains("\"R1,R3,"));
    let marked: Vec<&jlcpcb::BomRow> = rows.iter().filter(|r| r.dnp).collect();
    assert!(!marked.is_empty());
    assert!(marked.iter().all(|r| r
        .designators
        .iter()
        .all(|d| dnp.iter().any(|p| &p.reference == d))));
}

#[test]
fn parts_with_another_part_number_are_other_rows() {
    let mut a = part("R1", "R_0603", 1.0, 1.0, 0.0, Side::Top);
    let b = part("R2", "R_0603", 2.0, 1.0, 0.0, Side::Top);
    a.value = "10K".into();
    let source = Bom::parse(
        "Designator,Value,Footprint,Part Number\nR1,10k,R_0603,AAA\nR2,10k,R_0603,BBB\n",
        100,
    )
    .unwrap();
    let rows = jlcpcb::bom_rows(&[a, b], &[], Some(&source), &Options::default());
    assert_eq!(rows.len(), 2);
}

// ---- the dialects and the conversion

#[test]
fn the_dialect_of_each_example_file_is_told_from_its_header() {
    assert_eq!(cplconv::detect(&example("altium_pnp.csv")), Dialect::Altium);
    assert_eq!(cplconv::detect(&example("board.pos")), Dialect::Kicad);
    assert_eq!(cplconv::detect(&example("board.mnt")), Dialect::Kicad);
    assert_eq!(cplconv::detect(&example("centroid.csv")), Dialect::Jlcpcb);
    assert_eq!(cplconv::detect(&example("board_dnp.csv")), Dialect::Generic);
    assert_eq!(
        cplconv::detect("Ref,Val,Package,PosX,PosY,Rot,Side\nR1,1k,R,1,2,0,top\n"),
        Dialect::Kicad
    );
    assert_eq!(
        cplconv::detect("R1 1.0 2.0 R90 10k R_0603\n"),
        Dialect::Eagle
    );
    assert_eq!(Dialect::parse("EAGLE"), Some(Dialect::Eagle));
    assert_eq!(Dialect::parse("orcad"), None);
}

#[test]
fn every_dialect_is_read_into_millimetres_and_degrees() {
    let altium = cplconv::read(&example("altium_pnp.csv"), None).unwrap();
    assert_eq!(altium.dialect, Dialect::Altium);
    let r1 = altium.parts.iter().find(|p| p.reference == "R1").unwrap();
    assert!((r1.x - 10.0).abs() < 1e-3 && (r1.y - 20.0).abs() < 1e-3);
    let kicad = cplconv::read(&example("board.pos"), None).unwrap();
    assert_eq!(kicad.dialect, Dialect::Kicad);
    assert!(!kicad.parts.is_empty());
    let csv = cplconv::read(
        "Ref,Val,Package,PosX,PosY,Rot,Side\nR1,1k,R,1,2,90,bottom\n",
        None,
    )
    .unwrap();
    assert_eq!(csv.parts[0].side, Side::Bottom);
    assert!((csv.parts[0].rotation - 90.0).abs() < 1e-9);
}

#[test]
fn the_eagle_text_gives_the_bottom_side_by_the_letter_m() {
    let read = cplconv::read(&example("board.mnt"), Some(Dialect::Eagle)).unwrap();
    let d1 = read.parts.iter().find(|p| p.reference == "D1").unwrap();
    assert_eq!(d1.side, Side::Bottom);
    let r2 = read.parts.iter().find(|p| p.reference == "R2").unwrap();
    assert_eq!(r2.side, Side::Top);
    assert!((r2.rotation - 90.0).abs() < 1e-9);
}

#[test]
fn a_file_that_is_nothing_is_an_error_that_says_so() {
    for text in ["", "just words\n", "a,b,c\n1,2,3\n"] {
        let e = cplconv::read(text, None).unwrap_err();
        assert!(
            matches!(
                e.code,
                Code::ImportBad | Code::ImportFormat | Code::PartsBad
            ),
            "{text:?}: {e}"
        );
    }
}

fn pair() -> Vec<Part> {
    vec![
        part("A1", "R_0603", 12.0, 7.0, 30.0, Side::Top),
        part("B1", "R_0603", -3.5, 40.25, 270.0, Side::Bottom),
    ]
}

#[test]
fn the_origin_moves_and_y_flips_in_that_order() {
    let c = Conversion {
        origin_x: 2.0,
        origin_y: 5.0,
        flip_y: true,
        ..Conversion::default()
    };
    let out = cplconv::convert(&pair(), &c).unwrap();
    assert!((out[0].x - 10.0).abs() < 1e-12 && (out[0].y + 2.0).abs() < 1e-12);
    assert!((out[1].x + 5.5).abs() < 1e-12 && (out[1].y + 35.25).abs() < 1e-12);
    assert!(Conversion::default().is_identity());
    assert_eq!(
        cplconv::convert(&pair(), &Conversion::default()).unwrap(),
        pair()
    );
}

#[test]
fn the_rotation_changes_sign_for_a_clockwise_file_or_a_flip_but_not_for_both() {
    let one = |flip_y: bool, clockwise: bool| {
        let c = Conversion {
            flip_y,
            clockwise,
            ..Conversion::default()
        };
        cplconv::convert(&pair()[..1], &c).unwrap()[0].rotation
    };
    assert!((one(false, false) - 30.0).abs() < 1e-9);
    assert!((one(false, true) - 330.0).abs() < 1e-9);
    assert!((one(true, false) - 330.0).abs() < 1e-9);
    assert!((one(true, true) - 30.0).abs() < 1e-9);
}

#[test]
fn a_turn_is_added_and_the_rule_of_the_bottom_side_comes_last() {
    let c = Conversion {
        rotation_offset: 90.0,
        bottom: BottomRotation::Mirror,
        ..Conversion::default()
    };
    let out = cplconv::convert(&pair(), &c).unwrap();
    // the top part: 30 + 90; the bottom part: 270 + 90 = 0, then mirrored: 0
    assert!((out[0].rotation - 120.0).abs() < 1e-9);
    assert!(out[1].rotation.abs() < 1e-9);
    let half = Conversion {
        bottom: BottomRotation::HalfTurn,
        ..Conversion::default()
    };
    let out = cplconv::convert(&pair(), &half).unwrap();
    assert!((out[0].rotation - 30.0).abs() < 1e-9);
    assert!((out[1].rotation - 90.0).abs() < 1e-9);
}

#[test]
fn a_conversion_with_a_number_that_is_not_finite_or_a_place_too_far_is_refused() {
    for c in [
        Conversion {
            origin_x: f64::NAN,
            ..Conversion::default()
        },
        Conversion {
            rotation_offset: f64::INFINITY,
            ..Conversion::default()
        },
        Conversion {
            origin_y: 2.0e6,
            ..Conversion::default()
        },
    ] {
        assert_eq!(
            cplconv::convert(&pair(), &c).unwrap_err().code,
            Code::PartsBad
        );
    }
    let far = vec![part("A1", "R", 999_999.0, 0.0, 0.0, Side::Top)];
    let c = Conversion {
        origin_x: -10.0,
        ..Conversion::default()
    };
    assert!(cplconv::convert(&far, &c).is_err());
}

#[test]
fn what_a_conversion_does_is_undone_on_boards_made_by_chance() {
    let mut rng = Rng(0xC0FFEE);
    for round in 0..200 {
        let parts: Vec<Part> = (0..1 + rng.below(12))
            .map(|i| {
                part(
                    &format!("P{i}"),
                    "R_0603",
                    rng.unit() * 400.0 - 200.0,
                    rng.unit() * 400.0 - 200.0,
                    rng.unit() * 359.99,
                    if rng.below(2) == 0 {
                        Side::Top
                    } else {
                        Side::Bottom
                    },
                )
            })
            .collect();
        let c = Conversion {
            origin_x: rng.unit() * 100.0 - 50.0,
            origin_y: rng.unit() * 100.0 - 50.0,
            flip_y: rng.below(2) == 0,
            clockwise: rng.below(2) == 0,
            rotation_offset: rng.unit() * 720.0 - 360.0,
            bottom: BottomRotation::ALL[rng.below(3) as usize],
        };
        let there = cplconv::convert(&parts, &c).unwrap();
        let back = cplconv::undo(&there, &c).unwrap();
        for (a, b) in parts.iter().zip(&back) {
            let turn = (a.rotation - b.rotation).rem_euclid(360.0);
            assert!(
                (a.x - b.x).abs() < 1e-9
                    && (a.y - b.y).abs() < 1e-9
                    && (turn < 1e-6 || 360.0 - turn < 1e-6),
                "round {round} {c:?}: {a:?} {b:?}"
            );
            assert!((0.0..360.0).contains(&there[0].rotation));
        }
    }
}

#[test]
fn the_settings_of_the_conversion_are_keys_with_a_range_and_a_default_that_changes_nothing() {
    for k in [
        "cpl_origin_x_mm",
        "cpl_origin_y_mm",
        "cpl_flip_y",
        "cpl_rotation_cw",
        "cpl_rotation_offset_deg",
        "cpl_bottom_rotation",
        "fab_dnp_mode",
        "fab_dnp_column",
        "fab_tht_words",
    ] {
        assert!(KEYS.contains(&k), "{k}");
    }
    let d = MachineConfig::default();
    assert!(
        d.cpl_origin_x_mm == 0.0
            && d.cpl_origin_y_mm == 0.0
            && !d.cpl_flip_y
            && !d.cpl_rotation_cw
            && d.cpl_rotation_offset_deg == 0.0
            && d.cpl_bottom_rotation == BottomRotation::Keep
    );
    let e = MachineConfig::parse("cpl_rotation_offset_deg = 361\n").unwrap_err();
    assert_eq!(e.code, Code::ConfigOutOfRange);
    assert!(e.to_string().contains("cpl_rotation_offset_deg"));
    let e = MachineConfig::parse("cpl_bottom_rotation = sideways\n").unwrap_err();
    assert!(e.to_string().contains("cpl_bottom_rotation"));
    let e = MachineConfig::parse("fab_dnp_mode = maybe\n").unwrap_err();
    assert!(e.to_string().contains("fab_dnp_mode"));
}

#[test]
fn the_column_of_the_parts_not_to_be_mounted_is_found_by_the_name_it_was_written_with() {
    let (parts, dnp) = board();
    let options = Options {
        dnp_column: "Do Not Place".into(),
        ..Options::default()
    };
    let text = jlcpcb::cpl(&parts, &dnp, &options);
    // read with the default name the column is only another column, and every part is to be mounted
    let blind = cplconv::read(&text, None).unwrap();
    assert!(blind.dnp.is_empty());
    let seen = cplconv::read_with(&text, None, "Do Not Place").unwrap();
    assert_eq!(seen.dnp.len(), dnp.len());
    assert_eq!(seen.parts.len(), parts.len());
}
