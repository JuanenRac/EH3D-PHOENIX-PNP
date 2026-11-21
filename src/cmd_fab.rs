// EH3D-PHOENIX-PNP - the commands of the files for assembly: export-fab, fab-check, convert-cpl, ipc-name, footprint-find, dfm-check, stencil-apertures, sim-busy and sim-tower.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//
// Each one reads its files, calls the library and prints; none of them moves a machine or writes anything but the files named by --out, --cpl-out and --bom-out, and a file that exists is
// never written over.

use crate::cli_args::{finish, flag, num, switch};
use crate::cmd_flow::deliver;
use crate::cmd_nodes::fail;
use crate::cmd_tools::language_and_config;
use crate::commands::read_text;
use eh3d_phoenix_pnp::asmexport::{self, Format, Sides, Unit};
use eh3d_phoenix_pnp::bom::Bom;
use eh3d_phoenix_pnp::busy;
use eh3d_phoenix_pnp::catalog::Catalog;
use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::cplconv::{self, Conversion, Dialect};
use eh3d_phoenix_pnp::dfm;
use eh3d_phoenix_pnp::fpsearch;
use eh3d_phoenix_pnp::ipcname::{self, Density, IpcName, Shape};
use eh3d_phoenix_pnp::jlcpcb::{self, DnpMode, Options};
use eh3d_phoenix_pnp::parts::{parse_all, BottomRotation};
use eh3d_phoenix_pnp::stencil;
use eh3d_phoenix_pnp::tower;

/// A pair `X,Y` from the text of an option.
fn pair(text: &str, what: &str) -> Result<(f64, f64), String> {
    let (x, y) = text
        .split_once(',')
        .ok_or_else(|| format!("{what} {text} is not X,Y"))?;
    Ok((num(x, what)?, num(y, what)?))
}

/// The options of the files for an assembly house: the settings, then what the command line says.
fn fab_options(args: &mut Vec<String>, cfg: &MachineConfig) -> Result<Options, String> {
    let dnp_mode = match flag(args, "--dnp-mode")? {
        Some(v) => {
            DnpMode::parse(&v).ok_or_else(|| format!("--dnp-mode {v} is not column or omit"))?
        }
        None => cfg.fab_dnp_mode,
    };
    let side = match flag(args, "--side")? {
        Some(v) => match Sides::parse(&v) {
            Some(Sides::All) => None,
            Some(Sides::Only(s)) => Some(s),
            None => return Err(format!("--side {v} is not all, top or bottom")),
        },
        None => None,
    };
    Ok(Options {
        tht_words: cfg.fab_tht_words.clone(),
        dnp_mode,
        dnp_column: cfg.fab_dnp_column.clone(),
        mount_column: switch(args, "--mount"),
        side,
    })
}

/// `export-fab PARTS [--bom BOM] [--side all|top|bottom] [--mount] [--dnp-mode column|omit] [--cpl-out F] [--bom-out F]`: the placement list and the bill of materials in the shape that board
/// assembly houses ask for. With neither `--cpl-out` nor `--bom-out` the placement list is printed. The part numbers of the bill come from `--bom` (the bill of materials of the CAD program).
pub fn export_fab(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let options = fab_options(&mut args, &cfg)?;
    let bom_path = flag(&mut args, "--bom")?;
    let cpl_out = flag(&mut args, "--cpl-out")?;
    let bom_out = flag(&mut args, "--bom-out")?;
    finish(&args, 1)?;
    let path = args.first().ok_or("export-fab needs a table of parts")?;
    let parsed = parse_all(&read_text(path, cfg.max_file_bytes)?, &cfg.profile)?;
    let mut inputs = vec![path.clone()];
    let source = match &bom_path {
        Some(p) => {
            inputs.push(p.clone());
            Some(
                Bom::parse(&read_text(p, cfg.max_file_bytes)?, 100_000)
                    .map_err(|e| e.localized(lang))?,
            )
        }
        None => None,
    };
    let list = jlcpcb::cpl(&parsed.parts, &parsed.dnp, &options);
    let problems = jlcpcb::check_cpl(&list);
    if let Some(p) = problems.first() {
        return fail(format!("line {}: {}", p.line, p.what));
    }
    if cpl_out.is_some() || bom_out.is_none() {
        deliver(&list, cpl_out.as_deref(), &inputs, lang)?;
    }
    if let Some(o) = &bom_out {
        let rows = jlcpcb::bom_rows(&parsed.parts, &parsed.dnp, source.as_ref(), &options);
        deliver(&jlcpcb::bom_csv(&rows, &options), Some(o), &inputs, lang)?;
    }
    if cpl_out.is_some() || bom_out.is_some() {
        println!(
            "{} parts in the placement list",
            jlcpcb::count(&parsed.parts, &parsed.dnp, &options)
        );
    }
    Ok(())
}

