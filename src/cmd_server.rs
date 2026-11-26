// EH3D-PHOENIX-PNP - the commands that talk to the EH3D server: server-send, server-status, server-job, server-feeders and server-calibrate.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//
// They work only with `mode = server` in the settings (or, for server-send, with --sim, which talks to a server in memory), and never move anything here: the machine is the server's.
// The token of the settings is sent to the server and never printed.

use crate::cli_args::{finish, flag, num, switch};
use crate::cmd_nodes::fail;
use crate::cmd_tools::language_and_config;
use crate::commands::read_text;
use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::error::{Code, Lang, PnpError};
use eh3d_phoenix_pnp::feeding;
use eh3d_phoenix_pnp::parts::parse_all;
use eh3d_phoenix_pnp::report;
use eh3d_phoenix_pnp::server::{
    self, calibrate_request, dry_run, feeder_rows, feeders_request, placements, Calibrate, JobInfo,
    Order, Remote, Target, Transport,
};
use std::thread;
use std::time::Duration;

/// The way to the server of the settings (see `server::target_for`).
fn target(cfg: &MachineConfig, sim: bool) -> Result<Target, String> {
    server::target_for(cfg, sim)
}

/// The machine of the target, in the language of the command.
fn remote(t: &Target, sim: bool, lang: Lang) -> Result<Remote<Box<dyn Transport>>, String> {
    server::connect(t, sim, lang.code())
}

/// Writes `text` to `out` when it is given (never over a file that exists), else prints it.
fn deliver(text: &str, out: Option<&str>, lang: Lang) -> Result<(), String> {
    match out {
        Some(o) => {
            report::write_new(o, text).map_err(|e| e.localized(lang))?;
            println!("written to {o}");
        }
        None => print!("{text}"),
    }
    Ok(())
}

fn pause(t: &Target, sim: bool) -> impl FnMut() {
    let ms = if sim { 0 } else { t.poll_ms };
    move || {
        if ms > 0 {
            thread::sleep(Duration::from_millis(ms));
        }
    }
}

/// The reading of a job is printed when its state changes or it gets to another tenth of the work.
fn print_progress(last: &mut Option<(String, u64)>, info: &JobInfo) {
    let key = (info.state.clone(), info.percent() / 10);
    if last.as_ref() != Some(&key) {
        println!("{}", info.line());
        *last = Some(key);
    }
}

