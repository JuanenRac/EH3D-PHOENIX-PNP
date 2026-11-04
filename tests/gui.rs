//! The window without a screen: the words in eight languages, the colours, the files that are loaded and told apart, the plan and its problems, the fixes, the simulated job that is
//! paused, resumed and stepped, the geometry of the view, the tables and their selection, the guide, the settings and the whole path from files to a finished job.

use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::error::{Code, Lang, PnpError};
use eh3d_phoenix_pnp::gui::app::{AppState, Tab};
use eh3d_phoenix_pnp::gui::log::{diagnostics, Level, Log, MAX_LINES};
use eh3d_phoenix_pnp::gui::problems::{self, Fix};
use eh3d_phoenix_pnp::gui::samples;
use eh3d_phoenix_pnp::gui::session::{Next, Session};
use eh3d_phoenix_pnp::gui::settings::{settings_path, Layers, UiSettings};
use eh3d_phoenix_pnp::gui::tables::{
    feeder_rows, kind_rows, natural_cmp, part_rows, plan_rows, sort_parts, CellEdit, PartColumn,
    Selection, SortState,
};
use eh3d_phoenix_pnp::gui::texts::{table, tr, Text};
use eh3d_phoenix_pnp::gui::theme::ThemeId;
use eh3d_phoenix_pnp::gui::view::{
    outline, package_size, polarity_mark, Bounds, Scene, Transform, Viewport,
};
use eh3d_phoenix_pnp::gui::wizard::{PlanState, Step, Wizard};
use eh3d_phoenix_pnp::gui::workspace::{classify, FileKind, Workspace};
use eh3d_phoenix_pnp::job::Phase;
use eh3d_phoenix_pnp::parts::Side;
use eh3d_phoenix_pnp::preflight::{has_errors, Severity, Source};
use std::cmp::Ordering;
use std::path::PathBuf;

const PARTS: &str = "ref,val,package,x,y,rot,side\nR1,10k,R,10,10,0,top\nR2,10k,R,20,10,90,top\nR3,10k,R,30,10,180,top\nC1,100n,C,10,20,0,top\nC2,100n,C,20,20,0,top\nU1,MCU,Q,40,15,0,top\n";
const FEEDERS: &str =
    "id,value,package,x,y,count\nF1,10k,R,-20,10,10\nF2,100n,C,-20,20,10\nF3,MCU,Q,-20,40,2\n";

fn loaded() -> Workspace {
    let mut ws = Workspace::new(Lang::En);
    ws.load_text("parts.csv", PARTS).unwrap();
    ws.load_text("feeders.csv", FEEDERS).unwrap();
    ws
}

