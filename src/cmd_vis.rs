// EH3D-PHOENIX-PNP - the commands of the look at the part, the fiducials and the library of feeders.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.

use crate::cli_args::{finish, flag, num};
use crate::cmd_nodes::{fail, not_passed};
use crate::commands::{read_text, refuse_same_file};
use eh3d_phoenix_pnp::catalog::Catalog;
use eh3d_phoenix_pnp::config::{MachineConfig, MAX_CONFIG_BYTES};
use eh3d_phoenix_pnp::error::Lang;
use eh3d_phoenix_pnp::feeding;
use eh3d_phoenix_pnp::fidcheck;
use eh3d_phoenix_pnp::fiducial;
use eh3d_phoenix_pnp::inspect;
use eh3d_phoenix_pnp::parts::parse_all;
use eh3d_phoenix_pnp::vision::{plan_shots, Axis, VisionConfig, MAX_SHOTS, MAX_VISION_BYTES};
use eh3d_phoenix_pnp::visionsim;

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

/// `part-inspect`: the readings of the parts on the nozzle against the sizes of their packages. The exit code is 2 when a part is refused or has no size to be judged against.
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
        fail(format!("{} parts are not accepted", refused + unknown))
    } else {
        Ok(())
    }
}

/// The vision settings of `--vision FILE` (the defaults without it).
fn vision_settings(args: &mut Vec<String>, lang: Lang) -> Result<VisionConfig, String> {
    match flag(args, "--vision")? {
        Some(p) => VisionConfig::parse(&read_text(&p, MAX_VISION_BYTES as u64)?)
            .map_err(|e| e.localized(lang)),
        None => Ok(VisionConfig::default()),
    }
}

/// `--seed N`, a whole number (1 without it).
fn seed_option(args: &mut Vec<String>) -> Result<u64, String> {
    match flag(args, "--seed")? {
        Some(v) => {
            let n = num(&v, "--seed")?;
            if n.fract() != 0.0 || !(0.0..=9_007_199_254_740_991.0).contains(&n) {
                return Err(format!("--seed {v} is not a whole number from 0"));
            }
            Ok(n as u64)
        }
        None => Ok(1),
    }
}

/// `X,Y` given to an option.
fn xy(text: &str, what: &str) -> Result<(f64, f64), String> {
    let (a, b) = text
        .split_once(',')
        .ok_or_else(|| format!("{what} '{text}' is not X,Y"))?;
    Ok((num(a, what)?, num(b, what)?))
}

/// Writes a made file to `--out` (never over a file that exists) or prints it.
fn emit(out: &Option<String>, inputs: &[String], text: &str, lang: Lang) -> Result<(), String> {
    match out {
        Some(o) => {
            refuse_same_file(inputs, o)?;
            eh3d_phoenix_pnp::report::write_new(o, text).map_err(|e| e.localized(lang))
        }
        None => {
            print!("{text}");
            Ok(())
        }
    }
}

/// `vision-readings`: the readings that a simulated camera gives of the parts of a position file, in the file that `part-inspect` reads. Every part is drawn from its package in the table of
/// sizes (`--sizes`, `--packages`) hanging by chance within `--spread MM` of the axis and turned within `--turn DEG`, with the noise, blur and failures of the vision settings (`--vision`);
/// `--wrong-every N` makes every N-th part a part of another size; `--seed` makes the readings repeat. A part with no size, and a part in whose picture the pipeline finds no part, give no
/// reading and are told on the error output.
pub fn vision_readings(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let sizes = flag(&mut args, "--sizes")?;
    let packages = flag(&mut args, "--packages")?;
    let out = flag(&mut args, "--out")?;
    let vision = vision_settings(&mut args, lang)?;
    let seed = seed_option(&mut args)?;
    let mut spread = visionsim::Spread::default();
    if let Some(v) = flag(&mut args, "--spread")? {
        spread.offset_mm = num(&v, "--spread")?;
    }
    if let Some(v) = flag(&mut args, "--turn")? {
        spread.turn_deg = num(&v, "--turn")?;
    }
    if let Some(v) = flag(&mut args, "--wrong-every")? {
        let n = num(&v, "--wrong-every")?;
        if n.fract() != 0.0 || !(0.0..=1_000_000.0).contains(&n) {
            return Err(format!("--wrong-every {v} is not a whole number from 0"));
        }
        spread.wrong_every = n as usize;
    }
    if !(0.0..=50.0).contains(&spread.offset_mm) || !(0.0..=90.0).contains(&spread.turn_deg) {
        return Err("--spread wants 0 to 50 millimetres and --turn 0 to 90 degrees".into());
    }
    finish(&args, 1)?;
    let path = args
        .first()
        .ok_or("vision-readings needs a table of parts")?
        .clone();
    if sizes.is_none() && packages.is_none() {
        return Err("vision-readings needs --sizes FILE or --packages FILE".into());
    }
    let l = |e: eh3d_phoenix_pnp::error::PnpError| e.localized(lang);
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
    let parsed = parse_all(&read_text(&path, cfg.max_file_bytes)?, &cfg.profile)?;
    let mut items = Vec::new();
    let mut without = Vec::new();
    for part in &parsed.parts {
        match table.lookup(&part.package) {
            Some(size) => items.push((part.reference.clone(), part.package.clone(), size.clone())),
            None => without.push(part.reference.clone()),
        }
    }
    let seen = visionsim::read_parts(&items, &vision, &spread, seed);
    let mut inputs = vec![path];
    inputs.extend(sizes.iter().cloned());
    inputs.extend(packages.iter().cloned());
    emit(&out, &inputs, &visionsim::readings_csv(&seen), lang)?;
    let failed = visionsim::part_failures(&seen);
    eprintln!(
        "{} readings, {} pictures without a part, {} parts without a size",
        seen.len() - failed.len(),
        failed.len(),
        without.len()
    );
    for f in &failed {
        eprintln!("no reading - {f}");
    }
    if !without.is_empty() {
        eprintln!("no size for the package of: {}", without.join(" "));
    }
    Ok(())
}

