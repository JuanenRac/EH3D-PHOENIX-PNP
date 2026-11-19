// EH3D-PHOENIX-PNP - the commands that compare and count: parts-diff, config-diff, board-stats, queue-stock and feeder-deplete.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//
// Each one reads its files, calls the library and prints; none of them moves a machine or writes anything but the file named by --out, and a file that exists is never written over.

use crate::cli_args::{finish, flag, num};
use crate::cmd_queue::queue_dir;
use crate::cmd_tools::language_and_config;
use crate::commands::{read_text, refuse_same_file};
use eh3d_phoenix_pnp::config::{MachineConfig, MAX_CONFIG_BYTES};
use eh3d_phoenix_pnp::error::{Lang, PnpError};
use eh3d_phoenix_pnp::parts::{parse_all, Side};
use eh3d_phoenix_pnp::queue::{self, Kind};
use eh3d_phoenix_pnp::{boardstats, configdiff, deplete, feeding, partsdiff, queuestock, report};
use std::collections::BTreeMap;

/// `parts-diff FIRST SECOND [--tol MM] [--rot-tol DEG] [--out F]`: what changed between two tables of parts, part by part (added, removed, moved, turned, another value, another package,
/// the other side). The report goes to the screen, or the changes as a CSV to `--out` (never over a file that exists). The exit code is 1 when the tables differ.
pub fn parts_diff(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let mut settings = partsdiff::Settings::default();
    if let Some(v) = flag(&mut args, "--tol")? {
        settings.tol_mm = num(&v, "--tol")?;
    }
    if let Some(v) = flag(&mut args, "--rot-tol")? {
        settings.rot_tol_deg = num(&v, "--rot-tol")?;
    }
    let out = flag(&mut args, "--out")?;
    finish(&args, 2)?;
    let (a, b) = match (args.first(), args.get(1)) {
        (Some(a), Some(b)) => (a.clone(), b.clone()),
        _ => return Err("parts-diff needs two tables of parts".into()),
    };
    let l = |e: PnpError| e.localized(lang);
    let first = parse_all(&read_text(&a, cfg.max_file_bytes)?, &cfg.profile)?;
    let second = parse_all(&read_text(&b, cfg.max_file_bytes)?, &cfg.profile)?;
    let diff = partsdiff::diff(&first.parts, &second.parts, &settings).map_err(l)?;
    print!("{}", diff.text());
    if let Some(o) = &out {
        refuse_same_file(&[a, b], o)?;
        report::write_new(o, &diff.csv()).map_err(l)?;
        println!("written to {o}");
    }
    if diff.is_same() {
        Ok(())
    } else {
        Err(format!("{} parts differ", diff.parts_changed()))
    }
}

/// `config-diff FIRST SECOND [--lang CODE]`: the settings that two configuration files give differently, in what they mean (`yes` and `on`, `5` and `5.0`, and a line that gives the
/// default are the same as they are). The exit code is 1 when the machines differ.
pub fn config_diff(mut args: Vec<String>) -> Result<(), String> {
    let lang = match flag(&mut args, "--lang")? {
        Some(l) => Lang::from_code(&l)
            .ok_or_else(|| format!("--lang '{l}' is not one of en es de fr it ja zh pt"))?,
        None => Lang::En,
    };
    finish(&args, 2)?;
    let (a, b) = match (args.first(), args.get(1)) {
        (Some(a), Some(b)) => (a.clone(), b.clone()),
        _ => return Err("config-diff needs two configuration files".into()),
    };
    let first = read_text(&a, MAX_CONFIG_BYTES as u64)?;
    let second = read_text(&b, MAX_CONFIG_BYTES as u64)?;
    let diff = configdiff::diff(&first, &second).map_err(|(which, e)| {
        let path = if which == "first" { &a } else { &b };
        format!("{path}: {}", e.localized(lang))
    })?;
    print!("{}", diff.text());
    if diff.is_same() {
        Ok(())
    } else {
        Err(format!("{} settings differ", diff.changes.len()))
    }
}

/// `board-stats PARTS [--near MM] [--limit N]`: what the table holds (the parts on each side, the kinds, a line for each package, the box of the centres) and, with `--near`, the pairs of
/// parts of one side whose centres are closer than that distance. The exit code is 1 when `--near` finds a pair.
pub fn board_stats(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let near = match flag(&mut args, "--near")? {
        Some(v) => Some(num(&v, "--near")?),
        None => None,
    };
    let limit = match flag(&mut args, "--limit")? {
        Some(v) => v
            .trim()
            .parse::<usize>()
            .map_err(|_| format!("--limit '{v}' is not a whole number"))?,
        None => 20,
    };
    finish(&args, 1)?;
    let path = args.first().ok_or("board-stats needs a table of parts")?;
    let parsed = parse_all(&read_text(path, cfg.max_file_bytes)?, &cfg.profile)?;
    print!(
        "{}",
        boardstats::stats(&parsed.parts, parsed.dnp.len()).text()
    );
    let Some(within) = near else {
        return Ok(());
    };
    let pairs = boardstats::near(&parsed.parts, within).map_err(|e| e.localized(lang))?;
    print!("{}", boardstats::near_text(&pairs, within, limit));
    if pairs.is_empty() {
        Ok(())
    } else {
        Err(format!("{} pairs of parts are too near", pairs.len()))
    }
}

