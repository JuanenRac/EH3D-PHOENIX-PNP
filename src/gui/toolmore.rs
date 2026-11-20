// EH3D-PHOENIX-PNP - the tools of the Tools tab that compare and count, without the window.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! The panels of the Tools tab for what the command line does with `parts-diff`, `config-diff`, `board-stats`, `queue-stock` and `feeder-deplete`, and for the journal of a simulated job
//! (`simulate --journal`). Each one reads the text that was typed or read from a file (or the tables that are loaded), calls the same library functions as its command and gives text, with the
//! same refusals and the same codes of error. Nothing here moves a machine and nothing is written: the tab writes the result to the output folder, never over a file that exists.

use super::workspace::{read_file, Workspace};
use crate::bom::Bom;
use crate::config::MAX_CONFIG_BYTES;
use crate::error::{Code, Lang, PnpError};
use crate::parts::{parse_all, Part};
use crate::preflight::has_errors;
use crate::server::{self, calibrate_request, dry_run, feeder_rows, placements, Calibrate};
use crate::{
    asidesheet, asmexport, asmimport, boardstats, bommerge, changeover, configdiff, deplete,
    exchange, feeding, hotspots, job, jobdiff, journal, nozzles, nozzlez, panelfids, partsdiff,
    queuestock, refill, runs, runway, shiftoee, slots, tapesteps,
};
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

/// The table of feeders after reels were changed, as `feeder-refill` says it: what each refilled feeder holds now, and the table as a CSV. The table is `feeders`, or the one that is loaded
/// when it is blank; `refills` is the table `id,count[,x,y]` and is needed.
pub fn feeder_refill_text(
    ws: &Workspace,
    feeders: &str,
    refills: &str,
    lang: Lang,
) -> Result<(String, String), String> {
    if refills.trim().is_empty() {
        return Err("(type or read the table of refills: id,count[,x,y])".to_string());
    }
    let max = ws.cfg.max_file_bytes;
    sized("the table of refills", refills, max)?;
    let table = if feeders.trim().is_empty() {
        if ws.feeders.is_empty() {
            return Err("(no table of feeders is loaded: load one or type the table)".to_string());
        }
        feeding::to_csv(&ws.feeders)
    } else {
        sized("the table of feeders", feeders, max)?;
        feeders.to_string()
    };
    let rows = refill::parse_refills(refills).map_err(|e| e.localized(lang))?;
    let found = refill::refill(&table, &rows).map_err(|e| e.localized(lang))?;
    Ok((shown(&found.text()), found.csv))
}

/// The parts that a person has to place by hand after the job of a saved state, as `aside-sheet` says it: the first lines of the sheet and the sheet as a CSV. The state is `state`, or the
/// state of the job that is under way (`job_state`) when it is blank; with `pending` the parts that the job did not get to are on the sheet too.
pub fn aside_sheet_text(
    ws: &Workspace,
    state: &str,
    job_state: &str,
    pending: bool,
    lang: Lang,
) -> Result<(String, String), String> {
    if !ws.can_plan() {
        return Err("(load a table of parts and a table of feeders)".to_string());
    }
    let state = if state.trim().is_empty() {
        if job_state.trim().is_empty() {
            return Err("(type or read a saved state, or start a job in the Job tab)".to_string());
        }
        job_state
    } else {
        sized("the state", state, ws.cfg.max_file_bytes)?;
        state
    };
    let planned = ws.make_plan().map_err(|e| e.localized(lang))?;
    if has_errors(&planned.checks) {
        return Err(PnpError::new(Code::PreflightFailed, "see the Problems tab").localized(lang));
    }
    let mut run = job::Job::new(planned.plan, ws.feeders.clone(), ws.cfg.clone())
        .map_err(|e| e.localized(lang))?;
    run.restore_state(state).map_err(|e| e.localized(lang))?;
    let sheet = asidesheet::build(&run, &ws.parts, pending);
    Ok((shown(&sheet.text()), sheet.csv()))
}