/// `server-send PARTS FEEDERS [--offset X,Y] [--keep-feeders] [--start] [--follow] [--timeout S] [--sim] [--partial] [--dry-run]`: puts the table of feeders on the machine of the server (unless
/// `--keep-feeders`), prepares the placements as a job and, with `--start` or `--follow`, starts it; `--follow` reads the job until it is over. The parts that no feeder holds are told, and
/// with no `--partial` they stop the command. `--dry-run` asks the server to go through the job without taking a part off a feeder. `--sim` does it all against a server in memory, to see what
/// would be sent. The exit code is 2 when the job did not end `done`.
pub fn server_send(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let sim = switch(&mut args, "--sim");
    let dry = switch(&mut args, "--dry-run");
    let keep = switch(&mut args, "--keep-feeders");
    let follow = switch(&mut args, "--follow");
    let start = switch(&mut args, "--start") || follow;
    let partial = switch(&mut args, "--partial");
    let offset = match flag(&mut args, "--offset")? {
        Some(v) => {
            let (x, y) = v
                .split_once(',')
                .ok_or_else(|| format!("--offset {v} is not X,Y"))?;
            (num(x, "--offset X")?, num(y, "--offset Y")?)
        }
        None => (0.0, 0.0),
    };
    let timeout = match flag(&mut args, "--timeout")? {
        Some(v) => {
            let s = num(&v, "--timeout")?;
            if !(1.0..=86_400.0).contains(&s) {
                return Err(format!("--timeout {v} is not from 1 to 86400 seconds"));
            }
            s
        }
        None => 3600.0,
    };
    finish(&args, 2)?;
    let (parts_path, feeders_path) = match (args.first(), args.get(1)) {
        (Some(a), Some(b)) => (a.clone(), b.clone()),
        _ => return Err("server-send needs a table of parts and a table of feeders".into()),
    };
    let t = target(&cfg, sim)?;
    let parsed = parse_all(&read_text(&parts_path, cfg.max_file_bytes)?, &cfg.profile)?;
    let feeders = feeding::from_csv_with(
        &read_text(&feeders_path, cfg.max_file_bytes)?,
        cfg.tray_fill,
    )
    .map_err(|e| e.localized(cfg.language))?;
    let rows = feeder_rows(&feeders)?;
    let mut made = placements(&parsed.parts, &rows, offset)?;
    if dry {
        made.request = dry_run(made.request);
    }
    if !made.unmatched.is_empty() {
        let list = made.unmatched.join(", ");
        if !partial {
            return Err(format!(
                "{} parts have no feeder that holds their value and package ({list}); use --partial to send the others",
                made.unmatched.len()
            ));
        }
        println!("left out, no feeder holds them: {list}");
    }
    if made.count == 0 {
        return Err("no part is left to send".into());
    }
    let max_polls = ((timeout * 1000.0 / t.poll_ms.max(1) as f64) as u32).max(2);
    let mut r = remote(&t, sim, lang)?;
    let mut wait = pause(&t, sim);
    if !keep {
        let load = r.create("feeders", &feeders_request(&rows, true))?;
        r.order(&load.id, Order::Start)?;
        let done = r.follow(&load.id, max_polls, &mut |_| {}, &mut wait)?;
        if done.state != "done" {
            return Err(format!(
                "the table of feeders was not taken: {}",
                done.line()
            ));
        }
        println!("feeders: {} slots loaded on {}", rows.len(), t.machine);
    }
    let name = std::path::Path::new(&parts_path)
        .file_stem()
        .map_or_else(|| "board".to_string(), |s| s.to_string_lossy().into_owned());
    let job = r.create(&name, &made.request)?;
    let hash_line = job.authorization_text(r.lang);
    println!(
        "job {} prepared: {} placements on {}{}",
        job.id,
        made.count,
        t.machine,
        if dry {
            " (dry run: no part is taken off a feeder)"
        } else {
            ""
        }
    );
    println!("{hash_line}");
    if !start {
        println!(
            "it waits, queued, until it is started (server-job {} --start)",
            job.id
        );
        return Ok(());
    }
    r.order(&job.id, Order::Start)?;
    if !follow {
        println!("job {} started", job.id);
        return Ok(());
    }
    let mut last = None;
    let end = r.follow(
        &job.id,
        max_polls,
        &mut |i| print_progress(&mut last, i),
        &mut wait,
    )?;
    if end.state == "done" {
        Ok(())
    } else {
        fail(format!("the job did not end done: {}", end.line()))
    }
}

/// `server-request PARTS FEEDERS --out DIR [--offset X,Y] [--partial]`: writes the two requests that `server-send` would make, `feeders.json` and `place.json`, in the folder `DIR` (made if it is not
/// there; a file that exists is never written over), for other programs that talk to the server. Nothing is sent and no server is needed.
pub fn server_request(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let partial = switch(&mut args, "--partial");
    let offset = match flag(&mut args, "--offset")? {
        Some(v) => {
            let (x, y) = v
                .split_once(',')
                .ok_or_else(|| format!("--offset {v} is not X,Y"))?;
            (num(x, "--offset X")?, num(y, "--offset Y")?)
        }
        None => (0.0, 0.0),
    };
    let dir = flag(&mut args, "--out")?.ok_or("server-request needs --out DIR")?;
    finish(&args, 2)?;
    let [parts_path, feeders_path] = args.as_slice() else {
        return Err("server-request needs a table of parts and a table of feeders".into());
    };
    let parsed = parse_all(&read_text(parts_path, cfg.max_file_bytes)?, &cfg.profile)?;
    let feeders =
        feeding::from_csv_with(&read_text(feeders_path, cfg.max_file_bytes)?, cfg.tray_fill)
            .map_err(|e| e.localized(lang))?;
    let rows = feeder_rows(&feeders)?;
    let made = placements(&parsed.parts, &rows, offset)?;
    if !made.unmatched.is_empty() && !partial {
        return Err(format!(
            "{} parts have no feeder that holds their value and package ({}); use --partial to leave them out",
            made.unmatched.len(),
            made.unmatched.join(", ")
        ));
    }
    if made.count == 0 {
        return Err("no part is left to send".into());
    }
    let pretty = |v: &serde_json::Value| {
        serde_json::to_string_pretty(v)
            .map(|t| {
                t + "
"
            })
            .map_err(|e| e.to_string())
    };
    let (f, p) = (
        std::path::Path::new(&dir).join("feeders.json"),
        std::path::Path::new(&dir).join("place.json"),
    );
    if f.exists() || p.exists() {
        return Err(PnpError::new(Code::ReportExists, dir.as_str()).localized(lang));
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("{dir}: {e}"))?;
    report::write_new(
        &f.to_string_lossy(),
        &pretty(&feeders_request(&rows, true))?,
    )
    .map_err(|e| e.localized(lang))?;
    report::write_new(&p.to_string_lossy(), &pretty(&made.request)?)
        .map_err(|e| e.localized(lang))?;
    println!(
        "{} feeders and {} placements written to {dir}",
        rows.len(),
        made.count
    );
    Ok(())
}

