//! The tools of the Tools tab that compare and count, without the window: the parts diff, the settings diff, the board statistics, the stock of a queue, the feeders after a job and the
//! journal of a simulated job. Each one gives what its command gives, with the same refusals, and what it writes is never written over a file.

use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::error::Lang;
use eh3d_phoenix_pnp::gui::texts::{table, tr, Text};
use eh3d_phoenix_pnp::gui::toolbox::{Tool, Toolbox};
use eh3d_phoenix_pnp::gui::toolmore::{
    board_stats_text, config_diff_text, feeder_deplete_text, journal_text, parts_diff_text,
    queue_stock_text, shown, SHOWN_LINES,
};
use eh3d_phoenix_pnp::gui::workspace::Workspace;
use eh3d_phoenix_pnp::job::Job;
use eh3d_phoenix_pnp::partsdiff::Settings;
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

fn board() -> Workspace {
    let mut ws = Workspace::new(Lang::En);
    ws.load_text("board.pos", &example("board.pos")).unwrap();
    ws
}

fn ready() -> Workspace {
    let mut ws = board();
    ws.load_text("feeders.csv", &example("feeders.csv"))
        .unwrap();
    ws
}

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("phoenix_pnp_toolmore_{tag}_{}", std::process::id()));
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

fn out_of(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

// ---- the tools of the tab

#[test]
fn the_six_tools_have_a_key_a_word_a_file_and_the_texts_they_read() {
    assert_eq!(Tool::ALL.len(), 29);
    for (tool, key, name, first, second) in [
        (Tool::PartsDiff, "partsdiff", "parts_diff.csv", true, true),
        (
            Tool::ConfigDiff,
            "configdiff",
            "config_diff.txt",
            true,
            true,
        ),
        (
            Tool::BoardStats,
            "boardstats",
            "board_stats.txt",
            false,
            false,
        ),
        (
            Tool::QueueStock,
            "queuestock",
            "queue_stock.csv",
            false,
            false,
        ),
        (
            Tool::FeederDeplete,
            "deplete",
            "feeders_after_job.csv",
            true,
            true,
        ),
        (Tool::Journal, "journal", "journal.csv", false, false),
    ] {
        assert_eq!(tool.key(), key);
        assert_eq!(Tool::from_key(key), Some(tool));
        assert_eq!(tool.output_name(), Some(name));
        assert_eq!((tool.reads_text(), tool.reads_second()), (first, second));
        assert!(!tr(tool.text(), Lang::En).is_empty());
    }
    // the others read no second text
    assert!(!Tool::Camera.reads_second() && !Tool::Queue.reads_second());
}

#[test]
fn the_new_words_are_in_the_eight_languages_and_the_portuguese_is_not_the_spanish() {
    let new = [
        Text::ToolPartsDiff,
        Text::ToolConfigDiff,
        Text::ToolBoardStats,
        Text::ToolQueueStock,
        Text::ToolFeederDeplete,
        Text::ToolJournal,
        Text::HintPartsDiff,
        Text::HintConfigDiff,
        Text::HintBoardStats,
        Text::HintQueueStock,
        Text::HintFeederDeplete,
        Text::HintJournal,
        Text::ToolSecondText,
        Text::TolPlaceMm,
        Text::TolTurnDeg,
        Text::NearMm,
        Text::NearLimit,
        Text::JournalSteps,
    ];
    assert_eq!(new.len(), 18);
    assert_eq!(Text::COUNT, table().len());
    for word in new {
        let row = &table()[word as usize];
        assert_eq!(row.len(), 8);
        assert!(row.iter().all(|w| !w.trim().is_empty()), "{word:?}");
        // the six that name a tool, and the hints, are not the same text in two languages
        if (word as usize) <= Text::HintJournal as usize {
            let mut seen = std::collections::BTreeSet::new();
            assert!(row.iter().all(|w| seen.insert(*w)), "{word:?}: {row:?}");
        }
    }
    // Portugal, not Brazil
    assert!(tr(Text::HintQueueStock, Lang::Pt).contains("partilham"));
    assert!(tr(Text::ToolConfigDiff, Lang::Pt).contains("definições"));
}

// ---- the parts diff

#[test]
fn the_parts_diff_says_what_changed_against_the_table_that_is_loaded() {
    let ws = board();
    let second = example("board.pos")
        .replace(
            "R2        10k       R_0603             15.000",
            "R2        22k       R_0603             15.500",
        )
        .replace("D1        LED", "D2        LED");
    let (info, csv) = parts_diff_text(&ws, "", &second, &Settings::default(), Lang::En).unwrap();
    assert!(info.contains("R2"), "{info}");
    assert!(info.contains("D1") && info.contains("D2"), "{info}");
    assert!(csv.lines().count() >= 4, "{csv}");
    // the same as the command gives for the same two files
    let dir = scratch("pdiff");
    let b = dir.join("second.pos");
    std::fs::write(&b, &second).unwrap();
    let o = run(&["parts-diff", "examples/board.pos", b.to_str().unwrap()]);
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(out_of(&o).trim_end(), info.trim_end());
}

#[test]
fn the_parts_diff_takes_a_typed_first_table_and_says_when_nothing_changed() {
    let ws = Workspace::new(Lang::En);
    let text = example("board.pos");
    let (info, csv) = parts_diff_text(&ws, &text, &text, &Settings::default(), Lang::En).unwrap();
    assert!(!info.contains("R1:"), "{info}");
    assert_eq!(csv.lines().count(), 1, "only the header: {csv}");
    // a place within the tolerance is the same place, and a wider tolerance is told
    let nudged = text.replace("10.000     20.000", "10.004     20.000");
    let (_, csv) = parts_diff_text(&ws, &text, &nudged, &Settings::default(), Lang::En).unwrap();
    assert_eq!(csv.lines().count(), 1);
    let tight = Settings {
        tol_mm: 0.001,
        rot_tol_deg: 0.5,
    };
    let (_, csv) = parts_diff_text(&ws, &text, &nudged, &tight, Lang::En).unwrap();
    assert!(csv.lines().count() > 1, "{csv}");
}

#[test]
fn the_parts_diff_refuses_what_the_command_refuses() {
    let ws = board();
    let text = example("board.pos");
    let ok = Settings::default();
    // no second table, and no first table when none is loaded
    assert!(parts_diff_text(&ws, "", "  ", &ok, Lang::En).is_err());
    let bare = Workspace::new(Lang::En);
    assert!(parts_diff_text(&bare, "", &text, &ok, Lang::En)
        .unwrap_err()
        .contains("no table of parts is loaded"));
    // a reference that is twice in a table is an error with its code, and so is a tolerance out of range
    let twice =
        format!("{text}R1        10k       R_0603             1.000      1.000      0.0000  top\n");
    let e = parts_diff_text(&ws, "", &twice, &ok, Lang::En).unwrap_err();
    assert!(e.contains("PHOENIX-PNP-"), "{e}");
    let wild = Settings {
        tol_mm: 500.0,
        rot_tol_deg: 0.5,
    };
    let e = parts_diff_text(&ws, "", &text, &wild, Lang::En).unwrap_err();
    assert!(e.contains("PHOENIX-PNP-"), "{e}");
    // a table over the size that a file may have
    let mut small = ws;
    small.cfg.max_file_bytes = 100;
    let e = parts_diff_text(&small, "", &text, &ok, Lang::En).unwrap_err();
    assert!(
        e.contains("PHOENIX-PNP-") && e.contains("limit is 100"),
        "{e}"
    );
}

// ---- the settings diff

#[test]
fn the_settings_diff_tells_a_meaning_from_a_way_of_writing() {
    let (info, text) = config_diff_text(
        "pick_s = 0.3\nvacuum = yes\nfeed_s = 0.5\n",
        "pick_s = 0.30\nvacuum = on\nfeed_s = 0.8\nplace_s = 0.45\n",
        Lang::En,
    )
    .unwrap();
    assert!(info.is_empty());
    assert!(
        text.contains("feed_s") && text.contains("place_s"),
        "{text}"
    );
    assert!(!text.contains("vacuum"), "{text}");
    // the command says the same for the same files
    let dir = scratch("cdiff");
    let (a, b) = (dir.join("a.cfg"), dir.join("b.cfg"));
    std::fs::write(&a, "pick_s = 0.3\nvacuum = yes\nfeed_s = 0.5\n").unwrap();
    std::fs::write(
        &b,
        "pick_s = 0.30\nvacuum = on\nfeed_s = 0.8\nplace_s = 0.45\n",
    )
    .unwrap();
    let o = run(&["config-diff", a.to_str().unwrap(), b.to_str().unwrap()]);
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(out_of(&o), text);
    // two files that mean the same are not a difference
    let (_, same) = config_diff_text("vacuum = yes\n", "vacuum = on\n", Lang::En).unwrap();
    assert!(!same.contains("feed_s"), "{same}");
}

#[test]
fn the_settings_diff_names_the_file_that_is_not_valid_and_wants_both() {
    assert!(config_diff_text("", "pick_s = 1\n", Lang::En)
        .unwrap_err()
        .contains("two configuration files"));
    let e = config_diff_text("pick_s = 1\n", "pick_s = lots\n", Lang::En).unwrap_err();
    assert!(
        e.starts_with("the second file: ") && e.contains("PHOENIX-PNP-"),
        "{e}"
    );
    let e = config_diff_text("pick_s = lots\n", "pick_s = 1\n", Lang::En).unwrap_err();
    assert!(e.starts_with("the first file: "), "{e}");
    // in another language the reason is in that language
    let es = config_diff_text("pick_s = lots\n", "pick_s = 1\n", Lang::Es).unwrap_err();
    assert_ne!(es, e);
    let big = "x = 1\n".repeat(400_000);
    assert!(config_diff_text(&big, "pick_s = 1\n", Lang::En)
        .unwrap_err()
        .contains("limit is"));
}

// ---- the statistics of the board

#[test]
fn the_board_statistics_count_the_table_and_find_the_parts_that_sit_too_near() {
    let ws = board();
    let (info, text) = board_stats_text(&ws, 0.0, 20, Lang::En).unwrap();
    assert!(info.is_empty());
    assert!(text.contains("R_0603") && text.contains("QFN-32"), "{text}");
    // the command says the same
    let o = run(&["board-stats", "examples/board.pos"]);
    assert!(o.status.success());
    assert_eq!(out_of(&o), text);
    // with a distance, the pairs of one side that are closer than that (R1 and R2 are 5 mm apart)
    let (_, near) = board_stats_text(&ws, 6.0, 20, Lang::En).unwrap();
    assert!(near.contains("R1") && near.contains("R2"), "{near}");
    let o = run(&["board-stats", "examples/board.pos", "--near", "6"]);
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(out_of(&o), near);
    let (_, none) = board_stats_text(&ws, 1.0, 20, Lang::En).unwrap();
    assert!(!none.contains("R1 "), "{none}");
    // the pairs shown are limited
    let (_, one) = board_stats_text(&ws, 40.0, 1, Lang::En).unwrap();
    assert!(one.lines().count() < near.lines().count() + 20);
}

#[test]
fn the_board_statistics_want_a_table_and_a_distance_in_range() {
    let bare = Workspace::new(Lang::En);
    assert!(board_stats_text(&bare, 0.0, 20, Lang::En)
        .unwrap_err()
        .contains("no table of parts"));
    let e = board_stats_text(&board(), 100_000.0, 20, Lang::En).unwrap_err();
    assert!(e.contains("PHOENIX-PNP-"), "{e}");
}

// ---- the stock of a queue

fn queue_in(dir: &Path, boards: &str) -> PathBuf {
    let table = dir.join("f.csv");
    std::fs::write(
        &table,
        "id,value,package,x,y,count\nF1,10k,R_0603,-20,10,100\nF2,100n,C_0603,-20,20,100\nF3,MCU,QFN-32,-20,40,25\nF4,LED,LED_0603,-20,50,50\n",
    )
    .unwrap();
    let q = dir.join("q.csv");
    for name in ["a", "b", "c"] {
        let o = run(&[
            "queue-add",
            q.to_str().unwrap(),
            name,
            "examples/board.pos",
            table.to_str().unwrap(),
            "--boards",
            boards,
        ]);
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    }
    q
}

#[test]
fn the_stock_of_a_queue_is_what_the_command_says_and_names_the_job_where_a_reel_ends() {
    let dir = scratch("qstock");
    let q = queue_in(&dir, "10");
    let ws = Workspace::new(Lang::En);
    let (info, csv) = queue_stock_text(&ws, q.to_str().unwrap(), Lang::En).unwrap();
    assert!(
        info.contains("MCU / QFN-32") && info.contains("runs out in the job"),
        "{info}"
    );
    assert!(csv.starts_with("feeders,value,package,needed,available,short,runs_out_in,feeders\n"));
    let o = run(&["queue-stock", q.to_str().unwrap()]);
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(out_of(&o), format!("{info}\n"));
    // with the boards that fit nothing falls short
    let dir = scratch("qstock_ok");
    let q = queue_in(&dir, "1");
    let (info, _) = queue_stock_text(&ws, q.to_str().unwrap(), Lang::En).unwrap();
    assert!(info.contains("ok: the feeders hold enough"), "{info}");
}

#[test]
fn the_stock_of_a_queue_refuses_a_path_that_is_not_a_queue_and_says_when_there_is_nothing_to_count()
{
    let ws = Workspace::new(Lang::En);
    assert!(queue_stock_text(&ws, "  ", Lang::En)
        .unwrap_err()
        .contains("path of the queue file"));
    let dir = scratch("qstock_bad");
    let e = queue_stock_text(&ws, dir.join("none.csv").to_str().unwrap(), Lang::En).unwrap_err();
    assert!(e.contains("there is no such queue file"), "{e}");
    // a queue file that was changed by hand is refused with its code
    let q = queue_in(&dir, "10");
    let mut text = std::fs::read_to_string(&q).unwrap();
    text = text.replace(",10,0,", ",11,0,");
    std::fs::write(&q, text).unwrap();
    let e = queue_stock_text(&ws, q.to_str().unwrap(), Lang::En).unwrap_err();
    assert!(e.contains("PHOENIX-PNP-060"), "{e}");
    // a queue with no job still to be made has nothing to count and no table to write
    let empty = dir.join("empty.csv");
    std::fs::write(&empty, eh3d_phoenix_pnp::queue::Queue::new().to_text()).unwrap();
    let (info, csv) = queue_stock_text(&ws, empty.to_str().unwrap(), Lang::En).unwrap();
    assert!(info.starts_with("nothing to count"), "{info}");
    assert!(csv.is_empty());
}

// ---- the feeders after a job

/// The saved state of the job of the loaded tables after `steps` steps.
fn state_after(ws: &Workspace, steps: usize) -> String {
    let planned = ws.make_plan().unwrap();
    let mut job = Job::new(planned.plan, ws.feeders.clone(), ws.cfg.clone()).unwrap();
    let _ = job.run(Some(steps));
    job.save_state()
}

#[test]
fn the_feeders_after_a_job_are_what_the_command_writes_for_the_same_state() {
    let ws = ready();
    let state = state_after(&ws, 100);
    let (info, csv) = feeder_deplete_text(&ws, "", &state, "", Lang::En).unwrap();
    assert!(info.contains("F1: 100 -> 97 (3 taken)"), "{info}");
    let table = eh3d_phoenix_pnp::feeding::to_csv(&ws.feeders);
    assert!(csv.lines().next().unwrap().contains("count"), "{csv}");
    let dir = scratch("deplete");
    let (f, s) = (dir.join("f.csv"), dir.join("s.json"));
    std::fs::write(&f, &table).unwrap();
    std::fs::write(&s, &state).unwrap();
    let out = dir.join("after.csv");
    let o = run(&[
        "feeder-deplete",
        f.to_str().unwrap(),
        "--state",
        s.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(std::fs::read_to_string(&out).unwrap(), csv);
    // a typed table of feeders is used before the one that is loaded
    let typed =
        example("feeders.csv").replace("F1,10k,R_0603,-20,10,100", "F1,10k,R_0603,-20,10,50");
    // it is the typed table that the state is judged against: this one held fewer parts than the state says are left
    let e = feeder_deplete_text(&ws, &typed, &state, "", Lang::En).unwrap_err();
    assert!(e.contains("held 50 parts"), "{e}");
}

#[test]
fn the_feeders_after_a_job_take_the_state_of_the_job_under_way_when_none_is_given() {
    let ws = ready();
    let under_way = state_after(&ws, 4);
    let (info, _) = feeder_deplete_text(&ws, "", "", &under_way, Lang::En).unwrap();
    assert!(info.contains("parts taken from"), "{info}");
    // a state that is given wins over the one of the job
    let finished = state_after(&ws, 100);
    let (a, _) = feeder_deplete_text(&ws, "", &finished, &under_way, Lang::En).unwrap();
    let (b, _) = feeder_deplete_text(&ws, "", &finished, "", Lang::En).unwrap();
    assert_eq!(a, b);
}

#[test]
fn the_feeders_after_a_job_refuse_what_the_command_refuses() {
    let ws = ready();
    let bare = Workspace::new(Lang::En);
    assert!(feeder_deplete_text(&bare, "", "{}", "", Lang::En)
        .unwrap_err()
        .contains("no table of feeders"));
    assert!(feeder_deplete_text(&ws, "", "", "", Lang::En)
        .unwrap_err()
        .contains("saved state"));
    // a state that is not JSON, or not of this table, is refused with its code
    let e = feeder_deplete_text(&ws, "", "not json", "", Lang::En).unwrap_err();
    assert!(e.contains("PHOENIX-PNP-"), "{e}");
    let other = "id,value,package,x,y,count\nF1,10k,R_0603,-20,10,100\n";
    let e = feeder_deplete_text(&ws, other, &state_after(&ws, 100), "", Lang::En).unwrap_err();
    assert!(e.contains("PHOENIX-PNP-"), "{e}");
    // a state that says more parts are left than the table held is refused
    let mut loaded = ws;
    let state = state_after(&loaded, 100);
    for f in loaded.feeders.iter_mut() {
        f.base.count = 1;
    }
    let tiny = eh3d_phoenix_pnp::feeding::to_csv(&loaded.feeders);
    let e = feeder_deplete_text(&loaded, &tiny, &state, "", Lang::En).unwrap_err();
    assert!(e.contains("PHOENIX-PNP-"), "{e}");
}

// ---- the journal

#[test]
fn the_journal_has_a_row_for_every_step_of_the_plan_and_is_what_simulate_writes() {
    let ws = ready();
    let (info, csv) = journal_text(&ws, 0, Lang::En).unwrap();
    // six parts of the top side, a pick and a place for each
    assert_eq!(csv.lines().count(), 1 + 12, "{csv}");
    assert!(csv.starts_with("step,at_s,took_s,action,reference,feeder,nozzle,result,status\n"));
    assert!(
        info.starts_with("12 steps (6 picks, 6 places), 0 that did not end well"),
        "{info}"
    );
    let dir = scratch("journal");
    let j = dir.join("j.csv");
    let o = run(&[
        "simulate",
        "examples/board.pos",
        "examples/feeders.csv",
        "--journal",
        j.to_str().unwrap(),
    ]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(std::fs::read_to_string(&j).unwrap(), csv);
}

#[test]
fn the_journal_can_be_cut_at_a_number_of_steps_and_says_when_a_plan_is_not_run() {
    let ws = ready();
    let (info, csv) = journal_text(&ws, 3, Lang::En).unwrap();
    assert_eq!(csv.lines().count(), 4, "{csv}");
    assert!(info.starts_with("3 steps"), "{info}");
    // nothing loaded, or nothing to plan with
    let bare = Workspace::new(Lang::En);
    assert!(journal_text(&bare, 0, Lang::En)
        .unwrap_err()
        .contains("load a table of parts"));
    assert!(journal_text(&board(), 0, Lang::En).is_err());
    // a plan with an error is not run, as `simulate` does not run it
    let mut broken = ready();
    broken.feeders.retain(|f| f.base.id != "F3");
    let e = journal_text(&broken, 0, Lang::En).unwrap_err();
    assert!(e.contains("PHOENIX-PNP-"), "{e}");
}

#[test]
fn a_job_that_an_error_stopped_still_gives_its_journal_and_says_which_error() {
    let mut ws = ready();
    // every pick fails and the policy is to stop: the first step is the last row
    ws.cfg = MachineConfig::parse("sim_pick_fail_rate = 1\n").unwrap();
    let (info, csv) = journal_text(&ws, 0, Lang::En).unwrap();
    assert_eq!(csv.lines().count(), 2, "{csv}");
    assert!(
        csv.lines().nth(1).unwrap().contains("PHOENIX-PNP-"),
        "{csv}"
    );
    assert!(
        info.contains("1 steps (1 picks, 0 places), 1 that did not end well"),
        "{info}"
    );
    assert!(info.contains("stopped: PHOENIX-PNP-"), "{info}");
}

// ---- the tab

#[test]
fn the_tab_runs_each_tool_and_writes_its_result_to_a_new_file_only() {
    let ws = ready();
    let cfg = MachineConfig::default();
    let mut tb = Toolbox::new(&cfg);
    assert_eq!((tb.near_mm, tb.near_limit, tb.journal_steps), (0.0, 20, 0));
    assert_eq!((tb.diff_tol_mm, tb.diff_rot_tol_deg), (0.01, 0.5));
    let dir = scratch("tab");
    let out = dir.to_str().unwrap();
    // the journal
    tb.tool = Tool::Journal;
    tb.journal_steps = 2;
    tb.run(&ws, Lang::En);
    assert!(tb.error.is_empty(), "{}", tb.error);
    assert_eq!(tb.made_by, Some(Tool::Journal));
    assert_eq!(tb.output.lines().count(), 3);
    let path = tb.save(out, Lang::En).unwrap();
    assert!(path.ends_with("journal.csv"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), tb.output);
    // a second save does not write over the file
    assert!(tb.save(out, Lang::En).is_none());
    assert!(!tb.error.is_empty());
    // the parts diff with a typed second table
    tb.tool = Tool::PartsDiff;
    tb.parts_second = example("board.pos").replace("U1        MCU", "U9        MCU");
    tb.run(&ws, Lang::En);
    assert!(tb.error.is_empty(), "{}", tb.error);
    assert!(tb.info.contains("U9"), "{}", tb.info);
    assert!(tb.save(out, Lang::En).unwrap().ends_with("parts_diff.csv"));
    // the settings diff, from two typed files
    tb.tool = Tool::ConfigDiff;
    tb.config_first = "feed_s = 0.5\n".to_string();
    tb.config_second = "feed_s = 0.7\n".to_string();
    tb.run(&ws, Lang::En);
    assert!(tb.output.contains("feed_s"), "{}", tb.output);
    // the board statistics follow the form
    tb.tool = Tool::BoardStats;
    tb.near_mm = 6.0;
    tb.run(&ws, Lang::En);
    assert!(tb.output.contains("R1"), "{}", tb.output);
    // the feeders after the job take the state of the job under way
    tb.tool = Tool::FeederDeplete;
    tb.job_state = state_after(&ws, 100);
    tb.run(&ws, Lang::En);
    assert!(tb.info.contains("parts taken from"), "{}", tb.info);
    // a tool that fails says so and leaves no result
    tb.tool = Tool::QueueStock;
    tb.queue.file = dir.join("none.csv").to_str().unwrap().to_string();
    tb.run(&ws, Lang::En);
    assert!(
        tb.error.contains("there is no such queue file"),
        "{}",
        tb.error
    );
    assert!(tb.output.is_empty() && tb.made_by.is_none());
}

#[test]
fn the_second_text_is_read_from_its_own_file_and_what_was_read_is_never_written_over() {
    let ws = ready();
    let dir = scratch("second");
    let cfg = MachineConfig::default();
    let mut tb = Toolbox::new(&cfg);
    tb.tool = Tool::ConfigDiff;
    let a = dir.join("a.cfg");
    std::fs::write(&a, "feed_s = 0.9\n").unwrap();
    tb.second_path_text = a.to_str().unwrap().to_string();
    assert!(tb.read_second_file());
    assert_eq!(tb.config_second, "feed_s = 0.9\n");
    assert!(tb.config_first.is_empty());
    // the first text has its own path
    tb.path_text = a.to_str().unwrap().to_string();
    assert!(tb.read_file());
    assert_eq!(tb.config_first, "feed_s = 0.9\n");
    // a file that is not there is an error that says which
    tb.second_path_text = dir.join("none.cfg").to_str().unwrap().to_string();
    assert!(!tb.read_second_file());
    assert!(tb.error.contains("none.cfg"), "{}", tb.error);
    // a tool without a second text has nowhere to put it
    tb.tool = Tool::Journal;
    tb.second_path_text = a.to_str().unwrap().to_string();
    assert!(!tb.read_second_file());
    // the result is never written over a file that was read: the folder of the output holds the file that was read
    tb.tool = Tool::Journal;
    tb.run(&ws, Lang::En);
    let j = dir.join("journal.csv");
    std::fs::write(&j, "x").unwrap();
    std::fs::remove_file(&j).unwrap();
    tb.path_text = j.to_str().unwrap().to_string();
    // the file does not exist yet: it is a path that was typed to read, and the output goes to the same name
    let saved = tb.save(dir.to_str().unwrap(), Lang::En);
    assert!(saved.is_none());
    assert!(tb.error.contains("also an input"), "{}", tb.error);
}

#[test]
fn a_long_result_shows_its_first_lines_and_says_how_many_more() {
    let text: String = (0..SHOWN_LINES + 5)
        .map(|i| format!("line {i}\n"))
        .collect();
    let s = shown(&text);
    assert_eq!(s.lines().count(), SHOWN_LINES + 1);
    assert!(s.ends_with("... 5 more lines"), "{s}");
    assert_eq!(shown("a\nb\n"), "a\nb");
    assert_eq!(shown(""), "");
}
