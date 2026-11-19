// EH3D-PHOENIX-PNP - the commands that check, calibrate and make files: bom-check, bom-order, camera-calibrate, camera-pattern, camera-combined, package-kicad, place-learn, nozzle-calibrate, feeder-teach, panelize and polarity-sheet.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//
// Each one reads its files, calls the library and prints; none of them moves or writes anything but the file named by --out, and a file that exists is never written over.

use crate::cli_args::{finish, flag, num, switch};
use crate::commands::{read_text, refuse_same_file, write_atomic};
use eh3d_phoenix_pnp::bom::{
    buy_after_stock, buy_csv, compare, order as bom_order_lines, order_csv, Bom,
};
use eh3d_phoenix_pnp::calibrate::{
    calibrate_camera, fit_runout, parse_camera_points, parse_runout_samples, taught_csv,
    teach_file, teach_strip, teach_tray, Taught,
};
use eh3d_phoenix_pnp::combined::{
    allowed, calibrate_combined, parse_combined_touch, resolve_origin,
};
use eh3d_phoenix_pnp::config::{MachineConfig, MAX_CONFIG_BYTES};
use eh3d_phoenix_pnp::error::{Lang, PnpError};
use eh3d_phoenix_pnp::feeding;
use eh3d_phoenix_pnp::kicadmod;
use eh3d_phoenix_pnp::learn::{self, LearnSettings, Learned};
use eh3d_phoenix_pnp::nozzlelife;
use eh3d_phoenix_pnp::panel::{grid_panel, Grid};
use eh3d_phoenix_pnp::parts::{parse_all, Side};
use eh3d_phoenix_pnp::pattern::{calibrate_pattern, parse_pattern};
use eh3d_phoenix_pnp::pkggen;
use eh3d_phoenix_pnp::polarity;
use eh3d_phoenix_pnp::report;
use eh3d_phoenix_pnp::stock::stock_plan;
use std::path::Path;

