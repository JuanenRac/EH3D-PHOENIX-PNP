//! The Tools tab without the window: the settings that start its forms, each tool on the files of the examples, what it applies to the machine and what it writes, and that it never panics.

use eh3d_phoenix_pnp::config::{MachineConfig, KEYS};
use eh3d_phoenix_pnp::error::Lang;
use eh3d_phoenix_pnp::gui::app::{AppState, Tab};
use eh3d_phoenix_pnp::gui::settings::UiSettings;
use eh3d_phoenix_pnp::gui::toolbox::{
    apply_camera, apply_runout, backlash_text, bom_text, camera_text, panel_text, polarity_text,
    runout_text, stock_text, teach_text, PanelForm, Tool, Toolbox, MAX_TOOL_BYTES,
};
use eh3d_phoenix_pnp::gui::workspace::Workspace;
use eh3d_phoenix_pnp::motion::BacklashMode;
use eh3d_phoenix_pnp::panel::Panel;
use eh3d_phoenix_pnp::parts::{from_csv, Side};
use std::path::PathBuf;

fn example(name: &str) -> String {
    std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("examples")
            .join(name),
    )
    .unwrap()
}

fn near(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

fn board() -> Workspace {
    let mut ws = Workspace::new(Lang::En);
    ws.load_text("board.pos", &example("board.pos")).unwrap();
    ws
}

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("phoenix_pnp_toolbox_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

// ---- the settings that start the forms

#[test]
fn the_settings_of_the_forms_have_defaults_ranges_and_a_line_in_the_example() {
    let d = MachineConfig::default();
    assert_eq!(d.order_boards, 1);
    assert_eq!((d.panel_cols, d.panel_rows), (2, 2));
    assert_eq!((d.panel_pitch_x_mm, d.panel_pitch_y_mm), (50.0, 50.0));
    assert_eq!(
        (
            d.panel_fiducial_mm,
            d.panel_board_width_mm,
            d.panel_board_height_mm
        ),
        (0.0, 0.0, 0.0)
    );
    // the example holds the defaults, and every key is in the list of keys
    let e = MachineConfig::parse(&example("machine.cfg")).unwrap();
    assert_eq!(e.order_boards, d.order_boards);
    assert_eq!((e.panel_cols, e.panel_rows), (d.panel_cols, d.panel_rows));
    assert_eq!(
        (e.panel_pitch_x_mm, e.panel_pitch_y_mm),
        (d.panel_pitch_x_mm, d.panel_pitch_y_mm)
    );
    assert_eq!(
        (
            e.panel_fiducial_mm,
            e.panel_board_width_mm,
            e.panel_board_height_mm
        ),
        (
            d.panel_fiducial_mm,
            d.panel_board_width_mm,
            d.panel_board_height_mm
        )
    );
    for key in [
        "order_boards",
        "panel_cols",
        "panel_rows",
        "panel_pitch_x_mm",
        "panel_pitch_y_mm",
        "panel_fiducial_mm",
        "panel_board_width_mm",
        "panel_board_height_mm",
    ] {
        assert!(KEYS.contains(&key), "{key} is not in KEYS");
        assert!(
            example("machine.cfg").contains(&format!("\n{key} = ")),
            "{key}"
        );
    }
    let ranges: [(&str, [&str; 2], [&str; 2]); 8] = [
        ("order_boards", ["1", "100000"], ["0", "100001"]),
        ("panel_cols", ["1", "100"], ["0", "101"]),
        ("panel_rows", ["1", "100"], ["0", "101"]),
        ("panel_pitch_x_mm", ["0.001", "10000"], ["0", "10001"]),
        ("panel_pitch_y_mm", ["0.001", "10000"], ["0", "10001"]),
        ("panel_fiducial_mm", ["0", "1000"], ["-1", "1001"]),
        ("panel_board_width_mm", ["0", "10000"], ["-1", "10001"]),
        ("panel_board_height_mm", ["0", "10000"], ["-1", "10001"]),
    ];
    for (key, good, bad) in ranges {
        for v in good {
            MachineConfig::parse(&format!("{key} = {v}\n"))
                .unwrap_or_else(|e| panic!("{key} {v}: {e:?}"));
        }
        for v in bad {
            assert!(
                MachineConfig::parse(&format!("{key} = {v}\n")).is_err(),
                "{key} {v}"
            );
        }
        assert!(
            MachineConfig::parse(&format!("{key} = x\n")).is_err(),
            "{key}"
        );
    }
    let c = MachineConfig::parse("order_boards = 7\npanel_cols = 3\npanel_pitch_x_mm = 61.5\n")
        .unwrap();
    let t = Toolbox::new(&c);
    assert_eq!((t.boards, t.panel.cols, t.panel.pitch_x_mm), (7, 3, 61.5));
    assert_eq!(t.tool, Tool::Backlash);
}

#[test]
fn a_configuration_that_is_loaded_gives_the_forms_their_values_and_keeps_what_was_typed() {
    let mut a = AppState::new(UiSettings::default());
    a.tools.panel.board_file = "mine.pos".to_string();
    a.tools.panel.x_out = "P2".to_string();
    a.ws.load_text("machine.cfg", "order_boards = 5\npanel_rows = 4\n")
        .unwrap();
    a.tools.take_defaults(&a.ws.cfg);
    assert_eq!((a.tools.boards, a.tools.panel.rows), (5, 4));
    assert_eq!(
        (
            a.tools.panel.board_file.as_str(),
            a.tools.panel.x_out.as_str()
        ),
        ("mine.pos", "P2")
    );
}

#[test]
fn the_tools_have_keys_and_the_tab_is_one_of_the_tabs() {
    assert_eq!(Tool::ALL.len(), 9);
    for t in Tool::ALL {
        assert_eq!(Tool::from_key(t.key()), Some(t));
        assert_eq!(
            Tool::from_key(&format!(" {} ", t.key().to_uppercase())),
            Some(t)
        );
    }
    assert_eq!(Tool::from_key("hammer"), None);
    assert!(Tool::Camera.reads_text() && !Tool::Panel.reads_text());
    assert_eq!(Tool::Panel.output_name(), Some("panel.json"));
    assert_eq!(Tool::Backlash.output_name(), None);
    assert_eq!(Tab::from_key("tools"), Some(Tab::Tools));
    assert_eq!(Tab::Tools.key(), "tools");
}

// ---- the settings tools

#[test]
fn the_slack_and_the_runout_are_set_in_their_ranges_and_the_plan_is_made_again() {
    let mut a = AppState::new(UiSettings::default());
    a.set_backlash(0.1, 0.25, BacklashMode::Approach);
    assert_eq!(
        (a.ws.cfg.backlash_x_mm, a.ws.cfg.backlash_y_mm),
        (0.1, 0.25)
    );
    assert_eq!(a.ws.cfg.backlash_mode, BacklashMode::Approach);
    // out of range is kept in range and a number that is not one leaves the value
    a.set_backlash(9.0, f64::NAN, BacklashMode::Off);
    assert_eq!(
        (a.ws.cfg.backlash_x_mm, a.ws.cfg.backlash_y_mm),
        (2.0, 0.25)
    );
    a.set_backlash(-3.0, f64::INFINITY, BacklashMode::Off);
    assert_eq!(
        (a.ws.cfg.backlash_x_mm, a.ws.cfg.backlash_y_mm),
        (0.0, 0.25)
    );
    a.set_runout(0.2, 45.0);
    assert_eq!(
        (a.ws.cfg.nozzle_runout_mm, a.ws.cfg.nozzle_runout_deg),
        (0.2, 45.0)
    );
    a.set_runout(50.0, -500.0);
    assert_eq!(
        (a.ws.cfg.nozzle_runout_mm, a.ws.cfg.nozzle_runout_deg),
        (5.0, -180.0)
    );
    a.set_runout(f64::NAN, f64::NAN);
    assert_eq!(
        (a.ws.cfg.nozzle_runout_mm, a.ws.cfg.nozzle_runout_deg),
        (5.0, -180.0)
    );
    a.set_spare(12.5, 3, 40);
    assert_eq!(
        (
            a.ws.cfg.order_spare_percent,
            a.ws.cfg.order_spare_min,
            a.tools.boards
        ),
        (12.5, 3, 40)
    );
    a.set_spare(500.0, 5000, 0);
    assert_eq!(
        (
            a.ws.cfg.order_spare_percent,
            a.ws.cfg.order_spare_min,
            a.tools.boards
        ),
        (100.0, 1000, 1)
    );
    a.set_spare(f64::NAN, 0, 1_000_000);
    assert_eq!(
        (a.ws.cfg.order_spare_percent, a.tools.boards),
        (100.0, 100_000)
    );
    // the lines the tool shows are lines the configuration reads back
    let text = backlash_text(&a.ws.cfg);
    let back = MachineConfig::parse(&text).unwrap();
    assert_eq!(back.nozzle_runout_mm, 5.0);
    assert_eq!(back.backlash_y_mm, 0.25);
}

// ---- the calibrations

#[test]
fn the_runout_tool_fits_the_example_applies_it_and_refuses_what_is_wrong() {
    let mut tb = Toolbox::new(&MachineConfig::default());
    tb.tool = Tool::Runout;
    tb.runout_text = example("nozzle_readings.csv");
    let ws = board();
    tb.run(&ws, Lang::En);
    assert!(tb.error.is_empty(), "{}", tb.error);
    assert!(
        tb.info.starts_with("8 readings: the axis is at X 150.0"),
        "{}",
        tb.info
    );
    assert!(
        tb.output.contains("nozzle_runout_mm = 0.1"),
        "{}",
        tb.output
    );
    assert_eq!(tb.made_by, Some(Tool::Runout));
    let mut cfg = MachineConfig::default();
    assert!(tb.apply(&mut cfg, Lang::En));
    assert!(near(cfg.nozzle_runout_mm, 0.12, 5e-3) && near(cfg.nozzle_runout_deg, 30.0, 3.0));
    // wrong text: an error in the language asked, and nothing is applied
    tb.runout_text = "1,2\n".to_string();
    tb.run(&ws, Lang::Es);
    assert!(tb.error.contains("PHOENIX-PNP-058"), "{}", tb.error);
    assert!(tb.output.is_empty() && tb.info.is_empty() && tb.made_by.is_none());
    let mut other = MachineConfig::default();
    assert!(!tb.apply(&mut other, Lang::En));
    assert_eq!(other, MachineConfig::default());
    // a runout above the 5 mm of the setting is refused
    tb.runout_text = "0,100,0\n90,0,100\n180,-100,0\n270,0,-100\n".to_string();
    tb.run(&ws, Lang::En);
    assert!(tb.error.contains("above the 5 mm"), "{}", tb.error);
    let e = runout_text("garbage", Lang::En).unwrap_err();
    assert!(e.contains("PHOENIX-PNP-058"), "{e}");
}

#[test]
fn the_camera_tool_fits_the_example_and_applies_everything_but_the_lens() {
    let (found, info, lines) = camera_text(&example("camera_points.csv"), Lang::En).unwrap();
    assert!(info.starts_with("7 sightings: a pixel is 0.02"), "{info}");
    assert!(lines.contains("camera_units_per_px = 0.02"), "{lines}");
    let mut cfg = MachineConfig::default();
    cfg.camera.lens_scale = 1.1;
    cfg.camera.lens_k1 = 0.001;
    apply_camera(&mut cfg, &found);
    assert!(near(cfg.camera.units_per_px, 0.02, 1e-4));
    assert!(near(cfg.camera.offset_x_mm, -0.15, 5e-3));
    assert!(!cfg.camera.flip_x && !cfg.camera.flip_y);
    assert_eq!((cfg.camera.lens_scale, cfg.camera.lens_k1), (1.1, 0.001));
    // what is applied is a camera the configuration accepts
    MachineConfig::parse(&lines).unwrap();
    let mut r = MachineConfig::default();
    let (run, _, _) = runout_text(&example("nozzle_readings.csv"), Lang::En).unwrap();
    apply_runout(&mut r, &run);
    assert!(r.nozzle_runout_mm > 0.0);
    assert!(camera_text("1,2,3,4\n", Lang::De)
        .unwrap_err()
        .contains("PHOENIX-PNP-058"));
}

#[test]
fn the_window_state_applies_a_calibration_and_says_so() {
    let mut a = AppState::new(UiSettings::default());
    a.tools.tool = Tool::Camera;
    a.tools.camera_text = example("camera_points.csv");
    a.tools_apply();
    assert!(a.tools.error.is_empty(), "{}", a.tools.error);
    assert_eq!(a.tools.notice, "Applied to the machine");
    assert!(near(a.ws.cfg.camera.units_per_px, 0.02, 1e-4));
    // a text that does not fit applies nothing and puts the error in the log
    a.tools.camera_text = "x".to_string();
    let before = a.ws.cfg.clone();
    a.tools_apply();
    assert!(!a.tools.error.is_empty());
    assert_eq!(a.ws.cfg, before);
    // a tool that fits nothing applies nothing
    a.tools.tool = Tool::Panel;
    a.tools.error.clear();
    a.tools_apply();
    assert!(a.tools.error.is_empty());
}

// ---- the sheets and the files

#[test]
fn the_feeders_tool_teaches_the_example() {
    let (info, csv) = teach_text(&example("feeder_measures.csv"), Lang::En).unwrap();
    assert!(info.starts_with("F2: a strip of 40 parts"), "{info}");
    assert!(info.contains("\nF3: a tray of 3 by 2 cells"), "{info}");
    assert!(
        csv.starts_with("id,kind,x,y,count,cols,rows,pitch_x,pitch_y,pitch,rotation\nF2,strip,")
    );
    assert!(teach_text("F1,reel,0,0,1,1,,,3,\n", Lang::En).is_err());
}

#[test]
fn the_polarity_tool_lists_the_parts_that_have_a_direction_by_side() {
    let ws = board();
    let (summary, csv) = polarity_text(&ws, None);
    assert!(summary.contains("have a direction"), "{summary}");
    assert!(csv.starts_with("reference,value,package,side,x_mm,y_mm,rotation_deg,checked\n"));
    let all = csv.lines().count() - 1;
    assert!(all > 0, "{csv}");
    let (_, top) = polarity_text(&ws, Some(Side::Top));
    let (_, bottom) = polarity_text(&ws, Some(Side::Bottom));
    assert_eq!(top.lines().count() + bottom.lines().count() - 2, all);
    // a part that a fix left out is not on the sheet
    let mut ws = board();
    let first = csv
        .lines()
        .nth(1)
        .unwrap()
        .split(',')
        .next()
        .unwrap()
        .to_string();
    ws.excluded.insert(first.clone());
    let (_, less) = polarity_text(&ws, None);
    assert_eq!(less.lines().count() - 1, all - 1);
    assert!(!less.lines().any(|l| l.starts_with(&format!("{first},"))));
}

#[test]
fn the_bom_tool_checks_the_board_and_orders_the_parts_with_the_spare_of_the_configuration() {
    let mut ws = board();
    let (info, csv) = bom_text(&ws, &example("bom.csv"), 1, Lang::En).unwrap();
    assert!(
        info.starts_with("ok: the BOM and the board agree"),
        "{info}"
    );
    assert!(csv.lines().count() > 1, "{csv}");
    // the same for ten boards with a spare of 10 per cent or 2 parts: more parts are ordered
    let one: usize = csv.lines().skip(1).count();
    ws.cfg.order_spare_percent = 10.0;
    ws.cfg.order_spare_min = 2;
    let (_, ten) = bom_text(&ws, &example("bom.csv"), 10, Lang::En).unwrap();
    assert_eq!(ten.lines().skip(1).count(), one);
    assert_ne!(ten, csv);
    // a BOM that disagrees says where
    let wrong = "Designator,Comment,Footprint,Quantity\nR1,22k,R_0603,1\nZ9,1u,C_0603,1\n";
    let (info, _) = bom_text(&ws, wrong, 1, Lang::En).unwrap();
    assert!(info.contains("Z9") && info.contains("R1"), "{info}");
    // without a table of parts there is nothing to compare with, but the order is made
    let (info, csv) =
        bom_text(&Workspace::new(Lang::En), &example("bom.csv"), 2, Lang::En).unwrap();
    assert!(info.contains("no table of parts is loaded"), "{info}");
    assert!(csv.lines().count() > 1);
    // boards out of range and a BOM that is not one are errors
    assert!(bom_text(&ws, &example("bom.csv"), 0, Lang::En).is_err());
    assert!(bom_text(&ws, "", 1, Lang::En).is_err());
    assert!(bom_text(&ws, "\"", 1, Lang::Fr).is_err());
}

#[test]
fn the_panel_tool_writes_a_panel_the_program_reads_with_the_fiducials_around_the_real_size() {
    let ws = board();
    let mut form = PanelForm::from_config(&MachineConfig::default());
    form.cols = 3;
    form.rows = 2;
    form.pitch_x_mm = 60.0;
    form.pitch_y_mm = 50.0;
    form.fiducial_mm = 4.0;
    form.board_width_mm = 55.0;
    form.board_height_mm = 45.0;
    form.x_out = "P2, P5".to_string();
    let text = panel_text(&ws, &form, Lang::En).unwrap();
    // the board is named by the file that was loaded
    let panel = Panel::parse(&text).unwrap();
    assert_eq!(panel.boards()[0].1, "board.pos");
    let loader = |_: &str| {
        from_csv(
            "ref,val,package,x,y,rot,side
R1,10k,R,10,20,0,top
",
        )
    };
    let e = panel.expand(&loader).unwrap();
    assert_eq!(e.x_out, ["P2", "P5"]);
    // 2 * 60 + 55 = 175 wide and 50 + 45 = 95 high
    assert_eq!(e.fiducials[1], ("f2".to_string(), 179.0, -4.0));
    assert_eq!(e.fiducials[2], ("f3".to_string(), -4.0, 99.0));
    // a name typed in the form wins over the file that was loaded
    form.board_file = "  other.pos ".to_string();
    assert_eq!(
        Panel::parse(&panel_text(&ws, &form, Lang::En).unwrap())
            .unwrap()
            .boards()[0]
            .1,
        "other.pos"
    );
    // nothing loaded: a name is still given
    let bare = Workspace::new(Lang::En);
    form.board_file.clear();
    assert_eq!(
        Panel::parse(&panel_text(&bare, &form, Lang::En).unwrap())
            .unwrap()
            .boards()[0]
            .1,
        "board.pos"
    );
    // errors come in the language asked
    form.cols = 0;
    assert!(panel_text(&ws, &form, Lang::Es)
        .unwrap_err()
        .contains("PHOENIX-PNP-017"));
    form.cols = 2;
    form.x_out = "P9".to_string();
    assert!(panel_text(&ws, &form, Lang::En)
        .unwrap_err()
        .contains("PHOENIX-PNP-042"));
}

// ---- files: reading the text and writing the result

#[test]
fn the_text_of_a_tool_is_read_from_a_file_with_a_limit() {
    let dir = scratch("read");
    let mut tb = Toolbox::new(&MachineConfig::default());
    tb.tool = Tool::Camera;
    let good = dir.join("c.csv");
    std::fs::write(&good, example("camera_points.csv")).unwrap();
    tb.path_text = format!("  {}  ", good.display());
    assert!(tb.read_file());
    assert_eq!(tb.camera_text, example("camera_points.csv"));
    // a tool of settings has no text, an empty path reads nothing, a missing file is an error
    tb.tool = Tool::Panel;
    assert!(!tb.read_file());
    tb.tool = Tool::Bom;
    tb.path_text = String::new();
    assert!(!tb.read_file() && tb.error.is_empty());
    tb.path_text = dir.join("none.csv").to_string_lossy().into_owned();
    assert!(!tb.read_file());
    assert!(!tb.error.is_empty());
    // a file over the limit is refused without being read
    let big = dir.join("big.csv");
    let f = std::fs::File::create(&big).unwrap();
    f.set_len(MAX_TOOL_BYTES + 1).unwrap();
    tb.path_text = big.to_string_lossy().into_owned();
    assert!(!tb.read_file());
    assert!(tb.error.contains("over the"), "{}", tb.error);
    assert!(tb.bom_text.is_empty());
    // a file that is not text is refused too
    let bin = dir.join("bin.csv");
    std::fs::write(&bin, [0xffu8, 0xfe, 0x00, 0x80]).unwrap();
    tb.path_text = bin.to_string_lossy().into_owned();
    assert!(!tb.read_file());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_result_is_written_once_under_the_name_of_the_tool_and_never_over_a_file() {
    let dir = scratch("save");
    let out = dir.join("out dir");
    let ws = board();
    let mut tb = Toolbox::new(&MachineConfig::default());
    // nothing was run: nothing to write
    assert!(tb.save(out.to_str().unwrap(), Lang::En).is_none() && tb.error.is_empty());
    tb.tool = Tool::Polarity;
    tb.run(&ws, Lang::En);
    let path = tb.save(out.to_str().unwrap(), Lang::En).unwrap();
    assert!(path.ends_with("polarity.csv"), "{path}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), tb.output);
    // the second time the file is there: a refusal in the language asked, and the file is as it was
    let before = std::fs::read_to_string(&path).unwrap();
    assert!(tb.save(out.to_str().unwrap(), Lang::Es).is_none());
    assert!(tb.error.contains("PHOENIX-PNP-040"), "{}", tb.error);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
    // a tool that makes no file writes none
    tb.tool = Tool::Backlash;
    tb.run(&ws, Lang::En);
    assert!(tb.save(out.to_str().unwrap(), Lang::En).is_none());
    // the panel goes to panel.json, and the bom order to bom_order.csv
    tb.tool = Tool::Panel;
    tb.run(&ws, Lang::En);
    assert!(tb
        .save(out.to_str().unwrap(), Lang::En)
        .unwrap()
        .ends_with("panel.json"));
    tb.tool = Tool::Bom;
    tb.bom_text = example("bom.csv");
    tb.run(&ws, Lang::En);
    assert!(tb
        .save(out.to_str().unwrap(), Lang::En)
        .unwrap()
        .ends_with("bom_order.csv"));
    // the window state writes into its output folder and says where
    let mut a = AppState::new(UiSettings::default());
    a.ui.out_dir = dir.join("state").to_string_lossy().into_owned();
    a.tools.tool = Tool::Feeders;
    a.tools.feeders_text = example("feeder_measures.csv");
    a.tools_run();
    a.tools_save();
    assert!(
        a.tools.notice.starts_with("Written to ") && a.tools.notice.ends_with("feeders_taught.csv"),
        "{}",
        a.tools.notice
    );
    a.tools_save();
    assert!(!a.tools.error.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_state_reads_a_file_into_the_open_tool_and_runs_it() {
    let dir = scratch("state");
    let file = dir.join("readings.csv");
    std::fs::write(&file, example("nozzle_readings.csv")).unwrap();
    let mut a = AppState::new(UiSettings::default());
    a.tools.tool = Tool::Runout;
    a.tools.path_text = file.to_string_lossy().into_owned();
    a.tools_read_file();
    assert_eq!(a.tools.runout_text, example("nozzle_readings.csv"));
    assert!(a.tools.info.starts_with("8 readings"), "{}", a.tools.info);
    a.tools.path_text = dir.join("nothing").to_string_lossy().into_owned();
    a.tools_read_file();
    assert!(!a.tools.error.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_text_tools_never_panic_on_garbage_or_cut_files() {
    let ws = board();
    let files = [
        example("camera_points.csv"),
        example("nozzle_readings.csv"),
        example("feeder_measures.csv"),
        example("bom.csv"),
    ];
    for good in &files {
        for cut in 0..good.len() {
            if !good.is_char_boundary(cut) {
                continue;
            }
            let t = &good[..cut];
            let _ = camera_text(t, Lang::En);
            let _ = runout_text(t, Lang::En);
            let _ = teach_text(t, Lang::En);
            let _ = bom_text(&ws, t, 1, Lang::En);
        }
    }
    for junk in [
        "",
        "\"",
        ",,,",
        "\u{feff}",
        "1e999,1e999,1e999,1e999",
        "nan,nan,nan",
        "#\n#\n",
    ] {
        let _ = camera_text(junk, Lang::En);
        let _ = runout_text(junk, Lang::En);
        let _ = teach_text(junk, Lang::En);
        let _ = bom_text(&ws, junk, 1, Lang::En);
    }
}

#[test]
fn the_stock_tool_adds_the_boards_up_against_the_feeders_that_are_loaded() {
    let mut ws = board();
    ws.load_text("feeders.csv", &example("feeders.csv"))
        .unwrap();
    let (info, csv) = stock_text(&ws, None, 1, Lang::En).unwrap();
    assert!(
        info.starts_with("ok: the feeders hold enough for 1 boards"),
        "{info}"
    );
    assert!(
        csv.starts_with("value,package,per_board,available,needed,short,boards_covered,feeders\n")
    );
    let (info, _) = stock_text(&ws, None, 40, Lang::En).unwrap();
    assert!(
        info.contains("MCU / QFN-32: 40 needed") && info.contains("short: 2 kinds of 4"),
        "{info}"
    );
    // one side only, and a part that a fix left out is not counted
    let (info, csv) = stock_text(&ws, Some(Side::Bottom), 60, Lang::En).unwrap();
    assert!(info.contains("LED / LED_0603: 60 needed"), "{info}");
    assert_eq!(csv.lines().count(), 2);
    let mut less = board();
    less.load_text("feeders.csv", &example("feeders.csv"))
        .unwrap();
    less.excluded.insert("U1".to_string());
    let (info, _) = stock_text(&less, None, 40, Lang::En).unwrap();
    assert!(!info.contains("MCU"), "{info}");
    // no feeders: everything falls short, and a number of boards out of range is an error
    let bare = board();
    let (info, _) = stock_text(&bare, None, 1, Lang::En).unwrap();
    assert!(info.contains("no feeder holds it"), "{info}");
    assert!(stock_text(&ws, None, 0, Lang::Es)
        .unwrap_err()
        .contains("PHOENIX-PNP-017"));
    // the tool runs and is written under its name
    let dir = scratch("stock");
    let mut tb = Toolbox::new(&MachineConfig::default());
    tb.tool = Tool::Stock;
    tb.boards = 40;
    tb.run(&ws, Lang::En);
    assert!(tb.error.is_empty(), "{}", tb.error);
    assert!(tb.info.contains("short:"), "{}", tb.info);
    let path = tb.save(dir.to_str().unwrap(), Lang::En).unwrap();
    assert!(path.ends_with("stock.csv"), "{path}");
    assert_eq!(Tool::from_key("stock"), Some(Tool::Stock));
    let _ = std::fs::remove_dir_all(&dir);
}