/// The most bytes of a queue file.
const MAX_QUEUE_BYTES: u64 = 1_048_576;

/// `queue-stock QUEUE [--out F] [--config F]`: whether the feeders hold enough parts for all the jobs of the queue that are still to be made, one after the other, and the job in which
/// each reel that falls short runs out. Jobs that share a table of feeders share its reels; a panel is not counted (the line says so). The exit code is 1 when a reel falls short.
pub fn queue_stock(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let out = flag(&mut args, "--out")?;
    finish(&args, 1)?;
    let path = args
        .first()
        .ok_or("queue-stock needs a queue file")?
        .clone();
    let l = |e: PnpError| e.localized(lang);
    // a queue file that is not there is not an empty queue: the name is most likely wrong
    if !std::path::Path::new(&path).is_file() {
        return Err(format!("{path}: there is no such queue file"));
    }
    let q = queue::read_file(&path, MAX_QUEUE_BYTES).map_err(l)?;
    let dir = queue_dir(&path);
    // the jobs that are still to be made, by the table of feeders they take from, in the order of the queue
    let mut groups: BTreeMap<String, Vec<queuestock::JobNeed>> = BTreeMap::new();
    let mut panels = 0usize;
    for e in q.to_do() {
        let boards = u64::from(e.boards.saturating_sub(e.done));
        if boards == 0 {
            continue;
        }
        if e.kind == Kind::Panel {
            panels += 1;
            continue;
        }
        let own = if e.config.trim().is_empty() {
            cfg.clone()
        } else {
            let p = queue::resolve(&e.config, &dir);
            MachineConfig::parse(&read_text(&p, MAX_CONFIG_BYTES as u64)?).map_err(l)?
        };
        let source = queue::resolve(&e.source, &dir);
        let parsed = parse_all(&read_text(&source, own.max_file_bytes)?, &own.profile)?;
        let side = if e.side.trim().eq_ignore_ascii_case("bottom") {
            Side::Bottom
        } else {
            Side::Top
        };
        groups
            .entry(queue::resolve(&e.feeders, &dir))
            .or_default()
            .push(queuestock::JobNeed {
                name: e.name.clone(),
                parts: parsed
                    .parts
                    .into_iter()
                    .filter(|p| p.side == side)
                    .collect(),
                boards,
            });
    }
    if panels > 0 {
        println!("{panels} panel jobs are not counted");
    }
    if groups.is_empty() {
        println!("nothing to count: no job of a table of parts is still to be made");
        return Ok(());
    }
    let mut all_csv = String::new();
    let mut short = 0usize;
    for (feeders_path, jobs) in &groups {
        let feeders =
            feeding::from_csv_with(&read_text(feeders_path, cfg.max_file_bytes)?, cfg.tray_fill)
                .map_err(l)?;
        let kinds = queuestock::needs(jobs, &feeders);
        println!("feeders {feeders_path}:");
        print!("{}", queuestock::text(&kinds, jobs.len()));
        short += kinds.iter().filter(|k| k.short > 0).count();
        for (i, line) in queuestock::csv(&kinds).lines().enumerate() {
            if i == 0 {
                if all_csv.is_empty() {
                    all_csv.push_str(&format!("feeders,{line}\n"));
                }
            } else {
                all_csv.push_str(&format!(
                    "{},{line}\n",
                    eh3d_phoenix_pnp::parts::csv_field(feeders_path)
                ));
            }
        }
    }
    if let Some(o) = &out {
        let mut inputs = vec![path.clone()];
        inputs.extend(groups.keys().cloned());
        refuse_same_file(&inputs, o)?;
        report::write_new(o, &all_csv).map_err(l)?;
        println!("written to {o}");
    }
    if short == 0 {
        Ok(())
    } else {
        Err(format!("{short} kinds of part fall short for the queue"))
    }
}

/// `feeder-deplete FEEDERS --state STATE [--out F]`: the table of feeders as it stands after the job that the saved state (`simulate --state`) is of: the count of each feeder is what is left
/// and the next place of a strip moves along. Without `--out` it only says what each feeder gave; with it the table is written (never over a file that exists, and never over the table
/// that was read). The exit code is 1 when a feeder is low or empty.
pub fn feeder_deplete(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let state = flag(&mut args, "--state")?.ok_or("feeder-deplete needs --state FILE")?;
    let out = flag(&mut args, "--out")?;
    finish(&args, 1)?;
    let feeders = args
        .first()
        .ok_or("feeder-deplete needs a table of feeders")?
        .clone();
    let l = |e: PnpError| e.localized(lang);
    let result = deplete::deplete(
        &read_text(&feeders, cfg.max_file_bytes)?,
        &read_text(&state, cfg.max_file_bytes)?,
    )
    .map_err(l)?;
    print!("{}", result.text());
    if let Some(o) = &out {
        refuse_same_file(&[feeders, state], o)?;
        report::write_new(o, &result.csv).map_err(l)?;
        println!("written to {o}");
    }
    if result.low() == 0 {
        Ok(())
    } else {
        Err(format!("{} feeders are low or empty", result.low()))
    }
}
