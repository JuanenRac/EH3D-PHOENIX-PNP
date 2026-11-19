// EH3D-PHOENIX-PNP - the commands that talk to the EH3D server: server-send, server-status and server-job.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//
// They work only with `mode = server` in the settings (or, for server-send, with --sim, which talks to a server in memory), and never move anything here: the machine is the server's.
// The token of the settings is sent to the server and never printed.

use crate::cli_args::{finish, flag, num, switch};
use crate::cmd_tools::language_and_config;
use crate::commands::read_text;
use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::error::{Code, PnpError};
use eh3d_phoenix_pnp::feeding;
use eh3d_phoenix_pnp::parts::parse_all;
use eh3d_phoenix_pnp::report;
use eh3d_phoenix_pnp::server::{
    feeder_rows, feeders_request, placements, HttpTransport, JobInfo, Mode, Order, Remote,
    SimServer, Target, Transport,
};
use std::thread;
use std::time::Duration;

/// The way to the server of the settings, or why there is none (the program works locally, or the address or the machine is missing).
fn target(cfg: &MachineConfig, sim: bool) -> Result<Target, String> {
    if sim {
        let machine = if cfg.server_machine.is_empty() {
            "sim-1"
        } else {
            cfg.server_machine.as_str()
        };
        return Target::new(
            "sim",
            "",
            machine,
            cfg.server_timeout_s,
            cfg.server_poll_ms.into(),
            cfg.server_attempts,
        );
    }
    if cfg.mode != Mode::Server {
        return Err("mode is local in the settings: set mode = server (with server_address and server_machine) to talk to the server".into());
    }
    Target::new(
        &cfg.server_address,
        &cfg.server_token,
        &cfg.server_machine,
        cfg.server_timeout_s,
        cfg.server_poll_ms.into(),
        cfg.server_attempts,
    )
}

fn remote(t: &Target, sim: bool) -> Remote<Box<dyn Transport>> {
    let transport: Box<dyn Transport> = if sim {
        let mut s = SimServer::new(&[t.machine.as_str()]);
        s.steps_per_read = 50;
        Box::new(s)
    } else {
        Box::new(HttpTransport::new(t))
    };
    Remote::new(transport, t)
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

/// `server-send PARTS FEEDERS [--offset X,Y] [--keep-feeders] [--start] [--follow] [--timeout S] [--sim] [--partial]`: puts the table of feeders on the machine of the server (unless
/// `--keep-feeders`), prepares the placements as a job and, with `--start` or `--follow`, starts it; `--follow` reads the job until it is over. The parts that no feeder holds are told, and
/// with no `--partial` they stop the command. `--sim` does it all against a server in memory, to see what would be sent. The exit code is 1 when the job did not end `done`.
pub fn server_send(mut args: Vec<String>) -> Result<(), String> {
    let (_lang, cfg) = language_and_config(&mut args)?;
    let sim = switch(&mut args, "--sim");
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
    let made = placements(&parsed.parts, &rows, offset)?;
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
    let mut r = remote(&t, sim);
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
    println!(
        "job {} prepared: {} placements on {}",
        job.id, made.count, t.machine
    );
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
        Err(format!("the job did not end done: {}", end.line()))
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

/// `server-status [JOB]`: the machine of the settings (its state and the feeders the server holds) or, with a job id, how that job goes.
pub fn server_status(mut args: Vec<String>) -> Result<(), String> {
    let (_lang, cfg) = language_and_config(&mut args)?;
    finish(&args, 1)?;
    let t = target(&cfg, false)?;
    let mut r = remote(&t, false);
    if let Some(id) = args.first() {
        println!("{}", r.job(id)?.line());
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

/// `server-job ID --start | --pause | --resume | --cancel`: tells a job of the server what to do and prints how it stands.
pub fn server_job(mut args: Vec<String>) -> Result<(), String> {
    let (_lang, cfg) = language_and_config(&mut args)?;
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
    let [order] = orders.as_slice() else {
        return Err("say one of --start, --pause, --resume or --cancel".into());
    };
    let t = target(&cfg, false)?;
    let mut r = remote(&t, false);
    println!("{}", r.order(id, *order)?.line());
    Ok(())
}
