// EH3D-PHOENIX-PNP - the commands of the flow of a job that need no machine: camera-selftest, machine-template, lot-trace, export-assembler, split-job, failure-odds and pos-transform.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//
// Each one reads its files, calls the library and prints; none of them moves a machine or writes anything but the file named by --out, and a file that exists is never written over.

use crate::cli_args::{finish, flag, num, switch};
use crate::cmd_tools::language_and_config;
use crate::commands::{read_text, refuse_same_file};
use eh3d_phoenix_pnp::asmexport::{self, Format, Sides, Unit};
use eh3d_phoenix_pnp::calibsim;
use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::error::Lang;
use eh3d_phoenix_pnp::failodds;
use eh3d_phoenix_pnp::feeding::{self, FeederSpec, Picks};
use eh3d_phoenix_pnp::lottrace;
use eh3d_phoenix_pnp::parts::{parse_all, Part};
use eh3d_phoenix_pnp::reorient::{self, Transform};
use eh3d_phoenix_pnp::report;
use eh3d_phoenix_pnp::splitjob;
use eh3d_phoenix_pnp::template;

/// A whole number in a range, from the text of an option.
fn whole(text: &str, what: &str, lo: f64, hi: f64) -> Result<u64, String> {
    let n = num(text, what)?;
    if n.fract() != 0.0 || !(lo..=hi).contains(&n) {
        return Err(format!(
            "{what} {text} is not a whole number from {lo} to {hi}"
        ));
    }
    Ok(n as u64)
}

/// A pair `X,Y` from the text of an option.
fn pair(text: &str, what: &str) -> Result<(f64, f64), String> {
    let (x, y) = text
        .split_once(',')
        .ok_or_else(|| format!("{what} {text} is not X,Y"))?;
    Ok((num(x, what)?, num(y, what)?))
}

/// Writes `text` to `out` when it is given, else prints it. A file that exists is never written over, and never one of the inputs.
fn deliver(text: &str, out: Option<&str>, inputs: &[String], lang: Lang) -> Result<(), String> {
    match out {
        Some(o) => {
            refuse_same_file(inputs, o)?;
            report::write_new(o, text).map_err(|e| e.localized(lang))?;
            println!("written to {o}");
        }
        None => print!("{text}"),
    }
    Ok(())
}

/// The table of parts and of feeders of a command that needs both, with the feeder of every part.
struct Tables {
    parts: Vec<Part>,
    feeders: Vec<FeederSpec>,
    picks: Picks,
    inputs: Vec<String>,
}

fn tables(cfg: &MachineConfig, parts_path: &str, feeders_path: &str) -> Result<Tables, String> {
    let parsed = parse_all(&read_text(parts_path, cfg.max_file_bytes)?, &cfg.profile)?;
    let feeders =
        feeding::from_csv_with(&read_text(feeders_path, cfg.max_file_bytes)?, cfg.tray_fill)
            .map_err(|e| e.localized(cfg.language))?;
    let picks = feeding::assign(&parsed.parts, &feeders, None);
    Ok(Tables {
        parts: parsed.parts,
        feeders,
        picks,
        inputs: vec![parts_path.to_string(), feeders_path.to_string()],
    })
}

/// `camera-selftest [--points N] [--noise PX] [--field PX] [--seed S] [--tol MM] [--out F]`: makes up the camera of the settings, looks at a mark in places chosen by chance, fits the camera as
/// `camera-calibrate` does and says how far the one that was found is from the one that was made up. `--out` writes the sightings as the file that `camera-calibrate` reads. The exit code
/// is 1 when the error is over `--tol` (0.05 mm by default) anywhere on the picture.
pub fn camera_selftest(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let mut plan = calibsim::Plan {
        seed: cfg.sim_seed,
        ..calibsim::Plan::default()
    };
    if let Some(v) = flag(&mut args, "--points")? {
        plan.points = whole(&v, "--points", 3.0, calibsim::MAX_POINTS as f64)? as usize;
    }
    if let Some(v) = flag(&mut args, "--noise")? {
        let n = num(&v, "--noise")?;
        if !(0.0..=50.0).contains(&n) {
            return Err(format!("--noise {v} is not from 0 to 50 pixels"));
        }
        plan.noise_px = n;
    }
    if let Some(v) = flag(&mut args, "--field")? {
        let n = num(&v, "--field")?;
        if !(10.0..=5000.0).contains(&n) {
            return Err(format!("--field {v} is not from 10 to 5000 pixels"));
        }
        plan.half_px = n;
    }
    if let Some(v) = flag(&mut args, "--seed")? {
        plan.seed = whole(&v, "--seed", 0.0, 9_007_199_254_740_992.0)?;
    }
    let tol = match flag(&mut args, "--tol")? {
        Some(v) => {
            let n = num(&v, "--tol")?;
            if !(0.0001..=10.0).contains(&n) {
                return Err(format!("--tol {v} is not from 0.0001 to 10 mm"));
            }
            n
        }
        None => 0.05,
    };
    let out = flag(&mut args, "--out")?;
    finish(&args, 0)?;
    if let Some(o) = &out {
        if std::path::Path::new(o).exists() {
            return Err(eh3d_phoenix_pnp::error::PnpError::new(
                eh3d_phoenix_pnp::error::Code::ReportExists,
                o.as_str(),
            )
            .localized(lang));
        }
    }
    let test = calibsim::selftest(&cfg.camera, &plan).map_err(|e| e.localized(lang))?;
    print!("{}", test.text(tol));
    if let Some(o) = &out {
        report::write_new(o, &test.csv()).map_err(|e| e.localized(lang))?;
        println!("written to {o}");
    }
    if test.within(tol) {
        Ok(())
    } else {
        Err(format!(
            "the camera that was found is {:.4} mm from the true one, over {tol} mm",
            test.max_mm
        ))
    }
}