/// `server-discover [--sim]`: looks for servers on the local network and prints the `host:port` of each that answers. It asks nothing of the network unless `server_mdns = on` is in the
/// settings, and it needs a build with the feature `mdns`; `--sim` answers with a server in memory so that the rest can be tried without a network.
pub fn server_discover(mut args: Vec<String>) -> Result<(), String> {
    let (_, cfg) = language_and_config(&mut args)?;
    let sim = switch(&mut args, "--sim");
    finish(&args, 0)?;
    let found = server::discover(&cfg, sim)?;
    if found.is_empty() {
        println!("no server answered");
    }
    for address in &found {
        println!("{address}");
    }
    Ok(())
}

/// `server-status [JOB]`: the machine of the settings (its state and the feeders the server holds) or, with a job id, how that job goes.
pub fn server_status(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    finish(&args, 1)?;
    let t = target(&cfg, false)?;
    let mut r = remote(&t, false, lang)?;
    if let Some(id) = args.first() {
        let job = r.job(id)?;
        println!("{}", job.line());
        println!("{}", job.authorization_text(r.lang));
        return Ok(());
    }
    let m = r.machine()?;
    println!(
        "{}: {} ({}), {} feeders",
        m.id,
        m.state,
        m.plugin,
        m.feeders.len()
    );
    for (slot, part, count) in &m.feeders {
        println!("  slot {slot}: {part}, {count} left");
    }
    Ok(())
}

/// `server-job ID --authorize HASH | --start [--hash HASH] | --pause | --resume | --cancel`: tells a job of the server what to do and prints how it stands. `--authorize` takes the hash of
/// the server that `server-status ID` shows (never the one of `toolpath-hash`): the job is asked for again and nothing is authorised when it is no longer that one. `--start --hash`
/// names the hash with the start.
pub fn server_job(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let authorize = flag(&mut args, "--authorize")?;
    let named = flag(&mut args, "--hash")?;
    let mut orders = Vec::new();
    for (word, order) in [
        ("--start", Order::Start),
        ("--pause", Order::Pause),
        ("--resume", Order::Resume),
        ("--cancel", Order::Cancel),
    ] {
        if switch(&mut args, word) {
            orders.push(order);
        }
    }
    finish(&args, 1)?;
    let id = args.first().ok_or("server-job needs the id of a job")?;
    let order = match (authorize.is_some(), orders.as_slice()) {
        (true, []) => None,
        (false, [order]) => Some(*order),
        _ => {
            return Err(
                "say one of --authorize HASH, --start, --pause, --resume or --cancel".into(),
            )
        }
    };
    if named.is_some() && order != Some(Order::Start) {
        return Err("--hash goes with --start".into());
    }
    let t = target(&cfg, false)?;
    let mut r = remote(&t, false, lang)?;
    let job = match order {
        None => r.authorize(id, authorize.as_deref().unwrap_or(""))?,
        Some(Order::Start) => r.start(id, named.as_deref())?,
        Some(other) => r.order(id, other)?,
    };
    println!("{}", job.line());
    if order.is_none() {
        println!("{}", job.authorization_text(r.lang));
    }
    Ok(())
}

/// A whole number from the text of a word of a command.
fn whole(text: &str, what: &str, hi: u64) -> Result<u64, String> {
    text.trim()
        .parse::<u64>()
        .ok()
        .filter(|n| *n <= hi)
        .ok_or_else(|| format!("{what} '{text}' is not a whole number from 0 to {hi}"))
}