/// `fab-check FILE`: reads a placement list and says what an assembly house would refuse (a list in a designator cell, a designator twice, a layer that is not Top or Bottom, a place that is
/// not a number). The exit code is 2 when there is a line.
pub fn fab_check(mut args: Vec<String>) -> Result<(), String> {
    let (_, cfg) = language_and_config(&mut args)?;
    finish(&args, 1)?;
    let path = args.first().ok_or("fab-check needs a placement list")?;
    let problems = jlcpcb::check_cpl(&read_text(path, cfg.max_file_bytes)?);
    for p in &problems {
        println!("line {}: {}", p.line, p.what);
    }
    match problems.len() {
        0 => {
            println!("the placement list would be accepted");
            Ok(())
        }
        n => fail(format!("{n} lines would be refused")),
    }
}

/// `convert-cpl FILE [--from kicad|altium|eagle|jlcpcb|generic] [--to jlcpcb|cpl|kicad-pos|generic] [--origin X,Y] [--flip-y] [--cw] [--rot-offset DEG] [--bottom keep|mirror|half_turn] [--out F]`:
/// a position file of a CAD program brought to the origin and the rotation of another. The settings `cpl_*` are the defaults of the options.
pub fn convert_cpl(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let from =
        match flag(&mut args, "--from")? {
            Some(v) => Some(Dialect::parse(&v).ok_or_else(|| {
                format!("--from {v} is not kicad, altium, eagle, jlcpcb or generic")
            })?),
            None => None,
        };
    let to = flag(&mut args, "--to")?.unwrap_or_else(|| "jlcpcb".to_string());
    let origin = match flag(&mut args, "--origin")? {
        Some(v) => pair(&v, "--origin")?,
        None => (cfg.cpl_origin_x_mm, cfg.cpl_origin_y_mm),
    };
    let rotation_offset = match flag(&mut args, "--rot-offset")? {
        Some(v) => num(&v, "--rot-offset")?,
        None => cfg.cpl_rotation_offset_deg,
    };
    let bottom = match flag(&mut args, "--bottom")? {
        Some(v) => match v.to_ascii_lowercase().as_str() {
            "keep" => BottomRotation::Keep,
            "mirror" => BottomRotation::Mirror,
            "half_turn" => BottomRotation::HalfTurn,
            _ => return Err(format!("--bottom {v} is not keep, mirror or half_turn")),
        },
        None => cfg.cpl_bottom_rotation,
    };
    let conversion = Conversion {
        origin_x: origin.0,
        origin_y: origin.1,
        flip_y: switch(&mut args, "--flip-y") || cfg.cpl_flip_y,
        clockwise: switch(&mut args, "--cw") || cfg.cpl_rotation_cw,
        rotation_offset,
        bottom,
    };
    let out = flag(&mut args, "--out")?;
    let options = fab_options(&mut args, &cfg)?;
    finish(&args, 1)?;
    let path = args.first().ok_or("convert-cpl needs a position file")?;
    let read = cplconv::read_with(
        &read_text(path, cfg.max_file_bytes)?,
        from,
        &cfg.fab_dnp_column,
    )
    .map_err(|e| e.localized(lang))?;
    let parts = cplconv::convert(&read.parts, &conversion).map_err(|e| e.localized(lang))?;
    let dnp = cplconv::convert(&read.dnp, &conversion).map_err(|e| e.localized(lang))?;
    let text = match to.to_ascii_lowercase().as_str() {
        "jlcpcb" => jlcpcb::cpl(&parts, &dnp, &options),
        other => {
            let format = Format::parse(other)
                .ok_or_else(|| format!("--to {to} is not jlcpcb, cpl, kicad-pos or generic"))?;
            asmexport::write(&parts, format, Unit::Mm, Sides::All)
        }
    };
    let note = format!(
        "read as {}: {} parts ({} not to be mounted)",
        read.dialect.name(),
        read.parts.len(),
        read.dnp.len()
    );
    deliver(&text, out.as_deref(), std::slice::from_ref(path), lang)?;
    if out.is_some() {
        println!("{note}");
    } else {
        eprintln!("{note}");
    }
    Ok(())
}

