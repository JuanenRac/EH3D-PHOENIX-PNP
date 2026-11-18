//! The panel made from a grid of one board: the places, the names, the boards marked as bad, the fiducials, the checks of the numbers and the command `panelize`.

use eh3d_phoenix_pnp::error::Code;
use eh3d_phoenix_pnp::panel::{grid_panel, Grid, Panel, MAX_GRID_SIDE};
use eh3d_phoenix_pnp::parts::{from_csv, Part};
use std::path::PathBuf;
use std::process::{Command, Output};

const BOARD: &str = "ref,val,package,x,y,rot,side\nR1,10k,R,10,20,0,top\nR2,10k,R,15,20,90,top\n";

fn loader(file: &str) -> Result<Vec<Part>, String> {
    match file {
        "board.pos" => from_csv(BOARD),
        other => Err(format!("{other}: no such board")),
    }
}

fn grid(cols: usize, rows: usize) -> Grid {
    Grid {
        board_file: "board.pos".to_string(),
        cols,
        rows,
        pitch_x_mm: 50.0,
        pitch_y_mm: 40.0,
        rotation_deg: 0.0,
        fiducial_margin_mm: 0.0,
        board_width_mm: 0.0,
        board_height_mm: 0.0,
        x_out: Vec::new(),
    }
}

fn find<'a>(parts: &'a [Part], reference: &str) -> &'a Part {
    parts
        .iter()
        .find(|p| p.reference == reference)
        .unwrap_or_else(|| panic!("{reference} is not in the panel"))
}

#[test]
fn the_boards_are_named_along_the_rows_from_the_lower_left_and_put_at_the_pitch() {
    let text = grid_panel(&grid(3, 2)).unwrap();
    let e = Panel::parse(&text).unwrap().expand(&loader).unwrap();
    assert_eq!(e.parts.len(), 12);
    for (name, x, y) in [
        ("P1/R1", 10.0, 20.0),
        ("P2/R1", 60.0, 20.0),
        ("P3/R1", 110.0, 20.0),
        ("P4/R1", 10.0, 60.0),
        ("P6/R1", 110.0, 60.0),
    ] {
        let p = find(&e.parts, name);
        assert_eq!((p.x, p.y), (x, y), "{name}");
    }
    assert!(e.fiducials.is_empty());
    assert!(e.x_out.is_empty());
}

#[test]
fn the_panel_file_is_a_panel_the_program_reads_and_it_names_the_board_file() {
    let text = grid_panel(&grid(2, 1)).unwrap();
    assert!(text.ends_with("}\n"));
    let panel = Panel::parse(&text).unwrap();
    assert_eq!(
        panel.boards(),
        vec![("board".to_string(), "board.pos".to_string())]
    );
    assert_eq!(panel.root(), "panel");
    // the same grid gives the same text
    assert_eq!(text, grid_panel(&grid(2, 1)).unwrap());
}

#[test]
fn every_board_is_turned_by_the_rotation_about_its_own_origin() {
    let mut g = grid(2, 1);
    g.rotation_deg = 90.0;
    let e = Panel::parse(&grid_panel(&g).unwrap())
        .unwrap()
        .expand(&loader)
        .unwrap();
    // (10, 20) turned a quarter is (-20, 10); the second board is 50 further along X
    let (a, b) = (find(&e.parts, "P1/R1"), find(&e.parts, "P2/R1"));
    assert!(
        (a.x + 20.0).abs() < 1e-9 && (a.y - 10.0).abs() < 1e-9,
        "{} {}",
        a.x,
        a.y
    );
    assert!(
        (b.x - 30.0).abs() < 1e-9 && (b.y - 10.0).abs() < 1e-9,
        "{} {}",
        b.x,
        b.y
    );
    assert!((a.rotation - 90.0).abs() < 1e-9);
}