fn temp_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("phoenix_pnp_gui_{tag}_{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn started(cfg_text: &str) -> (Workspace, Session) {
    let mut ws = loaded();
    ws.load_text("machine.cfg", cfg_text).unwrap();
    let planned = ws.make_plan().unwrap();
    let mut s = Session::new(4.0);
    s.start(planned.plan.clone(), ws.feeders.clone(), ws.cfg.clone())
        .unwrap();
    (ws, s)
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

fn sample_pair(parts: &str, feeders: &str) -> Vec<(String, String)> {
    vec![
        ("p.csv".to_string(), parts.to_string()),
        ("f.csv".to_string(), feeders.to_string()),
    ]
}

// ---- the words and the colours

#[test]
fn every_word_has_a_row_and_every_row_has_eight_filled_columns() {
    assert_eq!(table().len(), Text::COUNT);
    for (i, row) in table().iter().enumerate() {
        for cell in row {
            assert!(!cell.is_empty(), "row {i} has an empty cell");
            assert_eq!(cell.trim(), *cell, "row {i} has spaces round a cell");
        }
    }
}

#[test]
fn the_words_are_really_translated() {
    assert_eq!(tr(Text::Plan, Lang::En), "Plan");
    assert_eq!(tr(Text::Plan, Lang::Pt), "Planear");
    assert_eq!(tr(Text::TabLoadSheet, Lang::Es), "Hoja de carga");
    assert_eq!(tr(Text::StatusPlaced, Lang::De), "Bestückt");
    // the Japanese and the Chinese columns are written in their own letters, but for the few rows that are only a unit or an id
    let ascii_ja = table().iter().filter(|r| r[5].is_ascii()).count();
    let ascii_zh = table().iter().filter(|r| r[6].is_ascii()).count();
    assert!(ascii_ja <= 3, "{ascii_ja} Japanese cells are plain ASCII");
    assert!(ascii_zh <= 2, "{ascii_zh} Chinese cells are plain ASCII");
    // Portuguese of Portugal
    assert_eq!(tr(Text::TabLog, Lang::Pt), "Registo");
    assert_eq!(tr(Text::TabLog, Lang::Es), "Registro");
}

#[test]
fn the_themes_have_keys_and_distinct_colours() {
    for id in ThemeId::ALL {
        assert_eq!(ThemeId::from_key(id.key()), Some(id));
        assert_eq!(ThemeId::from_key(&id.key().to_ascii_uppercase()), Some(id));
    }
    assert_eq!(ThemeId::from_key("neon"), None);
    let phoenix = ThemeId::Phoenix.palette();
    let light = ThemeId::Light.palette();
    assert!(phoenix.dark && !light.dark);
    assert_ne!(phoenix.background, light.background);
    // the phoenix is green: the accent has more green than red or blue
    assert!(phoenix.accent.1 > phoenix.accent.0 && phoenix.accent.1 > phoenix.accent.2);
}

#[test]
fn the_window_keys_of_the_configuration_are_read_and_checked() {
    let d = MachineConfig::default();
    assert!(near(d.gui_steps_per_s, 4.0));
    assert_eq!(d.gui_theme, "phoenix");
    assert!(near(d.gui_part_size_mm, 2.0));
    let c = MachineConfig::parse("gui_steps_per_s = 25\ngui_theme = LIGHT\ngui_part_size_mm = 3.5")
        .unwrap();
    assert!(near(c.gui_steps_per_s, 25.0));
    assert_eq!(c.gui_theme, "light");
    assert!(near(c.gui_part_size_mm, 3.5));
    assert_eq!(
        MachineConfig::parse("gui_steps_per_s = 0")
            .unwrap_err()
            .code,
        Code::ConfigOutOfRange
    );
    assert_eq!(
        MachineConfig::parse("gui_theme = neon").unwrap_err().code,
        Code::ConfigBadValue
    );
    assert_eq!(
        MachineConfig::parse("gui_part_size_mm = 100")
            .unwrap_err()
            .code,
        Code::ConfigOutOfRange
    );
}

// ---- the files

#[test]
fn a_file_is_told_from_its_name_and_its_first_line() {
    assert_eq!(classify("board.pos", ""), Some(FileKind::Parts));
    assert_eq!(classify("machine.cfg", ""), Some(FileKind::Config));
    assert_eq!(classify("panel.json", ""), Some(FileKind::Panel));
    let csv = |head: &str| classify("x.csv", head);
    assert_eq!(csv("ref,val,package,x,y,rot,side\n"), Some(FileKind::Parts));
    assert_eq!(csv("id,value,package,x,y,count\n"), Some(FileKind::Feeders));
    assert_eq!(csv("id;value;package;x;y;count\n"), Some(FileKind::Feeders));
    assert_eq!(csv("id,x,y,max_width_mm\n"), Some(FileKind::Slots));
    assert_eq!(csv("package,height_mm\nR,1\n"), Some(FileKind::Packages));
    assert_eq!(csv("value,package,rank\n"), Some(FileKind::Catalog));
    assert_eq!(
        classify("x.txt", "### Module positions\n"),
        Some(FileKind::Parts)
    );
    assert_eq!(classify("x.txt", "{\"boards\":{}}"), Some(FileKind::Panel));
    assert_eq!(
        classify("x.txt", "# comment\nnozzles = 2\n"),
        Some(FileKind::Config)
    );
    assert_eq!(classify("x.txt", "\n\n"), None);
}

#[test]
fn the_workspace_plans_what_was_loaded() {
    let mut ws = Workspace::new(Lang::En);
    assert!(!ws.can_plan());
    assert_eq!(ws.load_text("parts.csv", PARTS).unwrap(), FileKind::Parts);
    assert!(!ws.can_plan());
    assert_eq!(
        ws.load_text("feeders.csv", FEEDERS).unwrap(),
        FileKind::Feeders
    );
    assert!(ws.can_plan());
    let p = ws.make_plan().unwrap();
    assert_eq!(p.plan.plan.placements.len(), 6);
    assert!(!has_errors(&p.checks));
    assert_eq!(ws.names.get(&FileKind::Parts).unwrap(), "parts.csv");
    ws.clear();
    assert!(ws.parts.is_empty() && ws.names.is_empty());
}

#[test]
fn a_configuration_loaded_after_the_parts_turns_them_again() {
    let mut ws = loaded();
    assert!(near(ws.parts[0].rotation, 0.0));
    ws.load_text("machine.cfg", "rotation = R : 90\n").unwrap();
    assert!(near(ws.parts[0].rotation, 90.0));
    assert!(near(ws.parts[1].rotation, 180.0));
    // loading it twice does not turn them twice
    ws.load_text("machine.cfg", "rotation = R : 90\n").unwrap();
    assert!(near(ws.parts[0].rotation, 90.0));
}

#[test]
fn a_configuration_that_names_the_language_changes_it() {
    let mut ws = Workspace::new(Lang::En);
    ws.load_text("a.cfg", "nozzles = 2\n").unwrap();
    assert_eq!(ws.lang, Lang::En);
    ws.load_text("b.cfg", "language = ja\n").unwrap();
    assert_eq!(ws.lang, Lang::Ja);
}

#[test]
fn a_bad_file_is_told_with_its_code_in_the_language_and_nothing_changes() {
    let mut ws = loaded();
    ws.lang = Lang::Es;
    let e = ws
        .load_text("f.csv", "id,value,package,x,y,count\nF1,10k,R,a,b,c\n")
        .unwrap_err();
    assert!(e.contains(&Code::FeederBadRow.label()), "{e}");
    assert!(e.contains(Code::FeederBadRow.message(Lang::Es)), "{e}");
    assert_eq!(ws.feeders.len(), 3);
    let e = ws.load_text("c.cfg", "nozzles = 99\n").unwrap_err();
    assert!(e.contains(&Code::ConfigOutOfRange.label()), "{e}");
    assert_eq!(ws.cfg.nozzles, 1);
}

#[test]
fn a_file_larger_than_the_limit_is_refused() {
    let mut ws = Workspace::new(Lang::En);
    ws.load_text("m.cfg", "max_file_bytes = 1024\n").unwrap();
    let big = "x".repeat(2000);
    let e = ws.load_kind(FileKind::Parts, "big.csv", &big).unwrap_err();
    assert!(e.contains(&Code::FileTooBig.label()), "{e}");
}

#[test]
fn a_panel_is_read_from_its_path_with_its_boards() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples");
    let mut ws = Workspace::new(Lang::En);
    assert_eq!(
        ws.load_path(&root.join("panel.json")).unwrap(),
        FileKind::Panel
    );
    assert_eq!(ws.parts.len(), 14);
    assert!(ws.parts.iter().any(|p| p.reference == "A/R1"));
    assert!(ws.parts.iter().any(|p| p.reference == "B/R1"));
    assert_eq!(ws.names.get(&FileKind::Panel).unwrap(), "panel.json");
}

