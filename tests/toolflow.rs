//! The panels of the Tools tab for the reels that last, the nozzles compared, the trouble spots of the runs and the changes of the feeders between the jobs of a queue, and the values of the
//! forms that live in the machine file: each panel gives what its command gives, and the values are read from the file and kept in it.

use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::error::Lang;
use eh3d_phoenix_pnp::gui::app::AppState;
use eh3d_phoenix_pnp::gui::settings::UiSettings;
use eh3d_phoenix_pnp::gui::texts::{table, tr, Text};
use eh3d_phoenix_pnp::gui::toolbox::{Tool, Toolbox};
use eh3d_phoenix_pnp::gui::toolmore::{
    changeover_text, hotspots_text, nozzle_compare_text, runway_text, server_machine_text,
};
use eh3d_phoenix_pnp::gui::workspace::Workspace;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn example(name: &str) -> String {
    std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("examples")
            .join(name),
    )
    .unwrap()
}

fn ready() -> Workspace {
    let mut ws = Workspace::new(Lang::En);
    ws.load_text("board.pos", &example("board.pos")).unwrap();
    ws.load_text("feeders.csv", &example("feeders.csv"))
        .unwrap();
    ws
}

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("phoenix_pnp_flow_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_phoenix-pnp"))
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap()
}

fn said(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).trim_end().to_string()
}

fn p(path: &Path) -> &str {
    path.to_str().unwrap()
}

// ---- the tools and their words

#[test]
fn the_four_tools_have_a_key_a_word_a_file_and_read_no_text() {
    assert_eq!(Tool::ALL.len(), 40);
    for (tool, key, name) in [
        (Tool::ReelRunway, "runway", "reel_runway.csv"),
        (Tool::NozzleCompare, "nozzles", "nozzle_compare.csv"),
        (Tool::Hotspots, "hotspots", "hotspots.csv"),
        (Tool::QueueChangeover, "changeover", "queue_changeover.csv"),
    ] {
        assert_eq!(tool.key(), key);
        assert_eq!(Tool::from_key(key), Some(tool));
        assert_eq!(Tool::from_key(&key.to_uppercase()), Some(tool));
        assert_eq!(tool.output_name(), Some(name));
        assert!(!tool.reads_text() && !tool.reads_second(), "{key}");
        assert!(!tr(tool.text(), Lang::En).is_empty());
    }
    let mut keys: Vec<&str> = Tool::ALL.iter().map(|t| t.key()).collect();
    keys.sort_unstable();
    keys.dedup();
    assert_eq!(keys.len(), 40);
    let mut files: Vec<&str> = Tool::ALL.iter().filter_map(|t| t.output_name()).collect();
    files.sort_unstable();
    let before = files.len();
    files.dedup();
    assert_eq!(files.len(), before, "two tools write the same file");
}

