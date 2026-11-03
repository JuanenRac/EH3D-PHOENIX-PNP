use eh3d_pnp_manager::feeders::{assign, assign_side, from_csv as feeders_csv, Feeder};
use eh3d_pnp_manager::parts::{from_csv, from_kicad_pos, parse, Part, Side};
use eh3d_pnp_manager::plan::{plan, Settings};

const POS: &str = "### Module positions\n## Unit = mm, Angle = deg.\n# Ref Val Package PosX PosY Rot Side\nR1 10k R_0603 10.000 20.000 0.0000 top\nR2 10k R_0603 15.000 20.000 450.0 top\nD1 LED LED_0603 30.000 10.000 -90 bottom\n";

fn part(r: &str, v: &str, p: &str, x: f64, y: f64, side: Side) -> Part {
    Part {
        reference: r.into(),
        value: v.into(),
        package: p.into(),
        x,
        y,
        rotation: 0.0,
        side,
    }
}

fn feeder(id: &str, v: &str, p: &str, x: f64, y: f64, count: u32) -> Feeder {
    Feeder {
        id: id.into(),
        value: v.into(),
        package: p.into(),
        x,
        y,
        count,
    }
}

#[test]
fn a_kicad_position_file_is_read_and_the_angles_are_normalised() {
    let p = from_kicad_pos(POS).unwrap();
    assert_eq!(p.len(), 3);
    assert_eq!(
        (
            p[0].reference.as_str(),
            p[0].value.as_str(),
            p[0].package.as_str(),
            p[0].x,
            p[0].y
        ),
        ("R1", "10k", "R_0603", 10.0, 20.0)
    );
    assert_eq!(p[1].rotation, 90.0, "450 degrees is 90");
    assert_eq!(
        (p[2].rotation, p[2].side),
        (270.0, Side::Bottom),
        "-90 degrees is 270"
    );
}

#[test]
fn inches_in_a_kicad_file_become_millimetres() {
    let p = from_kicad_pos("## Unit = inch, Angle = deg.\nR1 10k R_0603 1.0 0.5 0 top\n").unwrap();
    assert!((p[0].x - 25.4).abs() < 1e-9 && (p[0].y - 12.7).abs() < 1e-9);
}

