// EH3D-PHOENIX-PNP - command line.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
#![forbid(unsafe_code)]

mod cli_args;
mod commands;

use cli_args::{finish, flag, num, switch};
use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::feeders::{assign, assign_side, from_csv as feeders_csv};
use eh3d_phoenix_pnp::parts::{parse, Side};
use eh3d_phoenix_pnp::plan::{plan, Settings};
use std::env;
use std::fs;
use std::process::ExitCode;

const HELP: &str = "phoenix-pnp: pick and place assembly

  phoenix-pnp [gui [FILES...]]                 the window, with the files loaded (built with --features gui; run with no command it opens too)
  phoenix-pnp gui [FILES...] --shot OUT.png [--size WxH] [--theme phoenix|light|contrast] [--wait N] [--lang CODE] [--window TAB] [--steps N] [--open FILE] [--replace]   a picture of the window with the files loaded, on the tab TAB (job, parts, packages, feeders, panels, boards, loadsheet, machine, problems or log) after N steps of the simulated job (built with --features gui)
  phoenix-pnp parts PARTS                      the parts of a position file (KiCad .pos or CSV) in millimetres
  phoenix-pnp check PARTS FEEDERS              which parts have no feeder and which feeders will run out
  phoenix-pnp plan PARTS FEEDERS OUT.json [--nozzles 2] [--side top|bottom] [--board-width MM] [--origin-x MM] [--speed MM_S] [--partial]

  phoenix-pnp schedule PARTS FEEDERS OUT.json [OPTIONS] [--report-text F] [--gcode F] [--missing F] [--partial]
  phoenix-pnp preflight PARTS FEEDERS [OPTIONS]            the checks before a job, without writing anything
  phoenix-pnp simulate PARTS FEEDERS [OPTIONS] [--state F] [--steps N] [--report F] [--report-csv F] [--report-text F] [--report-html F] [--vision F] [--recipe F] [--partial]
  phoenix-pnp loadsheet PARTS SLOTS OUT.csv [OPTIONS] [--feeders-out F]
  phoenix-pnp config-check FILE [--lang CODE]
  phoenix-pnp check-bottom PARTS [--config FILE] [--lang CODE]   the angle each rule of the bottom side (keep, mirror, half_turn) gives to the parts of the bottom side
  phoenix-pnp tables-check [--parts F] [--packages F] [--catalog F] [--feeders F] [--slots F] [--config F] [--lang CODE]   whether each table is valid, with the code and the row of an error
  phoenix-pnp recipe-check FILE [--config F] [--lang CODE]   whether a recipe is valid, with the code and the line of an error
  phoenix-pnp import-centroid FILE [--format auto|csv|eagle|kicad-pos] [--map F] [--dnp R1-R5,C7] [--out F] [--config F] [--lang CODE]   a position file of a CAD program as the table of parts
  phoenix-pnp vision-test [--vision F] [--size WxH] [--part LxS] [--mm-per-px MM] [--dx MM] [--dy MM] [--turn DEG] [--noise N] [--seed N] [--lang CODE]   measures the picture of a part made in memory and prints the error

OPTIONS of schedule, preflight, simulate and loadsheet: --config FILE (the machine settings), --packages FILE and --catalog FILE (the tables of packages and parts), --panel FILE (a panel in place of
PARTS), --dnp R1,R2 (references to leave out as not to be mounted; a column dnp, fitted or populate in a CSV does the same), --fiducials FILE (with --panel: the measured places of the fiducials, a CSV id,x,y; an id is a mark of the panel, or board/reference for a part that is a mark of its board, such as A/FID1), --x-out A,B/C (with --panel: boards or panels of the panel to leave out as bad; a child with x_out set to true in the panel file is left out too), --side top|bottom, --board-width MM, --origin-x MM, --nozzles N, --speed MM_S, --order NAME and --lang CODE (the language of the error messages: en es de fr it ja zh pt). schedule and
simulate use the whole configuration (ranks, nozzle tips, the limits of the nozzle, the time of the moves) and print the checks before the job; they stop on an error unless --partial is given.
simulate runs the plan on a simulated machine; --vision FILE gives the steps of the vision pipeline (the settings vision = pipeline and align = on make the simulated camera measure a picture of each part); --recipe FILE gives the recipe of the job (the steps it goes through and the parameters by package, feeder and tip; a recipe with the align step and without it turns that step on and off). With --state the job is saved when it stops (after N steps, or on an error) and resumed from that file the next time. --report-text writes the work report as text in the language of --lang, --report-html the same as a page that holds everything it shows (a file that exists is never written over), --report-csv and --report with the sections of the report after the statistics.

