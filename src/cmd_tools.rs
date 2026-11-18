// EH3D-PHOENIX-PNP - the commands that check, calibrate and make files: bom-check, bom-order, camera-calibrate, nozzle-calibrate, feeder-teach, panelize and polarity-sheet.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//
// Each one reads its files, calls the library and prints; none of them moves or writes anything but the file named by --out, and a file that exists is never written over.

use crate::cli_args::{finish, flag, num};
use crate::commands::{read_text, refuse_same_file};
use eh3d_phoenix_pnp::bom::{compare, order as bom_order_lines, order_csv, Bom};
use eh3d_phoenix_pnp::calibrate::{
    calibrate_camera, fit_runout, parse_camera_points, parse_runout_samples, teach_strip,
    teach_tray,
};
use eh3d_phoenix_pnp::config::{MachineConfig, MAX_CONFIG_BYTES};
use eh3d_phoenix_pnp::error::{Lang, PnpError};
use eh3d_phoenix_pnp::panel::{grid_panel, Grid};
use eh3d_phoenix_pnp::parts::{parse_all, Side};
use eh3d_phoenix_pnp::polarity;
use eh3d_phoenix_pnp::report;
use std::path::Path;

/// The language and the configuration of a command: `--lang` wins over the key `language` of `--config`.
fn language_and_config(args: &mut Vec<String>) -> Result<(Lang, MachineConfig), String> {
    let lang_option = match flag(args, "--lang")? {
        Some(l) => Some(
            Lang::from_code(&l)
                .ok_or_else(|| format!("--lang '{l}' is not one of en es de fr it ja zh pt"))?,
        ),
        None => None,
    };
    let early = lang_option.unwrap_or(Lang::En);
    let cfg = match flag(args, "--config")? {
        Some(p) => MachineConfig::parse(&read_text(&p, MAX_CONFIG_BYTES as u64)?)
            .map_err(|e| e.localized(early))?,
        None => MachineConfig::default(),
    };
    Ok((lang_option.unwrap_or(cfg.language), cfg))
}

/// `bom-check`: whether the bill of materials and the table of parts agree. The findings go to the standard output; the exit code is 1 when there is any.
pub fn bom_check(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    finish(&args, 2)?;
    let (parts_path, bom_path) = match (args.first(), args.get(1)) {
        (Some(a), Some(b)) => (a.clone(), b.clone()),
        _ => return Err("bom-check needs a table of parts and a BOM".into()),
    };
    let parsed = parse_all(&read_text(&parts_path, cfg.max_file_bytes)?, &cfg.profile)?;
    let bom = Bom::parse(&read_text(&bom_path, cfg.max_file_bytes)?, cfg.max_parts)
        .map_err(|e| e.localized(lang))?;
    let report = compare(&bom, &parsed.parts, &parsed.dnp);
    print!("{}", report.text());
    if report.is_clean() {
        Ok(())
    } else {
        Err(format!(
            "the BOM and the board disagree in {} places",
            report.problems()
        ))
    }
}

/// `camera-calibrate`: the settings of the camera from sightings of a mark at known places of the head; prints how good the fit is and the lines to put in the configuration.
pub fn camera_calibrate(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    finish(&args, 1)?;
    let path = args
        .first()
        .ok_or("camera-calibrate needs a file of sightings")?;
    let l = |e: PnpError| e.localized(lang);
    let points = parse_camera_points(&read_text(path, cfg.max_file_bytes)?).map_err(l)?;
    let found = calibrate_camera(&points).map_err(l)?;
    println!("{}", found.summary());
    print!("{}", found.config_lines());
    Ok(())
}

/// `nozzle-calibrate`: the runout of the nozzle tip from readings of its place at several angles; prints how good the fit is and the lines to put in the configuration.
pub fn nozzle_calibrate(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    finish(&args, 1)?;
    let path = args
        .first()
        .ok_or("nozzle-calibrate needs a file of readings")?;
    let l = |e: PnpError| e.localized(lang);
    let samples = parse_runout_samples(&read_text(path, cfg.max_file_bytes)?).map_err(l)?;
    let found = fit_runout(&samples).map_err(l)?;
    if found.radius_mm > 5.0 {
        return Err(format!(
            "the runout is {:.3} mm, above the 5 mm that nozzle_runout_mm accepts: check the readings",
            found.radius_mm
        ));
    }
    println!("{}", found.summary());
    print!("{}", found.config_lines());
    Ok(())
}

