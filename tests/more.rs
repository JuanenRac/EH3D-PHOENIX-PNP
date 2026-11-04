//! More tests: units and delimiters of the position files, the rows that are wrong, the file of feeders and the keys of parts and feeders.

use eh3d_phoenix_pnp::feeders::{from_csv as feeders_csv, Feeder};
use eh3d_phoenix_pnp::parts::{from_csv, from_kicad_pos, parse, Part, Side, MAX_PARTS};

const HEAD: &str = "Designator,Value,Package,X,Y,Rotation,Side\n";

fn one(row: &str) -> Result<Vec<Part>, String> {
    from_csv(&format!("{HEAD}{row}\n"))
}

#[test]
fn a_bare_number_is_millimetres_and_the_units_mm_mil_and_in_are_understood() {
    let p = one("R1,10k,0603,12.5mm,0.5in,0,top").unwrap();
    assert!((p[0].x - 12.5).abs() < 1e-9);
    assert!((p[0].y - 12.7).abs() < 1e-9);
    let p = one("R1,10k,0603,100mil,200 mil,0,top").unwrap();
    assert!((p[0].x - 2.54).abs() < 1e-9 && (p[0].y - 5.08).abs() < 1e-9);
    let p = one("R1,10k,0603,7,-3.25,0,top").unwrap();
    assert_eq!((p[0].x, p[0].y), (7.0, -3.25));
}

#[test]
fn a_semicolon_table_with_a_byte_order_mark_and_other_names_of_the_columns_is_read() {
    let text = "\u{feff}Ref;Val;Footprint;PosX;PosY;Rot;Layer\nC1;100n;0402;1,5;2;90;Bottom\n";
    // the comma of "1,5" would be the decimal comma of a European table: it is not a number for this reader, which must say so and not guess
    assert!(from_csv(text).is_err());
    let ok = "\u{feff}Ref;Val;Footprint;PosX;PosY;Rot;Layer\nC1;100n;0402;1.5;2;90;Bottom\n";
    let p = from_csv(ok).unwrap();
    assert_eq!(p.len(), 1);
    assert_eq!(
        (
            p[0].reference.as_str(),
            p[0].value.as_str(),
            p[0].package.as_str()
        ),
        ("C1", "100n", "0402")
    );
    assert_eq!(p[0].side, Side::Bottom);
    assert_eq!(p[0].rotation, 90.0);
}

#[test]
fn comment_lines_and_blank_lines_are_skipped_in_a_table() {
    let text = format!("// exported by a CAD program\n\n{HEAD}\nR1,1k,0603,1,2,0,top\n\n// end\nR2,2k,0603,3,4,0,top\n");
    assert_eq!(from_csv(&text).unwrap().len(), 2);
}

#[test]
fn a_table_that_is_empty_or_has_no_header_for_a_column_is_an_error_that_names_it() {
    assert!(from_csv("").unwrap_err().contains("empty"));
    assert!(from_csv("\n\n").is_err());
    let e = from_csv("Designator,Value,Package,X,Y,Rotation\nR1,1k,0603,1,2,0\n").unwrap_err();
    assert!(e.contains("side"), "{e}");
    let e = from_csv("Value,Package,X,Y,Rotation,Side\n1k,0603,1,2,0,top\n").unwrap_err();
    assert!(e.contains("reference"), "{e}");
}

#[test]
fn a_wrong_row_is_reported_with_its_number_in_the_file() {
    let text = format!("{HEAD}R1,1k,0603,1,2,0,top\nR2,1k,0603,abc,2,0,top\n");
    let e = from_csv(&text).unwrap_err();
    assert!(e.contains("row 3"), "{e}");
    let text = format!("{HEAD}R1,1k,0603,1,2\n");
    let e = from_csv(&text).unwrap_err();
    assert!(e.contains("row 2") && e.contains("fields"), "{e}");
}

#[test]
fn a_position_beyond_the_size_of_any_board_and_a_number_that_is_not_finite_are_refused() {
    assert!(one("R1,1k,0603,20000,0,0,top").is_err());
    assert!(one("R1,1k,0603,0,-20000,0,top").is_err());
    assert!(one("R1,1k,0603,NaN,0,0,top").is_err());
    assert!(one("R1,1k,0603,inf,0,0,top").is_err());
    // inches that make a distance beyond the limit are refused too
    assert!(one("R1,1k,0603,500in,0,0,top").is_err());
    assert!(one("R1,1k,0603,300in,0,0,top").is_ok());
}

#[test]
fn a_file_with_more_parts_than_the_limit_is_refused_and_one_at_the_limit_is_not() {
    let mut text = String::from(HEAD);
    for i in 0..MAX_PARTS {
        text.push_str(&format!("R{i},1k,0603,1,2,0,top\n"));
    }
    assert_eq!(from_csv(&text).unwrap().len(), MAX_PARTS);
    text.push_str("RX,1k,0603,1,2,0,top\n");
    assert!(from_csv(&text).unwrap_err().contains("more than"));
}