#[test]
fn the_new_words_are_in_the_eight_languages_and_in_the_language_of_portugal() {
    let new = [
        Text::ToolReelRunway,
        Text::ToolNozzleCompare,
        Text::ToolHotspots,
        Text::ToolQueueChangeover,
        Text::HintReelRunway,
        Text::HintNozzleCompare,
        Text::HintHotspots,
        Text::HintQueueChangeover,
        Text::RunwayBoards,
        Text::NozzlesMost,
        Text::KeepValues,
        Text::ValuesKept,
        Text::NoConfigToKeep,
        Text::ToolServerMachine,
        Text::HintServerMachine,
        Text::ServerSimulated,
    ];
    assert_eq!(Text::COUNT, table().len());
    // words were added after the last of these: the rows that come after are checked by their own tests
    assert!(Text::BomBoardsSecond as usize + 1 < Text::COUNT);
    assert_eq!(new[0] as usize, Text::RunsSeed as usize + 1);
    for (i, word) in new.iter().enumerate() {
        assert_eq!(*word as usize, new[0] as usize + i, "{word:?} is in order");
        let row = &table()[*word as usize];
        assert_eq!(row.len(), 8);
        assert!(row.iter().all(|w| !w.trim().is_empty()), "{word:?}");
        let mut seen = std::collections::BTreeSet::new();
        assert!(row.iter().all(|w| seen.insert(*w)), "{word:?}: {row:?}");
    }
    // the language of Portugal, not the one of Brazil
    assert!(tr(Text::KeepValues, Lang::Pt).contains("ficheiro"));
    assert!(tr(Text::NoConfigToKeep, Lang::Pt).contains("ficheiro"));
    assert!(tr(Text::HintQueueChangeover, Lang::Pt).contains("ficheiro"));
    assert!(tr(Text::HintNozzleCompare, Lang::Pt).contains("definições"));
    assert!(!tr(Text::HintReelRunway, Lang::Pt).contains("arquivo"));
    assert!(tr(Text::HintServerMachine, Lang::Pt).contains("definições"));
    assert!(!tr(Text::HintServerMachine, Lang::Pt).contains("configurações"));
    assert!(tr(Text::ServerSimulated, Lang::Pt).contains("Usar um servidor"));
    // the words a person types the number for end in a clear label in each language
    assert_eq!(tr(Text::RunwayBoards, Lang::En), "Boards wanted");
    assert_eq!(tr(Text::NozzlesMost, Lang::En), "Most nozzles");
}

// ---- the reels

#[test]
fn the_runway_panel_is_what_the_command_says_and_marks_the_reels_that_fall_short() {
    let ws = ready();
    let (info, csv) = runway_text(&ws, 1, Lang::En).unwrap();
    let o = run(&["reel-runway", "examples/board.pos", "examples/feeders.csv"]);
    assert_eq!(said(&o), info);
    assert!(
        csv.starts_with(
            "feeder,value,package,count,per_board,boards,left_over\nF3,MCU,QFN-32,10,1,10,0\n"
        ),
        "{csv}"
    );
    let (info, _) = runway_text(&ws, 20, Lang::En).unwrap();
    assert!(info.contains("0 parts over  <- short"), "{info}");
    assert!(
        info.ends_with("short: 1 feeders last fewer than the 20 boards asked for"),
        "{info}"
    );
    // nothing loaded, a plan with errors
    assert!(runway_text(&Workspace::new(Lang::En), 1, Lang::En)
        .unwrap_err()
        .contains("load a table of parts"));
    let mut broken = ready();
    broken.feeders.retain(|f| f.base.id != "F3");
    assert!(runway_text(&broken, 1, Lang::En)
        .unwrap_err()
        .contains("PHOENIX-PNP-"));
}

// ---- the nozzles

#[test]
fn the_nozzles_panel_is_what_the_command_says_up_to_the_number_asked() {
    let ws = ready();
    let (info, csv) = nozzle_compare_text(&ws, 6, Lang::En).unwrap();
    let o = run(&[
        "nozzle-compare",
        "examples/board.pos",
        "examples/feeders.csv",
    ]);
    assert_eq!(said(&o), info);
    assert_eq!(csv.lines().count(), 7);
    let (info, csv) = nozzle_compare_text(&ws, 2, Lang::En).unwrap();
    assert_eq!(csv.lines().count(), 3);
    assert_eq!(info.lines().filter(|l| l.starts_with("  ")).count(), 3);
    assert!(nozzle_compare_text(&Workspace::new(Lang::En), 2, Lang::En)
        .unwrap_err()
        .contains("load a table of parts"));
    // a configuration with two heads says they were left out
    let mut heads = ready();
    heads.cfg = MachineConfig::parse("head = a : 2\nhead = b : 3\n").unwrap();
    let (info, _) = nozzle_compare_text(&heads, 3, Lang::En).unwrap();
    assert!(
        info.starts_with("the 2 heads of the configuration are left out"),
        "{info}"
    );
}

// ---- the trouble spots