#[test]
fn a_csv_is_read_with_either_delimiter_quotes_and_units() {
    let comma = "Designator,Value,Footprint,Mid X,Mid Y,Rotation,Layer\nR1,\"10k, 1%\",R_0603,10mm,20mm,90,Top\nC1,100n,C_0603,492mil,0.5in,0,Bottom\n";
    let p = from_csv(comma).unwrap();
    assert_eq!(p[0].value, "10k, 1%");
    assert!((p[1].x - 12.4968).abs() < 1e-3 && (p[1].y - 12.7).abs() < 1e-9);
    assert_eq!(p[1].side, Side::Bottom);
    let semi = "ref;val;package;x;y;rot;side\nR1;10k;R_0603;1;2;0;top\n";
    assert_eq!(from_csv(semi).unwrap()[0].y, 2.0);
    assert_eq!(
        parse(comma).unwrap().len(),
        2,
        "told apart from a KiCad file by its first line"
    );
    assert_eq!(parse(POS).unwrap().len(), 3);
    assert_eq!(
        parse("\u{feff}ref,val,package,x,y,rot,side\nR1,1k,R,0,0,0,top\n")
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn a_damaged_position_file_says_the_row() {
    assert!(from_csv("").unwrap_err().contains("empty"));
    assert!(from_csv("ref,val,x,y\nR1,1,2,3\n")
        .unwrap_err()
        .contains("package"));
    assert!(
        from_csv("ref,val,package,x,y,rot,side\nR1,1k,R,x,0,0,top\n")
            .unwrap_err()
            .contains("row 2")
    );
    assert!(
        from_csv("ref,val,package,x,y,rot,side\nR1,1k,R,0,0,0,middle\n")
            .unwrap_err()
            .contains("not a side")
    );
    assert!(
        from_csv("ref,val,package,x,y,rot,side\nR1,1k,R,99999999,0,0,top\n")
            .unwrap_err()
            .contains("not a position")
    );
    assert!(
        from_csv("ref,val,package,x,y,rot,side\nR1,1k,R,0,0,0,top\nR1,2k,R,1,1,0,top\n")
            .unwrap_err()
            .contains("twice")
    );
    assert!(from_csv("ref,val,package,x,y,rot,side\nR1,1k\n")
        .unwrap_err()
        .contains("row 2"));
    assert!(from_kicad_pos("R1 10k R_0603 1 2 0\n")
        .unwrap_err()
        .contains("7"));
    assert!(from_kicad_pos("R1 10k R_0603 1 2 nan top\n").is_err());
}

#[test]
fn the_feeders_are_read_and_checked() {
    let f = feeders_csv(
        "id,value,package,x,y,count\nF1,10k,R_0603,-20,10,100\nF2,100n,C_0603,-20,20,0\n",
    )
    .unwrap();
    assert_eq!((f.len(), f[0].count, f[1].count), (2, 100, 0));
    assert!(feeders_csv("id,value\n").unwrap_err().contains("exactly"));
    assert!(
        feeders_csv("id,value,package,x,y,count\nF1,a,b,0,0,5\nF1,c,d,0,0,5\n")
            .unwrap_err()
            .contains("twice")
    );
    assert!(feeders_csv("id,value,package,x,y,count\nF1,a,b,0,0,-5\n")
        .unwrap_err()
        .contains("count"));
    assert!(feeders_csv("id,value,package,x,y,count\nF1,a,b,0,0\n")
        .unwrap_err()
        .contains("6"));
}

#[test]
fn a_part_takes_the_feeder_of_its_value_and_package_without_regard_to_case() {
    let parts = vec![
        part("R1", "10K", "r_0603", 0.0, 0.0, Side::Top),
        part("C1", "100n", "C_0603", 1.0, 0.0, Side::Top),
        part("U1", "MCU", "QFN", 2.0, 0.0, Side::Top),
    ];
    let feeders = vec![
        feeder("F1", "10k", "R_0603", 0.0, 0.0, 5),
        feeder("F2", "100n", "C_0603", 0.0, 0.0, 5),
    ];
    let a = assign(&parts, &feeders);
    assert_eq!(a.assignments.len(), 2);
    assert_eq!(a.without_feeder, vec!["U1".to_string()]);
    assert!(a.short.is_empty());
}

#[test]
fn a_feeder_that_runs_out_is_reported_and_a_second_feeder_of_the_kind_takes_over() {
    let parts: Vec<Part> = (0..5)
        .map(|i| part(&format!("R{i}"), "10k", "R", f64::from(i), 0.0, Side::Top))
        .collect();
    let one = assign(&parts, &[feeder("F1", "10k", "R", 0.0, 0.0, 3)]);
    assert_eq!(
        (one.assignments.len(), one.short.clone()),
        (3, vec![("F1".to_string(), 2)])
    );
    let two = assign(
        &parts,
        &[
            feeder("F1", "10k", "R", 0.0, 0.0, 3),
            feeder("F2", "10k", "R", 1.0, 0.0, 2),
        ],
    );
    assert_eq!(two.assignments.len(), 5);
    assert!(two.short.is_empty());
    assert_eq!(two.assignments.iter().filter(|a| a.feeder == 1).count(), 2);
}

fn line_of_parts(n: usize) -> (Vec<Part>, Vec<Feeder>) {
    let parts: Vec<Part> = (0..n)
        .map(|i| {
            part(
                &format!("R{i}"),
                "10k",
                "R",
                100.0 + i as f64 * 2.0,
                50.0,
                Side::Top,
            )
        })
        .collect();
    (parts, vec![feeder("F1", "10k", "R", 0.0, 50.0, 1000)])
}

#[test]
fn every_part_is_placed_once() {
    let (parts, feeders) = line_of_parts(7);
    let a = assign(&parts, &feeders);
    let p = plan(
        &parts,
        &feeders,
        &a,
        &Settings {
            nozzles: 3,
            ..Settings::default()
        },
    )
    .unwrap();
    let mut refs: Vec<&str> = p.placements.iter().map(|x| x.reference.as_str()).collect();
    refs.sort_unstable();
    refs.dedup();
    assert_eq!(refs.len(), 7);
    assert_eq!(p.batches, 3);
    assert!(p
        .placements
        .iter()
        .all(|x| x.nozzle < 3 && x.feeder == "F1"));
}

#[test]
fn more_nozzles_mean_less_travel() {
    let (parts, feeders) = line_of_parts(12);
    let a = assign(&parts, &feeders);
    let travel = |n| {
        plan(
            &parts,
            &feeders,
            &a,
            &Settings {
                nozzles: n,
                ..Settings::default()
            },
        )
        .unwrap()
        .travel_mm
    };
    let (one, two, six) = (travel(1), travel(2), travel(6));
    assert!(two < one && six < two, "{one} {two} {six}");
}

#[test]
fn the_walk_over_the_board_goes_to_the_nearest_part_each_time() {
    let parts = vec![
        part("A", "x", "p", 90.0, 0.0, Side::Top),
        part("B", "x", "p", 10.0, 0.0, Side::Top),
        part("C", "x", "p", 50.0, 0.0, Side::Top),
    ];
    let feeders = vec![feeder("F", "x", "p", 0.0, 0.0, 9)];
    let p = plan(
        &parts,
        &feeders,
        &assign(&parts, &feeders),
        &Settings::default(),
    )
    .unwrap();
    assert_eq!(
        p.placements
            .iter()
            .map(|x| x.reference.as_str())
            .collect::<Vec<_>>(),
        ["B", "C", "A"]
    );
}

#[test]
fn the_bottom_side_is_mirrored_and_the_other_side_is_left_out() {
    let mut parts = vec![
        part("T1", "x", "p", 10.0, 5.0, Side::Top),
        part("B1", "x", "p", 10.0, 5.0, Side::Bottom),
    ];
    parts[1].rotation = 30.0;
    let feeders = vec![feeder("F", "x", "p", 0.0, 0.0, 9)];
    let a = assign(&parts, &feeders);
    let bottom = plan(
        &parts,
        &feeders,
        &a,
        &Settings {
            side: Side::Bottom,
            board_width_mm: Some(100.0),
            ..Settings::default()
        },
    )
    .unwrap();
    assert_eq!(bottom.placements.len(), 1);
    assert_eq!(
        (
            bottom.placements[0].reference.as_str(),
            bottom.placements[0].x,
            bottom.placements[0].rotation
        ),
        ("B1", 90.0, 150.0)
    );
    let top = plan(&parts, &feeders, &a, &Settings::default()).unwrap();
    assert_eq!((top.placements.len(), top.placements[0].x), (1, 10.0));
    assert!(plan(
        &parts,
        &feeders,
        &a,
        &Settings {
            side: Side::Bottom,
            ..Settings::default()
        }
    )
    .unwrap_err()
    .contains("width"));
}

#[test]
fn the_settings_are_checked() {
    let (parts, feeders) = line_of_parts(2);
    let a = assign(&parts, &feeders);
    for bad in [
        Settings {
            nozzles: 0,
            ..Settings::default()
        },
        Settings {
            nozzles: 7,
            ..Settings::default()
        },
        Settings {
            speed_mm_s: 0.0,
            ..Settings::default()
        },
        Settings {
            speed_mm_s: f64::NAN,
            ..Settings::default()
        },
        Settings {
            pick_s: -1.0,
            ..Settings::default()
        },
    ] {
        assert!(plan(&parts, &feeders, &a, &bad).is_err(), "{bad:?}");
    }
}

#[test]
fn an_empty_board_is_an_empty_plan_and_the_json_has_the_placements() {
    let p = plan(&[], &[], &assign(&[], &[]), &Settings::default()).unwrap();
    assert_eq!((p.placements.len(), p.batches, p.travel_mm), (0, 0, 0.0));
    let (parts, feeders) = line_of_parts(2);
    let json = plan(
        &parts,
        &feeders,
        &assign(&parts, &feeders),
        &Settings::default(),
    )
    .unwrap()
    .to_json();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["placements"].as_array().unwrap().len(), 2);
    assert_eq!(v["placements"][0]["feeder"], "F1");
    assert_eq!(v["schema_version"], 1);
}

#[test]
fn a_forged_assignment_is_an_error_not_a_panic() {
    let (parts, feeders) = line_of_parts(1);
    let mut a = assign(&parts, &feeders);
    a.assignments[0].part = 99;
    assert!(plan(&parts, &feeders, &a, &Settings::default())
        .unwrap_err()
        .contains("does not exist"));
    let mut b = assign(&parts, &feeders);
    b.assignments[0].feeder = 99;
    assert!(plan(&parts, &feeders, &b, &Settings::default())
        .unwrap_err()
        .contains("does not exist"));
}

// ---- more edges: byte order marks, units, exhausted feeders, exact travel and the command line ----

use std::path::PathBuf;
use std::process::{Command, Output};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("eh3d_pnp_{}_{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_eh3d-pnp"))
        .args(args)
        .output()
        .unwrap()
}