/// `machine-template [NAME] [--out F]`: with no name, the templates there are; with one, its settings, printed or written to `--out` (never over a file that exists).
pub fn machine_template(mut args: Vec<String>) -> Result<(), String> {
    let out = flag(&mut args, "--out")?;
    finish(&args, 1)?;
    let Some(name) = args.first() else {
        print!("{}", template::list_text());
        return Ok(());
    };
    let t = template::find(name).ok_or_else(|| {
        let names: Vec<&str> = template::all().iter().map(|t| t.name).collect();
        format!("{name}: there is no such template ({})", names.join(", "))
    })?;
    let text = t.text();
    // what is given is read by the same reader as any file of settings
    MachineConfig::parse(&text).map_err(|e| e.localized(Lang::En))?;
    deliver(&text, out.as_deref(), &[], Lang::En)
}

/// `lot-trace PARTS FEEDERS LOTS [--serial S] [--out F]`: the lot of the reel that each placed part came from, by the lots that the table `feeder,lot[,reel][,date_code][,supplier]` says are in
/// the feeders. `--out` writes a row for each part (with `--serial` as the board). The exit code is 1 when a part has no feeder or a feeder that gives parts has no lot.
pub fn lot_trace(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let serial = flag(&mut args, "--serial")?.unwrap_or_default();
    if serial.chars().count() > 64 || serial.chars().any(char::is_control) {
        return Err("--serial is up to 64 characters with no control character".into());
    }
    let out = flag(&mut args, "--out")?;
    finish(&args, 3)?;
    let [parts, feeders, lots] = args.as_slice() else {
        return Err(
            "lot-trace needs a table of parts, a table of feeders and a table of lots".into(),
        );
    };
    let t = tables(&cfg, parts, feeders)?;
    let table = lottrace::parse_lots(&read_text(lots, cfg.max_file_bytes)?)
        .map_err(|e| e.localized(lang))?;
    let found = lottrace::trace(&t.parts, &t.feeders, &t.picks, &table);
    print!("{}", found.text());
    if let Some(o) = &out {
        let mut inputs = t.inputs.clone();
        inputs.push(lots.clone());
        deliver(&found.csv(&serial), Some(o), &inputs, lang)?;
    }
    if found.is_complete() {
        Ok(())
    } else {
        Err("some parts have no lot".into())
    }
}

/// `export-assembler PARTS [--format cpl|kicad-pos|generic] [--side all|top|bottom] [--unit mm|inch] [--out F]`: the parts in the shape that an assembler or another program asks for.
pub fn export_assembler(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let format = match flag(&mut args, "--format")? {
        Some(v) => Format::parse(&v)
            .ok_or_else(|| format!("--format {v} is not cpl, kicad-pos or generic"))?,
        None => Format::Cpl,
    };
    let sides = match flag(&mut args, "--side")? {
        Some(v) => {
            Sides::parse(&v).ok_or_else(|| format!("--side {v} is not all, top or bottom"))?
        }
        None => Sides::All,
    };
    let unit = match flag(&mut args, "--unit")? {
        Some(v) => Unit::parse(&v).ok_or_else(|| format!("--unit {v} is not mm or inch"))?,
        None => Unit::Mm,
    };
    let out = flag(&mut args, "--out")?;
    finish(&args, 1)?;
    let path = args
        .first()
        .ok_or("export-assembler needs a table of parts")?;
    let parsed = parse_all(&read_text(path, cfg.max_file_bytes)?, &cfg.profile)?;
    let text = asmexport::write(&parsed.parts, format, unit, sides);
    let n = asmexport::count(&parsed.parts, sides);
    deliver(&text, out.as_deref(), std::slice::from_ref(path), lang)?;
    if out.is_some() {
        println!("{n} parts in the shape {}", format.name());
    }
    Ok(())
}

