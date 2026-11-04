//! Panels: boards and panels inside panels, each in its place, turned and named.

use eh3d_phoenix_pnp::error::Code;
use eh3d_phoenix_pnp::panel::Panel;
use eh3d_phoenix_pnp::parts::{from_csv, Part};
use eh3d_phoenix_pnp::units::rotate_xy;
use std::cell::Cell;

const BOARD: &str = "ref,val,package,x,y,rot,side\nR1,10k,R,10,20,0,top\nR2,10k,R,15,20,90,top\n";

const PANEL: &str = r#"{
  "schema_version": 1,
  "boards": { "main": "board.pos" },
  "panels": {
    "top": { "children": [
               { "id": "A", "board": "main", "x": 0, "y": 0, "rotation": 0 },
               { "id": "B", "panel": "pair", "x": 60, "y": 0, "rotation": 90 } ],
             "fiducials": [ { "id": "f1", "x": 2, "y": 2 } ] },
    "pair": { "children": [ { "id": "C", "board": "main", "x": 0, "y": 0, "rotation": 0 } ],
              "fiducials": [ { "id": "g1", "x": 1, "y": 0 } ] }
  },
  "root": "top"
}"#;

fn loader(file: &str) -> Result<Vec<Part>, String> {
    match file {
        "board.pos" => from_csv(BOARD),
        other => Err(format!("{other}: no such board")),
    }
}

fn find<'a>(parts: &'a [Part], reference: &str) -> &'a Part {
    parts
        .iter()
        .find(|p| p.reference == reference)
        .unwrap_or_else(|| panic!("{reference} is not in the panel"))
}

#[test]
fn the_boards_are_put_in_their_places_and_named_by_their_children() {
    let panel = Panel::parse(PANEL).unwrap();
    assert_eq!(
        panel.boards(),
        vec![("main".to_string(), "board.pos".to_string())]
    );
    let e = panel.expand(&loader).unwrap();
    let names: Vec<&str> = e.parts.iter().map(|p| p.reference.as_str()).collect();
    assert_eq!(names, vec!["A/R1", "A/R2", "B/C/R1", "B/C/R2"]);
    let a1 = find(&e.parts, "A/R1");
    assert_eq!((a1.x, a1.y, a1.rotation), (10.0, 20.0, 0.0));
    // a board inside a panel turned a quarter: (10, 20) becomes (-20, 10) and then moves by (60, 0)
    let c1 = find(&e.parts, "B/C/R1");
    assert!(
        (c1.x - 40.0).abs() < 1e-9 && (c1.y - 10.0).abs() < 1e-9,
        "{} {}",
        c1.x,
        c1.y
    );
    assert!((c1.rotation - 90.0).abs() < 1e-9);
    let c2 = find(&e.parts, "B/C/R2");
    assert!((c2.x - 40.0).abs() < 1e-9 && (c2.y - 15.0).abs() < 1e-9);
    assert!((c2.rotation - 180.0).abs() < 1e-9);
    // the fiducials of the panels follow the same transforms
    assert_eq!(e.fiducials.len(), 2);
    assert_eq!(e.fiducials[0], ("f1".to_string(), 2.0, 2.0));
    let g = &e.fiducials[1];
    assert_eq!(g.0, "B/g1");
    assert!(
        (g.1 - 60.0).abs() < 1e-9 && (g.2 - 1.0).abs() < 1e-9,
        "{g:?}"
    );
}

#[test]
fn a_board_used_many_times_is_read_once() {
    let panel = Panel::parse(PANEL).unwrap();
    let calls = Cell::new(0);
    let counting = |file: &str| {
        calls.set(calls.get() + 1);
        loader(file)
    };
    let e = panel.expand(&counting).unwrap();
    assert_eq!(e.parts.len(), 4);
    assert_eq!(calls.get(), 1);
}

#[test]
fn the_same_panel_gives_the_same_parts_every_time() {
    let panel = Panel::parse(PANEL).unwrap();
    assert_eq!(
        panel.expand(&loader).unwrap(),
        panel.expand(&loader).unwrap()
    );
}

#[test]
fn a_panel_turned_as_a_whole_turns_every_part_about_its_origin() {
    for quarter in 0..8 {
        let deg = f64::from(quarter) * 45.0;
        let text = format!(
            r#"{{"boards": {{"main": "board.pos"}}, "panels": {{"p": {{"children": [{{"id": "A", "board": "main", "x": 0, "y": 0, "rotation": {deg}}}]}}}}, "root": "p"}}"#
        );
        let e = Panel::parse(&text).unwrap().expand(&loader).unwrap();
        let plain = from_csv(BOARD).unwrap();
        for (turned, original) in e.parts.iter().zip(&plain) {
            let (x, y) = rotate_xy(original.x, original.y, deg);
            assert!((turned.x - x).abs() < 1e-9 && (turned.y - y).abs() < 1e-9);
            assert!((turned.rotation - (original.rotation + deg).rem_euclid(360.0)).abs() < 1e-9);
            // the distance to the origin does not change
            assert!((turned.x.hypot(turned.y) - original.x.hypot(original.y)).abs() < 1e-9);
        }
    }
}

fn code_of(text: &str) -> Code {
    Panel::parse(text).unwrap_err().code
}