#[test]
fn the_fiducials_are_three_and_outside_the_grid() {
    let mut g = grid(3, 2);
    g.fiducial_margin_mm = 5.0;
    let e = Panel::parse(&grid_panel(&g).unwrap())
        .unwrap()
        .expand(&loader)
        .unwrap();
    assert_eq!(
        e.fiducials,
        vec![
            ("f1".to_string(), -5.0, -5.0),
            ("f2".to_string(), 155.0, -5.0),
            ("f3".to_string(), -5.0, 85.0),
        ]
    );
}

#[test]
fn the_real_size_of_the_board_puts_the_fiducials_around_the_grid_as_it_is() {
    // 3 x 2 boards of 40 x 30 mm at a pitch of 50 x 40: the grid reaches 2 * 50 + 40 = 140 by 40 + 30 = 70, not 150 by 80
    let mut g = grid(3, 2);
    g.fiducial_margin_mm = 5.0;
    g.board_width_mm = 40.0;
    g.board_height_mm = 30.0;
    let e = Panel::parse(&grid_panel(&g).unwrap())
        .unwrap()
        .expand(&loader)
        .unwrap();
    assert_eq!(
        e.fiducials,
        vec![
            ("f1".to_string(), -5.0, -5.0),
            ("f2".to_string(), 145.0, -5.0),
            ("f3".to_string(), -5.0, 75.0),
        ]
    );
    // only one of the two sizes: the other direction keeps the whole pitch
    g.board_height_mm = 0.0;
    let e = Panel::parse(&grid_panel(&g).unwrap())
        .unwrap()
        .expand(&loader)
        .unwrap();
    assert_eq!(e.fiducials[1], ("f2".to_string(), 145.0, -5.0));
    assert_eq!(e.fiducials[2], ("f3".to_string(), -5.0, 85.0));
    // a size with no margin puts no fiducials, and one board is as wide as its size
    g.fiducial_margin_mm = 0.0;
    assert!(Panel::parse(&grid_panel(&g).unwrap())
        .unwrap()
        .expand(&loader)
        .unwrap()
        .fiducials
        .is_empty());
    let mut one = grid(1, 1);
    one.fiducial_margin_mm = 2.0;
    one.board_width_mm = 40.0;
    one.board_height_mm = 30.0;
    let e = Panel::parse(&grid_panel(&one).unwrap())
        .unwrap()
        .expand(&loader)
        .unwrap();
    assert_eq!(e.fiducials[1], ("f2".to_string(), 42.0, -2.0));
    assert_eq!(e.fiducials[2], ("f3".to_string(), -2.0, 32.0));
}

#[test]
fn panelize_takes_the_real_size_of_the_board_and_refuses_a_bad_one() {
    let base = [
        "panelize",
        "examples/board.pos",
        "--cols",
        "2",
        "--rows",
        "1",
        "--pitch-x",
        "70",
        "--pitch-y",
        "50",
        "--fiducials",
        "3",
    ];
    let mut with = base.to_vec();
    with.extend(["--board-size", "60,45"]);
    let o = run(&with);
    assert!(o.status.success(), "{}", text(&o.stderr));
    let panel = Panel::parse(&text(&o.stdout)).unwrap();
    let loader = |_: &str| from_csv(BOARD);
    let e = panel.expand(&loader).unwrap();
    // 70 + 60 = 130 wide and 45 high
    assert_eq!(e.fiducials[1], ("f2".to_string(), 133.0, -3.0));
    assert_eq!(e.fiducials[2], ("f3".to_string(), -3.0, 48.0));
    for bad in ["60", "60,x", "-5,10", "a,b"] {
        let mut args = base.to_vec();
        args.extend(["--board-size", bad]);
        assert!(!run(&args).status.success(), "{bad}");
    }
}