const HEAD: &str = "ref,val,package,x,y,rot,side\n";

#[test]
fn a_kicad_file_saved_with_a_byte_order_mark_and_windows_line_ends_is_still_read() {
    let with_bom = format!("\u{feff}{}", POS.replace('\n', "\r\n"));
    let parts = parse(&with_bom).unwrap();
    assert_eq!(parts, parse(POS).unwrap());
    assert_eq!(from_kicad_pos(&with_bom).unwrap().len(), 3);
}

#[test]
fn a_kicad_file_with_tabs_and_blank_lines_is_read() {
    let text = "## Unit = mm, Angle = deg.\n\n\tR1\t10k\tR_0603\t1.5\t2.5\t0\ttop\t\n\n";
    let p = from_kicad_pos(text).unwrap();
    assert_eq!((p.len(), p[0].x, p[0].y), (1, 1.5, 2.5));
}

#[test]
fn a_millimetre_header_with_the_letters_in_does_not_turn_the_file_into_inches() {
    // "origin" and "pin" contain "in": only the unit decides
    let p = from_kicad_pos("## Unit = mm, Origin = pin 1\nR1 1k R 10 20 0 top\n").unwrap();
    assert_eq!((p[0].x, p[0].y), (10.0, 20.0));
    let q = from_kicad_pos("## Unit = inches\nR1 1k R 1 1 0 top\n").unwrap();
    assert!((q[0].x - 25.4).abs() < 1e-9);
}