#[test]
fn a_kicad_position_file_is_read_with_its_unit_and_a_row_with_the_wrong_fields_is_refused() {
    let mm = "## Unit = mm, Angle = deg.\n# Ref Val Package PosX PosY Rot Side\nR1 10k R_0603 10.0 20.0 90.0 top\n";
    let p = from_kicad_pos(mm).unwrap();
    assert_eq!((p[0].x, p[0].y), (10.0, 20.0));
    let inch = "## Unit = inch, Angle = deg.\nR1 10k R_0603 1.0 2.0 0.0 bottom\n";
    let p = from_kicad_pos(inch).unwrap();
    assert!((p[0].x - 25.4).abs() < 1e-9 && (p[0].y - 50.8).abs() < 1e-9);
    assert_eq!(p[0].side, Side::Bottom);
    let e = from_kicad_pos("R1 10k R_0603 10.0 20.0 top\n").unwrap_err();
    assert!(e.contains("line 1") && e.contains('7'), "{e}");
}

#[test]
fn the_reader_that_chooses_by_the_content_takes_both_formats() {
    assert_eq!(
        parse(&format!("{HEAD}R1,1k,0603,1,2,0,top\n"))
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        parse("## Unit = mm\nR1 1k R_0603 1.0 2.0 0.0 top\n")
            .unwrap()
            .len(),
        1
    );
    assert!(parse("").is_err());
}

#[test]
fn a_part_and_a_feeder_agree_on_the_key_without_regard_to_case_or_blanks() {
    let part = Part {
        reference: "R1".into(),
        value: " 10K ".into(),
        package: "0603".into(),
        x: 0.0,
        y: 0.0,
        rotation: 0.0,
        side: Side::Top,
    };
    let feeder = Feeder {
        id: "F1".into(),
        value: "10k".into(),
        package: " 0603".into(),
        x: 0.0,
        y: 0.0,
        count: 5,
    };
    assert_eq!(part.key(), feeder.key());
    assert_eq!(part.key(), "10k|0603");
}

#[test]
fn the_feeders_need_the_exact_header_and_six_fields_in_a_row() {
    assert!(feeders_csv("").unwrap_err().contains("empty"));
    assert!(feeders_csv("id,value,package,x,y\nF1,1k,0603,1,2\n")
        .unwrap_err()
        .contains("exactly"));
    // the header is read without regard to case or blanks
    let ok = feeders_csv("ID, Value ,Package,X,Y,Count\nF1,1k,0603,1,2,10\n").unwrap();
    assert_eq!(ok.len(), 1);
    assert_eq!(ok[0].count, 10);
    let e = feeders_csv("id,value,package,x,y,count\nF1,1k,0603,1,2\n").unwrap_err();
    assert!(e.contains("row 2") && e.contains("not 6"), "{e}");
    let e = feeders_csv("id,value,package,x,y,count\nF1,1k,0603,1,2,-4\n").unwrap_err();
    assert!(e.contains("count"), "{e}");
    let e = feeders_csv("id,value,package,x,y,count\nF1,1k,0603,1e9,2,4\n").unwrap_err();
    assert!(e.contains("position"), "{e}");
}

#[test]
fn a_feeder_id_cannot_be_empty_or_used_twice() {
    let e = feeders_csv("id,value,package,x,y,count\nF1,1k,0603,1,2,4\nF1,2k,0603,3,4,4\n")
        .unwrap_err();
    assert!(e.contains("row 3") && e.contains("twice"), "{e}");
    assert!(feeders_csv("id,value,package,x,y,count\n,1k,0603,1,2,4\n").is_err());
}

#[test]
fn a_file_of_feeders_with_a_byte_order_mark_and_blank_lines_is_read() {
    let text = "\u{feff}id,value,package,x,y,count\n\nF1,1k,0603,1,2,4\n\nF2,2k,0603,3,4,0\n";
    let f = feeders_csv(text).unwrap();
    assert_eq!(f.len(), 2);
    assert_eq!(f[1].count, 0);
}

#[test]
fn quoted_fields_hold_the_delimiter_and_a_line_break_in_parts_and_in_feeders() {
    let text = format!("{HEAD}\"R1\",\"10k, 1%\",\"0603\nlong\",1,2,0,top\n");
    let p = from_csv(&text).unwrap();
    assert_eq!(p[0].value, "10k, 1%");
    assert_eq!(p[0].package, "0603\nlong");
    let semicolon = "Designator;Value;Package;X;Y;Rotation;Side\nR1;\"a;b\";0603;1;2;0;top\n";
    assert_eq!(from_csv(semicolon).unwrap()[0].value, "a;b");
    // the file of feeders is read by the same reader: quotes, a semicolon and a line break in a field
    let f = feeders_csv("id;value;package;x;y;count\nF1;\"10k; 1%\";0603;5;6;100\n").unwrap();
    assert_eq!((f[0].value.as_str(), f[0].count), ("10k; 1%", 100));
    let f = feeders_csv("id,value,package,x,y,count\r\nF1,\"10k, 1%\",0603,5,6,100\r\n").unwrap();
    assert_eq!(f[0].value, "10k, 1%");
    assert!(from_csv(&format!("{HEAD}R1,\"10k,0603,1,2,0,top\n")).is_err());
}