#[test]
fn boards_marked_as_bad_are_left_out_and_named_in_the_report_of_the_panel() {
    let mut g = grid(2, 2);
    g.x_out = vec!["P2".to_string(), "P3".to_string()];
    let e = Panel::parse(&grid_panel(&g).unwrap())
        .unwrap()
        .expand(&loader)
        .unwrap();
    assert_eq!(e.x_out, ["P2", "P3"]);
    assert_eq!(e.parts.len(), 4);
    assert!(e
        .parts
        .iter()
        .all(|p| !p.reference.starts_with("P2/") && !p.reference.starts_with("P3/")));
}

#[test]
fn a_name_that_the_grid_does_not_have_is_an_error_and_so_is_a_grid_all_bad() {
    let mut g = grid(2, 1);
    g.x_out = vec!["P3".to_string()];
    let e = grid_panel(&g).unwrap_err();
    assert_eq!(e.code, Code::PanelXOutUnknown);
    assert!(e.detail.contains("P1 to P2"), "{}", e.detail);
    g.x_out = vec!["P1".to_string(), "P2".to_string()];
    // the file is valid; it is expanding it that finds that nothing is left to place
    let text = grid_panel(&g).unwrap();
    assert_eq!(
        Panel::parse(&text)
            .unwrap()
            .expand(&loader)
            .unwrap_err()
            .code,
        Code::PanelAllBad
    );
}

#[test]
fn the_numbers_of_the_grid_are_checked() {
    let mut cases: Vec<(Grid, &str)> = Vec::new();
    let mut add = |f: &dyn Fn(&mut Grid), needle: &'static str| {
        let mut g = grid(2, 2);
        f(&mut g);
        cases.push((g, needle));
    };
    add(&|g| g.cols = 0, "1 to 100 boards");
    add(&|g| g.rows = MAX_GRID_SIDE + 1, "1 to 100 boards");
    add(&|g| g.pitch_x_mm = 0.0, "pitch along X");
    add(&|g| g.pitch_y_mm = f64::NAN, "pitch along Y");
    add(&|g| g.pitch_x_mm = 20_000.0, "pitch along X");
    add(&|g| g.rotation_deg = 361.0, "from -360 to 360");
    add(&|g| g.rotation_deg = f64::INFINITY, "from -360 to 360");
    add(&|g| g.fiducial_margin_mm = -1.0, "margin of the fiducials");
    add(&|g| g.board_width_mm = -1.0, "width of the board");
    add(&|g| g.board_height_mm = f64::NAN, "height of the board");
    add(&|g| g.board_width_mm = 20_000.0, "width of the board");
    add(&|g| g.board_file = "  ".to_string(), "file of the board");
    add(&|g| g.board_file = "a\nb".to_string(), "file of the board");
    for (g, needle) in cases {
        let e = grid_panel(&g).unwrap_err();
        assert_eq!(e.code, Code::PlanSetting, "{needle}");
        assert!(e.detail.contains(needle), "{needle}: {}", e.detail);
    }
    // the most that is allowed in each direction, and too many boards altogether
    assert!(grid_panel(&grid(1, MAX_GRID_SIDE)).is_ok());
    let e = grid_panel(&grid(MAX_GRID_SIDE, MAX_GRID_SIDE)).unwrap_err();
    assert!(e.detail.contains("more than 1000 boards"), "{}", e.detail);
}

#[test]
fn a_board_file_with_quotes_or_a_backslash_is_kept_whole() {
    let mut g = grid(1, 1);
    g.board_file = "my \"board\" v2\\final.pos".to_string();
    let panel = Panel::parse(&grid_panel(&g).unwrap()).unwrap();
    assert_eq!(panel.boards()[0].1, "my \"board\" v2\\final.pos");
}

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