/// What differs between two files of `job-export`, as `job-diff` says it: the first lines of the report and the differences as a CSV. A file that is not valid is named as the first or the
/// second, with its code.
pub fn job_diff_text(
    ws: &Workspace,
    first: &str,
    second: &str,
    lang: Lang,
) -> Result<(String, String), String> {
    if first.trim().is_empty() || second.trim().is_empty() {
        return Err("(type or read the two files of job-export)".to_string());
    }
    let max = ws.cfg.max_file_bytes;
    sized("the first file", first, max)?;
    sized("the second file", second, max)?;
    let read = |name: &str, text: &str| {
        exchange::parse(text).map_err(|e| format!("the {name} file: {}", e.localized(lang)))
    };
    let found = jobdiff::diff(&read("first", first)?, &read("second", second)?);
    Ok((shown(&found.text()), found.csv()))
}

/// The places of the loaded feeders that make the plan the fastest, as `feeder-slots` says it: the first lines of the report and the table of feeders with the new places as a CSV. At most
/// `max_plans` plans are made.
pub fn feeder_slots_text(
    ws: &Workspace,
    max_plans: usize,
    lang: Lang,
) -> Result<(String, String), String> {
    if !ws.can_plan() {
        return Err("(load a table of parts and a table of feeders)".to_string());
    }
    let parts = ws.active_parts();
    let options = ws.planning_options(&parts);
    let found = slots::optimize(
        &parts,
        &feeding::to_csv(&ws.feeders),
        &ws.catalog,
        &ws.cfg,
        &options,
        max_plans,
    )
    .map_err(|e| e.localized(lang))?;
    Ok((shown(&found.text()), found.csv))
}

/// The job of the loaded tables run `count` times on the simulated machine from the seeds `seed`, `seed + 1` and so on, as `simulate-runs` says it: what the runs say as a whole and a row
/// for each run as a CSV. A plan with errors is not run.
pub fn runs_text(
    ws: &Workspace,
    count: usize,
    seed: u64,
    lang: Lang,
) -> Result<(String, String), String> {
    if !ws.can_plan() {
        return Err("(load a table of parts and a table of feeders to run the job)".to_string());
    }
    let planned = ws.make_plan().map_err(|e| e.localized(lang))?;
    if has_errors(&planned.checks) {
        return Err(PnpError::new(Code::PreflightFailed, "see the Problems tab").localized(lang));
    }
    let found = runs::run_many(&planned.plan, &ws.feeders, &ws.cfg, count, seed)
        .map_err(|e| e.localized(lang))?;
    Ok((shown(&found.text()), found.csv()))
}

/// How many whole boards each reel of the loaded feeders lasts for the plan of the loaded tables, as `reel-runway` says it: the first lines of the report (the reels that last fewer than
/// `wanted` boards are marked) and the table as a CSV. A plan with errors is not read.
pub fn runway_text(ws: &Workspace, wanted: usize, lang: Lang) -> Result<(String, String), String> {
    if !ws.can_plan() {
        return Err("(load a table of parts and a table of feeders)".to_string());
    }
    let planned = ws.make_plan().map_err(|e| e.localized(lang))?;
    if has_errors(&planned.checks) {
        return Err(PnpError::new(Code::PreflightFailed, "see the Problems tab").localized(lang));
    }
    let used: Vec<&str> = planned
        .plan
        .plan
        .placements
        .iter()
        .map(|p| p.feeder.as_str())
        .collect();
    let rows = runway::runway(&used, &ws.feeders);
    Ok((
        shown(&runway::text(&rows, wanted as u64)),
        runway::csv(&rows),
    ))
}