/// A whole number from the text of an option, in a range.
fn whole(text: &str, what: &str, lo: u32, hi: u32) -> Result<u32, String> {
    text.trim()
        .parse::<u32>()
        .ok()
        .filter(|v| (lo..=hi).contains(v))
        .ok_or_else(|| format!("{what} {text} is not a whole number from {lo} to {hi}"))
}

/// `ipc-name NAME... [--density L|N|M]` or `ipc-name --make FAMILY ...`: a name of the convention of IPC-7351B taken apart and written the one way, or the name of a chip package of the table
/// (`R_0603`) in that convention, or a name made from the sizes (`--make RESC --length 1.6 --width 0.8 --height 0.55`, `--make SOIC --pitch 1.27 --span 6 --height 1.75 --pins 8`,
/// `--make QFP --pitch 0.5 --length 10 --width 10 --height 1.6 --pins 48`). The exit code is 2 when a name is not understood.
pub fn ipc_name(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let asked = match flag(&mut args, "--density")? {
        Some(v) => {
            Some(Density::parse(&v).ok_or_else(|| format!("--density {v} is not L, N or M"))?)
        }
        None => None,
    };
    let density = asked.unwrap_or(cfg.ipc_density);
    if let Some(family) = flag(&mut args, "--make")? {
        let mut size = |name: &str| -> Result<Option<f64>, String> {
            flag(&mut args, name)?.map(|v| num(&v, name)).transpose()
        };
        let (pitch, span) = (size("--pitch")?, size("--span")?);
        let (length, width, height) = (size("--length")?, size("--width")?, size("--height")?);
        let pins = flag(&mut args, "--pins")?
            .map(|v| whole(&v, "--pins", 1, 10_000))
            .transpose()?;
        finish(&args, 0)?;
        let need = |v: Option<f64>, what: &str| v.ok_or_else(|| format!("--make needs {what}"));
        let shape = match (pitch, span, pins) {
            (Some(p), Some(s), Some(n)) => Shape::Dual {
                pitch_mm: p,
                span_mm: s,
                height_mm: need(height, "--height")?,
                pins: n,
            },
            (Some(p), None, Some(n)) => Shape::Quad {
                pitch_mm: p,
                length_mm: need(length, "--length")?,
                width_mm: need(width, "--width")?,
                height_mm: need(height, "--height")?,
                pins: n,
            },
            (None, None, None) => Shape::Chip {
                length_mm: need(length, "--length")?,
                width_mm: need(width, "--width")?,
                height_mm: need(height, "--height")?,
            },
            _ => {
                return Err(
                    "--make takes a chip (--length --width --height), a part with leads on two sides (--pitch --span --height --pins) or on four (--pitch --length --width --height --pins)"
                        .into(),
                )
            }
        };
        let made = IpcName {
            family: family.to_ascii_uppercase(),
            shape,
            density,
        };
        println!("{}", made.name().map_err(|e| e.localized(lang))?);
        return Ok(());
    }
    finish(&args, 1000)?;
    if args.is_empty() {
        return Err("ipc-name needs a name or --make".into());
    }
    let mut failed = 0usize;
    for name in &args {
        let parsed = ipcname::parse(name)
            .map(|mut n| {
                n.density = asked.unwrap_or(n.density);
                (n, false)
            })
            .or_else(|_| ipcname::from_package(name, density).map(|n| (n, true)));
        match parsed {
            Ok((n, from_package)) => {
                let written = n.name().map_err(|e| e.localized(lang))?;
                let how = if from_package {
                    "made from the package"
                } else {
                    "read"
                };
                println!("{name} -> {written} ({how}: {})", ipcname::describe(&n));
            }
            Err(e) => {
                failed += 1;
                eprintln!("{name}: {}", e.localized(lang));
            }
        }
    }
    match failed {
        0 => Ok(()),
        n => fail(format!("{n} names were not understood")),
    }
}