#[test]
fn lengths_with_units_spaces_and_signs_are_converted_and_the_limit_applies_after_converting() {
    let doc = |x: &str| format!("{HEAD}R1,1k,R,{x},0,0,top\n");
    assert!((from_csv(&doc("-1000mil")).unwrap()[0].x + 25.4).abs() < 1e-9);
    assert!((from_csv(&doc("2 in")).unwrap()[0].x - 50.8).abs() < 1e-9);
    assert!((from_csv(&doc("1.5E1MM")).unwrap()[0].x - 15.0).abs() < 1e-9);
    assert!(from_csv(&doc("10000mm")).is_ok());
    assert!(from_csv(&doc("10000.01mm")).is_err());
    assert!(from_csv(&doc("400in")).is_err(), "400 inches is 10160 mm");
    for bad in [
        "", "mm", "1..2", "inf", "-inf", "NaN", "1e999", "5 meters", "0x10",
    ] {
        let e = from_csv(&doc(bad)).unwrap_err();
        assert!(e.contains("row 2"), "{bad:?}: {e}");
    }
}

#[test]
fn angles_wrap_into_the_circle_whatever_their_size() {
    let doc = |a: &str| format!("{HEAD}R1,1k,R,0,0,{a},top\n");
    for (given, want) in [
        ("0", 0.0),
        ("360", 0.0),
        ("-1", 359.0),
        ("720.5", 0.5),
        ("-450", 270.0),
        ("1e9", 1e9_f64.rem_euclid(360.0)),
    ] {
        let r = from_csv(&doc(given)).unwrap()[0].rotation;
        assert!(
            (r - want).abs() < 1e-6 && (0.0..360.0).contains(&r),
            "{given}: {r}"
        );
    }
    assert!(from_csv(&doc("inf")).is_err());
}

#[test]
fn the_headers_of_other_cad_programs_are_found_by_name_in_any_order_and_case() {
    let altium = "Designator,Comment,Layer,Footprint,Center-X(mm),Center-Y(mm),Rotation\nR1,10k,TopLayer,0603,1,2,90\nC1,1u,BottomLayer,0603,3,4,0\n";
    let p = from_csv(altium).unwrap();
    assert_eq!((p[0].side, p[1].side), (Side::Top, Side::Bottom));
    assert_eq!((p[1].x, p[1].y), (3.0, 4.0));
    let shuffled = "SIDE,Rot,REFDES,y,X,PATTERN,VAL\nBack,10,U1,2,1,SOIC8,NE555\n";
    let q = from_csv(shuffled).unwrap();
    assert_eq!(
        (
            q[0].reference.as_str(),
            q[0].package.as_str(),
            q[0].side,
            q[0].rotation
        ),
        ("U1", "SOIC8", Side::Bottom, 10.0)
    );
}