/// The loaded board planned with one nozzle on the head, then two, up to `most`, as `nozzle-compare` says it: the first lines of the table and the table as a CSV.
pub fn nozzle_compare_text(
    ws: &Workspace,
    most: usize,
    lang: Lang,
) -> Result<(String, String), String> {
    if !ws.can_plan() {
        return Err("(load a table of parts and a table of feeders)".to_string());
    }
    let planned = ws.make_plan().map_err(|e| e.localized(lang))?;
    let options = ws.planning_options(&planned.parts);
    let rows = nozzles::compare(
        &planned.parts,
        &ws.feeders,
        &planned.picks,
        &ws.catalog,
        &ws.cfg,
        &options,
        most,
    );
    Ok((
        shown(&nozzles::text(&rows, ws.cfg.nozzles, ws.cfg.heads.len())),
        nozzles::csv(&rows),
    ))
}

/// The job of the loaded tables run `count` times on the simulated machine from the seeds `seed`, `seed + 1` and so on, and for each feeder what went wrong, as `simulate-hotspots` says it:
/// the first lines of the report and a row for each feeder as a CSV. A plan with errors is not run.
pub fn hotspots_text(
    ws: &Workspace,
    count: usize,
    seed: u64,
    lang: Lang,
) -> Result<(String, String), String> {
    if !ws.can_plan() {
        return Err("(load a table of parts and a table of feeders to run the job)".to_string());
    }
    let planned = ws.make_plan().map_err(|e| e.localized(lang))?;
    if has_errors(&planned.checks) {
        return Err(PnpError::new(Code::PreflightFailed, "see the Problems tab").localized(lang));
    }
    let found = hotspots::run_many(&planned.plan, &ws.feeders, &ws.cfg, count, seed)
        .map_err(|e| e.localized(lang))?;
    Ok((shown(&found.text()), found.csv()))
}

/// What has to be changed on the feeders between one job of the queue file `path` and the next, for the jobs that are still to be made, as `queue-changeover` says it: the first lines of
/// the report and the table of the feeders as a CSV. The files of the jobs are read from the folder of the queue.
pub fn changeover_text(ws: &Workspace, path: &str, lang: Lang) -> Result<(String, String), String> {
    let path = path.trim();
    if path.is_empty() {
        return Err("(type the path of the queue file)".to_string());
    }
    let reader = |p: &str, max: u64| read_file(Path::new(p), max).map(|(text, _)| text);
    let found = changeover::from_queue(path, &ws.cfg, lang, &reader)?;
    Ok((shown(&found.text), found.csv))
}

/// A whole number from a word of the script of the server panel.
fn script_number(text: &str, what: &str, hi: u64) -> Result<u64, String> {
    text.parse::<u64>()
        .ok()
        .filter(|n| *n <= hi)
        .ok_or_else(|| format!("{what} '{text}' is not a whole number from 0 to {hi}"))
}

/// The offset `X,Y` of a calibration, in millimetres.
fn script_offset(text: &str) -> Result<(f64, f64), String> {
    let (x, y) = text
        .split_once(',')
        .ok_or_else(|| format!("the offset '{text}' is not X,Y"))?;
    let number = |t: &str| {
        t.trim()
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .ok_or_else(|| format!("the offset '{text}' is not X,Y"))
    };
    Ok((number(x)?, number(y)?))
}