fn many(n: usize) -> (Vec<Part>, Vec<Feeder>) {
    let mut state: u64 = 0x2545_F491_4F6C_DD1D;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state % 100_000) as f64 / 1000.0
    };
    let parts = (0..n)
        .map(|i| Part {
            reference: format!("R{i}"),
            value: format!("{}k", i % 40),
            package: "0603".into(),
            x: next(),
            y: next(),
            rotation: 0.0,
            side: Side::Top,
        })
        .collect();
    let feeders = (0..40)
        .map(|k| Feeder {
            id: format!("F{k}"),
            value: format!("{k}k"),
            package: "0603".into(),
            x: f64::from(k),
            y: 0.0,
            count: 100_000,
        })
        .collect();
    (parts, feeders)
}

#[test]
fn fifty_thousand_parts_are_checked_assigned_and_planned_in_moments() {
    use eh3d_phoenix_pnp::feeders::assign;
    use eh3d_phoenix_pnp::plan::{plan, Settings};
    let (parts, feeders) = many(MAX_PARTS);
    let started = std::time::Instant::now();
    let a = assign(&parts, &feeders);
    assert_eq!(a.assignments.len(), MAX_PARTS);
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
    assert_eq!(p.placements.len(), MAX_PARTS);
    assert!(
        started.elapsed().as_secs() < 60,
        "{:?} for {MAX_PARTS} parts",
        started.elapsed()
    );
}

#[test]
fn the_nearest_first_walk_is_the_plain_one_with_the_first_of_the_list_winning_a_tie() {
    use eh3d_phoenix_pnp::plan::nearest_walk;
    let brute = |pts: &[(f64, f64)]| {
        let mut left: Vec<usize> = (0..pts.len()).collect();
        let (mut here, mut out) = ((0.0, 0.0), Vec::new());
        while !left.is_empty() {
            let d = |k: usize| ((pts[k].0 - here.0).powi(2) + (pts[k].1 - here.1).powi(2)).sqrt();
            let i = (0..left.len())
                .min_by(|&a, &b| d(left[a]).partial_cmp(&d(left[b])).unwrap())
                .unwrap();
            let k = left.remove(i);
            here = pts[k];
            out.push(k);
        }
        out
    };
    let (parts, _) = many(500);
    let pts: Vec<(f64, f64)> = parts.iter().map(|p| (p.x, p.y)).collect();
    assert_eq!(nearest_walk(&pts, (0.0, 0.0)), brute(&pts));
    // repeated points and points on a line: equal distances everywhere
    let odd = vec![
        (5.0, 0.0),
        (5.0, 0.0),
        (0.0, 5.0),
        (5.0, 0.0),
        (-5.0, 0.0),
        (0.0, 0.0),
    ];
    assert_eq!(nearest_walk(&odd, (0.0, 0.0)), brute(&odd));
    assert!(nearest_walk(&[], (0.0, 0.0)).is_empty());
}

#[test]
fn the_bottom_side_is_mirrored_about_the_middle_of_the_board_wherever_its_left_edge_is() {
    use eh3d_phoenix_pnp::feeders::assign_side;
    use eh3d_phoenix_pnp::plan::{plan, Settings};
    let parts = from_csv(&format!("{HEAD}R1,1k,0603,10,5,90,bottom\n")).unwrap();
    let feeders = feeders_csv("id,value,package,x,y,count\nF1,1k,0603,0,0,5\n").unwrap();
    let a = assign_side(&parts, &feeders, Some(Side::Bottom));
    let at = |origin: f64| {
        plan(
            &parts,
            &feeders,
            &a,
            &Settings {
                side: Side::Bottom,
                board_width_mm: Some(40.0),
                origin_x_mm: origin,
                ..Settings::default()
            },
        )
        .unwrap()
        .placements[0]
            .x
    };
    assert_eq!(at(0.0), 30.0);
    // the middle of a board that starts at X 100 and is 40 wide is 120: a part at 10 mirrors to 2 * 120 - 10
    assert_eq!(at(100.0), 230.0);
}

#[test]
fn a_plan_is_not_written_over_a_file_it_was_made_from() {
    let dir = std::env::temp_dir().join(format!("eh3d_pnp_same_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let parts = dir.join("parts.csv");
    let feeders = dir.join("feeders.csv");
    std::fs::write(&parts, format!("{HEAD}R1,1k,0603,10,5,0,top\n")).unwrap();
    std::fs::write(&feeders, "id,value,package,x,y,count\nF1,1k,0603,0,0,5\n").unwrap();
    let before = std::fs::read_to_string(&parts).unwrap();
    for out in [&parts, &feeders] {
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_phoenix-pnp"))
            .args([
                "plan",
                parts.to_str().unwrap(),
                feeders.to_str().unwrap(),
                out.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains("also an input"));
    }
    assert_eq!(std::fs::read_to_string(&parts).unwrap(), before);
    let _ = std::fs::remove_dir_all(&dir);
}
