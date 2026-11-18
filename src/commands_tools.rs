// EH3D-PHOENIX-PNP - the commands that check and calibrate: bom-check, camera-calibrate, nozzle-calibrate, panelize and polarity-sheet.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//
// Each one reads its files, calls the library and prints; none of them moves or writes anything but the file named by --out, and a file that exists is never written over.

use crate::cli_args::{finish, flag, num};
use crate::commands::{read_text, refuse_same_file};
use eh3d_phoenix_pnp::bom::{compare, Bom};
use eh3d_phoenix_pnp::calibrate::{
    calibrate_camera, fit_runout, parse_camera_points, parse_runout_samples,
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