#[test]
fn the_bottom_side_is_planned_about_the_middle_of_the_board() {
    let mut a = AppState::new(UiSettings::default());
    assert_eq!(a.load_sample(&samples::examples()), 6);
    a.set_side(Side::Bottom);
    let planned = a.planned.as_ref().expect("the bottom side has a plan");
    assert_eq!(planned.plan.plan.placements.len(), 1);
    assert_eq!(planned.plan.plan.placements[0].reference, "D1");
}

// ---- the problems and their fixes

fn broken() -> Workspace {
    let mut ws = loaded();
    ws.load_text("parts.csv", &format!("{PARTS}X1,odd,Z,1,1,0,top\n"))
        .unwrap();
    ws.load_text(
        "feeders.csv",
        &FEEDERS.replace("F3,MCU,Q,-20,40,2", "F3,MCU,Q,-20,40,0"),
    )
    .unwrap();
    ws
}

#[test]
fn the_problems_name_what_they_are_about_and_offer_a_fix() {
    let ws = broken();
    let planned = ws.make_plan().unwrap();
    let list = problems::detect(&planned);
    assert!(list.iter().any(|p| p.severity == Severity::Error));
    let part = list
        .iter()
        .find(|p| p.source == Source::Part("X1".to_string()))
        .expect("a problem for X1");
    assert_eq!(part.fix, Some(Fix::DropPart("X1".to_string())));
    assert_eq!(part.about(Lang::De), "Teil X1");
    assert_eq!(part.severity_text(Lang::Es), "Error");
    assert!(part.fix_text(Lang::En).is_some());
    let feeder = list
        .iter()
        .find(|p| p.source == Source::Feeder("F3".to_string()))
        .expect("a problem for F3");
    assert_eq!(
        feeder.fix,
        Some(Fix::RaiseFeeder {
            id: "F3".to_string(),
            missing: 1
        })
    );
}

#[test]
fn a_fix_puts_the_problem_right_and_can_be_taken_back() {
    let mut ws = broken();
    let planned = ws.make_plan().unwrap();
    let fixes: Vec<Fix> = problems::detect(&planned)
        .into_iter()
        .filter_map(|p| p.fix)
        .collect();
    assert_eq!(fixes.len(), 2);
    assert!(!ws.can_undo());
    for f in &fixes {
        assert!(problems::apply(&mut ws, f));
    }
    assert!(ws.excluded.contains("X1"));
    assert_eq!(ws.feeders[2].base.count, 1);
    let again = ws.make_plan().unwrap();
    assert!(!has_errors(&again.checks));
    // a fix that was applied has nothing left to do
    assert!(!problems::apply(&mut ws, &fixes[0]));
    assert!(ws.undo());
    assert!(ws.undo());
    assert!(!ws.undo());
    assert!(ws.excluded.is_empty());
    assert_eq!(ws.feeders[2].base.count, 0);
    // a fix for something that does not exist changes nothing
    assert!(!problems::apply(&mut ws, &Fix::DropPart("Z9".to_string())));
    assert!(!problems::apply(
        &mut ws,
        &Fix::RaiseFeeder {
            id: "F9".to_string(),
            missing: 1
        }
    ));
}

#[test]
fn an_error_that_stops_the_plan_is_a_problem_with_its_code_in_the_language() {
    let e = PnpError::new(Code::PanelBad, "the root panel is missing");
    let p = problems::from_error(&e);
    assert_eq!(p.severity, Severity::Error);
    assert_eq!(p.code, Some(Code::PanelBad));
    let line = p.line(Lang::Ja);
    assert!(line.contains("PHOENIX-PNP-014"), "{line}");
    assert!(line.contains(Code::PanelBad.message(Lang::Ja)), "{line}");
}

// ---- the simulated job

#[test]
fn a_job_runs_to_its_end_by_the_clock_of_the_window() {
    let (_ws, mut s) = started("gui_steps_per_s = 1000\n");
    assert!(s.has_job());
    assert!(!s.is_running());
    assert!(near(s.rate(), 1000.0));
    assert!(matches!(s.next(), Next::Pick(_)));
    s.play();
    assert!(s.is_running());
    let mut steps = 0;
    for _ in 0..100 {
        steps += s.tick(0.5);
        if !s.is_running() {
            break;
        }
    }
    assert_eq!(s.phase(), Some(Phase::Finished));
    assert_eq!(s.progress(), (6, 6));
    assert!(steps >= 12, "{steps} steps for 6 parts");
    assert!(s.machine_seconds() > 0.0);
    assert!(s.head().is_some());
    assert_eq!(s.next(), Next::Done);
    assert!(s.last_error().is_none());
    // a job that is finished does not run again
    s.play();
    assert!(!s.is_running());
    s.reset();
    assert!(!s.has_job());
    assert_eq!(s.progress(), (0, 0));
    assert_eq!(s.tick(1.0), 0);
}