/// `vision-marks`: the measured places of the marks of a panel that a simulated camera gives, in the file that `fiducial-check` reads. The panel lies as `--shift X,Y`, `--turn DEG` and
/// `--scale S` say (the design as it is without them); every mark is a square of `--dot MM` (0.5 without it) seen from the place the design gives, with the noise, blur and failures of
/// the vision settings (`--vision`); `--seed` makes the readings repeat. A mark in whose picture the pipeline finds nothing gives no reading and is told on the error output.
pub fn vision_marks(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let out = flag(&mut args, "--out")?;
    let vision = vision_settings(&mut args, lang)?;
    let seed = seed_option(&mut args)?;
    let mut truth = visionsim::PanelTruth::default();
    if let Some(v) = flag(&mut args, "--shift")? {
        truth.shift = xy(&v, "--shift")?;
    }
    if let Some(v) = flag(&mut args, "--turn")? {
        truth.turn_deg = num(&v, "--turn")?;
    }
    if let Some(v) = flag(&mut args, "--scale")? {
        truth.scale = num(&v, "--scale")?;
        if !(0.5..=2.0).contains(&truth.scale) {
            return Err(format!("--scale {v} is not from 0.5 to 2"));
        }
    }
    let dot = match flag(&mut args, "--dot")? {
        Some(v) => num(&v, "--dot")?,
        None => 0.5,
    };
    if !(0.05..=50.0).contains(&dot) {
        return Err(format!("--dot {dot} is not from 0.05 to 50 millimetres"));
    }
    finish(&args, 1)?;
    let path = args
        .first()
        .ok_or("vision-marks needs the file of the design (id,x,y)")?
        .clone();
    let l = |e: eh3d_phoenix_pnp::error::PnpError| e.localized(lang);
    let design = fiducial::parse_measured(&read_text(&path, cfg.max_file_bytes)?).map_err(l)?;
    let seen = visionsim::read_marks(&design, &truth, &vision, dot, seed);
    emit(&out, &[path], &visionsim::marks_csv(&seen), lang)?;
    let failed = visionsim::mark_failures(&seen);
    eprintln!(
        "{} marks read, {} without a reading",
        seen.len() - failed.len(),
        failed.len()
    );
    for f in &failed {
        eprintln!("no reading - {f}");
    }
    Ok(())
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
    let (a, ids) = fidcheck::analyze_marks(&design, &measured, &cfg).map_err(l)?;
    print!("{}", fidcheck::report(&a, &ids));
    if a.fit.max > cfg.fiducial_max_residual_mm {
        return fail(format!(
            "a mark is {:.3} mm from where the fit puts it, the limit is {} mm",
            a.fit.max, cfg.fiducial_max_residual_mm
        ));
    }
    if !a.check.ok() {
        return fail("the marks put the panel somewhere unexpected".into());
    }
    Ok(())
}

/// `feeder-check`: whether the table of feeders agrees with itself. The exit code is 2 when there is an error (a warning alone does not fail).
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
        fail(format!("the table of feeders has {errors} errors"))
    } else {
        Ok(())
    }
}

/// `camera-shots LONG_MM SHORT_MM [--vision F] [--config F]`: the pictures needed to see a part of that size with the camera of the vision settings (the picture is `image_width` by
/// `image_height` pixels of `mm_per_px`, the long side of the part along `nominal_long_axis`), neighbours sharing `shot_overlap_pct` or more, and where the middle of each one is from
/// the middle of the part. It does not move anything and does not join pictures. The exit code is 2 when more than `shot_max` pictures are needed.
pub fn camera_shots(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let vision = match flag(&mut args, "--vision")? {
        Some(path) => VisionConfig::parse(&read_text(&path, MAX_VISION_BYTES as u64)?)
            .map_err(|e| e.localized(lang))?,
        None => VisionConfig::default(),
    };
    finish(&args, 2)?;
    let (Some(long), Some(short)) = (args.first(), args.get(1)) else {
        return Err("camera-shots needs the long and the short side of a part (mm)".into());
    };
    let side = |text: &String, what: &str| -> Result<f64, String> {
        let v = num(text, what)?;
        if !(0.05..=1000.0).contains(&v) {
            return Err(format!("{what} {text} is not from 0.05 to 1000"));
        }
        Ok(v)
    };
    let (long, short) = (side(long, "the long side")?, side(short, "the short side")?);
    let field = (
        vision.image_width as f64 * vision.mm_per_px,
        vision.image_height as f64 * vision.mm_per_px,
    );
    let part = match vision.nominal_long_axis {
        Axis::X => (long, short),
        Axis::Y => (short, long),
    };
    let plan = plan_shots(part, field, cfg.shot_overlap_pct / 100.0).ok_or_else(|| {
        format!("the part needs more than {MAX_SHOTS} pictures, or a size is not valid")
    })?;
    println!(
        "a part of {:.3} x {:.3} mm, a picture of {:.3} x {:.3} mm: {} pictures ({} across, {} down)",
        part.0,
        part.1,
        field.0,
        field.1,
        plan.shots.len(),
        plan.columns,
        plan.rows
    );
    for (i, (x, y)) in plan.shots.iter().enumerate() {
        println!("{},{x:.3},{y:.3}", i + 1);
    }
    if plan.shots.len() > cfg.shot_max as usize {
        return Err(not_passed(format!(
            "{} pictures are more than shot_max ({})",
            plan.shots.len(),
            cfg.shot_max
        )));
    }
    Ok(())
}
