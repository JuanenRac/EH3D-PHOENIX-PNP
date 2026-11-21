// EH3D-PHOENIX-PNP - the commands of the banks of feeders, of the composition of a panel and of the link to the nodes: slot-bank, panel-compose and node-sim.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//
// Each one reads its files, calls the library and prints; none of them moves a machine, and a file named by --out that exists is never written over. They end with the exit code 2 when a
// check of theirs is not passed (a bank that is not ready, a panel that does not fit, a link that was held), and with 1 when the arguments or the files are wrong.

use crate::cli_args::{finish, flag, num};
use crate::cmd_tools::language_and_config;
use crate::commands::read_text;
use eh3d_phoenix_pnp::nodelink::{Feeders, Head, ScNodeLink};
use eh3d_phoenix_pnp::nodesim::{Injection, SimFeeders, SimHead};
use eh3d_phoenix_pnp::panelcomp::Composition;
use eh3d_phoenix_pnp::report;
use eh3d_phoenix_pnp::revision;
use eh3d_phoenix_pnp::slotbank::{Bank, Geometry, Layout, State};
use std::sync::atomic::{AtomicBool, Ordering};

static CHECK_FAILED: AtomicBool = AtomicBool::new(false);

/// The error of a check that was not passed: the message, and the exit code of the program becomes 2.
pub fn not_passed(message: impl Into<String>) -> String {
    CHECK_FAILED.store(true, Ordering::SeqCst);
    message.into()
}

/// The exit code for an error: 2 when a check was not passed, else 1.
pub fn exit_code() -> u8 {
    if CHECK_FAILED.load(Ordering::SeqCst) {
        2
    } else {
        1
    }
}

fn bounded(text: &str, what: &str, lo: f64, hi: f64) -> Result<f64, String> {
    let v = num(text, what)?;
    if !(lo..=hi).contains(&v) {
        return Err(format!("{what} {text} is not from {lo} to {hi}"));
    }
    Ok(v)
}

fn refuse_existing(out: Option<&str>) -> Result<(), String> {
    match out {
        Some(o) if std::path::Path::new(o).exists() => {
            Err(format!("{o}: exists and is never written over"))
        }
        _ => Ok(()),
    }
}

/// `table-revision FILE`: the revision of a table, to hold a later write to it (`feeder-refill --out FILE --revision TOKEN`).
pub fn table_revision(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    finish(&args, 1)?;
    let path = args.first().ok_or("table-revision needs a table")?;
    let (_, token) = revision::read(path, cfg.max_file_bytes).map_err(|e| e.localized(lang))?;
    println!("{token}");
    Ok(())
}

/// `slot-bank TABLE [--tape MM] [--out F]`: the banks of feeders as the table `id,bank,first_slot,tape_mm,state,chain` puts them: a picture of each bank, the slots each feeder takes, and
/// where a tape of `--tape` millimetres would fit. The sizes of the slots are the `slot_*` settings. The exit code is 2 when a feeder is depleted or jammed.
pub fn slot_bank(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let tape = flag(&mut args, "--tape")?
        .map(|v| bounded(&v, "--tape", 2.0, 104.0))
        .transpose()?;
    let out = flag(&mut args, "--out")?;
    finish(&args, 1)?;
    refuse_existing(out.as_deref())?;
    let path = args.first().ok_or("slot-bank needs a table of the banks")?;
    let geo = Geometry::from_config(&cfg);
    let layout = Layout::from_csv(&read_text(path, cfg.max_file_bytes)?, geo)
        .map_err(|e| e.localized(lang))?;
    print!("{}", layout.text());
    if let Some(mm) = tape {
        let width = geo.slots_for(mm).map_err(|e| e.localized(lang))?;
        println!("a tape of {mm} mm takes {width} slots");
        for bank in Bank::ALL {
            match layout.first_fit(bank, mm) {
                Some(first) => println!(
                    "{} bank: it fits from slot {first} to {}",
                    bank.name(),
                    first + width - 1
                ),
                None => println!("{} bank: it does not fit", bank.name()),
            }
        }
    }
    if let Some(o) = &out {
        report::write_new(o, &layout.to_csv()).map_err(|e| e.localized(lang))?;
        println!("written to {o}");
    }
    let stopped = layout
        .entries()
        .iter()
        .filter(|e| matches!(e.state, State::Depleted | State::Jammed))
        .count();
    match stopped {
        0 => Ok(()),
        n => Err(not_passed(format!("{n} feeders are depleted or jammed"))),
    }
}