import-centroid reads a centroid CSV (--map FILE names its columns, see examples/centroid.map), the text of positions of EAGLE or the .pos of KiCad (--format auto tells them apart) and writes the table of parts ref,val,package,x,y,rot,side,dnp in millimetres to --out (a file that exists is never written over) or to the standard output; --dnp marks references and ranges (R1-R5) as not to be mounted. The settings import_unit and import_bottom_mirror_width_mm of the configuration say what a file that gives no unit holds and whether the X of the bottom side is mirrored.

FEEDERS is a CSV with the header id,value,package,x,y,count. plan refuses to write when a part has no feeder or a feeder runs out, unless --partial is given
(then only the parts that can be placed are in the plan). The bottom side needs --board-width to be mirrored, about the middle of a board that starts at X 0 (--origin-x MM says where its left edge is).";

fn read(path: &str) -> Result<String, String> {
    commands::read_text(path, MachineConfig::default().max_file_bytes)
}

/// Opens the window with the files named. Without the window in this build, no command shows the help and `gui` says why it cannot open.
#[cfg(feature = "gui")]
fn open_window(files: &[String]) -> Result<(), String> {
    let paths: Vec<std::path::PathBuf> = files.iter().map(std::path::PathBuf::from).collect();
    eh3d_phoenix_pnp::gui::window::run(&paths)
        .map_err(|e| format!("the window could not open: {e}"))
}

#[cfg(not(feature = "gui"))]
fn open_window(files: &[String]) -> Result<(), String> {
    if files.is_empty() {
        println!("{HELP}");
        return Ok(());
    }
    Err("this build has no window; build it with --features gui".into())
}

fn run(mut args: Vec<String>) -> Result<(), String> {
    if args.is_empty() {
        return open_window(&[]);
    }
    if matches!(args[0].as_str(), "help" | "--help" | "-h") {
        println!("{HELP}");
        return Ok(());
    }
    let cmd = args.remove(0);
    match cmd.as_str() {
        "gui" => {
            if cfg!(feature = "gui") {
                open_window(&args)
            } else {
                Err("this build has no window; build it with --features gui".into())
            }
        }
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
            if let Some(v) = flag(&mut args, "--origin-x")? {
                s.origin_x_mm = num(&v, "--origin-x")?;
            }
            let partial = switch(&mut args, "--partial");
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
            // the plan never goes over a file it was made from
            let same_file = |a: &str, b: &str| match (
                std::path::Path::new(a).canonicalize(),
                std::path::Path::new(b).canonicalize(),
            ) {
                (Ok(x), Ok(y)) => x == y,
                _ => a == b,
            };
            if args[..2].iter().any(|input| same_file(input, out)) {
                return Err(format!(
                    "{out}: the output file is also an input; write the plan to a new file"
                ));
            }
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
        "schedule" => commands::schedule(args),
        "preflight" => commands::run_preflight(args),
        "simulate" => commands::simulate(args),
        "loadsheet" => commands::run_loadsheet(args),
        "config-check" => commands::config_check(args),
        "check-bottom" => commands::check_bottom(args),
        "tables-check" => commands::tables_check(args),
        "recipe-check" => commands::recipe_check(args),
        "import-centroid" => commands::import_centroid(args),
        "vision-test" => commands::vision_test(args),
        other => Err(format!("{other}: unknown command (see help)")),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    if eh3d_phoenix_pnp::gui::shot::wants_picture(&args) {
        #[cfg(feature = "gui")]
        return ExitCode::from(eh3d_phoenix_pnp::gui::window::take_picture(&args));
        #[cfg(not(feature = "gui"))]
        {
            eprintln!("error: this build has no window; build it with --features gui");
            return ExitCode::from(1);
        }
    }
    match run(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}
