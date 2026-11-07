//! Boards marked as bad (X-out), the parts that are marks, and the marks of two levels (the panel and the board).

use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::error::Code;
use eh3d_phoenix_pnp::fiducial::FitMode;
use eh3d_phoenix_pnp::marks::{
    correct_levels, fit_levels, group_of, is_fiducial_part, marks_of, parse_list,
    take_fiducial_parts,
};
use eh3d_phoenix_pnp::panel::Panel;
use eh3d_phoenix_pnp::parts::{from_csv, Part};

const EXAMPLE: &str = include_str!("../examples/machine.cfg");

fn words() -> Vec<String> {
    vec!["fiducial".to_string()]
}

fn refs() -> Vec<String> {
    vec!["fid".to_string(), "fd".to_string()]
}

fn one(text: &str) -> Part {
    from_csv(&format!("ref,val,package,x,y,rot,side\n{text}\n"))
        .unwrap()
        .remove(0)
}

fn mark(id: &str, x: f64, y: f64) -> (String, f64, f64) {
    (id.to_string(), x, y)
}

fn find<'a>(parts: &'a [Part], reference: &str) -> &'a Part {
    parts
        .iter()
        .find(|p| p.reference == reference)
        .unwrap_or_else(|| panic!("{reference} is not in the list"))
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

// ---- the parts that are marks

#[test]
fn a_part_is_a_mark_by_a_word_of_its_value_or_package_or_by_a_prefix_of_its_reference() {
    let w = words();
    let r = refs();
    assert!(is_fiducial_part(
        &one("M1,1mm,Fiducial_1mm,0,0,0,top"),
        &w,
        &r
    ));
    assert!(is_fiducial_part(
        &one("M2,FIDUCIAL,round,0,0,0,top"),
        &w,
        &r
    ));
    assert!(is_fiducial_part(&one("FID1,x,y,0,0,0,top"), &w, &r));
    assert!(is_fiducial_part(&one("fd12,x,y,0,0,0,top"), &w, &r));
    // the last word of the reference is the one that counts
    assert!(is_fiducial_part(&one("A/B/FID3,x,y,0,0,0,top"), &w, &r));
    // a prefix alone, or a prefix and letters, is a part like any other
    assert!(!is_fiducial_part(&one("FID,x,y,0,0,0,top"), &w, &r));
    assert!(!is_fiducial_part(&one("FIDO1,x,y,0,0,0,top"), &w, &r));
    assert!(!is_fiducial_part(&one("FD1A,x,y,0,0,0,top"), &w, &r));
    assert!(!is_fiducial_part(&one("R1,10k,0603,0,0,0,top"), &w, &r));
    // with empty lists nothing is a mark
    assert!(!is_fiducial_part(
        &one("FID1,Fiducial,Fiducial,0,0,0,top"),
        &[],
        &[]
    ));
}

#[test]
fn taking_the_marks_leaves_the_other_parts_in_their_order() {
    let mut parts = from_csv(
        "ref,val,package,x,y,rot,side\nR1,10k,0603,1,1,0,top\nFID1,m,Fiducial,2,2,0,top\nC1,1u,0603,3,3,0,top\nFD2,m,x,4,4,0,top\nU1,ic,QFN,5,5,0,top\n",
    )
    .unwrap();
    let taken = take_fiducial_parts(&mut parts, &words(), &refs());
    let rest: Vec<&str> = parts.iter().map(|p| p.reference.as_str()).collect();
    let marks: Vec<&str> = taken.iter().map(|p| p.reference.as_str()).collect();
    assert_eq!(rest, vec!["R1", "C1", "U1"]);
    assert_eq!(marks, vec!["FID1", "FD2"]);
    assert_eq!(
        marks_of(&taken),
        vec![mark("FID1", 2.0, 2.0), mark("FD2", 4.0, 4.0)]
    );
}

#[test]
fn the_board_of_a_mark_is_what_comes_before_its_last_slash() {
    assert_eq!(group_of("FID1"), "");
    assert_eq!(group_of("A/FID1"), "A");
    assert_eq!(group_of("B/C/FID2"), "B/C");
    assert_eq!(
        parse_list(" A, B/C ,,"),
        vec!["A".to_string(), "B/C".to_string()]
    );
    assert!(parse_list("").is_empty());
}

// ---- the marks of two levels

fn parts_of_two_boards() -> Vec<Part> {
    from_csv(
        "ref,val,package,x,y,rot,side\nA/R1,10k,R,5,5,0,top\nAB/R1,10k,R,6,6,0,top\nB/R1,10k,R,65,5,0,top\n",
    )
    .unwrap()
}