/// `server-feeders [list | set SLOT PART COUNT | clear SLOT] [--sim] [--out F]`: the feeders that the machine of the server holds, with where each is picked from and its calibration (`list`, the
/// default), a slot declared or refilled at once with no job (`set`) or a slot emptied (`clear`); `set` and `clear` print the table after the change. `--out` writes the table as a CSV, never over
/// a file that exists. A machine that is running a job refuses a change. The exit code is 2 when a feeder holds no parts.
pub fn server_feeders(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let sim = switch(&mut args, "--sim");
    let out = flag(&mut args, "--out")?;
    finish(&args, 4)?;
    let t = target(&cfg, sim)?;
    let mut r = remote(&t, sim, lang)?;
    let word = args.first().cloned().unwrap_or_else(|| "list".to_string());
    let table = match (word.as_str(), args.len()) {
        ("list", 0 | 1) => r.feeders_table()?,
        ("set", 4) => {
            let slot = whole(&args[1], "the slot", u64::from(u32::MAX))? as u32;
            let count = whole(&args[3], "the count", u64::MAX)?;
            r.set_feeder(slot, &args[2], count)?
        }
        ("clear", 2) => {
            let slot = whole(&args[1], "the slot", u64::from(u32::MAX))? as u32;
            r.clear_feeder(slot)?
        }
        ("list" | "set" | "clear", _) => {
            return Err("server-feeders: list, set SLOT PART COUNT or clear SLOT".into())
        }
        (other, _) => return Err(format!("{other}: say list, set or clear")),
    };
    match out.as_deref() {
        Some(_) => deliver(&table.csv(), out.as_deref(), lang)?,
        None => print!("{}", table.text()),
    }
    match table.empty_count() {
        0 => Ok(()),
        n => fail(format!("{n} feeders hold no parts")),
    }
}

/// `server-calibrate feeder|nozzle|camera TARGET [--offset X,Y] [--timeout S] [--sim]`: makes a calibration job on the machine of the server, starts it and follows it to its end. The target of a
/// feeder is its slot; the one of a nozzle or a camera is a name. The offset (up to 5 mm on each axis, 0 when it is not given) is the one the command brings: the machine of the server measures
/// nothing today. For a feeder the table is printed after the job, so that the calibration can be read. The exit code is 2 when the job did not end `done`.
pub fn server_calibrate(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let sim = switch(&mut args, "--sim");
    let offset = match flag(&mut args, "--offset")? {
        Some(v) => {
            let (x, y) = v
                .split_once(',')
                .ok_or_else(|| format!("--offset {v} is not X,Y"))?;
            (num(x, "--offset X")?, num(y, "--offset Y")?)
        }
        None => (0.0, 0.0),
    };
    let timeout = match flag(&mut args, "--timeout")? {
        Some(v) => {
            let s = num(&v, "--timeout")?;
            if !(1.0..=86_400.0).contains(&s) {
                return Err(format!("--timeout {v} is not from 1 to 86400 seconds"));
            }
            s
        }
        None => 60.0,
    };
    finish(&args, 2)?;
    let [what, target_word] = args.as_slice() else {
        return Err("server-calibrate needs feeder, nozzle or camera and a target".into());
    };
    let what =
        Calibrate::parse(what).ok_or_else(|| format!("{what} is not feeder, nozzle or camera"))?;
    let request = calibrate_request(what, target_word, offset)?;
    let t = target(&cfg, sim)?;
    let max_polls = ((timeout * 1000.0 / t.poll_ms.max(1) as f64) as u32).max(2);
    let mut r = remote(&t, sim, lang)?;
    let mut wait = pause(&t, sim);
    let job = r.create(&format!("calibrate-{}", what.word()), &request)?;
    r.order(&job.id, Order::Start)?;
    let mut last = None;
    let end = r.follow(
        &job.id,
        max_polls,
        &mut |i| print_progress(&mut last, i),
        &mut wait,
    )?;
    if what == Calibrate::Feeder && end.state == "done" {
        print!("{}", r.feeders_table()?.text());
    }
    if end.state == "done" {
        Ok(())
    } else {
        fail(format!("the calibration did not end done: {}", end.line()))
    }
}
