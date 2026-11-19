// EH3D-PHOENIX-PNP - command line.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
#![forbid(unsafe_code)]

mod cli_args;
mod cmd_more;
mod cmd_queue;
mod cmd_tools;
mod cmd_vis;
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

  phoenix-pnp schedule PARTS FEEDERS OUT.json [OPTIONS] [--report-text F] [--gcode F] [--missing F] [--svg F] [--partial]
  phoenix-pnp preflight PARTS FEEDERS [OPTIONS]            the checks before a job, without writing anything
  phoenix-pnp simulate PARTS FEEDERS [OPTIONS] [--state F] [--steps N] [--report F] [--report-csv F] [--report-text F] [--report-html F] [--report-feeders F] [--nozzle-life F] [--vision F] [--recipe F] [--place-offsets F] [--hooks F] [--hooks-dry-run] [--journal F] [--partial]
  phoenix-pnp loadsheet PARTS SLOTS OUT.csv [OPTIONS] [--feeders-out F]
  phoenix-pnp setup-sheet PARTS FEEDERS [OPTIONS] [--boards N] [--life F] [--out F]   what to load in which slot, the parts missing for N boards, the nozzle tips and how worn they are, the calibrations the job depends on and whether they are set, in the language of --lang; exits with 1 when parts are missing
  phoenix-pnp order-compare PARTS FEEDERS [OPTIONS]   the board planned with every order of the places and every way of changing the nozzle tips, side by side: batches, tip changes, travel and seconds, the fastest marked and what it would save over the configuration
  phoenix-pnp config-check FILE [--lang CODE]
  phoenix-pnp check-bottom PARTS [--config FILE] [--lang CODE]   the angle each rule of the bottom side (keep, mirror, half_turn) gives to the parts of the bottom side
  phoenix-pnp tables-check [--parts F] [--packages F] [--catalog F] [--feeders F] [--slots F] [--config F] [--lang CODE]   whether each table is valid, with the code and the row of an error
  phoenix-pnp recipe-check FILE [--config F] [--lang CODE]   whether a recipe is valid, with the code and the line of an error
  phoenix-pnp bom-check PARTS BOM [--config F] [--lang CODE]   whether the bill of materials and the table of parts agree (references, values, packages, quantities)
  phoenix-pnp camera-calibrate FILE [--config F] [--lang CODE]   the settings of the camera from sightings of a mark (px,py,x_mm,y_mm), with how well they fit
  phoenix-pnp camera-pattern FILE [--pitch MM] [--config F] [--lang CODE]   the size of a pixel and the bending of the lens from the points of a pattern on a grid (col,row,px,py), with how well they fit; exits with 1 when a point is over camera_pattern_max_residual_mm
  phoenix-pnp camera-combined FILE [--pitch MM] [--origin X,Y | --touch] [--config F] [--lang CODE]   the camera, the turn of the sheet and the bending of the lens from a pattern fixed to the bed while the head moves (head_x,head_y,col,row,px,py); off until camera_combined is on; --touch (or camera_combined_touch = on) takes the origin from a row origin,X,Y of the file; exits with 1 when a sighting is over camera_pattern_max_residual_mm
  phoenix-pnp package-gen [NAME...] [--from PARTS] [--out F] [--config F] [--lang CODE]   the rows of a table of packages (height, tape, body) for packages named by their standard (0603, R0805, SOT-23, SOIC-8, TSSOP-14, QFN-32-5x5, LQFP-48...) and for those of a position file; exits with 1 when a name is not known
  phoenix-pnp package-kicad PATH... [--height MM] [--out F] [--config F] [--lang CODE]   the rows of a table of packages (body, tape, height) from footprint files of KiCad (.kicad_mod, or a folder of them): the body is the drawing on the fabrication layer; the height is that of the package when its name is a known one, or --height; exits with 1 when a footprint is not understood
  phoenix-pnp place-learn READINGS --state F [--residual] [--dry-run] [--config F] [--lang CODE]   learns the mean offset of the placement of each package from readings (package,dx_mm,dy_mm), leaving the strays out and holding it within place_learn_cap_mm; off until place_learn is on; --show and --forget PACKAGE read and trim the file
  phoenix-pnp job-export PARTS FEEDERS OUT [OPTIONS] [--state F] [--partial]   the plan and where the job is (the saved state of simulate, if given) in one JSON file for other programs
  phoenix-pnp job-import FILE [--csv F] [--config F] [--lang CODE]   reads and checks such a file and says what it holds; --csv writes the placements with their status as a table; exits with 1 when the file is not valid
  phoenix-pnp parts-diff FIRST SECOND [--tol MM] [--rot-tol DEG] [--out F] [--config F] [--lang CODE]   what changed between two tables of parts, part by part: added, removed, moved, turned, another value, another package or the other side; exits with 1 when they differ
  phoenix-pnp config-diff FIRST SECOND [--lang CODE]   the settings that two configuration files give differently, in what they mean (yes and on, 5 and 5.0 and a line that gives the default are the same); exits with 1 when the machines differ
  phoenix-pnp board-stats PARTS [--near MM] [--limit N] [--config F] [--lang CODE]   what a table of parts holds (the sides, the kinds, each package, the box of the centres) and, with --near, the pairs of parts of one side whose centres are closer than MM; exits with 1 when it finds a pair
  phoenix-pnp queue-stock QUEUE [--out F] [--config F] [--lang CODE]   whether the feeders hold enough parts for all the jobs of the queue that are still to be made, one after the other, and the job in which each reel that falls short runs out; exits with 1 when one does
  phoenix-pnp queue-add QUEUE NAME SOURCE FEEDERS [--panel] [--boards N] [--side top|bottom] [--config F] [--packages F] [--catalog F] [--relative]   puts a job (a table of parts, or a panel with --panel) at the end of a queue of jobs
  phoenix-pnp queue-status QUEUE [--check] [--config F]   the jobs of the queue and what became of each; --check plans the ones still to be made and exits with 1 when one cannot run now
  phoenix-pnp queue-confirm QUEUE NAME   records that the plan of the job, as it is now, was looked at and may be run (void when the files change)
  phoenix-pnp queue-edit QUEUE NAME skip|reset|revoke|remove   leaves a job out, puts it back to the start, takes its confirmation back or takes it out
  phoenix-pnp queue-run QUEUE [--only NAME] [--config F]   makes the jobs one board at a time on the simulated machine, writing the queue after every board; a job whose plan was not confirmed is not run (queue_confirm)
  phoenix-pnp nozzle-calibrate FILE [--config F] [--lang CODE]   the runout of the nozzle tip from readings at several angles (angle_deg,x_mm,y_mm)
  phoenix-pnp bom-order BOM [--boards N] [--spare-percent P] [--spare-min N] [--parts PARTS] [--have FEEDERS] [--out F] [--config F] [--lang CODE]   the parts to buy for N boards, with a spare (less what the feeders already hold)
  phoenix-pnp stock PARTS FEEDERS [--boards N] [--side top|bottom] [--out F] [--config F] [--lang CODE]   whether the feeders hold enough parts for N boards, kind by kind; exits with 1 when some fall short
  phoenix-pnp nozzle-life FILE [--warn N] [--reset TIP] [--config F] [--lang CODE]   the parts each nozzle tip has placed (a file kept beside the configuration); exits with 1 when a tip is at or over the warning level
  phoenix-pnp feeder-teach strip|tray [places and counts] | --measures FILE [--lang CODE]   the pitch and the angle of a strip or a tray from places measured on the machine
  phoenix-pnp panelize BOARD --cols N --rows N --pitch-x MM --pitch-y MM [--rotation DEG] [--fiducials MM] [--board-size W,H] [--x-out P2,P5] [--out F] [--lang CODE]   a panel file for a grid of copies of one board
  phoenix-pnp part-inspect READINGS [--sizes F] [--packages F] [--out F] [--config F] [--lang CODE]   the parts on the nozzle (ref,package,long_mm,short_mm,dx_mm,dy_mm,rot_deg) against the sizes of their packages (a table of sizes, or the columns body_long_mm and body_short_mm of a table of packages)
  phoenix-pnp fiducial-check DESIGN MEASURED [--config F] [--lang CODE]   the turn, scale, shear and shift that the measured marks (id,x,y) put on the panel, against the limits
  phoenix-pnp vision-readings PARTS [--sizes F] [--packages F] [--vision F] [--seed N] [--spread MM] [--turn DEG] [--wrong-every N] [--out F] [--config F] [--lang CODE]   the readings that a simulated camera (with the noise, blur and failures of the vision settings) gives of the parts of a position file, in the file that part-inspect reads
  phoenix-pnp vision-marks DESIGN [--shift X,Y] [--turn DEG] [--scale S] [--dot MM] [--vision F] [--seed N] [--out F] [--config F] [--lang CODE]   the measured places of the marks of a panel that lies as said, as a simulated camera reads them, in the file that fiducial-check reads
  phoenix-pnp feeder-check FEEDERS [--packages F] [--catalog F] [--config F] [--lang CODE]   whether the table of feeders agrees with itself (slots, pitches, cells, places that clash or are out of reach)
  phoenix-pnp polarity-sheet PARTS [--side top|bottom] [--out F] [--config F] [--lang CODE]   the parts that have a direction, with their place and turn, as a CSV for the look after a job
  phoenix-pnp import-centroid FILE [--format auto|csv|eagle|kicad-pos] [--map F] [--dnp R1-R5,C7] [--out F] [--config F] [--lang CODE]   a position file of a CAD program as the table of parts
  phoenix-pnp vision-test [--vision F] [--size WxH] [--part LxS] [--mm-per-px MM] [--dx MM] [--dy MM] [--turn DEG] [--noise N] [--seed N] [--lang CODE]   measures the picture of a part made in memory and prints the error