#[test]
fn the_fit_of_a_board_moves_its_parts_and_the_fit_of_the_panel_moves_the_others() {
    let panel = vec![mark("f1", -5.0, -5.0), mark("f2", 105.0, -5.0)];
    let boards = vec![mark("A/FID1", 1.0, 1.0), mark("A/FID2", 9.0, 1.0)];
    // the panel is 0.1 to the right and 0.2 up; the board A is 0.5 to the right and 0.5 up
    let measured = vec![
        mark("f1", -4.9, -4.8),
        mark("f2", 105.1, -4.8),
        mark("A/FID1", 1.5, 1.5),
        mark("A/FID2", 9.5, 1.5),
    ];
    let fits = fit_levels(&panel, &boards, &measured, FitMode::Auto, 0.5).unwrap();
    let groups: Vec<&str> = fits.iter().map(|f| f.group.as_str()).collect();
    assert_eq!(groups, vec!["", "A"]);
    let mut parts = parts_of_two_boards();
    correct_levels(&mut parts, &fits);
    let a = find(&parts, "A/R1");
    assert!(near(a.x, 5.5) && near(a.y, 5.5), "{} {}", a.x, a.y);
    // AB is another board whose name only starts like A: it takes the fit of the panel
    let ab = find(&parts, "AB/R1");
    assert!(near(ab.x, 6.1) && near(ab.y, 6.2), "{} {}", ab.x, ab.y);
    let b = find(&parts, "B/R1");
    assert!(near(b.x, 65.1) && near(b.y, 5.2), "{} {}", b.x, b.y);
}

#[test]
fn a_board_with_marks_and_a_panel_without_a_fit_moves_only_that_board() {
    let boards = vec![mark("A/FID1", 1.0, 1.0), mark("A/FID2", 9.0, 1.0)];
    let measured = vec![mark("A/FID1", 1.5, 1.5), mark("A/FID2", 9.5, 1.5)];
    let fits = fit_levels(&[], &boards, &measured, FitMode::Auto, 0.5).unwrap();
    assert_eq!(fits.len(), 1);
    let mut parts = parts_of_two_boards();
    correct_levels(&mut parts, &fits);
    assert!(near(find(&parts, "A/R1").x, 5.5));
    assert!(near(find(&parts, "B/R1").x, 65.0));
    assert!(near(find(&parts, "AB/R1").x, 6.0));
}

#[test]
fn the_panel_turn_is_worked_out_by_hand() {
    // the marks (0, 0) and (100, 0) are measured at (1, 2) and (1, 102): a quarter turn and a shift of (1, 2).
    // The part at (50, 10), turned 30 degrees, goes to (-10 + 1, 50 + 2) = (-9, 52) and is turned 120 degrees.
    let panel = vec![mark("f1", 0.0, 0.0), mark("f2", 100.0, 0.0)];
    let measured = vec![mark("f1", 1.0, 2.0), mark("f2", 1.0, 102.0)];
    let fits = fit_levels(&panel, &[], &measured, FitMode::Auto, 0.01).unwrap();
    let mut parts = from_csv("ref,val,package,x,y,rot,side\nA/R1,x,y,50,10,30,top\n").unwrap();
    correct_levels(&mut parts, &fits);
    let p = &parts[0];
    assert!(near(p.x, -9.0) && near(p.y, 52.0), "{} {}", p.x, p.y);
    assert!(near(p.rotation, 120.0), "{}", p.rotation);
}

#[test]
fn the_turn_of_a_board_is_worked_out_by_hand() {
    // the board A has its marks at (10, 10) and (30, 10); they are measured at (10, 10) and (10, 30): a quarter turn about (10, 10).
    // The part at (20, 12) is 10 to the right of it and 2 above: it goes to (10 - 2, 10 + 10) = (8, 20), turned 90 degrees.
    let boards = vec![mark("A/FID1", 10.0, 10.0), mark("A/FID2", 30.0, 10.0)];
    let measured = vec![mark("A/FID1", 10.0, 10.0), mark("A/FID2", 10.0, 30.0)];
    let fits = fit_levels(&[], &boards, &measured, FitMode::Auto, 0.01).unwrap();
    let mut parts =
        from_csv("ref,val,package,x,y,rot,side\nA/R1,x,y,20,12,0,top\nB/R1,x,y,20,12,0,top\n")
            .unwrap();
    correct_levels(&mut parts, &fits);
    let a = find(&parts, "A/R1");
    assert!(near(a.x, 8.0) && near(a.y, 20.0), "{} {}", a.x, a.y);
    assert!(near(a.rotation, 90.0), "{}", a.rotation);
    let b = find(&parts, "B/R1");
    assert!(near(b.x, 20.0) && near(b.y, 12.0) && near(b.rotation, 0.0));
}