#[test]
fn a_job_can_be_paused_stepped_and_resumed() {
    let (_ws, mut s) = started("");
    s.set_rate(2.0);
    s.play();
    assert_eq!(s.tick(0.5), 1);
    s.pause();
    assert!(!s.is_running());
    assert_eq!(s.phase(), Some(Phase::Paused));
    assert_eq!(s.tick(0.5), 0);
    let before = s.next();
    assert!(s.step_once().is_none());
    assert_ne!(before, s.next());
    assert_eq!(s.phase(), Some(Phase::Paused));
    s.play();
    assert!(s.is_running());
    let mut guard = 0;
    while s.is_running() && guard < 1000 {
        s.tick(0.5);
        guard += 1;
    }
    assert_eq!(s.phase(), Some(Phase::Finished));
    assert_eq!(s.progress().0, 6);
}

#[test]
fn a_step_that_fails_stops_the_job_and_resume_tries_it_again() {
    let (_ws, mut s) = started("sim_pick_fail_rate = 1\nsim_seed = 3\ngui_steps_per_s = 1000\n");
    s.play();
    for _ in 0..50 {
        s.tick(0.5);
        if !s.is_running() {
            break;
        }
    }
    assert!(!s.is_running());
    assert_eq!(s.phase(), Some(Phase::Stopped));
    let first = s.last_error().cloned().expect("the error is kept");
    assert_eq!(first.code, Code::PickFailed);
    s.play();
    assert!(s.is_running());
    assert!(s.last_error().is_none());
    for _ in 0..50 {
        s.tick(0.5);
        if !s.is_running() {
            break;
        }
    }
    assert_eq!(s.phase(), Some(Phase::Stopped));
    assert!(s.last_error().is_some());
    // a single step after the error tries the step again and gives the error
    assert!(s.step_once().is_some());
}

#[test]
fn the_events_of_the_machine_come_once() {
    let (_ws, mut s) = started("gui_steps_per_s = 1000\n");
    s.play();
    s.tick(0.5);
    let first = s.take_events();
    assert!(!first.is_empty());
    assert!(first[0].contains(" s "));
    assert!(s.take_events().is_empty());
}

#[test]
fn the_clock_cannot_be_made_to_flood_the_job() {
    let (_ws, mut s) = started("");
    s.set_rate(f64::NAN);
    assert!(near(s.rate(), 4.0));
    s.set_rate(1e9);
    assert!(near(s.rate(), 1000.0));
    s.set_rate(4.0);
    s.play();
    // an hour that went by in one frame counts for half a second
    assert!(s.tick(3600.0) <= 2);
    assert_eq!(s.tick(f64::NAN), 0);
    assert_eq!(s.tick(-5.0), 0);
}

// ---- the view

#[test]
fn bounds_hold_points() {
    let mut b = Bounds::default();
    assert!(b.is_empty());
    assert!(near(b.width(), 0.0));
    b.add((10.0, 5.0));
    b.add((-10.0, 25.0));
    assert!(!b.is_empty());
    assert!(near(b.width(), 20.0) && near(b.height(), 20.0));
    assert_eq!(b.centre(), (0.0, 15.0));
}

#[test]
fn the_drawing_is_fitted_and_the_y_axis_goes_up() {
    let mut b = Bounds::default();
    b.add((0.0, 0.0));
    b.add((100.0, 50.0));
    let t = Transform::new(&b, (600.0, 400.0), 20.0, &Viewport::default());
    assert!(near(t.scale(), 5.6));
    let (cx, cy) = t.to_screen((50.0, 25.0));
    assert!(near(cx, 0.0) && near(cy, 0.0));
    assert!(t.to_screen((50.0, 50.0)).1 < 0.0);
    assert!(t.to_screen((100.0, 25.0)).0 > 0.0);
    for p in [(0.0, 0.0), (100.0, 50.0), (37.5, 12.25)] {
        let back = t.to_world(t.to_screen(p));
        assert!(near(back.0, p.0) && near(back.1, p.1));
    }
}

#[test]
fn the_wheel_zooms_about_the_pointer_and_the_drag_moves() {
    let mut b = Bounds::default();
    b.add((0.0, 0.0));
    b.add((100.0, 50.0));
    let mut v = Viewport::default();
    assert!(v.is_default());
    let at = (120.0, -40.0);
    let before = Transform::new(&b, (600.0, 400.0), 20.0, &v).to_world(at);
    v.wheel(300.0, at);
    assert!(v.zoom > 1.0 && !v.is_default());
    let after = Transform::new(&b, (600.0, 400.0), 20.0, &v).to_world(at);
    assert!(near(before.0, after.0) && near(before.1, after.1));
    v.drag(10.0, -5.0);
    assert!(v.pan.0 != 0.0);
    v.reset();
    assert!(v.is_default());
    // limits, and numbers that are not numbers
    v.wheel(1e6, (0.0, 0.0));
    assert!(v.zoom <= 400.0);
    v.wheel(-1e6, (0.0, 0.0));
    assert!(v.zoom >= 0.1);
    let kept = v;
    v.wheel(f64::NAN, (0.0, 0.0));
    v.drag(f64::INFINITY, 0.0);
    assert_eq!(v, kept);
}

