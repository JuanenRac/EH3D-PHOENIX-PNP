// EH3D-PHOENIX-PNP - the commands of the look at the part, the fiducials and the library of feeders.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.

use crate::cli_args::{finish, flag};
use crate::commands::{read_text, refuse_same_file};
use eh3d_phoenix_pnp::catalog::Catalog;
use eh3d_phoenix_pnp::config::{MachineConfig, MAX_CONFIG_BYTES};
use eh3d_phoenix_pnp::error::Lang;
use eh3d_phoenix_pnp::feeding;
use eh3d_phoenix_pnp::fidcheck;
use eh3d_phoenix_pnp::fiducial;
use eh3d_phoenix_pnp::inspect;

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

/// `part-inspect`: the readings of the parts on the nozzle against the sizes of their packages. The exit code is 1 when a part is refused or has no size to be judged against.
pub fn part_inspect(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let sizes = flag(&mut args, "--sizes")?;
    let packages = flag(&mut args, "--packages")?;
    let out = flag(&mut args, "--out")?;
    finish(&args, 1)?;
    let path = args
        .first()
        .ok_or("part-inspect needs a file of readings")?;
    if sizes.is_none() && packages.is_none() {
        return Err("part-inspect needs --sizes FILE or --packages FILE".into());
    }
    if let Some(o) = &out {
        let mut inputs = vec![path.clone()];
        inputs.extend(sizes.iter().cloned());
        inputs.extend(packages.iter().cloned());
        refuse_same_file(&inputs, o)?;
    }
    let l = |e: eh3d_phoenix_pnp::error::PnpError| e.localized(lang);
    // the table of sizes wins; the packages that it has no row for take the size of the body from the table of packages
    let mut table = match &sizes {
        Some(s) => inspect::SizeTable::parse(&read_text(s, cfg.max_file_bytes)?).map_err(l)?,
        None => inspect::SizeTable::default(),
    };
    if let Some(p) = &packages {
        let mut catalog = Catalog::new();
        catalog
            .add_packages(&read_text(p, cfg.max_file_bytes)?)
            .map_err(l)?;
        table = table.with_missing_from(&inspect::SizeTable::from_catalog(&catalog));
    }
    let readings = inspect::parse_readings(&read_text(path, cfg.max_file_bytes)?).map_err(l)?;
    let limits = inspect::Limits {
        size_tol_mm: cfg.part_size_tol_mm,
        angle_tol_deg: cfg.part_angle_tol_deg,
        offset_max_mm: cfg.part_offset_max_mm,
    };
    let rows = inspect::judge(&readings, &table, &limits);
    let csv = inspect::rows_csv(&rows);
    match &out {
        Some(o) => eh3d_phoenix_pnp::report::write_new(o, &csv).map_err(l)?,
        None => print!("{csv}"),
    }
    let (ok, refused, unknown) = inspect::tally(&rows);
    eprintln!("{ok} accepted, {refused} refused, {unknown} without a size to be judged against");
    if refused + unknown > 0 {
        Err(format!("{} parts are not accepted", refused + unknown))
    } else {
        Ok(())
    }
}

/// `fiducial-check`: what the measured places of the marks say about the panel (the turn, the scale along each axis, the shear, the shift) and whether the limits hold.
pub fn fiducial_check(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    finish(&args, 2)?;
    let (design_path, measured_path) = match (args.first(), args.get(1)) {
        (Some(a), Some(b)) => (a, b),
        _ => return Err("fiducial-check needs the file of the design and the file of the measured marks (id,x,y)".into()),
    };
    let l = |e: eh3d_phoenix_pnp::error::PnpError| e.localized(lang);
    let design =
        fiducial::parse_measured(&read_text(design_path, cfg.max_file_bytes)?).map_err(l)?;
    let measured =
        fiducial::parse_measured(&read_text(measured_path, cfg.max_file_bytes)?).map_err(l)?;
    if let Some(m) = measured.iter().find(|m| !design.iter().any(|d| d.0 == m.0)) {
        return Err(format!("the mark '{}' is not in the design", m.0));
    }
    let (mut ids, mut from, mut to) = (Vec::new(), Vec::new(), Vec::new());
    for d in &design {
        if let Some(m) = measured.iter().find(|m| m.0 == d.0) {
            ids.push(d.0.clone());
            from.push((d.1, d.2));
            to.push((m.1, m.2));
        }
    }
    let limits = fidcheck::Limits {
        max_rotation_deg: cfg.fiducial_max_rotation_deg,
        scale_tol_percent: cfg.fiducial_scale_tol_percent,
        shear_tol_deg: cfg.fiducial_shear_tol_deg,
        max_shift_mm: cfg.fiducial_max_shift_mm,
        level_turn_deg: cfg.fiducial_level_turn_deg,
    };
    let a = fidcheck::analyze_points(
        &from,
        &to,
        cfg.fiducial_mode,
        cfg.fiducial_max_residual_mm,
        &limits,
    )
    .map_err(l)?;
    print!("{}", fidcheck::report(&a, &ids));
    if a.fit.max > cfg.fiducial_max_residual_mm {
        return Err(format!(
            "a mark is {:.3} mm from where the fit puts it, the limit is {} mm",
            a.fit.max, cfg.fiducial_max_residual_mm
        ));
    }
    if !a.check.ok() {
        return Err("the marks put the panel somewhere unexpected".into());
    }
    Ok(())
}

/// `feeder-check`: whether the table of feeders agrees with itself. The exit code is 1 when there is an error (a warning alone does not fail).
pub fn feeder_check(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let packages = flag(&mut args, "--packages")?;
    let catalog_path = flag(&mut args, "--catalog")?;
    finish(&args, 1)?;
    let path = args.first().ok_or("feeder-check needs a file of feeders")?;
    let l = |e: eh3d_phoenix_pnp::error::PnpError| e.localized(lang);
    let feeders = feeding::from_csv(&read_text(path, cfg.max_file_bytes)?).map_err(l)?;
    let mut catalog = Catalog::new();
    if let Some(p) = &packages {
        catalog
            .add_packages(&read_text(p, cfg.max_file_bytes)?)
            .map_err(l)?;
    }
    if let Some(p) = &catalog_path {
        catalog
            .add_parts(&read_text(p, cfg.max_file_bytes)?)
            .map_err(l)?;
    }
    let rules = eh3d_phoenix_pnp::feedlib::Rules {
        pitch_step_mm: cfg.feeder_pitch_step_mm,
        min_gap_mm: cfg.feeder_min_gap_mm,
        x_min: cfg.limit_x_min_mm,
        x_max: cfg.limit_x_max_mm,
        y_min: cfg.limit_y_min_mm,
        y_max: cfg.limit_y_max_mm,
    };
    let found = eh3d_phoenix_pnp::feedlib::check(&feeders, Some(&catalog), &rules);
    print!("{}", eh3d_phoenix_pnp::feedlib::text(&found));
    let (errors, _) = eh3d_phoenix_pnp::feedlib::counts(&found);
    if errors > 0 {
        Err(format!("the table of feeders has {errors} errors"))
    } else {
        Ok(())
    }
}
