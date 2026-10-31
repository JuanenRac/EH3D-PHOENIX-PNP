use eh3d_pnp_manager::feeders::{assign, from_csv as feeders_csv, Feeder};
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