#[test]
fn the_size_of_a_body_comes_from_its_package() {
    assert_eq!(package_size("R_0603", 2.0), (1.6, 0.8));
    assert_eq!(package_size("c_1206", 2.0), (3.2, 1.6));
    assert_eq!(package_size("QFN-32", 2.0), (5.5, 5.5));
    assert_eq!(package_size("SOIC-8", 2.0), (5.0, 4.0));
    assert_eq!(package_size("SOT-23-5", 2.0), (2.9, 1.3));
    assert_eq!(package_size("weird", 2.5), (2.5, 2.5));
    assert_eq!(package_size("", 2.5), (2.5, 2.5));
}

#[test]
fn a_body_turns_with_its_part_and_the_mark_stays_on_the_body() {
    let flat = outline((10.0, 10.0), (2.0, 1.0), 0.0);
    let xs: Vec<f64> = flat.iter().map(|p| p.0).collect();
    let ys: Vec<f64> = flat.iter().map(|p| p.1).collect();
    assert!(near(xs.iter().copied().fold(f64::MIN, f64::max), 11.0));
    assert!(near(ys.iter().copied().fold(f64::MIN, f64::max), 10.5));
    let turned = outline((10.0, 10.0), (2.0, 1.0), 90.0);
    let xs: Vec<f64> = turned.iter().map(|p| p.0).collect();
    let ys: Vec<f64> = turned.iter().map(|p| p.1).collect();
    assert!(near(xs.iter().copied().fold(f64::MIN, f64::max), 10.5));
    assert!(near(ys.iter().copied().fold(f64::MIN, f64::max), 11.0));
    for angle in [0.0, 90.0, 180.0, 270.0] {
        let body = outline((0.0, 0.0), (4.0, 2.0), angle);
        let (lo_x, hi_x) = body
            .iter()
            .fold((f64::MAX, f64::MIN), |a, p| (a.0.min(p.0), a.1.max(p.0)));
        let (lo_y, hi_y) = body
            .iter()
            .fold((f64::MAX, f64::MIN), |a, p| (a.0.min(p.1), a.1.max(p.1)));
        for m in polarity_mark((0.0, 0.0), (4.0, 2.0), angle) {
            assert!((lo_x - 1e-9..=hi_x + 1e-9).contains(&m.0));
            assert!((lo_y - 1e-9..=hi_y + 1e-9).contains(&m.1));
        }
    }
    // the mark sits at the same end of the body whatever the turn: opposite ends for opposite turns
    let a = polarity_mark((0.0, 0.0), (4.0, 2.0), 0.0)[0];
    let b = polarity_mark((0.0, 0.0), (4.0, 2.0), 180.0)[0];
    assert!(near(a.0, -b.0) && near(a.1, -b.1));
}

#[test]
fn the_scene_holds_the_parts_the_feeders_and_the_path() {
    let ws = loaded();
    let planned = ws.make_plan().unwrap();
    let scene = Scene::build(&planned, &ws);
    assert_eq!(scene.shapes.len(), 6);
    assert_eq!(scene.feeders.len(), 3);
    // a pick and a place for every part
    assert_eq!(scene.path.len(), 12);
    assert!(scene.bounds.min.0 <= -20.0 && scene.bounds.max.0 >= 40.0);
    let first = &scene.shapes[0];
    assert_eq!(scene.hit(first.at, 0.1), Some(first.index));
    assert_eq!(scene.hit((500.0, 500.0), 1.0), None);
    assert_eq!(Scene::default().hit((0.0, 0.0), 1.0), None);
}

// ---- the tables

#[test]
fn names_are_ordered_so_that_r2_comes_before_r10() {
    assert_eq!(natural_cmp("R2", "R10"), Ordering::Less);
    assert_eq!(natural_cmp("R10", "R2"), Ordering::Greater);
    assert_eq!(natural_cmp("r1", "R1"), Ordering::Equal);
    assert_eq!(natural_cmp("C1", "R1"), Ordering::Less);
    assert_eq!(natural_cmp("R01", "R1"), Ordering::Equal);
    assert_eq!(natural_cmp("", "R1"), Ordering::Less);
    assert_eq!(natural_cmp("R1", "R1a"), Ordering::Less);
    assert_eq!(natural_cmp("A/R10", "A/R9"), Ordering::Greater);
}

#[test]
fn a_click_on_a_head_sorts_and_a_second_one_turns_it_round() {
    let mut s = SortState::new(PartColumn::Reference);
    assert!(s.ascending);
    s.click(PartColumn::Reference);
    assert!(!s.ascending);
    assert_eq!(s.mark(PartColumn::Reference), " v");
    assert_eq!(s.mark(PartColumn::X), "");
    s.click(PartColumn::X);
    assert!(s.ascending);
    assert_eq!(s.mark(PartColumn::X), " ^");
}