#[test]
fn comment_lines_and_blank_lines_in_a_csv_are_skipped_but_a_short_row_is_not() {
    let text = format!("// exported\n{HEAD}\nR1,1k,R,0,0,0,top\n// middle\nR2,1k,R,1,0,0,top\n");
    assert_eq!(from_csv(&text).unwrap().len(), 2);
    let short = format!("{HEAD}R1,1k,R,0,0,0,top\nR2,1k,R\n");
    let e = from_csv(&short).unwrap_err();
    assert!(e.contains("row 3") && e.contains("3 fields"), "{e}");
}

#[test]
fn references_with_control_characters_or_nothing_at_all_are_refused() {
    assert!(from_csv(&format!("{HEAD},1k,R,0,0,0,top\n"))
        .unwrap_err()
        .contains("reference"));
    assert!(from_csv(&format!("{HEAD}\"R\u{7}1\",1k,R,0,0,0,top\n"))
        .unwrap_err()
        .contains("control"));
    // a reference is case sensitive: R1 and r1 are two parts
    assert_eq!(
        from_csv(&format!("{HEAD}R1,1k,R,0,0,0,top\nr1,1k,R,0,0,0,top\n"))
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn non_ascii_values_and_references_are_kept_and_matched() {
    let text = format!("{HEAD}Ω1,\"10 kΩ\",R_0603,0,0,0,top\n");
    let parts = from_csv(&text).unwrap();
    assert_eq!(parts[0].reference, "Ω1");
    let feeders = feeders_csv("id,value,package,x,y,count\nF1,10 kΩ,r_0603,0,0,4\n").unwrap();
    assert_eq!(assign(&parts, &feeders).assignments.len(), 1);
}

#[test]
fn the_same_value_in_another_package_does_not_share_a_feeder() {
    let parts = vec![part("R1", "10k", "R_0402", 0.0, 0.0, Side::Top)];
    let feeders = vec![feeder("F1", "10k", "R_0603", 0.0, 0.0, 9)];
    let a = assign(&parts, &feeders);
    assert!(a.assignments.is_empty());
    assert_eq!(a.without_feeder, ["R1"]);
}

#[test]
fn an_empty_feeder_is_short_and_two_empty_feeders_blame_the_last_one() {
    let parts: Vec<Part> = (0..3)
        .map(|i| part(&format!("R{i}"), "1k", "R", 0.0, 0.0, Side::Top))
        .collect();
    let empty = assign(&parts, &[feeder("F1", "1k", "R", 0.0, 0.0, 0)]);
    assert_eq!(
        (empty.assignments.len(), empty.short),
        (0, vec![("F1".to_string(), 3)])
    );
    let both = assign(
        &parts,
        &[
            feeder("F1", "1k", "R", 0.0, 0.0, 1),
            feeder("F2", "1k", "R", 0.0, 0.0, 1),
        ],
    );
    assert_eq!(both.assignments.len(), 2);
    assert_eq!(both.short, vec![("F2".to_string(), 1)]);
}

#[test]
fn the_feeder_counts_are_never_exceeded() {
    let parts: Vec<Part> = (0..20)
        .map(|i| part(&format!("R{i}"), "1k", "R", i as f64, 0.0, Side::Top))
        .collect();
    let feeders = vec![
        feeder("F1", "1k", "R", 0.0, 0.0, 7),
        feeder("F2", "1k", "R", 0.0, 0.0, 5),
    ];
    let a = assign(&parts, &feeders);
    for (n, f) in feeders.iter().enumerate() {
        assert!(a.assignments.iter().filter(|x| x.feeder == n).count() <= f.count as usize);
    }
    assert_eq!(
        a.assignments.len() as u32 + a.short.iter().map(|s| s.1).sum::<u32>(),
        20
    );
}

#[test]
fn the_feeders_file_refuses_what_cannot_be_a_position_or_an_id() {
    let doc = |row: &str| format!("id,value,package,x,y,count\n{row}\n");
    for row in [
        "F1,a,b,nan,0,1",
        "F1,a,b,0,inf,1",
        "F1,a,b,10001,0,1",
        "F1,a,b,0,0,1.5",
        "F1,a,b,0,0,4294967296",
        ",a,b,0,0,1",
    ] {
        assert!(feeders_csv(&doc(row)).is_err(), "{row}");
    }
    assert!(feeders_csv(&doc("F1,a,b,-10000,10000,0")).is_ok());
    assert_eq!(
        feeders_csv("\u{feff}ID, Value ,PACKAGE,X,Y,Count\r\nF1,a,b,0,0,1\r\n")
            .unwrap()
            .len(),
        1
    );
    assert!(feeders_csv("").unwrap_err().contains("empty"));
}

#[test]
fn the_time_and_the_travel_of_a_plan_add_up_exactly() {
    // feeder at (0,0), one part at (30,40): 0 -> feeder 0, feeder -> part 50
    let parts = vec![part("R1", "x", "p", 30.0, 40.0, Side::Top)];
    let feeders = vec![feeder("F", "x", "p", 0.0, 0.0, 1)];
    let s = Settings {
        speed_mm_s: 100.0,
        pick_s: 1.0,
        place_s: 2.0,
        ..Settings::default()
    };
    let p = plan(&parts, &feeders, &assign(&parts, &feeders), &s).unwrap();
    assert!((p.travel_mm - 50.0).abs() < 1e-9);
    assert!((p.seconds - (0.5 + 3.0)).abs() < 1e-9, "{}", p.seconds);
    assert_eq!(p.placements[0].pick, (0.0, 0.0));
}

#[test]
fn the_same_input_gives_the_same_plan_every_time_even_with_equal_distances() {
    let parts: Vec<Part> = (0..9)
        .map(|i| {
            part(
                &format!("R{i}"),
                "x",
                "p",
                10.0 * f64::from(i % 3),
                10.0 * f64::from(i / 3),
                Side::Top,
            )
        })
        .collect();
    let feeders = vec![
        feeder("F1", "x", "p", -5.0, 0.0, 99),
        feeder("F2", "y", "p", 0.0, -5.0, 99),
    ];
    let a = assign(&parts, &feeders);
    let s = Settings {
        nozzles: 4,
        ..Settings::default()
    };
    let first = plan(&parts, &feeders, &a, &s).unwrap();
    for _ in 0..5 {
        assert_eq!(plan(&parts, &feeders, &a, &s).unwrap(), first);
    }
    assert_eq!(
        first.to_json(),
        plan(&parts, &feeders, &a, &s).unwrap().to_json()
    );
}

#[test]
fn a_batch_never_holds_more_parts_than_the_nozzles_and_numbers_them_from_zero() {
    let (parts, feeders) = line_of_parts(10);
    let a = assign(&parts, &feeders);
    let p = plan(
        &parts,
        &feeders,
        &a,
        &Settings {
            nozzles: 4,
            ..Settings::default()
        },
    )
    .unwrap();
    assert_eq!(p.batches, 3);
    for b in 0..p.batches {
        let nozzles: Vec<usize> = p
            .placements
            .iter()
            .filter(|x| x.batch == b)
            .map(|x| x.nozzle)
            .collect();
        assert!(nozzles.len() <= 4);
        assert_eq!(nozzles, (0..nozzles.len()).collect::<Vec<_>>());
    }
}

#[test]
fn a_part_on_the_other_side_makes_an_empty_plan_for_this_side() {
    let parts = vec![part("B1", "x", "p", 1.0, 1.0, Side::Bottom)];
    let feeders = vec![feeder("F", "x", "p", 0.0, 0.0, 1)];
    let p = plan(
        &parts,
        &feeders,
        &assign(&parts, &feeders),
        &Settings::default(),
    )
    .unwrap();
    assert_eq!((p.placements.len(), p.batches, p.seconds), (0, 0, 0.0));
}

#[test]
fn mirroring_the_bottom_twice_gives_back_the_top_place_and_rotation() {
    let mut p = part("B1", "x", "p", 12.0, 7.0, Side::Bottom);
    p.rotation = 75.0;
    let feeders = vec![feeder("F", "x", "p", 0.0, 0.0, 1)];
    let s = Settings {
        side: Side::Bottom,
        board_width_mm: Some(80.0),
        ..Settings::default()
    };
    let once = plan(&[p.clone()], &feeders, &assign(&[p.clone()], &feeders), &s)
        .unwrap()
        .placements[0]
        .clone();
    let mut again = part("B1", "x", "p", once.x, once.y, Side::Bottom);
    again.rotation = once.rotation;
    let twice = plan(&[again.clone()], &feeders, &assign(&[again], &feeders), &s)
        .unwrap()
        .placements[0]
        .clone();
    assert!(
        (twice.x - 12.0).abs() < 1e-9 && (twice.rotation - 75.0).abs() < 1e-9 && twice.y == 7.0
    );
    let flat = Settings {
        board_width_mm: Some(f64::NAN),
        ..s
    };
    assert!(plan(&[p.clone()], &feeders, &assign(&[p], &feeders), &flat).is_err());
}

#[test]
fn the_command_line_plans_writes_and_refuses_a_board_the_feeders_do_not_cover() {
    let dir = scratch("cli_plan");
    let parts = dir.join("parts.csv");
    let feeders = dir.join("feeders.csv");
    let out = dir.join("plan.json");
    std::fs::write(
        &parts,
        format!("{HEAD}R1,10k,R,10,10,0,top\nR2,10k,R,20,10,0,top\nU1,MCU,QFN,30,10,0,top\n"),
    )
    .unwrap();
    std::fs::write(&feeders, "id,value,package,x,y,count\nF1,10k,R,0,0,5\n").unwrap();
    let (p, f, o) = (
        parts.to_str().unwrap(),
        feeders.to_str().unwrap(),
        out.to_str().unwrap(),
    );
    let check = cli(&["check", p, f]);
    assert_eq!(check.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&check.stdout).contains("no feeder for U1"));
    let refused = cli(&["plan", p, f, o]);
    assert_eq!(refused.status.code(), Some(1));
    assert!(!out.exists(), "a refused plan must not leave a file behind");
    let partial = cli(&["plan", p, f, o, "--partial", "--nozzles", "2"]);
    assert!(
        partial.status.success(),
        "{}",
        String::from_utf8_lossy(&partial.stderr)
    );
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
    assert_eq!(v["placements"].as_array().unwrap().len(), 2);
    assert_eq!(v["batches"], 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_command_line_names_a_missing_file_a_bad_number_and_a_bad_side() {
    let dir = scratch("cli_errors");
    let parts = dir.join("parts.csv");
    let feeders = dir.join("feeders.csv");
    std::fs::write(&parts, format!("{HEAD}R1,10k,R,10,10,0,top\n")).unwrap();
    std::fs::write(&feeders, "id,value,package,x,y,count\nF1,10k,R,0,0,5\n").unwrap();
    let (p, f) = (parts.to_str().unwrap(), feeders.to_str().unwrap());
    let out = dir.join("o.json");
    let o = out.to_str().unwrap();
    let missing = dir.join("nope.csv");
    let m = missing.to_str().unwrap();
    for (args, word) in [
        (vec!["parts", m], "nope.csv"),
        (vec!["check", p, m], "nope.csv"),
        (vec!["plan", p, f, o, "--nozzles", "many"], "--nozzles"),
        (vec!["plan", p, f, o, "--nozzles", "0"], "nozzles"),
        (vec!["plan", p, f, o, "--nozzles", "7"], "nozzles"),
        (vec!["plan", p, f, o, "--side", "left"], "--side"),
        (vec!["plan", p, f, o, "--side", "bottom"], "board"),
        (vec!["plan", p, f, o, "--speed", "0"], "speed"),
        (vec!["plan", p, f, o, "--speed"], "needs a value"),
        (vec!["plan", p, f], "output file"),
        (vec!["plan", p], "feeders"),
    ] {
        let r = cli(&args);
        let e = String::from_utf8_lossy(&r.stderr).into_owned();
        assert_eq!(r.status.code(), Some(1), "{args:?}");
        assert!(e.starts_with("error:") && e.contains(word), "{args:?}: {e}");
    }
    assert!(!out.exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_parts_command_prints_every_part_in_millimetres_and_reads_a_file_that_is_not_utf8() {
    let dir = scratch("cli_parts");
    let file = dir.join("p.pos");
    let mut bytes = b"## Unit = inch, Angle = deg.\nR1 caf".to_vec();
    bytes.push(0xe9); // latin-1 e-acute: not UTF-8
    bytes.extend_from_slice(b" R_0603 1 1 0 top\n");
    std::fs::write(&file, bytes).unwrap();
    let r = cli(&["parts", file.to_str().unwrap()]);
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
    let shown = String::from_utf8_lossy(&r.stdout).into_owned();
    assert!(shown.contains("25.400") && shown.contains("R1"), "{shown}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_plan_for_one_side_ignores_the_parts_of_the_other_side() {
    let parts = vec![
        part("R1", "10k", "R", 0.0, 0.0, Side::Top),
        part("D1", "LED", "L", 1.0, 0.0, Side::Bottom),
        part("D2", "LED", "L", 2.0, 0.0, Side::Top),
    ];
    // no feeder for the LED on the top side except one that holds a single part
    let feeders = vec![
        feeder("F1", "10k", "R", 0.0, 0.0, 5),
        feeder("F2", "LED", "L", 0.0, 0.0, 1),
    ];
    // all the sides together: the LEDs share one feeder with one part
    let both = assign(&parts, &feeders);
    assert_eq!(both.short.len(), 1);
    let top = assign_side(&parts, &feeders, Some(Side::Top));
    assert!(
        top.short.is_empty() && top.without_feeder.is_empty(),
        "{top:?}"
    );
    assert_eq!(top.assignments.len(), 2);
    // the numbers stay the numbers of the whole list
    assert!(top.assignments.iter().any(|a| a.part == 2));
    let bottom = assign_side(&parts, &feeders, Some(Side::Bottom));
    assert_eq!(bottom.assignments.len(), 1);
    assert_eq!(bottom.assignments[0].part, 1);
    // without the LED feeder the top side is still fully covered
    let only_r = assign_side(&parts[..2], &feeders[..1], Some(Side::Top));
    assert!(only_r.without_feeder.is_empty());
}

#[test]
fn the_unit_in_the_header_of_a_position_column_is_the_unit_of_a_bare_number() {
    let mil = "Designator,Comment,Footprint,Center-X(mil),Center-Y(mil),Rotation,Layer\nR1,10k,0603,500,1000,0,Top\n";
    let p = from_csv(mil).unwrap();
    assert!(
        (p[0].x - 12.7).abs() < 1e-9 && (p[0].y - 25.4).abs() < 1e-9,
        "{p:?}"
    );
    let inch = "Ref;Val;Package;PosX [in];PosY(in);Rot;Side\nR1;10k;0603;1;2;0;top\n";
    let p = from_csv(inch).unwrap();
    assert!(
        (p[0].x - 25.4).abs() < 1e-9 && (p[0].y - 50.8).abs() < 1e-9,
        "{p:?}"
    );
    // a suffix on the value still wins, and a header with no unit is millimetres
    let mixed = "Ref,Val,Package,PosX(mil),PosY,Rot,Side\nR1,10k,0603,10mm,3,0,top\n";
    let p = from_csv(mixed).unwrap();
    assert!(
        (p[0].x - 10.0).abs() < 1e-9 && (p[0].y - 3.0).abs() < 1e-9,
        "{p:?}"
    );
}

#[test]
fn a_latin1_file_is_noted_a_fractional_nozzle_count_is_refused_and_no_temporary_plan_is_left() {
    let dir = scratch("cli_misc");
    let parts = dir.join("parts.csv");
    let feeders = dir.join("feeders.csv");
    let out = dir.join("plan.json");
    // a value with the single byte 0xEA is not valid UTF-8 (it is a Latin-1 text)
    let mut bytes = format!("{HEAD}R1,10 k").into_bytes();
    bytes.push(0xEA);
    bytes.extend_from_slice(b",R,10,10,0,top\n");
    std::fs::write(&parts, bytes).unwrap();
    std::fs::write(&feeders, "id,value,package,x,y,count\nF1,10 k,R,0,0,5\n").unwrap();
    let (p, f, o) = (
        parts.to_str().unwrap(),
        feeders.to_str().unwrap(),
        out.to_str().unwrap(),
    );
    let checked = cli(&["check", p, f]);
    assert!(
        String::from_utf8_lossy(&checked.stderr).contains("read as Latin-1"),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
    std::fs::write(&parts, format!("{HEAD}R1,10k,R,10,10,0,top\n")).unwrap();
    std::fs::write(&feeders, "id,value,package,x,y,count\nF1,10k,R,0,0,5\n").unwrap();
    let frac = cli(&["plan", p, f, o, "--nozzles", "2.9"]);
    assert_eq!(frac.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&frac.stderr).contains("whole number"));
    assert!(cli(&["plan", p, f, o]).status.success());
    assert!(out.exists() && !dir.join("plan.json.tmp").exists());
    let _ = std::fs::remove_dir_all(&dir);
}