fn count_option(
    args: &mut Vec<String>,
    name: &str,
    default: Option<usize>,
) -> Result<usize, String> {
    match flag(args, name)? {
        Some(v) => v
            .trim()
            .parse::<usize>()
            .map_err(|_| format!("{name} '{v}' is not a whole number")),
        None => default.ok_or_else(|| format!("panelize needs {name}")),
    }
}

/// `panelize`: the panel file for a grid of copies of one board, to `--out` (a file that exists is never written over) or to the standard output.
pub fn panelize(mut args: Vec<String>) -> Result<(), String> {
    let (lang, _cfg) = language_and_config(&mut args)?;
    let cols = count_option(&mut args, "--cols", None)?;
    let rows = count_option(&mut args, "--rows", None)?;
    let pitch_x = match flag(&mut args, "--pitch-x")? {
        Some(v) => num(&v, "--pitch-x")?,
        None => return Err("panelize needs --pitch-x".into()),
    };
    let pitch_y = match flag(&mut args, "--pitch-y")? {
        Some(v) => num(&v, "--pitch-y")?,
        None => return Err("panelize needs --pitch-y".into()),
    };
    let rotation_deg = match flag(&mut args, "--rotation")? {
        Some(v) => num(&v, "--rotation")?,
        None => 0.0,
    };
    let fiducial_margin_mm = match flag(&mut args, "--fiducials")? {
        Some(v) => num(&v, "--fiducials")?,
        None => 0.0,
    };
    let x_out: Vec<String> = flag(&mut args, "--x-out")?
        .map(|v| {
            v.split([',', ';', ' '])
                .filter(|w| !w.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let out_path = flag(&mut args, "--out")?;
    finish(&args, 1)?;
    let board = args
        .first()
        .ok_or("panelize needs the file of the board")?
        .clone();
    // a board beside the panel file is named by its file name alone, because the panel names its boards relative to itself
    let board_file = match &out_path {
        Some(out) if Path::new(&board).parent() == Path::new(out).parent() => Path::new(&board)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or(board.clone()),
        _ => board.clone(),
    };
    let text = grid_panel(&Grid {
        board_file,
        cols,
        rows,
        pitch_x_mm: pitch_x,
        pitch_y_mm: pitch_y,
        rotation_deg,
        fiducial_margin_mm,
        x_out,
    })
    .map_err(|e| e.localized(lang))?;
    match out_path {
        Some(out) => {
            refuse_same_file(std::slice::from_ref(&board), &out)?;
            report::write_new(&out, &text).map_err(|e| e.localized(lang))?;
            println!(
                "ok: {} boards ({cols} x {rows}); written to {out}",
                cols * rows
            );
        }
        None => {
            print!("{text}");
            eprintln!("ok: {} boards ({cols} x {rows})", cols * rows);
        }
    }
    Ok(())
}

/// `polarity-sheet`: the parts that have a direction (see `polarity_refs` and `polarity_packages`), with their place and turn, as a CSV for the look after a job; `--side` keeps one side.
pub fn polarity_sheet(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let side = match flag(&mut args, "--side")?.as_deref() {
        None => None,
        Some("top") => Some(Side::Top),
        Some("bottom") => Some(Side::Bottom),
        Some(other) => return Err(format!("--side '{other}' is not top or bottom")),
    };
    let out_path = flag(&mut args, "--out")?;
    finish(&args, 1)?;
    let path = args
        .first()
        .ok_or("polarity-sheet needs a table of parts")?
        .clone();
    let parsed = parse_all(&read_text(&path, cfg.max_file_bytes)?, &cfg.profile)?;
    let rows: Vec<_> = polarity::sheet(&parsed.parts, &cfg.polarity_refs, &cfg.polarity_packages)
        .into_iter()
        .filter(|p| side.is_none_or(|s| p.side == s))
        .collect();
    let total = parsed
        .parts
        .iter()
        .filter(|p| side.is_none_or(|s| p.side == s))
        .count();
    let csv = polarity::to_csv(&rows);
    match out_path {
        Some(out) => {
            refuse_same_file(std::slice::from_ref(&path), &out)?;
            report::write_new(&out, &csv).map_err(|e| e.localized(lang))?;
            println!("ok: {}; written to {out}", polarity::summary(&rows, total));
        }
        None => {
            print!("{csv}");
            eprintln!("ok: {}", polarity::summary(&rows, total));
        }
    }
    Ok(())
}

fn whole(args: &mut Vec<String>, name: &str, default: usize) -> Result<usize, String> {
    match flag(args, name)? {
        Some(v) => v
            .trim()
            .parse::<usize>()
            .map_err(|_| format!("{name} '{v}' is not a whole number")),
        None => Ok(default),
    }
}

/// `bom-order`: the list of parts to buy for a number of boards, with a spare; `--parts` leaves out the parts that table marks as not to be mounted. To `--out` (a file that exists is never
/// written over) or to the standard output.
pub fn bom_order(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let boards = whole(&mut args, "--boards", 1)?;
    let spare_min = whole(&mut args, "--spare-min", cfg.order_spare_min)?;
    let spare_percent = match flag(&mut args, "--spare-percent")? {
        Some(v) => num(&v, "--spare-percent")?,
        None => cfg.order_spare_percent,
    };
    let parts_path = flag(&mut args, "--parts")?;
    let out_path = flag(&mut args, "--out")?;
    finish(&args, 1)?;
    let path = args.first().ok_or("bom-order needs a BOM")?.clone();
    let l = |e: PnpError| e.localized(lang);
    let bom = Bom::parse(&read_text(&path, cfg.max_file_bytes)?, cfg.max_parts).map_err(l)?;
    let skipped: Vec<String> = match &parts_path {
        Some(p) => parse_all(&read_text(p, cfg.max_file_bytes)?, &cfg.profile)?
            .dnp
            .into_iter()
            .map(|p| p.reference)
            .collect(),
        None => Vec::new(),
    };
    let lines = bom_order_lines(&bom, boards, spare_percent, spare_min, &skipped).map_err(l)?;
    let csv = order_csv(&lines);
    let total: usize = lines.iter().map(|x| x.total).sum();
    let summary = format!(
        "{} lines, {total} parts for {boards} boards with {spare_percent}% spare (at least {spare_min})",
        lines.len()
    );
    match out_path {
        Some(out) => {
            refuse_same_file(std::slice::from_ref(&path), &out)?;
            report::write_new(&out, &csv).map_err(l)?;
            println!("ok: {summary}; written to {out}");
        }
        None => {
            print!("{csv}");
            eprintln!("ok: {summary}");
        }
    }
    Ok(())
}

/// A place written `X,Y` (millimetres).
fn pair(text: &str, what: &str) -> Result<(f64, f64), String> {
    let (a, b) = text
        .split_once(',')
        .ok_or_else(|| format!("{what} '{text}' is not X,Y"))?;
    Ok((num(a, what)?, num(b, what)?))
}

/// `feeder-teach strip|tray`: the pitch and the angle of a strip, or the pitches and the angle of a tray, from places that were measured on the machine; prints the columns of the file of
/// feeders that they set.
pub fn feeder_teach(mut args: Vec<String>) -> Result<(), String> {
    let (lang, _cfg) = language_and_config(&mut args)?;
    let l = |e: PnpError| e.localized(lang);
    let kind = args
        .first()
        .cloned()
        .ok_or("feeder-teach needs strip or tray")?;
    args.remove(0);
    let place = |args: &mut Vec<String>, name: &str| -> Result<(f64, f64), String> {
        let v = flag(args, name)?.ok_or_else(|| format!("feeder-teach {kind} needs {name}"))?;
        pair(&v, name)
    };
    let count = |args: &mut Vec<String>, name: &str| -> Result<u32, String> {
        let v = flag(args, name)?.ok_or_else(|| format!("feeder-teach {kind} needs {name}"))?;
        v.trim()
            .parse::<u32>()
            .map_err(|_| format!("{name} '{v}' is not a whole number"))
    };
    match kind.as_str() {
        "strip" => {
            let (first, last) = (place(&mut args, "--first")?, place(&mut args, "--last")?);
            let n = count(&mut args, "--count")?;
            finish(&args, 0)?;
            let found = teach_strip(first, last, n).map_err(l)?;
            println!("{}", found.summary());
            print!("{}", found.csv());
        }
        "tray" => {
            let (origin, col_end, row_end) = (
                place(&mut args, "--origin")?,
                place(&mut args, "--col-end")?,
                place(&mut args, "--row-end")?,
            );
            let (cols, rows) = (count(&mut args, "--cols")?, count(&mut args, "--rows")?);
            finish(&args, 0)?;
            let found = teach_tray(origin, col_end, row_end, cols, rows).map_err(l)?;
            println!("{}", found.summary());
            print!("{}", found.csv());
        }
        other => return Err(format!("feeder-teach '{other}' is not strip or tray")),
    }
    Ok(())
}