/// `panel-compose SPEC [--clearance MM] [--out F]`: checks the composition of a panel (the sheet, its rails, the outlines of the boards and where each unit goes and how it is turned) and
/// writes the panel file that the other commands read to `--out`. Nothing is written when a unit goes out of the area between the rails, two units overlap or two are closer than
/// `--clearance` (`panel_clearance_mm`). The exit code is 2 then.
pub fn panel_compose(mut args: Vec<String>) -> Result<(), String> {
    let (lang, mut cfg) = language_and_config(&mut args)?;
    if let Some(v) = flag(&mut args, "--clearance")? {
        cfg.panel_clearance_mm = bounded(&v, "--clearance", 0.0, 50.0)?;
    }
    let out = flag(&mut args, "--out")?;
    finish(&args, 1)?;
    refuse_existing(out.as_deref())?;
    let path = args
        .first()
        .ok_or("panel-compose needs a file of the composition")?;
    let comp =
        Composition::parse(&read_text(path, cfg.max_file_bytes)?).map_err(|e| e.localized(lang))?;
    print!("{}", comp.text(cfg.panel_clearance_mm));
    let findings = comp.check(cfg.panel_clearance_mm);
    if !findings.is_empty() {
        return Err(not_passed(format!(
            "{} problems: no panel file is written",
            findings.len()
        )));
    }
    match &out {
        Some(o) => {
            report::write_new(o, &comp.to_panel_json()).map_err(|e| e.localized(lang))?;
            println!("written to {o}");
        }
        None => print!("{}", comp.to_panel_json()),
    }
    Ok(())
}

/// `node-sim [--steps N] [--seed S] [--silence PM] [--unauthorized PM] [--interlock PM]`: a job on a head node and a feeder node that are made up, with faults drawn from the seed (per thousand
/// requests). Each step asks the head whether the machine may move, asks for the permission to move, advances a feeder, picks and places with a nozzle, and lets the permission go. It says
/// what the link saw. The same seed gives the same text. The exit code is 2 when the link was held.
pub fn node_sim(mut args: Vec<String>) -> Result<(), String> {
    let (_, cfg) = language_and_config(&mut args)?;
    let steps = flag(&mut args, "--steps")?
        .map(|v| crate::cmd_flow::whole(&v, "--steps", 1.0, 10_000.0))
        .transpose()?
        .unwrap_or(20);
    let seed = flag(&mut args, "--seed")?
        .map(|v| crate::cmd_flow::whole(&v, "--seed", 1.0, 1e15))
        .transpose()?
        .unwrap_or(1);
    let mut rate = |name: &str| -> Result<u32, String> {
        Ok(flag(&mut args, name)?
            .map(|v| crate::cmd_flow::whole(&v, name, 0.0, 1000.0))
            .transpose()?
            .unwrap_or(0) as u32)
    };
    let (silence, unauthorized, interlock) = (
        rate("--silence")?,
        rate("--unauthorized")?,
        rate("--interlock")?,
    );
    finish(&args, 0)?;
    let head_sim = SimHead::new(2, "t");
    let feed_sim = SimFeeders::new(4, Some(1000), "t");
    head_sim.set_injection(Injection::seeded(seed, silence, unauthorized, interlock));
    feed_sim.set_injection(Injection::seeded(seed ^ 0x9E37, silence, unauthorized, 0));
    let retries = 0;
    let link = |s: Box<dyn eh3d_serverclient::Transport>| {
        ScNodeLink::new("sim:1", "t", cfg.node_timeout_s, retries, s).without_waiting()
    };
    let mut head = Head::new(link(Box::new(head_sim.clone())), cfg.node_silence_limit);
    let mut feeders = Feeders::new(link(Box::new(feed_sim.clone())), cfg.node_silence_limit);
    let (mut moved, mut refused, mut skipped) = (0u32, 0u32, 0u32);
    for step in 1..=steps {
        if !head.xy_clear() {
            skipped += 1;
            println!(
                "step {step}: the head is not clear or cannot be heard: the machine does not move"
            );
            continue;
        }
        match head.set_xy_moving(true) {
            Ok(()) => {
                moved += 1;
                let _ = feeders.advance(1, 1);
                let _ = head.set_xy_moving(false);
                let picked = head.pick(1);
                let placed = head.place(1);
                println!(
                    "step {step}: moved; pick {}, place {}",
                    picked.is_ok(),
                    placed.is_ok()
                );
            }
            Err(e) => {
                refused += 1;
                println!("step {step}: the permission was refused: {e}");
            }
        }
    }
    println!(
        "{steps} steps: {moved} moved, {refused} refused, {skipped} not clear; the head answered {} requests, the feeders {}; head {}, feeders {}",
        head_sim.requests(),
        feed_sim.requests(),
        head.guard().held().map_or("not held", |h| h.name()),
        feeders.guard().held().map_or("not held", |h| h.name())
    );
    if head.guard().held().is_some() || feeders.guard().held().is_some() {
        return Err(not_passed("the link to a node was held"));
    }
    Ok(())
}
