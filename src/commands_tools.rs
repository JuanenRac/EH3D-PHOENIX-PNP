// EH3D-PHOENIX-PNP - the commands that check and calibrate: bom-check, camera-calibrate, nozzle-calibrate, panelize and polarity-sheet.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//
// Each one reads its files, calls the library and prints; none of them moves or writes anything but the file named by --out, and a file that exists is never written over.

use crate::cli_args::{finish, flag};
use crate::commands::read_text;
use eh3d_phoenix_pnp::bom::{compare, Bom};
use eh3d_phoenix_pnp::calibrate::{
    calibrate_camera, fit_runout, parse_camera_points, parse_runout_samples,
};
use eh3d_phoenix_pnp::config::{MachineConfig, MAX_CONFIG_BYTES};
use eh3d_phoenix_pnp::error::{Lang, PnpError};
use eh3d_phoenix_pnp::parts::parse_all;

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
