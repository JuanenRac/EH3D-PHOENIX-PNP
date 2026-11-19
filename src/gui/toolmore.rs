// EH3D-PHOENIX-PNP - the tools of the Tools tab that compare and count, without the window.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! The panels of the Tools tab for what the command line does with `parts-diff`, `config-diff`, `board-stats`, `queue-stock` and `feeder-deplete`, and for the journal of a simulated job
//! (`simulate --journal`). Each one reads the text that was typed or read from a file (or the tables that are loaded), calls the same library functions as its command and gives text, with the
//! same refusals and the same codes of error. Nothing here moves a machine and nothing is written: the tab writes the result to the output folder, never over a file that exists.

use super::workspace::{read_file, Workspace};
use crate::config::MAX_CONFIG_BYTES;
use crate::error::{Code, Lang, PnpError};
use crate::parts::{parse_all, Part};
use crate::preflight::has_errors;
use crate::{boardstats, configdiff, deplete, feeding, job, journal, partsdiff, queuestock};
use std::path::Path;

/// The most lines of a result that are shown above the text of the file; the rest is in the file.
pub const SHOWN_LINES: usize = 40;

/// The first `SHOWN_LINES` lines of `text`, with a line that says how many were left out.
pub fn shown(text: &str) -> String {
    let lines: Vec<&str> = text.trim_end().lines().collect();
    if lines.len() <= SHOWN_LINES {
        return lines.join("\n");
    }
    format!(
        "{}\n... {} more lines",
        lines[..SHOWN_LINES].join("\n"),
        lines.len() - SHOWN_LINES
    )
}

/// A text that was typed or read is no larger than `max` bytes, as the command refuses a file that is.
fn sized(what: &str, text: &str, max: u64) -> Result<(), String> {
    if text.len() as u64 > max {
        return Err(PnpError::new(
            Code::FileTooBig,
            format!("{what} has {} bytes, the limit is {max}", text.len()),
        )
        .to_string());
    }
    Ok(())
}

/// What changed between two tables of parts, part by part: the first one is `first`, or the table that is loaded when it is blank. Gives the report (the first lines) and the changes as a
/// CSV, as `parts-diff` does. A part that is twice in a table, or a tolerance out of range, is refused with its code.
pub fn parts_diff_text(
    ws: &Workspace,
    first: &str,
    second: &str,
    settings: &partsdiff::Settings,
    lang: Lang,
) -> Result<(String, String), String> {
    if second.trim().is_empty() {
        return Err("(type or read the second table of parts)".to_string());
    }
    let max = ws.cfg.max_file_bytes;
    sized("the second table", second, max)?;
    let old: Vec<Part> = if first.trim().is_empty() {
        if ws.parts.is_empty() {
            return Err(
                "(no table of parts is loaded: load one or type the first table)".to_string(),
            );
        }
        ws.parts.clone()
    } else {
        sized("the first table", first, max)?;
        parse_all(first, &ws.cfg.profile)?.parts
    };
    let new = parse_all(second, &ws.cfg.profile)?.parts;
    let found = partsdiff::diff(&old, &new, settings).map_err(|e| e.localized(lang))?;
    Ok((shown(&found.text()), found.csv()))
}

/// The settings that two configuration files give differently, in what they mean and not in how they are written, as `config-diff` says them: the report, which is what is
/// written. A file that is not valid is named as the first or the second, with its code.
pub fn config_diff_text(first: &str, second: &str, lang: Lang) -> Result<(String, String), String> {
    if first.trim().is_empty() || second.trim().is_empty() {
        return Err("(type or read the two configuration files)".to_string());
    }
    let max = MAX_CONFIG_BYTES as u64;
    sized("the first file", first, max)?;
    sized("the second file", second, max)?;
    let found = configdiff::diff(first, second).map_err(|(which, e)| {
        let name = if which == "first" { "first" } else { "second" };
        format!("the {name} file: {}", e.localized(lang))
    })?;
    Ok((String::new(), found.text()))
}

