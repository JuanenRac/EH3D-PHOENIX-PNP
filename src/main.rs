// EH3D-PNP-MANAGER - command line.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
#![forbid(unsafe_code)]

use eh3d_pnp_manager::feeders::{assign, from_csv as feeders_csv};
use eh3d_pnp_manager::parts::{parse, Side};
use eh3d_pnp_manager::plan::{plan, Settings};
use std::env;
use std::fs;
use std::process::ExitCode;

const HELP: &str = "eh3d-pnp: pick and place assembly

  eh3d-pnp parts PARTS                      the parts of a position file (KiCad .pos or CSV) in millimetres
  eh3d-pnp check PARTS FEEDERS              which parts have no feeder and which feeders will run out
  eh3d-pnp plan PARTS FEEDERS OUT.json [--nozzles 2] [--side top|bottom] [--board-width MM] [--speed MM_S] [--partial]

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
    fs::read(path)
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .map_err(|e| format!("{path}: {e}"))
}

fn run(mut args: Vec<String>) -> Result<(), String> {
    if args.is_empty() || matches!(args[0].as_str(), "help" | "--help" | "-h") {
        println!("{HELP}");
        return Ok(());
    }
    let cmd = args.remove(0);
    match cmd.as_str() {
        "parts" => {
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
                s.nozzles = num(&v, "--nozzles")? as usize;
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
            let parts = parse(&read(&args[0])?)?;
            let feeders = feeders_csv(&read(&args[1])?)?;
            let a = assign(&parts, &feeders);
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
            fs::write(out, p.to_json()).map_err(|e| format!("{out}: {e}"))?;
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