/// The language and the configuration of a command: `--lang` wins over the key `language` of `--config`.
pub fn language_and_config(args: &mut Vec<String>) -> Result<(Lang, MachineConfig), String> {
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

/// `camera-pattern`: the size of a pixel and the bending of the lens from the points of a pattern on a grid seen by the camera; prints how good the fit is, with the bending and without it,
/// and the lines to put in the configuration. The exit code is 1 when a point is farther from the model than `camera_pattern_max_residual_mm`.
pub fn camera_pattern(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let pitch = match flag(&mut args, "--pitch")? {
        Some(v) => num(&v, "--pitch")?,
        None => cfg.camera_pattern_pitch_mm,
    };
    finish(&args, 1)?;
    let path = args
        .first()
        .ok_or("camera-pattern needs a file of the points of the pattern")?;
    let l = |e: PnpError| e.localized(lang);
    let points = parse_pattern(&read_text(path, cfg.max_file_bytes)?).map_err(l)?;
    let found = calibrate_pattern(&points, pitch).map_err(l)?;
    println!("{}", found.summary());
    print!("{}", found.config_lines());
    match found.warning(cfg.camera_pattern_max_residual_mm) {
        Some(w) => Err(w),
        None => Ok(()),
    }
}

/// `camera-combined`: the camera, the turn of the sheet and the bending of the lens from the points of a pattern that is fixed to the bed while the head moves (`head_x,head_y,col,row,px,py`);
/// prints how good the fit is and the lines to put in the configuration. Off until `camera_combined` is on. `--pitch MM` and `--origin X,Y` take the place of
/// `camera_pattern_pitch_mm` and of `camera_pattern_origin_x_mm`, `camera_pattern_origin_y_mm`; `--touch` (or `camera_combined_touch = on`) takes the origin from a row `origin,X,Y` of the file instead. The exit code is 1 when a sighting is over `camera_pattern_max_residual_mm`.
pub fn camera_combined(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let pitch = match flag(&mut args, "--pitch")? {
        Some(v) => num(&v, "--pitch")?,
        None => cfg.camera_pattern_pitch_mm,
    };
    let given = match flag(&mut args, "--origin")? {
        Some(v) => {
            let (x, y) = v
                .split_once(',')
                .ok_or_else(|| format!("--origin '{v}' is not X,Y"))?;
            Some((num(x, "--origin")?, num(y, "--origin")?))
        }
        None => None,
    };
    let touch = switch(&mut args, "--touch") || cfg.camera_combined_touch;
    finish(&args, 1)?;
    allowed(&cfg)?;
    let path = args
        .first()
        .ok_or("camera-combined needs a file of the sightings of the pattern")?;
    let l = |e: PnpError| e.localized(lang);
    let (rows, file_origin) =
        parse_combined_touch(&read_text(path, cfg.max_file_bytes)?).map_err(l)?;
    let origin = resolve_origin(&cfg, touch, file_origin, given).map_err(l)?;
    let found = calibrate_combined(&rows, pitch, origin).map_err(l)?;
    println!("{}", found.summary());
    print!("{}", found.config_lines());
    match found.warning(cfg.camera_pattern_max_residual_mm) {
        Some(w) => Err(w),
        None => Ok(()),
    }
}

/// `package-gen [NAME...] [--from PARTS] [--out F]`: the rows of a table of packages (`package,height_mm,tape_mm,body_long_mm,body_short_mm`) for the packages called NAME (chips by EIA code,
/// SOT, SOD, SOIC, TSSOP, QFN, DFN and QFP) and for those of a position file; the sizes are the nominal ones of the standards. The rows go to the screen or to `--out` (never over a file that
/// exists). The exit code is 1 when a name is not known; the ones that are known are written all the same.
pub fn package_gen(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let from = flag(&mut args, "--from")?;
    let out = flag(&mut args, "--out")?;
    let mut names: Vec<String> = Vec::new();
    for a in &args {
        if a.starts_with("--") {
            return Err(format!("{a} is not an option of package-gen"));
        }
        names.push(a.clone());
    }
    if let Some(path) = &from {
        let parsed = parse_all(&read_text(path, cfg.max_file_bytes)?, &cfg.profile)?;
        names.extend(parsed.parts.iter().map(|p| p.package.trim().to_string()));
    }
    if names.is_empty() {
        return Err("package-gen needs the names of packages or --from FILE".into());
    }
    if let (Some(o), Some(f)) = (&out, &from) {
        refuse_same_file(std::slice::from_ref(f), o)?;
    }
    let mut seen: Vec<String> = Vec::new();
    let (mut specs, mut unknown) = (Vec::new(), Vec::new());
    for n in names.iter().filter(|n| !n.is_empty()) {
        let key = n.to_ascii_lowercase();
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        match pkggen::generate(n) {
            Ok(s) => specs.push(s),
            Err(e) => unknown.push(e.localized(lang)),
        }
    }
    let csv = pkggen::table_csv(&specs);
    match &out {
        Some(o) => report::write_new(o, &csv).map_err(|e| e.localized(lang))?,
        None => print!("{csv}"),
    }
    for u in &unknown {
        eprintln!("{u}");
    }
    if unknown.is_empty() {
        Ok(())
    } else {
        Err(format!("{} packages are not known", unknown.len()))
    }
}

/// The most footprints that `package-kicad` reads in one run.
const MAX_FOOTPRINTS: usize = 2000;

/// `package-kicad PATH... [--height MM] [--out F]`: the rows of a table of packages (`package,height_mm,tape_mm,body_long_mm,body_short_mm`) from footprints of KiCad. A PATH is a `.kicad_mod`
/// file or a folder (a `.pretty` library, not searched deeper) of them; the body is the drawing on the fabrication layer. The height is that of the package when the name is a known one, or
/// `--height`. The rows go to the screen or to `--out` (never over a file that exists); a footprint that is not understood is named on the error output with its reason, the others are written
/// all the same, and the exit code is 1.
pub fn package_kicad(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let height = match flag(&mut args, "--height")? {
        Some(v) => Some(num(&v, "--height")?),
        None => None,
    };
    let out = flag(&mut args, "--out")?;
    if let Some(a) = args.iter().find(|a| a.starts_with("--")) {
        return Err(format!("{a} is not an option of package-kicad"));
    }
    if args.is_empty() {
        return Err("package-kicad needs footprint files (.kicad_mod) or a folder of them".into());
    }
    let mut files: Vec<String> = Vec::new();
    for a in &args {
        let path = Path::new(a);
        if path.is_dir() {
            let mut found: Vec<String> = std::fs::read_dir(path)
                .map_err(|e| format!("{a}: {e}"))?
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| {
                    p.is_file()
                        && p.extension()
                            .is_some_and(|x| x.eq_ignore_ascii_case("kicad_mod"))
                })
                .map(|p| p.to_string_lossy().into_owned())
                .collect();
            found.sort();
            files.extend(found);
        } else {
            files.push(a.clone());
        }
        if files.len() > MAX_FOOTPRINTS {
            return Err(format!(
                "more than {MAX_FOOTPRINTS} footprints: read them in several runs"
            ));
        }
    }
    if files.is_empty() {
        return Err("there is no .kicad_mod file in the folder".into());
    }
    if let Some(o) = &out {
        refuse_same_file(&files, o)?;
    }
    let limit = cfg.max_file_bytes.min(kicadmod::MAX_BYTES as u64);
    let (mut specs, mut problems): (Vec<pkggen::PackageSpec>, Vec<String>) =
        (Vec::new(), Vec::new());
    for f in &files {
        let spec = read_text(f, limit).and_then(|text| {
            let fp = kicadmod::parse_footprint(&text).map_err(|e| e.localized(lang))?;
            if let Some(n) = fp.note() {
                eprintln!("note: {f}: {n}");
            }
            kicadmod::package_spec(&fp, height).map_err(|e| e.localized(lang))
        });
        match spec {
            Ok(s) if specs.iter().any(|o| o.name.eq_ignore_ascii_case(&s.name)) => {
                problems.push(format!("{f}: the package '{}' is listed twice", s.name));
            }
            Ok(s) => specs.push(s),
            Err(e) => problems.push(format!("{f}: {e}")),
        }
    }
    let csv = pkggen::table_csv(&specs);
    match &out {
        Some(o) => report::write_new(o, &csv).map_err(|e| e.localized(lang))?,
        None => print!("{csv}"),
    }
    for p in &problems {
        eprintln!("{p}");
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(format!("{} footprints were not read", problems.len()))
    }
}