#[test]
fn the_table_of_parts_shows_the_feeder_and_sorts() {
    let ws = loaded();
    let planned = ws.make_plan().unwrap();
    let mut rows = part_rows(&ws, Some(&planned), None);
    assert_eq!(rows.len(), 6);
    assert_eq!(
        rows.iter().find(|r| r.reference == "R1").unwrap().feeder,
        "F1"
    );
    assert!(rows.iter().all(|r| r.status.is_none() && !r.excluded));
    let mut by = SortState::new(PartColumn::X);
    by.click(PartColumn::X);
    sort_parts(&mut rows, by);
    assert_eq!(rows[0].reference, "U1");
    sort_parts(&mut rows, SortState::new(PartColumn::Reference));
    assert_eq!(rows[0].reference, "C1");
    // with no plan, no feeder is named
    let bare = part_rows(&ws, None, None);
    assert!(bare.iter().all(|r| r.feeder.is_empty()));
}

#[test]
fn the_rows_of_the_plan_follow_the_order_of_the_head() {
    let ws = loaded();
    let planned = ws.make_plan().unwrap();
    let rows = plan_rows(&planned, None);
    assert_eq!(rows.len(), 6);
    for (i, r) in rows.iter().enumerate() {
        assert_eq!(r.step, i + 1);
        assert!(r.nozzle >= 1 && r.batch >= 1);
        assert!(!r.tip_change);
    }
    assert!(!rows[0].value.is_empty());
}

#[test]
fn the_feeders_show_what_is_left_after_the_job() {
    let (ws, mut s) = started("gui_steps_per_s = 1000\n");
    let before = feeder_rows(&ws, s.job());
    assert!(before.iter().all(|r| r.left == r.count));
    s.play();
    for _ in 0..100 {
        s.tick(0.5);
        if !s.is_running() {
            break;
        }
    }
    let after = feeder_rows(&ws, s.job());
    let left = |id: &str| after.iter().find(|r| r.id == id).unwrap().left;
    assert_eq!(left("F1"), 7);
    assert_eq!(left("F2"), 8);
    assert_eq!(left("F3"), 1);
}

#[test]
fn the_kinds_of_part_are_counted() {
    let ws = loaded();
    let kinds = kind_rows(&ws);
    assert_eq!(kinds.len(), 3);
    assert_eq!(kinds[0].value, "10k");
    assert_eq!(kinds[0].count, 3);
    assert!(kinds.iter().all(|k| k.height_mm > 0.0 && k.tape_mm > 0.0));
}

#[test]
fn a_part_chosen_in_one_table_is_chosen_in_all_with_its_feeder() {
    let ws = loaded();
    let planned = ws.make_plan().unwrap();
    let mut sel = Selection::default();
    sel.select_part("C2", Some(&planned));
    assert!(sel.is_part("C2") && !sel.is_part("C1"));
    assert!(sel.is_feeder("F2"));
    sel.select_part("C2", None);
    assert!(sel.feeder.is_none());
    sel.select_feeder("F1");
    assert!(sel.is_feeder("F1") && sel.reference.is_none());
    sel.clear();
    assert_eq!(sel, Selection::default());
}

#[test]
fn a_count_is_edited_with_a_second_click() {
    let mut sel = Selection::default();
    let mut edit = CellEdit::default();
    assert!(!edit.click_count(&mut sel, "F1", 10));
    assert!(sel.is_feeder("F1") && edit.feeder.is_none());
    assert!(edit.click_count(&mut sel, "F1", 10));
    assert_eq!(edit.text, "10");
    edit.text = " 12 ".to_string();
    assert_eq!(edit.finish(), Some(("F1".to_string(), 12)));
    assert!(edit.feeder.is_none());
    assert!(edit.click_count(&mut sel, "F1", 12));
    edit.text = "twelve".to_string();
    assert_eq!(edit.finish(), None);
    assert!(edit.click_count(&mut sel, "F1", 12));
    edit.cancel();
    assert!(edit.feeder.is_none() && edit.text.is_empty());
}

// ---- the guide, the settings, the log

#[test]
fn the_guide_goes_forward_only_when_the_step_is_done() {
    let mut ws = Workspace::new(Lang::En);
    let mut w = Wizard::default();
    assert_eq!(w.step, Step::Parts);
    assert!(!w.next(&ws, PlanState::NoPlan));
    ws.load_text("parts.csv", PARTS).unwrap();
    assert!(w.next(&ws, PlanState::NoPlan));
    assert_eq!(w.step, Step::Feeders);
    assert!(!w.next(&ws, PlanState::NoPlan));
    ws.load_text("feeders.csv", FEEDERS).unwrap();
    assert!(w.next(&ws, PlanState::NoPlan));
    assert_eq!(w.step, Step::Machine);
    assert!(w.next(&ws, PlanState::NoPlan));
    assert_eq!(w.step, Step::Check);
    assert!(!w.next(&ws, PlanState::WithErrors));
    assert!(w.next(&ws, PlanState::Clean));
    assert_eq!(w.step, Step::Simulate);
    assert!(!w.next(&ws, PlanState::Clean));
    assert!(w.back());
    assert_eq!(w.step, Step::Check);
    assert_eq!(Step::Parts.prev(), None);
    assert_eq!(Step::Simulate.next(), None);
    assert_eq!(Step::Check.index(), 3);
}

#[test]
fn the_settings_are_written_and_read_back() {
    let s = UiSettings {
        lang: Lang::Pt,
        theme: ThemeId::Light,
        layers: Layers {
            path: false,
            ..Layers::default()
        },
        out_dir: "out files".to_string(),
        guide_seen: true,
    };
    assert_eq!(UiSettings::parse(&s.to_text()), s);
}

