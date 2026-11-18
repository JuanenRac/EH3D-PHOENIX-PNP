//! The look at the part on the nozzle: the table of sizes, the limits, the symmetry of the part, the file of readings and the CSV of the result.

use eh3d_phoenix_pnp::error::Code;
use eh3d_phoenix_pnp::inspect::{
    inspect, judge, parse_readings, rows_csv, tally, Limits, Measured, Reason, SizeRow, SizeTable,
};
use eh3d_phoenix_pnp::model::Offset;
use eh3d_phoenix_pnp::vision::Reading;

const SIZES: &str = include_str!("../examples/package_sizes.csv");
const READS: &str = include_str!("../examples/nozzle_reads.csv");

fn size(long: f64, short: f64) -> SizeRow {
    SizeRow {
        pattern: "x".into(),
        long_mm: long,
        short_mm: short,
    }
}

fn seen(long: f64, short: f64, dx: f64, dy: f64, rot: f64) -> Measured {
    Measured {
        long_mm: long,
        short_mm: short,
        dx_mm: dx,
        dy_mm: dy,
        rotation_deg: rot,
    }
}

#[test]
fn the_table_keeps_the_long_side_first_and_an_exact_name_wins_over_a_pattern() {
    let t = SizeTable::parse(
        "short_mm;package;long_mm\n0.8;R_0603;1.6\n1.3;SOT-23*;2.9\n1.0;SOT-23-5;3.0\n",
    )
    .unwrap();
    let r = t.lookup("r_0603").unwrap();
    assert_eq!((r.long_mm, r.short_mm), (1.6, 0.8));
    let s = t.lookup("sot-23-5").unwrap();
    assert_eq!(s.long_mm, 3.0, "the exact name is the one used");
    assert_eq!(t.lookup("SOT-23-3").unwrap().long_mm, 2.9);
    assert!(t.lookup("DO-214").is_none());
    // the sides given the wrong way round
    let swapped = SizeTable::parse("package,long_mm,short_mm\nA,0.5,1.0\n").unwrap();
    assert_eq!(swapped.rows[0].long_mm, 1.0);
}

#[test]
fn a_table_of_sizes_that_is_not_valid_is_refused_with_the_row() {
    for (text, what) in [
        ("package,long_mm\nA,1\n", "columns"),
        (
            "package,long_mm,short_mm\nA,1,0.5\na,1,0.5\n",
            "listed twice",
        ),
        ("package,long_mm,short_mm\nA,x,0.5\n", "not a size"),
        ("package,long_mm,short_mm\nA,0,0.5\n", "not a size"),
        ("package,long_mm,short_mm\nA,1\n", "2 fields"),
        ("", "empty"),
    ] {
        let e = SizeTable::parse(text).unwrap_err();
        assert_eq!(e.code, Code::CatalogBadRow, "{text}");
        assert!(e.detail.contains(what), "{text}: {}", e.detail);
    }
}

#[test]
fn a_part_of_the_right_size_that_hangs_straight_is_accepted() {
    let r = inspect(
        &size(1.6, 0.8),
        0.0,
        &seen(1.62, 0.79, 0.1, -0.05, 1.5),
        &Limits::default(),
    );
    assert!(r.accepted(), "{r:?}");
    assert!((r.long_err_mm - 0.02).abs() < 1e-9 && (r.short_err_mm + 0.01).abs() < 1e-9);
}