#[test]
fn panelize_writes_a_panel_that_simulate_can_plan() {
    let dir = std::env::temp_dir().join(format!("phoenix_pnp_panelize_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(root().join("examples/board.pos"), dir.join("board.pos")).unwrap();
    let panel = dir.join("grid.json");
    let board = dir.join("board.pos");
    let o = run(&[
        "panelize",
        board.to_str().unwrap(),
        "--cols",
        "2",
        "--rows",
        "2",
        "--pitch-x",
        "60",
        "--pitch-y",
        "50",
        "--fiducials",
        "4",
        "--x-out",
        "P4",
        "--out",
        panel.to_str().unwrap(),
    ]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    assert!(
        text(&o.stdout).starts_with("ok: 4 boards (2 x 2); written to "),
        "{}",
        text(&o.stdout)
    );
    let written = std::fs::read_to_string(&panel).unwrap();
    // the board beside the panel is named by its file name alone
    assert!(written.contains("\"board\": \"board.pos\""), "{written}");
    assert!(written.contains("\"x_out\": true"));
    // a file that exists is never written over
    let again = run(&[
        "panelize",
        board.to_str().unwrap(),
        "--cols",
        "1",
        "--rows",
        "1",
        "--pitch-x",
        "1",
        "--pitch-y",
        "1",
        "--out",
        panel.to_str().unwrap(),
    ]);
    assert!(!again.status.success());
    assert!(
        text(&again.stderr).contains("PHOENIX-PNP-040"),
        "{}",
        text(&again.stderr)
    );
    assert_eq!(std::fs::read_to_string(&panel).unwrap(), written);
    // the program reads it as a panel in place of a table of parts
    let o = run(&[
        "preflight",
        "--panel",
        panel.to_str().unwrap(),
        "examples/feeders.csv",
    ]);
    // the feeders of the example do not hold every kind of part of the grid, but the panel itself is read: the parts are those of three boards
    let out = text(&o.stdout) + &text(&o.stderr);
    assert!(!out.contains("PHOENIX-PNP-014"), "{out}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn panelize_prints_to_the_standard_output_and_is_strict() {
    let o = run(&[
        "panelize",
        "examples/board.pos",
        "--cols",
        "2",
        "--rows",
        "1",
        "--pitch-x",
        "70",
        "--pitch-y",
        "50",
    ]);
    assert!(o.status.success(), "{}", text(&o.stderr));
    let panel = Panel::parse(&text(&o.stdout)).unwrap();
    // the board is not beside a panel file, so it is named as it was given
    assert_eq!(panel.boards()[0].1, "examples/board.pos");
    assert!(text(&o.stderr).contains("ok: 2 boards"));
    for bad in [
        vec![
            "panelize",
            "examples/board.pos",
            "--rows",
            "1",
            "--pitch-x",
            "1",
            "--pitch-y",
            "1",
        ],
        vec![
            "panelize",
            "--cols",
            "1",
            "--rows",
            "1",
            "--pitch-x",
            "1",
            "--pitch-y",
            "1",
        ],
        vec![
            "panelize",
            "examples/board.pos",
            "--cols",
            "x",
            "--rows",
            "1",
            "--pitch-x",
            "1",
            "--pitch-y",
            "1",
        ],
        vec![
            "panelize",
            "examples/board.pos",
            "--cols",
            "1",
            "--rows",
            "1",
            "--pitch-x",
            "a",
            "--pitch-y",
            "1",
        ],
        vec![
            "panelize",
            "examples/board.pos",
            "--cols",
            "1",
            "--rows",
            "1",
            "--pitch-x",
            "1",
            "--pitch-y",
            "1",
            "--oops",
        ],
        vec![
            "panelize",
            "examples/board.pos",
            "extra",
            "--cols",
            "1",
            "--rows",
            "1",
            "--pitch-x",
            "1",
            "--pitch-y",
            "1",
        ],
    ] {
        assert!(!run(&bad).status.success(), "{bad:?}");
    }
    let o = run(&[
        "panelize",
        "examples/board.pos",
        "--cols",
        "0",
        "--rows",
        "1",
        "--pitch-x",
        "1",
        "--pitch-y",
        "1",
        "--lang",
        "de",
    ]);
    assert!(
        text(&o.stderr).contains("PHOENIX-PNP-017: Eine Einstellung des Plans"),
        "{}",
        text(&o.stderr)
    );
}