#[test]
fn a_settings_file_that_is_wrong_never_keeps_the_window_from_opening() {
    let d = UiSettings::default();
    assert_eq!(UiSettings::parse(""), d);
    assert_eq!(
        UiSettings::parse(
            "\u{0}\u{1} garbage = = =\nlanguage = xx\ntheme = neon\nlayer_path = maybe\n"
        ),
        d
    );
    assert_eq!(
        UiSettings::parse("language = de\nunknown = 1\n").lang,
        Lang::De
    );
    // a folder with a control character is not taken
    assert_eq!(UiSettings::parse("out_dir = a\u{7}b\n").out_dir, "");
}

#[test]
fn the_settings_live_in_the_folder_of_the_user() {
    let win = settings_path(Some("C:\\Users\\a\\AppData"), None, None).unwrap();
    assert!(win.to_string_lossy().contains("EH3D-PHOENIX-PNP"));
    assert!(win.ends_with("window.cfg"));
    let xdg = settings_path(None, Some("/x"), Some("/h")).unwrap();
    assert!(xdg.to_string_lossy().contains("eh3d-phoenix-pnp"));
    let home = settings_path(None, None, Some("/h")).unwrap();
    assert!(home.to_string_lossy().contains(".config"));
    assert!(settings_path(Some(""), Some(""), Some("")).is_none());
    assert!(settings_path(None, None, None).is_none());
}

#[test]
fn the_log_keeps_the_last_lines_and_says_them_as_text() {
    let mut log = Log::new();
    assert!(log.is_empty());
    log.info("one");
    log.warn("two");
    log.error("three");
    assert_eq!(log.text(), "[info] one\n[warn] two\n[error] three\n");
    assert_eq!(log.entries()[2].level, Level::Error);
    for i in 0..MAX_LINES + 10 {
        log.info(format!("line {i}"));
    }
    assert_eq!(log.len(), MAX_LINES);
    assert_eq!(
        log.entries().last().unwrap().text,
        format!("line {}", MAX_LINES + 9)
    );
    log.clear();
    assert!(log.is_empty());
}

// ---- the whole path

#[test]
fn the_state_goes_from_files_to_a_finished_job_without_a_window() {
    let mut a = AppState::new(UiSettings::default());
    assert_eq!(a.plan_state(), PlanState::NoPlan);
    assert!(!a.can_start());
    assert_eq!(a.load_sample(&samples::mini()), 2);
    assert!(a.planned.is_some() && a.scene.is_some());
    assert_eq!(a.plan_state(), PlanState::Clean);
    assert!(a.can_start());
    let m = a.summary().unwrap();
    assert_eq!(m.parts, 6);
    assert!(m.travel_mm > 0.0 && m.seconds > 0.0);
    a.ws.cfg.gui_steps_per_s = 1000.0;
    a.start();
    assert!(a.session.is_running());
    for _ in 0..50 {
        a.tick(0.5);
    }
    assert_eq!(a.session.phase(), Some(Phase::Finished));
    assert_eq!(a.session.progress(), (6, 6));
    assert!(a
        .log
        .entries()
        .iter()
        .any(|e| e.text.starts_with("Finished")));
    a.reset_job();
    assert!(!a.session.has_job());
}

#[test]
fn a_job_can_be_stepped_from_the_state_before_it_runs() {
    let mut a = AppState::new(UiSettings::default());
    a.load_sample(&samples::mini());
    a.step();
    assert!(a.session.has_job());
    assert_eq!(a.session.phase(), Some(Phase::Paused));
    assert!(!a.session.is_running());
    a.start();
    assert!(a.session.is_running());
    a.pause();
    assert_eq!(a.session.phase(), Some(Phase::Paused));
}

#[test]
fn a_job_with_errors_does_not_start_and_the_fixes_make_it_start() {
    let mut a = AppState::new(UiSettings::default());
    let parts = format!("{PARTS}X1,odd,Z,1,1,0,top\n");
    let feeders = FEEDERS.replace("F3,MCU,Q,-20,40,2", "F3,MCU,Q,-20,40,0");
    assert_eq!(a.load_sample(&sample_pair(&parts, &feeders)), 2);
    assert!(a.has_errors() && !a.can_start());
    assert_eq!(a.plan_state(), PlanState::WithErrors);
    a.start();
    assert!(!a.session.has_job());
    assert!(a
        .log
        .entries()
        .iter()
        .any(|e| e.text.contains(&Code::PreflightFailed.label())));
    for _ in 0..10 {
        match a.problems.iter().position(|p| p.fix.is_some()) {
            Some(i) => assert!(a.apply_fix(i)),
            None => break,
        }
    }
    assert!(!a.has_errors() && a.can_start());
    assert!(a.undo());
    assert!(a.undo());
    assert!(a.has_errors());
    assert!(!a.undo());
    assert!(!a.apply_fix(999));
}

#[test]
fn the_count_of_a_feeder_is_edited_and_the_plan_is_made_again() {
    let mut a = AppState::new(UiSettings::default());
    a.load_sample(&samples::mini());
    assert!(!a.has_errors());
    assert!(a.set_feeder_count("F1", 2));
    assert!(a.has_errors());
    assert!(!a.set_feeder_count("F1", 2));
    assert!(!a.set_feeder_count("F9", 2));
    assert!(a.undo());
    assert!(!a.has_errors());
}