/// What the Server machine panel does with the machine of the EH3D server (or a server in memory when `sim`): one line of `script` for each thing, in order, as `server-feeders`,
/// `server-calibrate` and `server-send --dry-run` do them. The words are `list`, `set SLOT PART COUNT`, `clear SLOT`, `calibrate feeder|nozzle|camera TARGET [X,Y]`, `load` (the loaded table
/// of feeders goes on the machine) and `dry-run` (the loaded board is placed as a dry run). A blank script lists the feeders. Gives what was done and the table of feeders after the last line,
/// and the table as a CSV. A line that is not one of these, or that the server refuses, stops the script and says which line.
pub fn server_machine_text(
    ws: &Workspace,
    script: &str,
    sim: bool,
    lang: Lang,
) -> Result<(String, String), String> {
    let t = server::target_for(&ws.cfg, sim)?;
    let mut r = server::connect(&t, sim, lang.code())?;
    let max_polls = ((60_000 / t.poll_ms.max(1)) as u32).max(2);
    let mut wait = || {
        if !sim {
            std::thread::sleep(std::time::Duration::from_millis(t.poll_ms));
        }
    };
    let mut said = String::new();
    let mut table = None;
    for (n, raw) in script.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let at = |why: String| format!("line {}: {why}", n + 1);
        let words: Vec<&str> = line.split_whitespace().collect();
        match words.as_slice() {
            ["list"] => table = Some(r.feeders_table().map_err(at)?),
            ["set", slot, part, count] => {
                let slot = script_number(slot, "the slot", u64::from(u32::MAX)).map_err(at)? as u32;
                let count = script_number(count, "the count", u64::MAX).map_err(at)?;
                table = Some(r.set_feeder(slot, part, count).map_err(at)?);
            }
            ["clear", slot] => {
                let slot = script_number(slot, "the slot", u64::from(u32::MAX)).map_err(at)? as u32;
                table = Some(r.clear_feeder(slot).map_err(at)?);
            }
            ["calibrate", what, target] | ["calibrate", what, target, _] => {
                let what = Calibrate::parse(what)
                    .ok_or_else(|| at(format!("{what} is not feeder, nozzle or camera")))?;
                let offset = match words.get(3) {
                    Some(o) => script_offset(o).map_err(at)?,
                    None => (0.0, 0.0),
                };
                let request = calibrate_request(what, target, offset).map_err(at)?;
                let end = r
                    .run_job(
                        &format!("calibrate-{}", what.word()),
                        &request,
                        max_polls,
                        &mut wait,
                    )
                    .map_err(at)?;
                said.push_str(&format!("{}\n", end.line()));
                if end.state != "done" {
                    return Err(at(format!("the calibration did not end done: {}", end.line())));
                }
                table = None;
            }
            ["load"] => {
                if ws.feeders.is_empty() {
                    return Err(at("(load a table of feeders)".to_string()));
                }
                let rows = feeder_rows(&ws.feeders).map_err(at)?;
                let end = r
                    .run_job(
                        "feeders",
                        &server::feeders_request(&rows, true),
                        max_polls,
                        &mut wait,
                    )
                    .map_err(at)?;
                said.push_str(&format!("{}\n", end.line()));
                if end.state != "done" {
                    return Err(at(format!("the table of feeders was not taken: {}", end.line())));
                }
                table = None;
            }
            ["dry-run"] => {
                if !ws.can_plan() {
                    return Err(at("(load a table of parts and a table of feeders)".to_string()));
                }
                let rows = feeder_rows(&ws.feeders).map_err(at)?;
                let made = placements(&ws.parts, &rows, (0.0, 0.0)).map_err(at)?;
                if !made.unmatched.is_empty() {
                    return Err(at(format!(
                        "{} parts have no feeder that holds their value and package ({})",
                        made.unmatched.len(),
                        made.unmatched.join(", ")
                    )));
                }
                let end = r
                    .run_job("dry-run", &dry_run(made.request), max_polls, &mut wait)
                    .map_err(at)?;
                said.push_str(&format!("{} (dry run)\n", end.line()));
                if end.state != "done" {
                    return Err(at(format!("the dry run did not end done: {}", end.line())));
                }
                table = None;
            }
            _ => {
                return Err(at(
                    "say list, set SLOT PART COUNT, clear SLOT, calibrate feeder|nozzle|camera TARGET [X,Y], load or dry-run"
                        .to_string(),
                ))
            }
        }
    }
    let table = match table {
        Some(t) => t,
        None => r.feeders_table()?,
    };
    Ok((shown(&format!("{said}{}", table.text())), table.csv()))
}

