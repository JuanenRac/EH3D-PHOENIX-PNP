// EH3D-PHOENIX-PNP - command line.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
#![forbid(unsafe_code)]

use eh3d_phoenix_pnp::feeders::{assign, assign_side, from_csv as feeders_csv};
use eh3d_phoenix_pnp::parts::{parse, Side};
use eh3d_phoenix_pnp::plan::{plan, Settings};
use std::env;
use std::fs;
use std::process::ExitCode;

const HELP: &str = "phoenix-pnp: pick and place assembly

  phoenix-pnp parts PARTS                      the parts of a position file (KiCad .pos or CSV) in millimetres
  phoenix-pnp check PARTS FEEDERS              which parts have no feeder and which feeders will run out
  phoenix-pnp plan PARTS FEEDERS OUT.json [--nozzles 2] [--side top|bottom] [--board-width MM] [--speed MM_S] [--partial]

FEEDERS is a CSV with the header id,value,package,x,y,count. plan refuses to write when a part has no feeder or a feeder runs out, unless --partial is given
(then only the parts that can be placed are in the plan). The bottom side needs --board-width to be mirrored.";

fn flag(args: &mut Vec<String>, name: &str) -> Result<Option<String>, String> {
    if let Some(i) = args.iter().position(|a| a == name) {
        let v = args
            .get(i + 1)
            .ok_or_else(|| format!("{name} needs a value"))?
            .clone();
        args.drain(i..=i + 1);
        return Ok(Some(v));
    }
    Ok(None)
}

fn num(text: &str, what: &str) -> Result<f64, String> {
    text.trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| format!("{what} '{text}' is not a number"))
}

fn read(path: &str) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    match String::from_utf8(bytes) {
        Ok(text) => Ok(text),
        Err(e) => {
            // the suite reads such text as Latin-1; saying so keeps a value with an omega or a micro sign from silently not matching
            eprintln!("note: {path}: the text is not UTF-8 and was read as Latin-1");
            Ok(e.into_bytes().iter().map(|&b| char::from(b)).collect())
        }
    }
}

/// What is left of the arguments once the options and the wanted words are taken must be nothing more than `max` words: an option that is written wrong or an extra word is an
/// error, not something to ignore (the program would then make its output with the default values).
fn finish(args: &[String], max: usize) -> Result<(), String> {
    if let Some(o) = args.iter().find(|a| a.starts_with("--")) {
        return Err(format!("{o}: unknown option (see help)"));
    }
    if args.len() > max {
        return Err(format!("{}: unexpected argument (see help)", args[max]));
    }
    Ok(())
}

fn run(mut args: Vec<String>) -> Result<(), String> {
    if args.is_empty() || matches!(args[0].as_str(), "help" | "--help" | "-h") {
        println!("{HELP}");
        return Ok(());
    }
    let cmd = args.remove(0);
    match cmd.as_str() {
        "parts" => {
            finish(&args, 1)?;
            for p in parse(&read(args.first().ok_or("parts needs a file")?)?)? {
                println!(
                    "{:<8} {:<14} {:<16} {:>9.3} {:>9.3} {:>6.1}  {:?}",
                    p.reference, p.value, p.package, p.x, p.y, p.rotation, p.side
                );
            }
            Ok(())
        }
        "check" | "plan" => {
            let mut s = Settings::default();
            if let Some(v) = flag(&mut args, "--nozzles")? {
                let n = num(&v, "--nozzles")?;
                if n.fract() != 0.0 || !(1.0..=6.0).contains(&n) {
                    return Err(format!("--nozzles {v} is not a whole number from 1 to 6"));
                }
                s.nozzles = n as usize;
            }
            if let Some(v) = flag(&mut args, "--speed")? {
                s.speed_mm_s = num(&v, "--speed")?;
            }
            if let Some(v) = flag(&mut args, "--board-width")? {
                s.board_width_mm = Some(num(&v, "--board-width")?);
            }
            if let Some(v) = flag(&mut args, "--side")? {
                s.side = match v.as_str() {
                    "top" => Side::Top,
                    "bottom" => Side::Bottom,
                    other => return Err(format!("--side '{other}' is not top or bottom")),
                };
            }
            let partial = if let Some(i) = args.iter().position(|a| a == "--partial") {
                args.remove(i);
                true
            } else {
                false
            };
            if args.len() < 2 {
                return Err(format!("{cmd} needs the parts and the feeders"));
            }
            finish(&args, if cmd == "check" { 2 } else { 3 })?;
            let parts = parse(&read(&args[0])?)?;
            let feeders = feeders_csv(&read(&args[1])?)?;
            // a plan is for one side: the other side's parts neither need feeders nor use them up
            let a = if cmd == "plan" {
                assign_side(&parts, &feeders, Some(s.side))
            } else {
                assign(&parts, &feeders)
            };
            for r in &a.without_feeder {
                println!("no feeder for {r}");
            }
            for (id, n) in &a.short {
                println!("feeder {id} is short of {n} parts");
            }
            let clean = a.without_feeder.is_empty() && a.short.is_empty();
            if cmd == "check" {
                return if clean {
                    println!("every part has a feeder with enough parts");
                    Ok(())
                } else {
                    Err("the feeders do not cover the board".into())
                };
            }
            if !clean && !partial {
                return Err("the feeders do not cover the board (use --partial to place only what can be placed)".into());
            }
            let out = args.get(2).ok_or("plan needs an output file")?;
            let p = plan(&parts, &feeders, &a, &s)?;
            // written beside and renamed, so that a failure leaves the old plan whole
            let tmp = format!("{out}.tmp");
            fs::write(&tmp, p.to_json())
                .and_then(|()| fs::rename(&tmp, out))
                .map_err(|e| {
                    let _ = fs::remove_file(&tmp);
                    format!("{out}: {e}")
                })?;
            println!(
                "{} parts in {} batches, {:.0} mm of travel, about {:.0} s",
                p.placements.len(),
                p.batches,
                p.travel_mm,
                p.seconds
            );
            Ok(())
        }
        other => Err(format!("{other}: unknown command (see help)")),
    }
}

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}