#[test]
fn each_limit_gives_its_own_reason_and_the_edge_is_still_accepted() {
    let lim = Limits {
        size_tol_mm: 0.25,
        angle_tol_deg: 4.0,
        offset_max_mm: 1.0,
    };
    let s = size(2.0, 1.0);
    // exactly on the limits (numbers that a float holds exactly)
    assert!(inspect(&s, 0.0, &seen(2.25, 0.75, 1.0, 0.0, 4.0), &lim).accepted());
    let r = inspect(&s, 0.0, &seen(2.5, 1.0, 0.0, 0.0, 0.0), &lim);
    assert!(
        matches!(r.reasons.as_slice(), [Reason::Long(d)] if (*d - 0.5).abs() < 1e-9),
        "{r:?}"
    );
    let r = inspect(&s, 0.0, &seen(2.0, 0.5, 0.0, 0.0, 0.0), &lim);
    assert!(
        matches!(r.reasons.as_slice(), [Reason::Short(d)] if (*d + 0.5).abs() < 1e-9),
        "{r:?}"
    );
    let r = inspect(&s, 0.0, &seen(2.0, 1.0, 0.0, 0.0, 10.0), &lim);
    assert!(
        matches!(r.reasons.as_slice(), [Reason::Turn(d)] if (*d - 10.0).abs() < 1e-9),
        "{r:?}"
    );
    let r = inspect(&s, 0.0, &seen(2.0, 1.0, 1.2, 0.0, 0.0), &lim);
    assert!(
        matches!(r.reasons.as_slice(), [Reason::Offset(d)] if (*d - 1.2).abs() < 1e-9),
        "{r:?}"
    );
    // all of them at once
    let r = inspect(&s, 0.0, &seen(3.0, 2.0, 3.0, 0.0, 20.0), &lim);
    assert_eq!(r.reasons.len(), 4, "{r:?}");
    assert!(r.reasons.iter().all(|x| !x.text().is_empty()));
}

#[test]
fn a_rectangle_looks_the_same_every_half_turn_and_a_square_every_quarter() {
    let lim = Limits::default();
    let rect = size(1.6, 0.8);
    for rot in [0.0, 180.0, -180.0, 360.0, 181.0, -1.0] {
        assert!(
            inspect(&rect, 0.0, &seen(1.6, 0.8, 0.0, 0.0, rot), &lim).accepted(),
            "{rot}"
        );
    }
    // a quarter turn of a rectangle is a part that lies the wrong way
    assert!(!inspect(&rect, 0.0, &seen(1.6, 0.8, 0.0, 0.0, 90.0), &lim).accepted());
    let square = size(5.0, 5.0);
    for rot in [0.0, 90.0, 180.0, 270.0, 88.0, -92.0] {
        assert!(
            inspect(&square, 0.0, &seen(5.0, 5.0, 0.0, 0.0, rot), &lim).accepted(),
            "{rot}"
        );
    }
    assert!(!inspect(&square, 0.0, &seen(5.0, 5.0, 0.0, 0.0, 45.0), &lim).accepted());
    // the expected angle moves the target
    assert!(inspect(&rect, 30.0, &seen(1.6, 0.8, 0.0, 0.0, 31.0), &lim).accepted());
    assert!(!inspect(&rect, 30.0, &seen(1.6, 0.8, 0.0, 0.0, 0.0), &lim).accepted());
}

#[test]
fn the_sides_that_a_reading_gives_the_other_way_round_are_put_in_order() {
    let r = inspect(
        &size(1.6, 0.8),
        0.0,
        &seen(0.8, 1.6, 0.0, 0.0, 0.0),
        &Limits::default(),
    );
    assert!(r.accepted(), "{r:?}");
}

#[test]
fn a_reading_of_the_vision_pipeline_becomes_a_measure_in_millimetres() {
    let reading = Reading {
        offset: Offset {
            dx_mm: 0.2,
            dy_mm: -0.1,
            rotation_deg: 3.0,
        },
        centre_px: (100.0, 100.0),
        long_px: 80.0,
        short_px: 40.0,
        area_px: 3200,
        threshold: 128,
    };
    let m = Measured::from_reading(&reading, 0.02);
    assert!((m.long_mm - 1.6).abs() < 1e-9 && (m.short_mm - 0.8).abs() < 1e-9);
    assert_eq!((m.dx_mm, m.dy_mm, m.rotation_deg), (0.2, -0.1, 3.0));
}