/// The heights of the nozzle tips from the probings of the bed that were typed (`nozzle,z_mm`, one per line) as `nozzle-zcal` says them: the first lines of the report and the table as a CSV.
/// When nothing was typed the probings are made up on the simulated machine (the nozzles and the seed of the settings), and the line that says so closes the report.
pub fn nozzle_z_text(
    ws: &Workspace,
    text: &str,
    tol_mm: f64,
    lang: Lang,
) -> Result<(String, String), String> {
    sized("the probings", text, ws.cfg.max_file_bytes)?;
    let (readings, truth) = if text.trim().is_empty() {
        let plan = nozzlez::Plan {
            nozzles: ws.cfg.nozzles.max(1),
            seed: ws.cfg.sim_seed,
            ..nozzlez::Plan::default()
        };
        let made = nozzlez::simulate(&plan).map_err(|e| e.localized(lang))?;
        (made.readings, Some((made.truth, plan)))
    } else {
        (
            nozzlez::parse_readings(text).map_err(|e| e.localized(lang))?,
            None,
        )
    };
    let report = nozzlez::analyse(&readings, tol_mm).map_err(|e| e.localized(lang))?;
    let mut info = report.text();
    if let Some((truth, plan)) = &truth {
        info.push_str(&format!(
            "simulated probings of {} nozzles, {} each (seed {}): the offsets are {:.4} mm from the heights that were made up; no machine was touched\n",
            plan.nozzles,
            plan.probes,
            plan.seed,
            nozzlez::error_against(&report, truth)
        ));
    }
    Ok((shown(&info), report.csv()))
}

/// The marks of every board of the loaded panel, as `panel-fids` says them: the parts of the panel are the ones of its boards put in their places (the boards marked as bad are left out), and
/// the marks are the parts of them that are fiducials. The first lines of the report and the table as a CSV.
pub fn panel_fids_text(
    ws: &Workspace,
    min_sep_mm: f64,
    lang: Lang,
) -> Result<(String, String), String> {
    let panel = ws.panel.as_ref().ok_or("(load a panel)")?;
    if ws.parts.is_empty() {
        return Err("(load a panel)".to_string());
    }
    if !(0.0..=1000.0).contains(&min_sep_mm) {
        return Err(format!("{min_sep_mm} is not from 0 to 1000 mm"));
    }
    // the boards marked as bad are in the panel itself: walking it without the parts of its boards finds them
    let left_out = panel
        .expand_with(&|_: &str| Ok(Vec::new()), &[])
        .map(|e| e.x_out)
        .map_err(|e| e.localized(lang))?;
    let report = panelfids::analyse(
        &ws.parts,
        &left_out,
        &ws.cfg.fiducial_words,
        &ws.cfg.fiducial_refs,
        min_sep_mm,
    );
    Ok((shown(&report.text()), report.csv()))
}

/// The tapes of the loaded feeders at every step of the plan, as `tape-steps` says it: the first lines of the report and the table of all the steps as a CSV. A plan with errors is not read.
pub fn tape_steps_text(ws: &Workspace, lang: Lang) -> Result<(String, String), String> {
    if !ws.can_plan() {
        return Err("(load a table of parts and a table of feeders)".to_string());
    }
    let planned = ws.make_plan().map_err(|e| e.localized(lang))?;
    if has_errors(&planned.checks) {
        return Err(PnpError::new(Code::PreflightFailed, "see the Problems tab").localized(lang));
    }
    let cfg = &ws.cfg;
    let limits = tapesteps::Limits {
        x_min: cfg.limit_x_min_mm,
        x_max: cfg.limit_x_max_mm,
        y_min: cfg.limit_y_min_mm,
        y_max: cfg.limit_y_max_mm,
        pitch_tol_mm: cfg.tape_pitch_tol_mm,
    };
    let placed: Vec<(&str, &str, (f64, f64))> = planned
        .plan
        .plan
        .placements
        .iter()
        .map(|x| (x.reference.as_str(), x.feeder.as_str(), x.pick))
        .collect();
    let steps = tapesteps::walk(&placed, &ws.feeders, &limits);
    Ok((shown(&tapesteps::text(&steps)), tapesteps::csv(&steps)))
}