/// `split-job PARTS FEEDERS --machines N [--out F]`: the feeders (and so the parts) shared between N machines so that each has about the same work. `--out` writes a row for each part. The
/// exit code is 1 when a part has no feeder.
pub fn split_job(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let machines = whole(
        &flag(&mut args, "--machines")?.ok_or("split-job needs --machines N")?,
        "--machines",
        1.0,
        splitjob::MAX_MACHINES as f64,
    )? as usize;
    let out = flag(&mut args, "--out")?;
    finish(&args, 2)?;
    let [parts, feeders] = args.as_slice() else {
        return Err("split-job needs a table of parts and a table of feeders".into());
    };
    let t = tables(&cfg, parts, feeders)?;
    let found = splitjob::split(
        &t.parts,
        &t.feeders,
        &t.picks,
        machines,
        cfg.pick_s + cfg.place_s,
    );
    print!("{}", found.text());
    if let Some(o) = &out {
        deliver(&found.csv(), Some(o), &t.inputs, lang)?;
    }
    if found.without_feeder.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{} parts have no feeder",
            found.without_feeder.len()
        ))
    }
}

/// `failure-odds PARTS FEEDERS [--rules F] [--min-ok PERCENT] [--out F]`: the chance that a board goes through with no part in error, from the chances of failure and the retries of the
/// settings (and, by package, of a file of rules). `--out` writes a row for each package. The exit code is 1 when the chance is under `--min-ok` (0 by default: never).
pub fn failure_odds(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let rules_path = flag(&mut args, "--rules")?;
    let min_ok = match flag(&mut args, "--min-ok")? {
        Some(v) => {
            let n = num(&v, "--min-ok")?;
            if !(0.0..=100.0).contains(&n) {
                return Err(format!("--min-ok {v} is not from 0 to 100"));
            }
            n
        }
        None => 0.0,
    };
    let out = flag(&mut args, "--out")?;
    finish(&args, 2)?;
    let [parts, feeders] = args.as_slice() else {
        return Err("failure-odds needs a table of parts and a table of feeders".into());
    };
    let t = tables(&cfg, parts, feeders)?;
    let mut inputs = t.inputs.clone();
    let rules = match &rules_path {
        Some(p) => {
            inputs.push(p.clone());
            failodds::parse_rules(&read_text(p, cfg.max_file_bytes)?)
                .map_err(|e| e.localized(lang))?
        }
        None => Vec::new(),
    };
    let odds = failodds::analyse(&t.parts, &t.feeders, &t.picks, &cfg, &rules);
    print!("{}", odds.text());
    if let Some(o) = &out {
        deliver(&odds.csv(), Some(o), &inputs, lang)?;
    }
    if odds.board_ok * 100.0 < min_ok {
        Err(format!(
            "{:.3}% of the boards go through with no error, under the {min_ok}% asked for",
            odds.board_ok * 100.0
        ))
    } else {
        Ok(())
    }
}

/// `pos-transform PARTS [--rotate DEG] [--flip] [--shift X,Y] [--zero] [--format F] [--unit U] [--out F]`: the whole table turned counter-clockwise about the origin, flipped left to right
/// (the sides swap), moved, and with `--zero` moved so that its lowest left corner is the origin, written in a shape of `export-assembler` (`kicad-pos` by default).
pub fn pos_transform(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let mut t = Transform::default();
    if let Some(v) = flag(&mut args, "--rotate")? {
        let d = num(&v, "--rotate")?;
        if !(-360.0..=360.0).contains(&d) {
            return Err(format!("--rotate {v} is not from -360 to 360 degrees"));
        }
        t.rotate_deg = d;
    }
    t.flip = switch(&mut args, "--flip");
    t.zero = switch(&mut args, "--zero");
    if let Some(v) = flag(&mut args, "--shift")? {
        t.shift = pair(&v, "--shift")?;
    }
    let format = match flag(&mut args, "--format")? {
        Some(v) => Format::parse(&v)
            .ok_or_else(|| format!("--format {v} is not cpl, kicad-pos or generic"))?,
        None => Format::KicadPos,
    };
    let unit = match flag(&mut args, "--unit")? {
        Some(v) => Unit::parse(&v).ok_or_else(|| format!("--unit {v} is not mm or inch"))?,
        None => Unit::Mm,
    };
    let out = flag(&mut args, "--out")?;
    finish(&args, 1)?;
    let path = args.first().ok_or("pos-transform needs a table of parts")?;
    let parsed = parse_all(&read_text(path, cfg.max_file_bytes)?, &cfg.profile)?;
    let moved = reorient::apply(&parsed.parts, &t)?;
    let text = asmexport::write(&moved, format, unit, Sides::All);
    deliver(&text, out.as_deref(), std::slice::from_ref(path), lang)?;
    if out.is_some() {
        println!("{}", reorient::summary(&moved));
    }
    Ok(())
}