/// `place-learn READINGS --state F [--residual] [--dry-run]`, `place-learn --state F --show` and `place-learn --state F --forget PACKAGE`: the offset of the placement of each package, learned
/// from readings (`package,dx_mm,dy_mm`: how far a part ended from its place) and kept in the file of `--state`, which is written beside and renamed. Learning is off until `place_learn` is on
/// (showing and forgetting are not). The strays are left out, a package with too few readings is not touched, and no offset is larger than `place_learn_cap_mm`. `--residual` says that the
/// readings were taken on parts placed with the offsets already applied. `--dry-run` shows what would change and writes nothing.
pub fn place_learn(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let state = flag(&mut args, "--state")?
        .ok_or("place-learn needs --state FILE (the file of the learned offsets)")?;
    let forget = flag(&mut args, "--forget")?;
    let show = switch(&mut args, "--show");
    let residual = switch(&mut args, "--residual");
    let dry = switch(&mut args, "--dry-run");
    finish(&args, 1)?;
    let l = |e: PnpError| e.localized(lang);
    let mut learned = if Path::new(&state).exists() {
        Learned::parse(&read_text(&state, cfg.max_file_bytes)?).map_err(l)?
    } else {
        Learned::new()
    };
    if show {
        if !args.is_empty() || forget.is_some() || residual || dry {
            return Err("--show is used alone, with --state".into());
        }
        println!("{}", learned.summary());
        return Ok(());
    }
    if let Some(name) = forget {
        if !args.is_empty() || residual {
            return Err("--forget is used with --state only".into());
        }
        if !learned.forget(&name) {
            return Err(format!("nothing is learned of the package '{name}'"));
        }
        if !dry {
            write_atomic(&state, &learned.to_text())?;
        }
        println!("{}", learned.summary());
        return Ok(());
    }
    learn::allowed(&cfg)?;
    let path = args
        .first()
        .ok_or("place-learn needs a file of readings (package,dx_mm,dy_mm)")?;
    refuse_same_file(std::slice::from_ref(path), &state)?;
    let readings = learn::parse_readings(&read_text(path, cfg.max_file_bytes)?).map_err(l)?;
    if readings.is_empty() {
        return Err("the file has no readings".into());
    }
    let outcomes = learned
        .learn(&readings, &LearnSettings::from_config(&cfg), residual)
        .map_err(l)?;
    for o in &outcomes {
        println!("{}", o.line());
    }
    let changed = outcomes.iter().filter(|o| o.skipped.is_none()).count();
    if dry {
        println!("{changed} packages would change; nothing was written (--dry-run)");
    } else if changed > 0 {
        write_atomic(&state, &learned.to_text())?;
        println!("{changed} packages learned; written to {state}");
    } else {
        println!("nothing was learned; {state} was not written");
    }
    if changed == 0 {
        Err("no package had enough readings".into())
    } else {
        Ok(())
    }
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
    let (board_width_mm, board_height_mm) = match flag(&mut args, "--board-size")? {
        Some(v) => pair(&v, "--board-size")?,
        None => (0.0, 0.0),
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
        board_width_mm,
        board_height_mm,
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
    let have_path = flag(&mut args, "--have")?;
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
    let total: usize = lines.iter().map(|x| x.total).sum();
    let mut summary = format!(
        "{} lines, {total} parts for {boards} boards with {spare_percent}% spare (at least {spare_min})",
        lines.len()
    );
    // with the feeders that are loaded, only what is missing is to be bought
    let csv = match &have_path {
        Some(h) => {
            let feeders = feeding::from_csv_with(&read_text(h, cfg.max_file_bytes)?, cfg.tray_fill)
                .map_err(l)?;
            let stock: Vec<(String, String, u32)> = feeders
                .iter()
                .filter(|f| f.enabled)
                .map(|f| (f.base.value.clone(), f.base.package.clone(), f.base.count))
                .collect();
            let rows = buy_after_stock(&lines, &stock);
            let (held, buy): (usize, usize) = rows
                .iter()
                .fold((0, 0), |(h, b), r| (h + r.on_hand, b + r.to_buy));
            summary.push_str(&format!("; {held} are in the feeders, {buy} to buy"));
            buy_csv(&rows)
        }
        None => order_csv(&lines),
    };
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

/// `nozzle-life`: the use of the nozzle tips kept in a file, with the tips that have placed `nozzle_life_warn` parts or more; `--reset TIP` records that a tip was replaced (the only way the
/// counts of a tip go down). The exit code is 1 when a tip is at or over the limit.
pub fn nozzle_life(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let warn_at = match flag(&mut args, "--warn")? {
        Some(v) => v
            .trim()
            .parse::<u64>()
            .map_err(|_| format!("--warn '{v}' is not a whole number"))?,
        None => cfg.nozzle_life_warn,
    };
    let reset = flag(&mut args, "--reset")?;
    finish(&args, 1)?;
    let path = args
        .first()
        .ok_or("nozzle-life needs the file of the use of the tips")?
        .clone();
    if !Path::new(&path).is_file() {
        return Err(format!("{path}: there is no such file"));
    }
    let l = |e: PnpError| e.localized(lang);
    let mut life = nozzlelife::read_file(&path, cfg.max_file_bytes).map_err(l)?;
    if let Some(tip) = reset {
        if !life.reset(&tip) {
            return Err(format!("the tip '{tip}' is not in {path}"));
        }
        nozzlelife::write_file(&path, &life).map_err(l)?;
        println!("ok: the tip {tip} was replaced, its counts are 0; written to {path}");
    }
    print!("{}", life.text(warn_at));
    let over = life.over(warn_at).len();
    if over > 0 {
        Err(format!(
            "{over} tips have placed {warn_at} parts or more (nozzle_life_warn)"
        ))
    } else {
        Ok(())
    }
}

/// `stock`: whether the feeders that are loaded hold enough parts for a number of boards, kind by kind. The findings go to the standard output (or the table as a CSV to `--out`, never over a
/// file that exists); the exit code is 1 when a kind falls short.
pub fn stock(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let boards = whole(&mut args, "--boards", cfg.order_boards)?;
    let side = match flag(&mut args, "--side")?.as_deref() {
        None => None,
        Some("top") => Some(Side::Top),
        Some("bottom") => Some(Side::Bottom),
        Some(other) => return Err(format!("--side '{other}' is not top or bottom")),
    };
    let out_path = flag(&mut args, "--out")?;
    finish(&args, 2)?;
    let (parts_path, feeders_path) = match (args.first(), args.get(1)) {
        (Some(a), Some(b)) => (a.clone(), b.clone()),
        _ => return Err("stock needs a table of parts and a table of feeders".into()),
    };
    let l = |e: PnpError| e.localized(lang);
    let parsed = parse_all(&read_text(&parts_path, cfg.max_file_bytes)?, &cfg.profile)?;
    let feeders = feeding::from_csv_with(
        &read_text(&feeders_path, cfg.max_file_bytes)?,
        cfg.tray_fill,
    )
    .map_err(l)?;
    let plan = stock_plan(&parsed.parts, &feeders, side, boards).map_err(l)?;
    print!("{}", plan.text());
    if let Some(out) = out_path {
        refuse_same_file(&[parts_path, feeders_path], &out)?;
        report::write_new(&out, &plan.csv()).map_err(l)?;
        println!("written to {out}");
    }
    if plan.is_enough() {
        Ok(())
    } else {
        Err(format!(
            "the feeders lack {} parts for {boards} boards",
            plan.short_parts()
        ))
    }
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
    let (lang, cfg) = language_and_config(&mut args)?;
    let l = |e: PnpError| e.localized(lang);
    // a file with the measures of several feeders: one line for each and the columns of the file of feeders that they set
    if let Some(path) = flag(&mut args, "--measures")? {
        finish(&args, 0)?;
        let rows = teach_file(&read_text(&path, cfg.max_file_bytes)?).map_err(l)?;
        for (id, t) in &rows {
            match t {
                Taught::Strip(s) => println!("{id}: {}", s.summary()),
                Taught::Tray(t) => println!("{id}: {}", t.summary()),
            }
        }
        print!("{}", taught_csv(&rows));
        return Ok(());
    }
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