/// The effectiveness of a shift of the loaded plan on the simulated machine, as `shift-oee` says it: the report and the figures as a CSV. A plan with errors is not read.
pub fn shift_oee_text(
    ws: &Workspace,
    runs_count: usize,
    seed: u64,
    shift_minutes: u32,
    reel_change_s: f64,
    lang: Lang,
) -> Result<(String, String), String> {
    if !ws.can_plan() {
        return Err("(load a table of parts and a table of feeders)".to_string());
    }
    let planned = ws.make_plan().map_err(|e| e.localized(lang))?;
    if has_errors(&planned.checks) {
        return Err(PnpError::new(Code::PreflightFailed, "see the Problems tab").localized(lang));
    }
    let l = move |e: PnpError| e.localized(lang);
    let found = runs::run_many(&planned.plan, &ws.feeders, &ws.cfg, runs_count, seed).map_err(l)?;
    let used: Vec<&str> = planned
        .plan
        .plan
        .placements
        .iter()
        .map(|x| x.feeder.as_str())
        .collect();
    let reels: Vec<(u32, u32)> = runway::runway(&used, &ws.feeders)
        .iter()
        .filter(|r| r.per_board > 0)
        .map(|r| (r.per_board, r.count))
        .collect();
    let oee = shiftoee::compute(&shiftoee::Inputs {
        ideal_s: planned.plan.plan.seconds,
        runs: found,
        reels,
        shift_s: f64::from(shift_minutes) * 60.0,
        reel_change_s,
    })
    .map_err(l)?;
    Ok((shown(&oee.text(shift_minutes)), oee.csv()))
}

/// A position file of an assembler (or of another program) read into parts in millimetres and written in the shape and the unit asked for, as `import-assembler` does. The line that says how
/// many parts were read and the shape that was found, and the table.
pub fn import_assembler_text(
    text: &str,
    format: asmexport::Format,
    unit: asmexport::Unit,
    max_bytes: u64,
    lang: Lang,
) -> Result<(String, String), String> {
    sized("the position file", text, max_bytes)?;
    let found = asmimport::parse(text).map_err(|e| e.localized(lang))?;
    let out = asmexport::write(&found.parts, format, unit, asmexport::Sides::All);
    Ok((
        format!(
            "{} parts read (shape of the file: {})",
            found.parts.len(),
            found.dialect
        ),
        out,
    ))
}

/// Two bills of materials added up into one list to buy, each for its number of boards, as `bom-merge` does; a bill that was left empty is not part of it. The first lines of the report and
/// the list as a CSV.
pub fn bom_merge_text(
    first: &str,
    second: &str,
    boards: (u64, u64),
    max_bytes: u64,
    lang: Lang,
) -> Result<(String, String), String> {
    let mut inputs = Vec::new();
    for (name, text, n) in [("first", first, boards.0), ("second", second, boards.1)] {
        if text.trim().is_empty() {
            continue;
        }
        sized("the bill of materials", text, max_bytes)?;
        let bom = Bom::parse(text, 100_000).map_err(|e| e.localized(lang))?;
        inputs.push(bommerge::Input {
            name: name.to_string(),
            boards: n,
            bom,
        });
    }
    if inputs.is_empty() {
        return Err("(type or open at least one bill of materials)".to_string());
    }
    let lines = bommerge::merge(&inputs).map_err(|e| e.localized(lang))?;
    Ok((
        shown(&bommerge::text(&lines, &inputs)),
        bommerge::csv(&lines),
    ))
}