#[test]
fn an_affine_fit_of_a_board_is_worked_out_by_hand() {
    // x is measured 0.2 percent longer, y 0.1 percent shorter, and everything is moved by (0.3, -0.2)
    let boards = vec![
        mark("A/F1", 0.0, 0.0),
        mark("A/F2", 100.0, 0.0),
        mark("A/F3", 0.0, 50.0),
    ];
    let measured = vec![
        mark("A/F1", 0.3, -0.2),
        mark("A/F2", 100.5, -0.2),
        mark("A/F3", 0.3, 49.75),
    ];
    let fits = fit_levels(&[], &boards, &measured, FitMode::Auto, 0.01).unwrap();
    assert_eq!(fits[0].fit.ids, vec!["A/F1", "A/F2", "A/F3"]);
    let mut parts = from_csv("ref,val,package,x,y,rot,side\nA/R1,x,y,40,20,0,top\n").unwrap();
    correct_levels(&mut parts, &fits);
    assert!(near(parts[0].x, 40.38) && near(parts[0].y, 19.78));
}

#[test]
fn the_marks_that_do_not_fit_are_refused_with_a_reason() {
    let panel = vec![mark("f1", 0.0, 0.0), mark("f2", 100.0, 0.0)];
    let boards = vec![mark("A/FID1", 1.0, 1.0), mark("A/FID2", 9.0, 1.0)];
    // a mark that nobody has
    let e = fit_levels(&panel, &boards, &[mark("zz", 0.0, 0.0)], FitMode::Auto, 0.5).unwrap_err();
    assert_eq!(e.code, Code::FiducialBad);
    assert!(
        e.detail.contains("the mark 'zz' is not in the panel"),
        "{}",
        e.detail
    );
    // one mark of a board is not enough, and the error says which board
    let e = fit_levels(
        &panel,
        &boards,
        &[mark("A/FID1", 1.0, 1.0)],
        FitMode::Auto,
        0.5,
    )
    .unwrap_err();
    assert_eq!(e.code, Code::FiducialBad);
    assert!(e.detail.starts_with("board A: "), "{}", e.detail);
    // a mark of a board that is 3 mm away from where the other puts it
    let three = vec![
        mark("A/F1", 0.0, 0.0),
        mark("A/F2", 10.0, 0.0),
        mark("A/F3", 0.0, 10.0),
        mark("A/F4", 10.0, 10.0),
    ];
    let measured = vec![
        mark("A/F1", 0.0, 0.0),
        mark("A/F2", 10.0, 0.0),
        mark("A/F3", 0.0, 10.0),
        mark("A/F4", 13.0, 10.0),
    ];
    let e = fit_levels(&[], &three, &measured, FitMode::Auto, 0.5).unwrap_err();
    assert!(
        e.detail.starts_with("board A: the mark 'A/F"),
        "{}",
        e.detail
    );
}

// ---- boards marked as bad

const BOARD: &str =
    "ref,val,package,x,y,rot,side\nR1,10k,R,10,20,0,top\nFID1,m,Fiducial,1,1,0,top\n";

fn loader(file: &str) -> Result<Vec<Part>, String> {
    match file {
        "board.pos" => from_csv(BOARD),
        other => Err(format!("{other}: no such board")),
    }
}

fn panel_with(x_out_b: &str) -> Panel {
    Panel::parse(&format!(
        r#"{{
  "schema_version": 1,
  "boards": {{ "main": "board.pos" }},
  "panels": {{
    "top": {{ "children": [
               {{ "id": "A", "board": "main", "x": 0, "y": 0, "rotation": 0 }},
               {{ "id": "B", "board": "main", "x": 60, "y": 0, "rotation": 0{x_out_b} }},
               {{ "id": "P", "panel": "pair", "x": 0, "y": 40, "rotation": 0 }} ],
             "fiducials": [ {{ "id": "f1", "x": 2, "y": 2 }} ] }},
    "pair": {{ "children": [ {{ "id": "C", "board": "main", "x": 0, "y": 0, "rotation": 0 }},
                             {{ "id": "D", "board": "main", "x": 60, "y": 0, "rotation": 0 }} ],
              "fiducials": [ {{ "id": "g1", "x": 1, "y": 0 }} ] }}
  }},
  "root": "top"
}}"#
    ))
    .unwrap()
}

fn references(parts: &[Part]) -> Vec<&str> {
    parts.iter().map(|p| p.reference.as_str()).collect()
}