OPTIONS of schedule, preflight, simulate, loadsheet and setup-sheet: --config FILE (the machine settings), --packages FILE and --catalog FILE (the tables of packages and parts), --panel FILE (a panel in place of
PARTS), --dnp R1,R2 (references to leave out as not to be mounted; a column dnp, fitted or populate in a CSV does the same), --fiducials FILE (with --panel: the measured places of the fiducials, a CSV id,x,y; an id is a mark of the panel, or board/reference for a part that is a mark of its board, such as A/FID1), --x-out A,B/C (with --panel: boards or panels of the panel to leave out as bad; a child with x_out set to true in the panel file is left out too), --side top|bottom, --board-width MM, --origin-x MM, --nozzles N, --speed MM_S, --order NAME and --lang CODE (the language of the error messages: en es de fr it ja zh pt). schedule and
simulate use the whole configuration (ranks, nozzle tips, the limits of the nozzle, the time of the moves) and print the checks before the job; they stop on an error unless --partial is given.
simulate runs the plan on a simulated machine; --journal FILE writes one row for every step (the part, the feeder, the nozzle, the second of the clock, how long it took and how it ended; a file that exists is never written over); --vision FILE gives the steps of the vision pipeline (the settings vision = pipeline and align = on make the simulated camera measure a picture of each part); --recipe FILE gives the recipe of the job (the steps it goes through and the parameters by package, feeder and tip; a recipe with the align step and without it turns that step on and off). With --state the job is saved when it stops (after N steps, or on an error) and resumed from that file the next time. --report-text writes the work report as text in the language of --lang, --report-html the same as a page that holds everything it shows (a file that exists is never written over), --report-csv and --report with the sections of the report after the statistics. --report-feeders writes the picks of each feeder as a CSV (planned and made, failures, the share of failures; a feeder over feeder_fail_percent is marked and named in the output).

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
        "setup-sheet" => commands::setup_sheet(args),
        "order-compare" => commands::order_compare(args),
        "config-check" => commands::config_check(args),
        "check-bottom" => commands::check_bottom(args),
        "tables-check" => commands::tables_check(args),
        "recipe-check" => commands::recipe_check(args),
        "import-centroid" => commands::import_centroid(args),
        "camera-calibrate" => cmd_tools::camera_calibrate(args),
        "camera-pattern" => cmd_tools::camera_pattern(args),
        "camera-combined" => cmd_tools::camera_combined(args),
        "parts-diff" => cmd_more::parts_diff(args),
        "config-diff" => cmd_more::config_diff(args),
        "board-stats" => cmd_more::board_stats(args),
        "queue-stock" => cmd_more::queue_stock(args),
        "package-gen" => cmd_tools::package_gen(args),
        "package-kicad" => cmd_tools::package_kicad(args),
        "place-learn" => cmd_tools::place_learn(args),
        "job-export" => commands::job_export(args),
        "job-import" => cmd_tools::job_import(args),
        "queue-add" => cmd_queue::queue_add(args),
        "queue-status" => cmd_queue::queue_status(args),
        "queue-confirm" => cmd_queue::queue_confirm(args),
        "queue-edit" => cmd_queue::queue_edit(args),
        "queue-run" => cmd_queue::queue_run(args),
        "nozzle-calibrate" => cmd_tools::nozzle_calibrate(args),
        "panelize" => cmd_tools::panelize(args),
        "polarity-sheet" => cmd_tools::polarity_sheet(args),
        "bom-order" => cmd_tools::bom_order(args),
        "feeder-teach" => cmd_tools::feeder_teach(args),
        "stock" => cmd_tools::stock(args),
        "nozzle-life" => cmd_tools::nozzle_life(args),
        "bom-check" => cmd_tools::bom_check(args),
        "part-inspect" => cmd_vis::part_inspect(args),
        "fiducial-check" => cmd_vis::fiducial_check(args),
        "vision-readings" => cmd_vis::vision_readings(args),
        "vision-marks" => cmd_vis::vision_marks(args),
        "feeder-check" => cmd_vis::feeder_check(args),
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
