//! The panels of the Tools tab for the reels that were changed, the parts that a person places by hand, the difference of two jobs, the places of the feeders and the many runs: each one gives
//! what its command gives, with the same refusals, and what it writes is never written over a file.

use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::error::Lang;
use eh3d_phoenix_pnp::exchange;
use eh3d_phoenix_pnp::gui::texts::{table, tr, Text};
use eh3d_phoenix_pnp::gui::toolbox::{Tool, Toolbox};
use eh3d_phoenix_pnp::gui::toolmore::{
    aside_sheet_text, feeder_refill_text, feeder_slots_text, job_diff_text, runs_text,
};
use eh3d_phoenix_pnp::gui::workspace::Workspace;
use eh3d_phoenix_pnp::job::Job;
use std::path::PathBuf;

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
    let d = std::env::temp_dir().join(format!("phoenix_pnp_panels_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// The saved state of the job of the loaded tables after `steps` steps.
fn state_after(ws: &Workspace, steps: usize) -> String {
    let planned = ws.make_plan().unwrap();
    let mut job = Job::new(planned.plan, ws.feeders.clone(), ws.cfg.clone()).unwrap();
    let _ = job.run(Some(steps));
    job.save_state()
}

/// A workspace whose job puts parts aside: half of the picks fail, with no retry and the policy defer.
fn defers() -> Workspace {
    let mut ws = ready();
    ws.cfg = MachineConfig::parse(
        "error_policy = defer\nsim_pick_fail_rate = 0.6\npick_retries = 0\nsim_seed = 7\n",
    )
    .unwrap();
    ws
}

#[test]
fn the_five_tools_have_a_key_a_word_a_file_and_the_texts_they_read() {
    assert_eq!(Tool::ALL.len(), 34);
    for (tool, key, name, first, second) in [
        (
            Tool::FeederRefill,
            "refill",
            "feeders_refilled.csv",
            true,
            true,
        ),
        (Tool::AsideSheet, "aside", "aside_sheet.csv", true, false),
        (Tool::JobDiff, "jobdiff", "job_diff.csv", true, true),
        (
            Tool::FeederSlots,
            "slots",
            "feeders_slots.csv",
            false,
            false,
        ),
        (Tool::Runs, "runs", "runs.csv", false, false),
    ] {
        assert_eq!(tool.key(), key);
        assert_eq!(Tool::from_key(key), Some(tool));
        assert_eq!(tool.output_name(), Some(name));
        assert_eq!((tool.reads_text(), tool.reads_second()), (first, second));
        assert!(!tr(tool.text(), Lang::En).is_empty());
    }
    // every key is its own
    let mut keys: Vec<&str> = Tool::ALL.iter().map(|t| t.key()).collect();
    keys.sort_unstable();
    keys.dedup();
    assert_eq!(keys.len(), 34);
}

#[test]
fn the_new_words_are_in_the_eight_languages_and_in_the_language_of_portugal() {
    let new = [
        Text::ToolFeederRefill,
        Text::ToolAsideSheet,
        Text::ToolJobDiff,
        Text::ToolFeederSlots,
        Text::ToolRuns,
        Text::HintFeederRefill,
        Text::HintAsideSheet,
        Text::HintJobDiff,
        Text::HintFeederSlots,
        Text::HintRuns,
        Text::AsidePending,
        Text::SlotsPlans,
        Text::RunsCount,
        Text::RunsSeed,
    ];
    assert_eq!(Text::COUNT, table().len());
    assert!((Text::RunsSeed as usize) < Text::COUNT);
    for word in new {
        let row = &table()[word as usize];
        assert!(row.iter().all(|w| !w.trim().is_empty()), "{word:?}");
        if (word as usize) <= Text::HintRuns as usize {
            let mut seen = std::collections::BTreeSet::new();
            assert!(row.iter().all(|w| seen.insert(*w)), "{word:?}: {row:?}");
        }
    }
    assert!(tr(Text::HintFeederRefill, Lang::Pt).contains("reposições"));
    assert!(tr(Text::ToolAsideSheet, Lang::Pt).contains("Peças"));
}

// ---- the reels that were changed

#[test]
fn a_reel_that_was_changed_is_a_refill_of_the_feeder_that_is_loaded_or_typed() {
    let ws = ready();
    let (info, csv) = feeder_refill_text(&ws, "", "id,count\nF3,500\n", Lang::En).unwrap();
    assert!(info.contains("F3: 10 -> 500 (refilled)"), "{info}");
    assert!(
        info.ends_with("1 feeders refilled, 490 parts more"),
        "{info}"
    );
    assert!(csv.contains("F3,MCU,QFN-32,-20,40,500"), "{csv}");
    // the command gives the same table for the same two files
    let dir = scratch("refill");
    let (f, r, out) = (dir.join("f.csv"), dir.join("r.csv"), dir.join("o.csv"));
    std::fs::write(&f, eh3d_phoenix_pnp::feeding::to_csv(&ws.feeders)).unwrap();
    std::fs::write(&r, "id,count\nF3,500\n").unwrap();
    let o = std::process::Command::new(env!("CARGO_BIN_EXE_phoenix-pnp"))
        .args(["feeder-refill", f.to_str().unwrap(), r.to_str().unwrap()])
        .args(["--out", out.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(o.status.success());
    assert_eq!(std::fs::read_to_string(&out).unwrap(), csv);
    // a typed table of feeders is used before the one that is loaded
    let typed = "id,value,package,x,y,count\nF9,1k,R,0,0,5\n";
    let (info, csv) = feeder_refill_text(&ws, typed, "id,count\nF9,50\n", Lang::En).unwrap();
    assert!(info.contains("F9: 5 -> 50"), "{info}");
    assert_eq!(csv, "id,value,package,x,y,count\nF9,1k,R,0,0,50\n");
}

#[test]
fn a_refill_that_does_not_fit_is_refused_with_its_code_and_a_missing_table_is_asked_for() {
    let ws = ready();
    assert!(feeder_refill_text(&ws, "", "  ", Lang::En)
        .unwrap_err()
        .contains("table of refills"));
    let e = feeder_refill_text(&ws, "", "id,count\nF9,5\n", Lang::En).unwrap_err();
    assert!(e.contains("PHOENIX-PNP-013") && e.contains("F9"), "{e}");
    let e = feeder_refill_text(&ws, "", "id,count\nF1,0\n", Lang::En).unwrap_err();
    assert!(e.contains("PHOENIX-PNP-013"), "{e}");
    let bare = Workspace::new(Lang::En);
    assert!(feeder_refill_text(&bare, "", "id,count\nF1,5\n", Lang::En)
        .unwrap_err()
        .contains("no table of feeders"));
    let mut small = ready();
    small.cfg.max_file_bytes = 10;
    let e = feeder_refill_text(&small, "", "id,count\nF1,5\n", Lang::En).unwrap_err();
    assert!(e.contains("limit is 10"), "{e}");
}

// ---- the parts by hand

#[test]
fn the_parts_by_hand_are_the_ones_the_state_of_the_job_says_were_left_behind() {
    let ws = defers();
    let state = state_after(&ws, 1_000);
    let (info, csv) = aside_sheet_text(&ws, &state, "", false, Lang::En).unwrap();
    assert!(info.contains("- put aside: "), "{info}");
    assert!(info.contains("parts for a person to place: "), "{info}");
    assert!(csv.starts_with("reference,value,package,side,x,y,rotation,status,reason,feeder\n"));
    assert!(csv.lines().count() >= 2);
    // the state of the job that is under way is used when none is typed, and a typed one wins
    let (a, _) = aside_sheet_text(&ws, "", &state, false, Lang::En).unwrap();
    assert_eq!(a, info);
    let (b, _) = aside_sheet_text(&ws, &state, &state_after(&ws, 2), false, Lang::En).unwrap();
    assert_eq!(b, info);
    // a job that stopped part way has parts that it did not get to, told only when asked for
    let half = state_after(&ws, 3);
    let (none, _) = aside_sheet_text(&ws, &half, "", false, Lang::En).unwrap();
    let (all, csv) = aside_sheet_text(&ws, &half, "", true, Lang::En).unwrap();
    assert!(!none.contains("not placed yet:"), "{none}");
    assert!(all.contains("not placed yet"), "{all}");
    assert!(csv.contains(",pending,"), "{csv}");
}

#[test]
fn the_parts_by_hand_refuse_what_the_command_refuses() {
    let ws = defers();
    let bare = Workspace::new(Lang::En);
    assert!(aside_sheet_text(&bare, "{}", "", false, Lang::En)
        .unwrap_err()
        .contains("load a table of parts"));
    assert!(aside_sheet_text(&ws, "", "", false, Lang::En)
        .unwrap_err()
        .contains("saved state"));
    let e = aside_sheet_text(&ws, "not json", "", false, Lang::En).unwrap_err();
    assert!(e.contains("PHOENIX-PNP-"), "{e}");
    // a state of another job
    let mut other = ready();
    other.feeders.pop();
    let e = aside_sheet_text(&ws, &state_after(&other, 5), "", false, Lang::En).unwrap_err();
    assert!(e.contains("PHOENIX-PNP-"), "{e}");
    // a job that left nothing behind
    let clean = ready();
    let (info, csv) =
        aside_sheet_text(&clean, &state_after(&clean, 1_000), "", false, Lang::En).unwrap();
    assert_eq!(info, "no part is left for a person to place");
    assert_eq!(
        csv,
        "reference,value,package,side,x,y,rotation,status,reason,feeder\n"
    );
}

// ---- the difference of two jobs

fn job_file(ws: &Workspace, steps: Option<usize>) -> String {
    let planned = ws.make_plan().unwrap();
    match steps {
        None => exchange::from_plan(&planned.plan),
        Some(n) => {
            let mut job = Job::new(planned.plan, ws.feeders.clone(), ws.cfg.clone()).unwrap();
            let _ = job.run(Some(n));
            exchange::from_job(&job)
        }
    }
}

#[test]
fn the_difference_of_two_jobs_is_what_the_command_says_and_names_the_file_that_is_not_valid() {
    let ws = ready();
    let (a, b) = (job_file(&ws, None), job_file(&ws, Some(4)));
    let (info, csv) = job_diff_text(&ws, &a, &b, Lang::En).unwrap();
    assert!(info.contains("plan phase: ready -> paused"), "{info}");
    assert!(csv.contains(",status,pending,placed"), "{csv}");
    let (same, csv) = job_diff_text(&ws, &a, &a, Lang::En).unwrap();
    assert_eq!(same, "the two jobs are the same");
    assert_eq!(csv, "reference,change,first,second\n");
    assert!(job_diff_text(&ws, &a, " ", Lang::En)
        .unwrap_err()
        .contains("two files of job-export"));
    let e = job_diff_text(&ws, &a, "{\"kind\": \"other\"}", Lang::En).unwrap_err();
    assert!(
        e.starts_with("the second file: ") && e.contains("PHOENIX-PNP-026"),
        "{e}"
    );
    let e = job_diff_text(&ws, "[]", &a, Lang::En).unwrap_err();
    assert!(e.starts_with("the first file: "), "{e}");
    let mut small = ready();
    small.cfg.max_file_bytes = 20;
    assert!(job_diff_text(&small, &a, &b, Lang::En)
        .unwrap_err()
        .contains("limit is 20"));
}

// ---- the places of the feeders

fn badly_placed() -> Workspace {
    let mut parts = String::from("ref,val,package,x,y,rot,side\n");
    for i in 0..8 {
        parts.push_str(&format!("R{},10k,R,{},10,0,top\n", i + 1, 10 + i * 5));
    }
    parts.push_str("C1,100n,C,30,25,0,top\n");
    let mut ws = Workspace::new(Lang::En);
    ws.load_text("parts.csv", &parts).unwrap();
    ws.load_text(
        "feeders.csv",
        "id,value,package,x,y,count,slot\nF1,10k,R,-200,10,50,S1\nF2,100n,C,-20,10,50,S2\n",
    )
    .unwrap();
    ws
}

#[test]
fn the_places_of_the_feeders_are_looked_for_on_the_tables_that_are_loaded() {
    let ws = badly_placed();
    let (info, csv) = feeder_slots_text(&ws, 1_000, Lang::En).unwrap();
    assert!(
        info.contains("F1: -200.000,10.000 -> -20.000,10.000 (slot S1 -> S2)"),
        "{info}"
    );
    assert!(info.contains("2 feeders move: the plan takes "), "{info}");
    let row = csv.lines().nth(1).unwrap();
    assert!(
        row.starts_with("F1,10k,R,-20,10,50,") && row.contains(",S2,"),
        "{csv}"
    );
    // the number of plans is a limit
    let (info, csv) = feeder_slots_text(&ws, 1, Lang::En).unwrap();
    assert!(info.contains("the search was cut"), "{info}");
    assert!(csv
        .lines()
        .nth(1)
        .unwrap()
        .starts_with("F1,10k,R,-200,10,50,"));
    assert!(feeder_slots_text(&ws, 0, Lang::En)
        .unwrap_err()
        .contains("PHOENIX-PNP-"));
    // already well placed, and nothing loaded
    let (info, _) = feeder_slots_text(&ready(), 1_000, Lang::En).unwrap();
    assert!(
        info.contains("feeders") || info.contains("well placed"),
        "{info}"
    );
    assert!(feeder_slots_text(&Workspace::new(Lang::En), 10, Lang::En)
        .unwrap_err()
        .contains("load a table of parts"));
}

// ---- many runs

#[test]
fn the_many_runs_are_what_the_command_says_for_the_same_seeds() {
    let ws = ready();
    let (info, csv) = runs_text(&ws, 3, 11, Lang::En).unwrap();
    assert!(
        info.starts_with(
            "3 runs from the seed 11: 3 placed every part, 0 were stopped by an error\n"
        ),
        "{info}"
    );
    assert_eq!(csv.lines().count(), 4);
    assert!(csv.lines().nth(1).unwrap().starts_with("1,11,finished,"));
    let o = std::process::Command::new(env!("CARGO_BIN_EXE_phoenix-pnp"))
        .args([
            "simulate-runs",
            "examples/board.pos",
            "examples/feeders.csv",
        ])
        .args(["--runs", "3", "--seed", "11"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&o.stdout).trim_end(), info);
    // with failures the runs differ and the errors are counted
    let (info, csv) = runs_text(&defers(), 10, 1, Lang::En).unwrap();
    assert!(info.contains("parts put aside in a run: "), "{info}");
    assert_eq!(csv.lines().count(), 11);
    // out of range, nothing loaded, a plan with errors
    assert!(runs_text(&ws, 0, 1, Lang::En)
        .unwrap_err()
        .contains("PHOENIX-PNP-"));
    assert!(runs_text(&Workspace::new(Lang::En), 1, 1, Lang::En)
        .unwrap_err()
        .contains("load a table of parts"));
    let mut broken = ready();
    broken.feeders.retain(|f| f.base.id != "F3");
    assert!(runs_text(&broken, 1, 1, Lang::En)
        .unwrap_err()
        .contains("PHOENIX-PNP-"));
}

// ---- the tab

#[test]
fn the_tab_runs_the_new_tools_and_writes_each_result_once() {
    let ws = defers();
    let mut tb = Toolbox::new(&ws.cfg);
    assert_eq!((tb.runs_count, tb.runs_seed, tb.slots_plans), (20, 7, 5000));
    assert!(!tb.aside_pending);
    let dir = scratch("tab");
    let out = dir.to_str().unwrap();
    // the runs
    tb.tool = Tool::Runs;
    tb.runs_count = 4;
    tb.run(&ws, Lang::En);
    assert!(tb.error.is_empty(), "{}", tb.error);
    assert_eq!(tb.output.lines().count(), 5);
    assert!(tb.save(out, Lang::En).unwrap().ends_with("runs.csv"));
    assert!(tb.save(out, Lang::En).is_none());
    // the parts by hand take the state of the job under way
    tb.tool = Tool::AsideSheet;
    tb.job_state = state_after(&ws, 1_000);
    tb.run(&ws, Lang::En);
    assert!(tb.error.is_empty(), "{}", tb.error);
    assert!(
        tb.info.contains("parts for a person to place"),
        "{}",
        tb.info
    );
    assert!(tb.save(out, Lang::En).unwrap().ends_with("aside_sheet.csv"));
    // the refill reads its second text from a file
    tb.tool = Tool::FeederRefill;
    let f = dir.join("refills.csv");
    std::fs::write(&f, "id,count\nF1,999\n").unwrap();
    tb.second_path_text = f.to_str().unwrap().to_string();
    assert!(tb.read_second_file());
    assert_eq!(tb.refill_rows, "id,count\nF1,999\n");
    tb.run(&ws, Lang::En);
    assert!(
        tb.output.contains("F1,10k,R_0603,-20,10,999"),
        "{}",
        tb.output
    );
    assert!(tb
        .save(out, Lang::En)
        .unwrap()
        .ends_with("feeders_refilled.csv"));
    // the difference of two jobs
    tb.tool = Tool::JobDiff;
    tb.jobdiff_first = job_file(&ws, None);
    tb.jobdiff_second = job_file(&ws, Some(2));
    tb.run(&ws, Lang::En);
    assert!(tb.error.is_empty(), "{}", tb.error);
    assert!(tb.save(out, Lang::En).unwrap().ends_with("job_diff.csv"));
    // the places of the feeders
    tb.tool = Tool::FeederSlots;
    tb.slots_plans = 50;
    tb.run(&ws, Lang::En);
    assert!(tb.error.is_empty(), "{}", tb.error);
    assert!(tb
        .save(out, Lang::En)
        .unwrap()
        .ends_with("feeders_slots.csv"));
    // a tool that fails leaves no result
    tb.tool = Tool::JobDiff;
    tb.jobdiff_second.clear();
    tb.run(&ws, Lang::En);
    assert!(tb.error.contains("two files of job-export"), "{}", tb.error);
    assert!(tb.output.is_empty() && tb.made_by.is_none());
}