#[test]
fn a_board_marked_as_bad_in_the_file_is_left_out_with_its_parts() {
    let panel = panel_with(r#", "x_out": true"#);
    let e = panel.expand(&loader).unwrap();
    assert_eq!(e.x_out, vec!["B".to_string()]);
    assert_eq!(
        references(&e.parts),
        vec!["A/R1", "A/FID1", "P/C/R1", "P/C/FID1", "P/D/R1", "P/D/FID1"]
    );
    // a board that is not marked is placed as it always was
    let all = panel_with("").expand(&loader).unwrap();
    assert!(all.x_out.is_empty());
    assert_eq!(all.parts.len(), 8);
}

#[test]
fn boards_named_on_the_command_line_are_left_out_too_and_a_panel_takes_all_it_holds() {
    let panel = panel_with("");
    let e = panel
        .expand_with(&loader, &["A".to_string(), "P/D".to_string()])
        .unwrap();
    assert_eq!(e.x_out, vec!["A".to_string(), "P/D".to_string()]);
    assert_eq!(
        references(&e.parts),
        vec!["B/R1", "B/FID1", "P/C/R1", "P/C/FID1"]
    );
    // the panel P left out: its boards and its marks go with it
    let e = panel.expand_with(&loader, &["P".to_string()]).unwrap();
    assert_eq!(e.x_out, vec!["P".to_string()]);
    assert_eq!(references(&e.parts).len(), 4);
    assert_eq!(e.fiducials, vec![("f1".to_string(), 2.0, 2.0)]);
    // and a panel that is whole keeps the marks of the panels inside it
    let whole = panel.expand(&loader).unwrap();
    let names: Vec<&str> = whole.fiducials.iter().map(|f| f.0.as_str()).collect();
    assert_eq!(names, vec!["f1", "P/g1"]);
}

#[test]
fn a_name_that_no_child_has_is_an_error_with_its_own_code() {
    let panel = panel_with("");
    let e = panel.expand_with(&loader, &["Z".to_string()]).unwrap_err();
    assert_eq!(e.code, Code::PanelXOutUnknown);
    assert_eq!(e.code.label(), "PHOENIX-PNP-042");
    assert!(e.detail.contains("'Z'"), "{}", e.detail);
    // the name of a board and not its path inside a panel is not a child either
    let e = panel.expand_with(&loader, &["C".to_string()]).unwrap_err();
    assert_eq!(e.code, Code::PanelXOutUnknown);
}

#[test]
fn a_panel_in_which_every_board_is_bad_is_an_error_with_its_own_code() {
    let panel = panel_with("");
    let all = ["A", "B", "P"].map(String::from);
    let e = panel.expand_with(&loader, &all).unwrap_err();
    assert_eq!(e.code, Code::PanelAllBad);
    assert_eq!(e.code.label(), "PHOENIX-PNP-043");
    // one board left is enough
    let two = ["A", "B"].map(String::from);
    assert!(panel.expand_with(&loader, &two).is_ok());
}

#[test]
fn the_flag_of_a_child_must_be_true_or_false() {
    let text = r#"{ "boards": { "m": "b" }, "panels": { "p": { "children": [
        { "id": "A", "board": "m", "x_out": "yes" } ] } }, "root": "p" }"#;
    let e = Panel::parse(text).unwrap_err();
    assert_eq!(e.code, Code::PanelBad);
    assert!(
        e.detail.contains("'x_out' is not true or false"),
        "{}",
        e.detail
    );
}

// ---- the settings

#[test]
fn the_settings_of_the_marks_in_the_example_are_the_defaults() {
    let default = MachineConfig::default();
    let example = MachineConfig::parse(EXAMPLE).unwrap();
    assert_eq!(example.fiducial_words, default.fiducial_words);
    assert_eq!(example.fiducial_refs, default.fiducial_refs);
    assert_eq!(default.fiducial_words, vec!["fiducial".to_string()]);
    assert_eq!(
        default.fiducial_refs,
        vec!["fid".to_string(), "fd".to_string()]
    );
}

#[test]
fn the_words_and_the_prefixes_of_the_marks_are_settings() {
    let c = MachineConfig::parse("fiducial_words = Mark, Target\nfiducial_refs = REF\n").unwrap();
    assert_eq!(
        c.fiducial_words,
        vec!["mark".to_string(), "target".to_string()]
    );
    assert_eq!(c.fiducial_refs, vec!["ref".to_string()]);
    let c = MachineConfig::parse("fiducial_words =\nfiducial_refs =\n").unwrap();
    assert!(c.fiducial_words.is_empty() && c.fiducial_refs.is_empty());
    let long = format!("fiducial_words = {}\n", "a".repeat(65));
    assert_eq!(
        MachineConfig::parse(&long).unwrap_err().code,
        Code::ConfigBadValue
    );
}