#[test]
fn the_hotspots_panel_is_what_the_command_says_for_the_same_seeds() {
    let mut ws = ready();
    ws.cfg = MachineConfig::parse(
        "error_policy = defer\npick_retries = 0\nplace_retries = 0\nfault_limit = 0\nsim_feeder_fail = F2 : 1\n",
    )
    .unwrap();
    let (info, csv) = hotspots_text(&ws, 3, 5, Lang::En).unwrap();
    assert!(
        info.contains("in 3 runs from the seed 5; F2 gave the most"),
        "{info}"
    );
    assert!(csv.contains("\nF2,0,6,100.00,0,6\n"), "{csv}");
    let dir = scratch("hot");
    let cfg = dir.join("fail.cfg");
    std::fs::write(&cfg, "error_policy = defer\npick_retries = 0\nplace_retries = 0\nfault_limit = 0\nsim_feeder_fail = F2 : 1\n").unwrap();
    let o = run(&[
        "simulate-hotspots",
        "examples/board.pos",
        "examples/feeders.csv",
        "--config",
        p(&cfg),
        "--runs",
        "3",
        "--seed",
        "5",
    ]);
    assert_eq!(said(&o), info);
    // a machine that does not fail
    let (calm, _) = hotspots_text(&ready(), 2, 1, Lang::En).unwrap();
    assert_eq!(calm, "no feeder gave trouble in 2 runs from the seed 1");
    // out of range, nothing loaded, a plan with errors
    assert!(hotspots_text(&ws, 0, 1, Lang::En)
        .unwrap_err()
        .contains("PHOENIX-PNP-"));
    assert!(hotspots_text(&Workspace::new(Lang::En), 1, 1, Lang::En)
        .unwrap_err()
        .contains("load a table of parts"));
    let mut broken = ready();
    broken.feeders.retain(|f| f.base.id != "F3");
    assert!(hotspots_text(&broken, 1, 1, Lang::En)
        .unwrap_err()
        .contains("PHOENIX-PNP-"));
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- the changes between the jobs

#[test]
fn the_changeover_panel_reads_the_queue_file_that_was_typed() {
    let dir = scratch("change");
    let (a, b) = (dir.join("a.csv"), dir.join("b.csv"));
    std::fs::write(
        &a,
        "id,value,package,x,y,count\nF1,10k,R_0603,-20,10,100\nF2,100n,C_0603,-20,20,100\n",
    )
    .unwrap();
    std::fs::write(
        &b,
        "id,value,package,x,y,count\nF1,10k,R_0603,-20,10,100\nF2,1u,C_0603,-20,20,100\n",
    )
    .unwrap();
    let q = dir.join("q.csv");
    for (name, table) in [("one", &a), ("two", &b)] {
        let o = run(&[
            "queue-add",
            p(&q),
            name,
            "examples/board.pos",
            p(table),
            "--boards",
            "2",
        ]);
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    }
    let ws = ready();
    let (info, csv) = changeover_text(&ws, p(&q), Lang::En).unwrap();
    assert!(
        info.starts_with("one -> two: 1 feeders to change or load, 1 stay as they are"),
        "{info}"
    );
    assert!(
        csv.contains("one,two,F2,,change,100n / C_0603,1u / C_0603\n"),
        "{csv}"
    );
    let o = run(&["queue-changeover", p(&q)]);
    assert_eq!(said(&o), info);
    // a path with blanks around it is the path
    assert!(changeover_text(&ws, &format!("  {}  ", p(&q)), Lang::En).is_ok());
    // nothing typed, a file that is not there
    assert!(changeover_text(&ws, "  ", Lang::En)
        .unwrap_err()
        .contains("type the path of the queue file"));
    assert!(changeover_text(&ws, p(&dir.join("none.csv")), Lang::En)
        .unwrap_err()
        .contains("there is no such queue file"));
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- the tab

#[test]
fn the_tab_runs_the_four_tools_and_writes_each_result_once() {
    let ws = ready();
    let mut tb = Toolbox::new(&ws.cfg);
    let dir = scratch("tab");
    let out = p(&dir);
    for (tool, file) in [
        (Tool::ReelRunway, "reel_runway.csv"),
        (Tool::NozzleCompare, "nozzle_compare.csv"),
        (Tool::Hotspots, "hotspots.csv"),
    ] {
        tb.tool = tool;
        tb.run(&ws, Lang::En);
        assert!(tb.error.is_empty(), "{:?} {}", tool, tb.error);
        assert!(!tb.output.is_empty());
        assert!(tb.save(out, Lang::En).unwrap().ends_with(file));
        // never over a file that is there
        assert!(tb.save(out, Lang::En).is_none());
    }
    // the forms start with what the tools take
    assert_eq!((tb.runway_boards, tb.nozzles_most), (1, 6));
    tb.tool = Tool::ReelRunway;
    tb.runway_boards = 20;
    tb.run(&ws, Lang::En);
    assert!(tb.info.contains("<- short"), "{}", tb.info);
    tb.tool = Tool::NozzleCompare;
    tb.nozzles_most = 2;
    tb.run(&ws, Lang::En);
    assert_eq!(tb.output.lines().count(), 3);
    // the queue file is the one of the Queue tool
    tb.tool = Tool::QueueChangeover;
    tb.queue.file.clear();
    tb.run(&ws, Lang::En);
    assert!(
        tb.error.contains("type the path of the queue file"),
        "{}",
        tb.error
    );
    assert!(tb.output.is_empty() && tb.made_by.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- the values of the forms

#[test]
fn the_forms_start_with_the_values_of_the_machine_file_and_take_them_again_when_it_is_loaded() {
    let cfg = MachineConfig::parse(
        "parts_diff_tol_mm = 0.2\nparts_diff_rot_tol_deg = 4\nboard_stats_near_mm = 2.5\nboard_stats_limit = 9\nfeeder_slots_max_plans = 77\nsimulate_runs_count = 33\njournal_steps = 12\n",
    )
    .unwrap();
    let mut tb = Toolbox::new(&cfg);
    assert_eq!((tb.diff_tol_mm, tb.diff_rot_tol_deg), (0.2, 4.0));
    assert_eq!((tb.near_mm, tb.near_limit), (2.5, 9));
    assert_eq!(
        (tb.slots_plans, tb.runs_count, tb.journal_steps),
        (77, 33, 12)
    );
    let v = tb.values();
    assert_eq!(v.simulate_runs_count, 33);
    assert_eq!(
        v,
        eh3d_phoenix_pnp::toolvalues::ToolValues::from_config(&cfg)
    );
    // what was typed is dropped when another file is loaded
    tb.near_limit = 500;
    tb.runs_count = 5;
    tb.take_defaults(&MachineConfig::default());
    assert_eq!(
        (tb.near_limit, tb.runs_count, tb.slots_plans),
        (20, 20, 5000)
    );
    assert_eq!(
        (
            tb.diff_tol_mm,
            tb.diff_rot_tol_deg,
            tb.near_mm,
            tb.journal_steps
        ),
        (0.01, 0.5, 0.0, 0)
    );
    // a number too big for the file is held to the largest, which the file refuses
    tb.near_limit = usize::MAX;
    assert_eq!(tb.values().board_stats_limit, u32::MAX);
    // the tools use the typed values
    let ws = ready();
    tb.near_limit = 1000;
    tb.near_mm = 12.0;
    tb.tool = Tool::BoardStats;
    tb.run(&ws, Lang::En);
    assert!(
        tb.output.contains("9 pairs of parts closer than 12 mm"),
        "{}",
        tb.output
    );
}

#[test]
fn the_window_keeps_the_values_in_the_file_that_was_loaded_and_nowhere_else() {
    let dir = scratch("keep");
    let mut app = AppState::new(UiSettings::default());
    // no file loaded: told, and nothing is written
    app.keep_tool_values();
    assert_eq!(app.tools.error, tr(Text::NoConfigToKeep, Lang::En));
    assert!(app.tools.notice.is_empty());
    let file = dir.join("mine.cfg");
    std::fs::write(
        &file,
        "# my machine\r\nnozzles = 3\r\nsimulate_runs_count = 8\r\n",
    )
    .unwrap();
    assert_eq!(app.load_paths(std::slice::from_ref(&file)), 1);
    assert_eq!(app.tools.runs_count, 8);
    app.tools.runs_count = 55;
    app.tools.near_limit = 7;
    app.tools.diff_tol_mm = 0.25;
    app.keep_tool_values();
    assert!(app.tools.error.is_empty(), "{}", app.tools.error);
    assert!(
        app.tools.notice.starts_with(tr(Text::ValuesKept, Lang::En)),
        "{}",
        app.tools.notice
    );
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(
        text.starts_with("# my machine\r\nnozzles = 3\r\nsimulate_runs_count = 55\r\n"),
        "{text:?}"
    );
    assert!(
        text.contains("board_stats_limit = 7\r\n") && text.contains("parts_diff_tol_mm = 0.25\r\n"),
        "{text:?}"
    );
    // the machine of the window has them too
    assert_eq!(
        (app.ws.cfg.simulate_runs_count, app.ws.cfg.board_stats_limit),
        (55, 7)
    );
    assert_eq!(app.ws.cfg.nozzles, 3);
    // a value that the file refuses leaves the file as it was
    let before = std::fs::read_to_string(&file).unwrap();
    app.tools.near_limit = 0;
    app.keep_tool_values();
    assert!(
        app.tools.error.contains("PHOENIX-PNP-"),
        "{}",
        app.tools.error
    );
    assert!(app.tools.notice.is_empty());
    assert_eq!(std::fs::read_to_string(&file).unwrap(), before);
    assert_eq!(app.ws.cfg.board_stats_limit, 7);
    // a sample is not a file: nothing to keep in
    let mut other = AppState::new(UiSettings::default());
    other.load_sample(&eh3d_phoenix_pnp::gui::samples::mini());
    other.keep_tool_values();
    assert_eq!(other.tools.error, tr(Text::NoConfigToKeep, Lang::En));
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- the machine of the server

#[test]
fn the_server_machine_tool_has_a_key_a_word_a_file_and_reads_a_text() {
    let tool = Tool::ServerMachine;
    assert_eq!(tool.key(), "server");
    assert_eq!(Tool::from_key(" Server "), Some(tool));
    assert_eq!(tool.output_name(), Some("server_feeders.csv"));
    assert!(tool.reads_text() && !tool.reads_second());
    assert_eq!(tr(tool.text(), Lang::En), "Server machine");
    let mut tb = Toolbox::new(&MachineConfig::default());
    assert!(!tb.server_sim && tb.server_script.is_empty());
    tb.tool = tool;
    tb.text_mut().unwrap().push_str("list");
    assert_eq!(tb.server_script, "list");
}

#[test]
fn the_server_machine_panel_does_its_lines_in_order_on_the_server_in_memory() {
    let ws = ready();
    // a blank script lists the feeders
    let (info, csv) = server_machine_text(&ws, "  \n# nothing\n", true, Lang::En).unwrap();
    assert!(
        info.starts_with("0 of 200 slots hold a part, 200 are free"),
        "{info}"
    );
    assert_eq!(csv.lines().count(), 1);
    let (info, csv) = server_machine_text(
        &ws,
        "set 3 R_0603_10k 150\nset 4 C_0603_100n 0\n",
        true,
        Lang::En,
    )
    .unwrap();
    assert!(info.contains("slot 3: R_0603_10k, 150 left"), "{info}");
    assert!(
        info.contains("slot 4: C_0603_100n, 0 left (empty)"),
        "{info}"
    );
    assert_eq!(csv.lines().count(), 3);
    let (info, _) =
        server_machine_text(&ws, "set 3 R_0603_10k 150\nclear 3\n", true, Lang::En).unwrap();
    assert!(info.starts_with("0 of 200 slots"), "{info}");
    // a calibration, the loaded table of feeders and the loaded board as a dry run
    let script = "set 3 R_0603_10k 150\ncalibrate feeder 3 0.5,-0.25\nload\ndry-run\n";
    let (info, csv) = server_machine_text(&ws, script, true, Lang::En).unwrap();
    let lines: Vec<&str> = info.lines().collect();
    assert!(lines[0].ends_with("done, 1 of 1 (100%)"), "{info}");
    assert!(lines[1].ends_with("done, 4 of 4 (100%)"), "{info}");
    assert!(
        lines[2].ends_with("done, 7 of 7 (100%) (dry run)"),
        "{info}"
    );
    // the dry run took no part off a feeder, so the table is the one that was loaded
    assert_eq!(csv.lines().count(), 5, "{csv}");
    assert!(
        csv.lines()
            .skip(1)
            .all(|l| l.split(',').nth(2) != Some("0")),
        "{csv}"
    );
    // a new server with only the table loaded holds the same counts
    // (the calibration of a slot stays with the slot, so only the counts are compared)
    let (_, loaded) = server_machine_text(&ws, "load\nlist\n", true, Lang::En).unwrap();
    let counts = |t: &str| -> Vec<String> {
        t.lines()
            .map(|l| l.split(',').take(3).collect::<Vec<_>>().join(","))
            .collect()
    };
    assert_eq!(counts(&loaded), counts(&csv));
    assert!(csv.contains("0.500,-0.250"), "{csv}");
}

#[test]
fn the_server_machine_panel_says_which_line_it_stopped_at() {
    let ws = ready();
    for (script, why) in [
        ("list\nbogus\n", "line 2: say list, set SLOT PART COUNT"),
        ("set x R 1", "line 1: the slot 'x' is not a whole number"),
        ("set 3 R", "line 1: say list"),
        (
            "clear 0",
            "line 1: the slot is a whole number from 1 to 200",
        ),
        (
            "\n\ncalibrate laser 1",
            "line 3: laser is not feeder, nozzle or camera",
        ),
        ("calibrate nozzle N1 5", "line 1: the offset '5' is not X,Y"),
        ("calibrate nozzle N1 9,0", "line 1: the offset on x"),
        (
            "calibrate feeder 999",
            "line 1: the target of a feeder is its slot",
        ),
    ] {
        let e = server_machine_text(&ws, script, true, Lang::En).unwrap_err();
        assert!(e.contains(why), "{script:?}: {e}");
    }
    // without a loaded board and feeders, load and dry-run say so
    let empty = Workspace::new(Lang::En);
    for script in ["load", "dry-run"] {
        let e = server_machine_text(&empty, script, true, Lang::En).unwrap_err();
        assert!(e.starts_with("line 1: (load a table of"), "{e}");
    }
    // the local mode is not touched
    let e = server_machine_text(&ws, "list", false, Lang::En).unwrap_err();
    assert!(e.contains("mode is local"), "{e}");
}

#[test]
fn the_server_machine_tool_runs_in_the_tab_and_writes_its_table_under_its_name() {
    let ws = ready();
    let dir = scratch("servertool");
    let mut tb = Toolbox::new(&MachineConfig::default());
    tb.tool = Tool::ServerMachine;
    tb.server_sim = true;
    tb.server_script = "set 2 C_0603_100n 40".to_string();
    tb.run(&ws, Lang::En);
    assert!(tb.error.is_empty(), "{}", tb.error);
    assert!(
        tb.info.contains("slot 2: C_0603_100n, 40 left"),
        "{}",
        tb.info
    );
    assert_eq!(tb.made_by, Some(Tool::ServerMachine));
    let saved = tb.save(p(&dir), Lang::En).unwrap();
    assert!(saved.ends_with("server_feeders.csv"), "{saved}");
    assert!(tb.save(p(&dir), Lang::En).is_none());
    // not simulated and in the local mode, it says so and keeps nothing
    tb.server_sim = false;
    tb.run(&ws, Lang::En);
    assert!(tb.error.contains("mode is local"), "{}", tb.error);
    let _ = std::fs::remove_dir_all(&dir);
}