/// `footprint-find QUERY... [--packages F] [--catalog F] [--limit N] [--distance N]`: the names of the table of packages that are the query, another name for the same footprint (`R_0603`,
/// `Resistor_SMD:R_0603_1608Metric` and `RESC1608X55N` are one), hold the words or are a few letters away. The exit code is 2 when a query finds nothing.
pub fn footprint_find(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let limit = match flag(&mut args, "--limit")? {
        Some(v) => whole(&v, "--limit", 1, 1000)? as usize,
        None => cfg.ipc_search_limit as usize,
    };
    let distance = match flag(&mut args, "--distance")? {
        Some(v) => whole(&v, "--distance", 0, 8)? as usize,
        None => cfg.ipc_search_distance as usize,
    };
    let mut catalog = Catalog::new();
    if let Some(p) = flag(&mut args, "--packages")? {
        catalog
            .add_packages(&read_text(&p, cfg.max_file_bytes)?)
            .map_err(|e| e.localized(lang))?;
    }
    if let Some(p) = flag(&mut args, "--catalog")? {
        catalog
            .add_parts(&read_text(&p, cfg.max_file_bytes)?)
            .map_err(|e| e.localized(lang))?;
    }
    finish(&args, 1000)?;
    if args.is_empty() {
        return Err("footprint-find needs a name to look for".into());
    }
    let library = catalog.package_names();
    let mut empty = 0usize;
    for query in &args {
        let hits = fpsearch::search(&library, query, distance, limit);
        println!("{query}:");
        if hits.is_empty() {
            empty += 1;
            println!("  no name of the table matches");
        }
        for line in fpsearch::text(&hits).lines() {
            println!("  {line}");
        }
    }
    match empty {
        0 => Ok(()),
        n => fail(format!("{n} queries found nothing")),
    }
}

/// `dfm-check PARTS FEATURES [--packages F] [--catalog F] [--out F]`: the checks of the board for assembly, in the language of `--lang`. `FEATURES` is a table of `kind,id,x_mm,y_mm,w_mm,h_mm,side`
/// with the outline, the pads, the holes, the ends of tracks, the fiducials, the tooling holes and the tabs. The limits are the settings `dfm_*`. The exit code is 2 when there is a warning or an
/// error.
pub fn dfm_check(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let mut inputs = Vec::new();
    let mut catalog = Catalog::new();
    if let Some(p) = flag(&mut args, "--packages")? {
        catalog
            .add_packages(&read_text(&p, cfg.max_file_bytes)?)
            .map_err(|e| e.localized(lang))?;
        inputs.push(p);
    }
    if let Some(p) = flag(&mut args, "--catalog")? {
        catalog
            .add_parts(&read_text(&p, cfg.max_file_bytes)?)
            .map_err(|e| e.localized(lang))?;
        inputs.push(p);
    }
    let out = flag(&mut args, "--out")?;
    finish(&args, 2)?;
    let [parts_path, features_path] = args.as_slice() else {
        return Err("dfm-check needs a table of parts and a table of features".into());
    };
    inputs.push(parts_path.clone());
    inputs.push(features_path.clone());
    let parts = parse_all(&read_text(parts_path, cfg.max_file_bytes)?, &cfg.profile)?.parts;
    let features = dfm::parse_features(&read_text(features_path, cfg.max_file_bytes)?)
        .map_err(|e| e.localized(lang))?;
    let report = dfm::check(&parts, &features, &catalog, &dfm::Limits::from_config(&cfg));
    print!("{}", report.text(lang));
    if let Some(o) = &out {
        deliver(&report.csv(lang), Some(o), &inputs, lang)?;
    }
    match report.problems() {
        0 => Ok(()),
        n => fail(format!("{n} findings need a look")),
    }
}