#[test]
fn a_bad_panel_is_refused_with_its_code() {
    let ok = |extra: &str| {
        format!(
            r#"{{"boards": {{"main": "board.pos"}}, "panels": {{"p": {{"children": [{extra}]}}}}, "root": "p"}}"#
        )
    };
    assert_eq!(code_of(""), Code::PanelBad);
    assert_eq!(code_of("[1, 2]"), Code::PanelBad);
    assert_eq!(code_of(r#"{"panels": {}}"#), Code::PanelBad);
    assert_eq!(
        code_of(r#"{"panels": {"p": {"children": []}}, "root": "q"}"#),
        Code::PanelBad
    );
    assert_eq!(
        code_of(r#"{"schema_version": 2, "panels": {"p": {"children": []}}, "root": "p"}"#),
        Code::PanelBad
    );
    // what a child names must exist, and a child names one thing
    assert_eq!(
        code_of(&ok(r#"{"id": "A", "board": "nope"}"#)),
        Code::PanelBad
    );
    assert_eq!(
        code_of(&ok(r#"{"id": "A", "panel": "nope"}"#)),
        Code::PanelBad
    );
    assert_eq!(
        code_of(&ok(r#"{"id": "A", "board": "main", "panel": "p"}"#)),
        Code::PanelBad
    );
    assert_eq!(code_of(&ok(r#"{"id": "A"}"#)), Code::PanelBad);
    // names
    assert_eq!(code_of(&ok(r#"{"board": "main"}"#)), Code::PanelBad);
    assert_eq!(
        code_of(&ok(r#"{"id": "A/B", "board": "main"}"#)),
        Code::PanelBad
    );
    assert_eq!(
        code_of(&ok(r#"{"id": "", "board": "main"}"#)),
        Code::PanelBad
    );
    assert_eq!(
        code_of(&ok(
            r#"{"id": "A", "board": "main"}, {"id": "A", "board": "main"}"#
        )),
        Code::PanelBad
    );
    // numbers
    assert_eq!(
        code_of(&ok(r#"{"id": "A", "board": "main", "x": "far"}"#)),
        Code::PanelBad
    );
    assert_eq!(
        code_of(&ok(r#"{"id": "A", "board": "main", "y": 1e9}"#)),
        Code::PanelBad
    );
    assert!(Panel::parse(&ok(r#"{"id": "A", "board": "main", "rotation": 1000}"#)).is_ok());
}

#[test]
fn a_panel_that_is_too_big_or_holds_itself_is_stopped() {
    let kids: Vec<String> = (0..1001)
        .map(|i| format!(r#"{{"id": "K{i}", "board": "main"}}"#))
        .collect();
    let big = format!(
        r#"{{"boards": {{"main": "board.pos"}}, "panels": {{"p": {{"children": [{}]}}}}, "root": "p"}}"#,
        kids.join(",")
    );
    assert_eq!(code_of(&big), Code::PanelTooBig);
    let selfish = r#"{"panels": {"p": {"children": [{"id": "A", "panel": "p"}, {"id": "B", "panel": "p"}]}}, "root": "p"}"#;
    let panel = Panel::parse(selfish).unwrap();
    assert_eq!(panel.expand(&loader).unwrap_err().code, Code::PanelTooBig);
}

#[test]
fn a_panel_with_too_many_children_in_all_is_stopped() {
    // three levels of 100 children make a million visits
    let many = |panel: &str| -> String {
        let kids: Vec<String> = (0..100)
            .map(|i| format!(r#"{{"id": "K{i}", "panel": "{panel}"}}"#))
            .collect();
        kids.join(",")
    };
    let text = format!(
        r#"{{"panels": {{"a": {{"children": [{}]}}, "b": {{"children": [{}]}}, "c": {{"children": []}}}}, "root": "a"}}"#,
        many("b"),
        many("c")
    );
    let panel = Panel::parse(&text).unwrap();
    // 100 + 100 * 100 = 10100 visits: fine
    assert!(panel.expand(&loader).is_ok());
    let deeper = format!(
        r#"{{"panels": {{"a": {{"children": [{}]}}, "b": {{"children": [{}]}}, "c": {{"children": [{}]}}, "d": {{"children": []}}}}, "root": "a"}}"#,
        many("b"),
        many("c"),
        many("d")
    );
    let e = Panel::parse(&deeper).unwrap().expand(&loader).unwrap_err();
    assert_eq!(e.code, Code::PanelTooBig);
}

#[test]
fn a_board_that_cannot_be_read_is_a_panel_error() {
    let text = r#"{"boards": {"x": "missing.pos"}, "panels": {"p": {"children": [{"id": "A", "board": "x"}]}}, "root": "p"}"#;
    let e = Panel::parse(text).unwrap().expand(&loader).unwrap_err();
    assert_eq!(e.code, Code::PanelBad);
    assert!(e.detail.contains("missing.pos"), "{}", e.detail);
}

#[test]
fn the_order_in_which_the_children_are_listed_gives_the_same_parts() {
    let a = r#"{"boards": {"m": "board.pos"}, "panels": {"p": {"children": [{"id": "A", "board": "m", "x": 0}, {"id": "B", "board": "m", "x": 50}]}}, "root": "p"}"#;
    let b = r#"{"boards": {"m": "board.pos"}, "panels": {"p": {"children": [{"id": "B", "board": "m", "x": 50}, {"id": "A", "board": "m", "x": 0}]}}, "root": "p"}"#;
    let mut pa = Panel::parse(a).unwrap().expand(&loader).unwrap().parts;
    let mut pb = Panel::parse(b).unwrap().expand(&loader).unwrap().parts;
    pa.sort_by(|x, y| x.reference.cmp(&y.reference));
    pb.sort_by(|x, y| x.reference.cmp(&y.reference));
    assert_eq!(pa, pb);
}