#[test]
fn the_machine_is_set_and_the_plan_is_made_again_within_the_limits() {
    let mut a = AppState::new(UiSettings::default());
    a.load_sample(&samples::mini());
    a.set_machine(2, 300.0);
    assert_eq!(a.ws.cfg.nozzles, 2);
    assert!(near(a.ws.cfg.x_speed_mm_s, 300.0));
    assert!(a.planned.is_some());
    a.set_machine(99, 1e9);
    assert_eq!(a.ws.cfg.nozzles, 6);
    assert!(near(a.ws.cfg.y_speed_mm_s, 5000.0));
    a.set_machine(0, f64::NAN);
    assert_eq!(a.ws.cfg.nozzles, 1);
    assert!(near(a.ws.cfg.x_speed_mm_s, 5000.0));
}

#[test]
fn the_samples_go_from_small_to_large_and_all_plan() {
    for (name, files, parts) in [
        ("mini", samples::mini(), 6),
        ("examples", samples::examples(), 6),
        ("large", samples::large(400), 400),
    ] {
        let mut a = AppState::new(UiSettings::default());
        assert_eq!(a.load_sample(&files), files.len(), "{name}");
        assert!(a.planned.is_some(), "{name} has no plan");
        assert!(!a.has_errors(), "{name} has errors");
        assert_eq!(a.summary().unwrap().parts, parts, "{name}");
    }
    assert_eq!(samples::large(0)[0].1.lines().count(), 2);
    assert_eq!(samples::large(1_000_000)[0].1.lines().count(), 5001);
}

#[test]
fn a_large_board_is_run_to_its_end() {
    let mut a = AppState::new(UiSettings::default());
    a.load_sample(&samples::large(400));
    a.set_machine(2, 400.0);
    a.ws.cfg.gui_steps_per_s = 1000.0;
    a.start();
    for _ in 0..20 {
        a.tick(0.5);
    }
    assert_eq!(a.session.phase(), Some(Phase::Finished));
    assert_eq!(a.session.progress(), (400, 400));
}

#[test]
fn the_files_are_loaded_by_path_with_the_configuration_first() {
    let dir = temp_dir("load");
    let write = |name: &str, text: &str| {
        let p = dir.join(name);
        std::fs::write(&p, text).unwrap();
        p
    };
    let feeders = write("feeders.csv", FEEDERS);
    let parts = write("parts.csv", PARTS);
    let cfg = write(
        "machine.cfg",
        "language = es\ngui_theme = light\nrotation = R : 90\n",
    );
    let mut a = AppState::new(UiSettings::default());
    assert_eq!(a.load_paths(&[feeders, parts, cfg]), 3);
    assert_eq!(a.lang(), Lang::Es);
    assert_eq!(a.ui.theme, ThemeId::Light);
    assert!(a.planned.is_some());
    // the configuration was read before the parts, so they were turned once
    assert!(near(a.ws.parts[0].rotation, 90.0));
    assert!(a.log.entries().iter().all(|e| e.level != Level::Error));
    // a file that is not there is told, without its folder
    let missing = dir.join("nope.csv");
    assert_eq!(a.load_paths(&[missing]), 0);
    let last = a.log.entries().last().unwrap();
    assert_eq!(last.level, Level::Error);
    let folder = dir.to_string_lossy().into_owned();
    assert!(!last.text.contains(&folder), "{}", last.text);
    // the diagnostics name the files but not where they are
    let text = diagnostics(&a.ws, &a.log);
    assert!(text.contains("file parts: parts.csv"), "{text}");
    assert!(text.contains(env!("CARGO_PKG_VERSION")));
    assert!(!text.contains(&folder), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_plan_the_report_and_the_load_sheet_are_written_to_the_folder() {
    let dir = temp_dir("save");
    let mut a = AppState::new(UiSettings::default());
    assert!(a.save_outputs().is_err());
    a.load_sample(&samples::mini());
    a.ui.out_dir = dir.to_string_lossy().into_owned();
    a.save_outputs().unwrap();
    let plan = std::fs::read_to_string(dir.join("plan.json")).unwrap();
    assert!(serde_json::from_str::<serde_json::Value>(&plan).is_ok());
    assert!(dir.join("report.txt").exists());
    assert!(!dir.join("load_sheet.csv").exists());
    a.load_sample(&samples::examples());
    a.save_outputs().unwrap();
    assert!(dir.join("load_sheet.csv").exists());
    assert!(dir.join("load_sheet_feeders.csv").exists());
    assert!(!dir.join("plan.json.tmp").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_tabs_have_a_word_each_and_the_window_starts_on_the_job() {
    let a = AppState::new(UiSettings::default());
    assert_eq!(a.tab, Tab::Job);
    assert!(a.wizard.open);
    let seen = UiSettings {
        guide_seen: true,
        ..UiSettings::default()
    };
    assert!(!AppState::new(seen).wizard.open);
    let words: Vec<&str> = Tab::ALL.iter().map(|t| tr(t.text(), Lang::En)).collect();
    assert_eq!(words.len(), 8);
    let mut sorted = words.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), 8);
}

#[test]
fn the_icon_of_the_window_is_in_the_repository() {
    let icon = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("images/brand/icon-256.png");
    let bytes = std::fs::read(icon).unwrap();
    assert_eq!(&bytes[1..4], b"PNG");
}