/// `stencil-apertures FEATURES [--out F]`: the openings of the paste stencil for the pads of a table of features (reduced by the settings `stencil_*`, the thermal pads split into windows) and
/// the warnings, in the language of `--lang`. `--out` writes the openings. The exit code is 2 when there is a warning.
pub fn stencil_apertures(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let out = flag(&mut args, "--out")?;
    finish(&args, 1)?;
    let path = args
        .first()
        .ok_or("stencil-apertures needs a table of features")?;
    let features = dfm::parse_features(&read_text(path, cfg.max_file_bytes)?)
        .map_err(|e| e.localized(lang))?;
    let made = stencil::make(&features, &stencil::Settings::from_config(&cfg));
    print!("{}", made.text(lang));
    if let Some(o) = &out {
        deliver(&made.csv(), Some(o), std::slice::from_ref(path), lang)?;
    }
    match made.warnings.len() {
        0 => Ok(()),
        n => fail(format!("{n} warnings")),
    }
}

/// `sim-busy JOURNAL [--out F]`: how busy each nozzle and feeder was in a simulated job and where the machine waited, read from the journal that `simulate --journal` writes. The longest
/// waits that are listed are `busy_gaps_listed`. The exit code is 2 when a step did not end well.
pub fn sim_busy(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let out = flag(&mut args, "--out")?;
    finish(&args, 1)?;
    let path = args.first().ok_or("sim-busy needs a journal")?;
    let steps =
        busy::read_journal(&read_text(path, cfg.max_file_bytes)?).map_err(|e| e.localized(lang))?;
    let found = busy::analyse(&steps, cfg.busy_gaps_listed as usize);
    print!("{}", found.text());
    if let Some(o) = &out {
        deliver(&found.csv(), Some(o), std::slice::from_ref(path), lang)?;
    }
    match found.failed {
        0 => Ok(()),
        n => fail(format!("{n} steps did not end well")),
    }
}

/// `sim-tower JOURNAL [--out F]`: the light a signal tower would show at each second of a simulated job: red for a step that failed, yellow for a slow step (`tower_slow_factor`) and for a
/// wait over `tower_idle_gap_s`, green the rest. `--out` writes the intervals. The exit code is 2 when the tower was red.
pub fn sim_tower(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let out = flag(&mut args, "--out")?;
    finish(&args, 1)?;
    let path = args.first().ok_or("sim-tower needs a journal")?;
    let steps =
        busy::read_journal(&read_text(path, cfg.max_file_bytes)?).map_err(|e| e.localized(lang))?;
    let intervals = tower::timeline(&steps, cfg.tower_idle_gap_s, cfg.tower_slow_factor);
    print!("{}", tower::text(&intervals));
    if let Some(o) = &out {
        deliver(
            &tower::csv(&intervals),
            Some(o),
            std::slice::from_ref(path),
            lang,
        )?;
    }
    if tower::seconds(&intervals)[2] > 0.0 {
        return fail("the tower was red".into());
    }
    Ok(())
}