/// What the table of parts that is loaded holds (the sides, the kinds, each package, the box of the centres) and, with `near_mm` above 0, the pairs of parts of one side whose centres are
/// closer than that, as `board-stats` says it. The text is the report, which is what is written.
pub fn board_stats_text(
    ws: &Workspace,
    near_mm: f64,
    limit: usize,
    lang: Lang,
) -> Result<(String, String), String> {
    if ws.parts.is_empty() && ws.dnp.is_empty() {
        return Err("(no table of parts is loaded)".to_string());
    }
    let mut text = boardstats::stats(&ws.parts, ws.dnp.len()).text();
    if near_mm > 0.0 {
        let pairs = boardstats::near(&ws.parts, near_mm).map_err(|e| e.localized(lang))?;
        text.push_str(&boardstats::near_text(&pairs, near_mm, limit));
    }
    Ok((String::new(), text))
}

/// Whether the feeders hold enough parts for the jobs of the queue file `path` that are still to be made, as `queue-stock` says it: the first lines of the report and the table of every
/// kind as a CSV (empty when there is nothing to count). The files of the jobs are read from the folder of the queue.
pub fn queue_stock_text(
    ws: &Workspace,
    path: &str,
    lang: Lang,
) -> Result<(String, String), String> {
    let path = path.trim();
    if path.is_empty() {
        return Err("(type the path of the queue file)".to_string());
    }
    let reader = |p: &str, max: u64| read_file(Path::new(p), max).map(|(text, _)| text);
    let found = queuestock::from_queue(path, &ws.cfg, lang, &reader)?;
    Ok((shown(&found.text), found.csv.unwrap_or_default()))
}

/// The table of feeders as it stands after the job of a saved state, as `feeder-deplete` says it: what each feeder gave, and the table with the new counts as a CSV. The table is `feeders`, or
/// the one that is loaded when it is blank; the state is `state`, or the state of the job that is under way (`job_state`) when it is blank.
pub fn feeder_deplete_text(
    ws: &Workspace,
    feeders: &str,
    state: &str,
    job_state: &str,
    lang: Lang,
) -> Result<(String, String), String> {
    let table = if feeders.trim().is_empty() {
        if ws.feeders.is_empty() {
            return Err("(no table of feeders is loaded: load one or type the table)".to_string());
        }
        feeding::to_csv(&ws.feeders)
    } else {
        sized("the table of feeders", feeders, ws.cfg.max_file_bytes)?;
        feeders.to_string()
    };
    let state = if state.trim().is_empty() {
        if job_state.trim().is_empty() {
            return Err("(type or read a saved state, or start a job in the Job tab)".to_string());
        }
        job_state
    } else {
        sized("the state", state, ws.cfg.max_file_bytes)?;
        state
    };
    let found = deplete::deplete(&table, state).map_err(|e| e.localized(lang))?;
    Ok((shown(&found.text()), found.csv))
}

/// The simulated job of the plan of the loaded tables, run for `steps` steps (all of them when 0), and one row for every step, as `simulate --journal` writes it: the line that sums it up, and the
/// journal as a CSV. A job that an error stopped still gives its journal, ending in the step that failed, and says which error. A plan with errors is not run.
pub fn journal_text(ws: &Workspace, steps: usize, lang: Lang) -> Result<(String, String), String> {
    if !ws.can_plan() {
        return Err("(load a table of parts and a table of feeders to run the job)".to_string());
    }
    let planned = ws.make_plan().map_err(|e| e.localized(lang))?;
    if has_errors(&planned.checks) {
        return Err(PnpError::new(Code::PreflightFailed, "see the Problems tab").localized(lang));
    }
    let mut run = job::Job::new(planned.plan, ws.feeders.clone(), ws.cfg.clone())
        .map_err(|e| e.localized(lang))?;
    let (outcome, rows, cut) = journal::run_recorded(&mut run, (steps > 0).then_some(steps), 0);
    let mut info = journal::summary(&rows);
    if let Err(e) = &outcome {
        info.push_str(&format!("\nstopped: {}", e.localized(lang)));
    }
    if cut {
        info.push_str(&format!("\ncut at {} rows", journal::MAX_ROWS));
    }
    let csv = journal::csv(&rows);
    Ok((info, csv))
}