#[test]
fn the_readings_are_read_by_name_and_a_bad_row_is_refused() {
    let r = parse_readings(READS).unwrap();
    assert_eq!(r.len(), 6);
    assert_eq!(r[1].expected_rotation_deg, 0.0);
    // without the column of the expected angle it is 0
    let r = parse_readings("rot_deg,ref,package,long_mm,short_mm,dx_mm,dy_mm\n5,A,P,1,1,0,0\n")
        .unwrap();
    assert_eq!(r[0].reference, "A");
    assert_eq!(
        (r[0].seen.rotation_deg, r[0].expected_rotation_deg),
        (5.0, 0.0)
    );
    let head = "ref,package,long_mm,short_mm,dx_mm,dy_mm,rot_deg\n";
    for (text, what) in [
        ("ref,package\nA,P\n".to_string(), "must hold"),
        (format!("{head}A,P,1,1,0,0\n"), "fields"),
        (format!("{head}A,P,1,1,0,0,0\nA,P,1,1,0,0,0\n"), "twice"),
        (format!("{head}A,P,x,1,0,0,0\n"), "not a number"),
        (format!("{head}A,P,-1,1,0,0,0\n"), "not a number"),
        (String::new(), "empty"),
    ] {
        let e = parse_readings(&text).unwrap_err();
        assert_eq!(e.code, Code::CalibrationBad, "{text}");
        assert!(e.detail.contains(what), "{text}: {}", e.detail);
    }
}

#[test]
fn the_examples_give_four_accepted_one_refused_and_one_that_cannot_be_judged() {
    let table = SizeTable::parse(SIZES).unwrap();
    let rows = judge(&parse_readings(READS).unwrap(), &table, &Limits::default());
    assert_eq!(tally(&rows), (4, 1, 1));
    let c1 = rows.iter().find(|r| r.reference == "C1").unwrap();
    let i = c1.inspection.as_ref().unwrap();
    assert!(i.reasons.iter().any(|r| matches!(r, Reason::Long(_))));
    assert!(i.reasons.iter().any(|r| matches!(r, Reason::Short(_))));
    assert!(rows
        .iter()
        .find(|r| r.reference == "D9")
        .unwrap()
        .inspection
        .is_none());
    let csv = rows_csv(&rows);
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(
        lines[0],
        "ref,package,verdict,long_err_mm,short_err_mm,turn_err_deg,offset_mm,reasons"
    );
    assert!(
        lines.contains(&"D9,DO-214,unknown,,,,,no size for this package"),
        "{csv}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("C1,C_0603,reject,0.800,0.400,")
                && l.contains("long side +0.800 mm off | short side +0.400 mm off")),
        "{csv}"
    );
    assert!(
        lines.iter().any(|l| l.starts_with("R1,R_0603,ok,")),
        "{csv}"
    );
}

#[test]
fn a_looser_limit_accepts_what_a_tighter_one_refuses() {
    let table = SizeTable::parse(SIZES).unwrap();
    let reads = parse_readings(READS).unwrap();
    let tight = Limits {
        size_tol_mm: 0.01,
        ..Limits::default()
    };
    assert!(tally(&judge(&reads, &table, &tight)).1 > 1);
    let loose = Limits {
        size_tol_mm: 5.0,
        ..Limits::default()
    };
    assert_eq!(tally(&judge(&reads, &table, &loose)), (5, 0, 1));
}

#[test]
fn the_settings_of_the_look_are_in_the_example_with_their_defaults_and_ranges() {
    use eh3d_phoenix_pnp::config::{MachineConfig, KEYS};
    let machine = include_str!("../examples/machine.cfg");
    for k in [
        "part_size_tol_mm",
        "part_angle_tol_deg",
        "part_offset_max_mm",
    ] {
        assert!(KEYS.contains(&k), "{k}");
        assert!(
            machine.contains(&format!(
                "
{k} = "
            )),
            "{k}"
        );
    }
    let e = MachineConfig::parse(machine).unwrap();
    let d = MachineConfig::default();
    let l = Limits::default();
    for c in [&e, &d] {
        assert_eq!(
            (
                c.part_size_tol_mm,
                c.part_angle_tol_deg,
                c.part_offset_max_mm
            ),
            (l.size_tol_mm, l.angle_tol_deg, l.offset_max_mm)
        );
    }
    for (key, ok, bad) in [
        ("part_size_tol_mm", ["0.01", "5"], ["0", "5.1"]),
        ("part_angle_tol_deg", ["0.1", "45"], ["0", "45.5"]),
        ("part_offset_max_mm", ["0.1", "20"], ["0", "21"]),
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
}
