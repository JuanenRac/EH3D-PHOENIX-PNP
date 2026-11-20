// EH3D-PHOENIX-PNP - the commands of the flow of a job that need no machine: camera-selftest, machine-template, lot-trace, export-assembler, split-job, failure-odds, pos-transform, nozzle-zcal,
// panel-fids, import-assembler and bom-merge.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//
// Each one reads its files, calls the library and prints; none of them moves a machine or writes anything but the file named by --out, and a file that exists is never written over.

use crate::cli_args::{finish, flag, num, switch};
use crate::cmd_tools::language_and_config;
use crate::commands::{read_text, refuse_same_file};
use eh3d_phoenix_pnp::asmexport::{self, Format, Sides, Unit};
use eh3d_phoenix_pnp::asmimport;
use eh3d_phoenix_pnp::bom::Bom;
use eh3d_phoenix_pnp::bommerge;
use eh3d_phoenix_pnp::calibsim;
use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::error::{Code, Lang, PnpError};
use eh3d_phoenix_pnp::failodds;
use eh3d_phoenix_pnp::feeding::{self, FeederSpec, Picks};
use eh3d_phoenix_pnp::lottrace;
use eh3d_phoenix_pnp::nozzlez;
use eh3d_phoenix_pnp::panel::Panel;
use eh3d_phoenix_pnp::panelfids;
use eh3d_phoenix_pnp::parts::{parse_all, Part};
use eh3d_phoenix_pnp::reorient::{self, Transform};
use eh3d_phoenix_pnp::report;
use eh3d_phoenix_pnp::splitjob;
use eh3d_phoenix_pnp::template;

/// A whole number in a range, from the text of an option.
pub(crate) fn whole(text: &str, what: &str, lo: f64, hi: f64) -> Result<u64, String> {
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
pub(crate) fn deliver(
    text: &str,
    out: Option<&str>,
    inputs: &[String],
    lang: Lang,
) -> Result<(), String> {
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

/// A number in a range from the text of an option, for the commands below.
fn bounded(text: &str, what: &str, lo: f64, hi: f64) -> Result<f64, String> {
    let n = num(text, what)?;
    if !(lo..=hi).contains(&n) {
        return Err(format!("{what} {text} is not from {lo} to {hi}"));
    }
    Ok(n)
}

/// Refuses an output that exists, before anything is done.
fn refuse_existing(out: Option<&str>, lang: Lang) -> Result<(), String> {
    if let Some(o) = out {
        if std::path::Path::new(o).exists() {
            return Err(PnpError::new(Code::ReportExists, o).localized(lang));
        }
    }
    Ok(())
}

/// `nozzle-zcal [FILE | --simulate] [--nozzles N] [--probes N] [--noise MM] [--spread MM] [--seed S] [--tol MM] [--out F] [--readings-out F]`: the height of every nozzle tip from probings of the
/// bed (`nozzle,z_mm`, from FILE) or from probings that are made up (`--simulate`: the heights of the tips are made up between plus and minus half of `--spread`, and every probing reads its
/// height with `--noise`). Gives each tip's mean height, how much its probings differ, its offset from the mean of the tips and its correction, and whether it is out of line (over `--tol`, the
/// `nozzle_z_tol_mm` of the settings), noisy or probed once. With `--simulate` it also says how far the offsets are from the heights that were made up. `--out` writes the table and
/// `--readings-out` the made-up probings, never over a file that exists. The exit code is 1 when a tip needs a look.
pub fn nozzle_zcal(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let simulate = switch(&mut args, "--simulate");
    let mut plan = nozzlez::Plan {
        nozzles: cfg.nozzles.max(1),
        seed: cfg.sim_seed,
        ..nozzlez::Plan::default()
    };
    if let Some(v) = flag(&mut args, "--nozzles")? {
        plan.nozzles = whole(&v, "--nozzles", 1.0, nozzlez::MAX_NOZZLES as f64)? as usize;
    }
    if let Some(v) = flag(&mut args, "--probes")? {
        plan.probes = whole(&v, "--probes", 1.0, nozzlez::MAX_PROBES as f64)? as usize;
    }
    if let Some(v) = flag(&mut args, "--noise")? {
        plan.noise_mm = bounded(&v, "--noise", 0.0, 1.0)?;
    }
    if let Some(v) = flag(&mut args, "--spread")? {
        plan.spread_mm = bounded(&v, "--spread", 0.0, 2.0)?;
    }
    if let Some(v) = flag(&mut args, "--seed")? {
        plan.seed = whole(&v, "--seed", 0.0, 9_007_199_254_740_992.0)?;
    }
    let tol = match flag(&mut args, "--tol")? {
        Some(v) => bounded(&v, "--tol", 0.001, 1.0)?,
        None => cfg.nozzle_z_tol_mm,
    };
    let out = flag(&mut args, "--out")?;
    let readings_out = flag(&mut args, "--readings-out")?;
    finish(&args, 1)?;
    if simulate == !args.is_empty() {
        return Err("nozzle-zcal needs a file of probings or --simulate, and not both".into());
    }
    if readings_out.is_some() && !simulate {
        return Err("--readings-out writes made-up probings: it needs --simulate".into());
    }
    refuse_existing(out.as_deref(), lang)?;
    refuse_existing(readings_out.as_deref(), lang)?;
    let (readings, truth) = if simulate {
        let made = nozzlez::simulate(&plan).map_err(|e| e.localized(lang))?;
        (made.readings, Some(made.truth))
    } else {
        let text = read_text(&args[0], cfg.max_file_bytes)?;
        (
            nozzlez::parse_readings(&text).map_err(|e| e.localized(lang))?,
            None,
        )
    };
    let report = nozzlez::analyse(&readings, tol).map_err(|e| e.localized(lang))?;
    print!("{}", report.text());
    if let Some(truth) = &truth {
        println!(
            "simulated probings of {} nozzles, {} each (seed {}): the offsets are {:.4} mm from the heights that were made up; no machine was touched",
            plan.nozzles,
            plan.probes,
            plan.seed,
            nozzlez::error_against(&report, truth)
        );
    }
    if let Some(o) = &readings_out {
        report::write_new(o, &nozzlez::readings_csv(&readings)).map_err(|e| e.localized(lang))?;
        println!("probings written to {o}");
    }
    if let Some(o) = &out {
        report::write_new(o, &report.csv()).map_err(|e| e.localized(lang))?;
        println!("written to {o}");
    }
    match report.problems() {
        0 => Ok(()),
        n => Err(format!("{n} nozzles need a look")),
    }
}

/// `panel-fids PANEL [--x-out P2,P5] [--min-sep MM] [--out F]`: the marks of every board of a panel (the parts of its file that are marks: `FID1`, or a part with a word of `fiducial_words`)
/// in the coordinates of the panel, with the place and the turn of the board counted in, and which boards cannot be aligned by their own marks: none, only one, or the farthest two closer than
/// `--min-sep` (the `panel_fid_min_sep_mm` of the settings). A board marked as bad is left out. `--out` writes a row for each mark, never over a file that exists. The exit code is 1 when a
/// board cannot be aligned.
pub fn panel_fids(mut args: Vec<String>) -> Result<(), String> {
    let (lang, mut cfg) = language_and_config(&mut args)?;
    if let Some(v) = flag(&mut args, "--min-sep")? {
        cfg.panel_fid_min_sep_mm = bounded(&v, "--min-sep", 0.0, 1000.0)?;
    }
    let x_out: Vec<String> = flag(&mut args, "--x-out")?
        .map(|v| {
            v.split([',', ';', ' '])
                .filter(|w| !w.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let out = flag(&mut args, "--out")?;
    finish(&args, 1)?;
    let path = args.first().ok_or("panel-fids needs a panel file")?.clone();
    refuse_existing(out.as_deref(), lang)?;
    let max = cfg.max_file_bytes;
    let panel = Panel::parse(&read_text(&path, max)?).map_err(|e| e.localized(lang))?;
    let base = std::path::Path::new(&path)
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_default();
    let profile = cfg.profile.clone();
    let load = |file: &str| -> Result<Vec<Part>, String> {
        let text = read_text(&base.join(file).to_string_lossy(), max)?;
        Ok(parse_all(&text, &profile)?.parts)
    };
    let report =
        panelfids::from_panel(&panel, &load, &x_out, &cfg).map_err(|e| e.localized(lang))?;
    print!("{}", report.text());
    if let Some(o) = &out {
        refuse_same_file(std::slice::from_ref(&path), o)?;
        report::write_new(o, &report.csv()).map_err(|e| e.localized(lang))?;
        println!("written to {o}");
    }
    match report.problems() {
        0 => Ok(()),
        n => Err(format!("{n} boards cannot be aligned by their own marks")),
    }
}

/// `import-assembler FILE [--format cpl|kicad-pos|generic] [--unit mm|inch] [--out F]`: reads a position file of an assembler or of another program (a designator column, the middle of the
/// part as `Mid X` or `Center-X(mm)`, a layer as `Top`, `T` or `TopLayer`, numbers with their unit) and writes the parts in millimetres in a shape that every command reads (`generic` by
/// default), or prints it. It says which shape it found. A row that cannot be read is an error with its line. `--out` never goes over a file that exists or over the file that was read.
pub fn import_assembler(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let format = match flag(&mut args, "--format")? {
        Some(v) => Format::parse(&v)
            .ok_or_else(|| format!("--format {v} is not cpl, kicad-pos or generic"))?,
        None => Format::Generic,
    };
    let unit = match flag(&mut args, "--unit")? {
        Some(v) => Unit::parse(&v).ok_or_else(|| format!("--unit {v} is not mm or inch"))?,
        None => Unit::Mm,
    };
    let out = flag(&mut args, "--out")?;
    finish(&args, 1)?;
    let path = args
        .first()
        .ok_or("import-assembler needs a position file")?;
    let found =
        asmimport::parse(&read_text(path, cfg.max_file_bytes)?).map_err(|e| e.localized(lang))?;
    let text = asmexport::write(&found.parts, format, unit, Sides::All);
    eprintln!(
        "{} parts read from {path} (shape: {})",
        found.parts.len(),
        found.dialect
    );
    deliver(&text, out.as_deref(), std::slice::from_ref(path), lang)
}

/// `bom-merge FILE[:BOARDS]... [--max-rows N] [--out F]`: the bills of materials of several boards added up into one list to buy, each for the number of boards that is wanted (1 when `:BOARDS`
/// is not given): the rows that are the same part (the value written alike, the package, the part number) are joined and every part says which bill it is for. `--out` writes the list as a
/// table, never over a file that exists or over a file that was read.
pub fn bom_merge(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let max_rows = match flag(&mut args, "--max-rows")? {
        Some(v) => whole(&v, "--max-rows", 1.0, 1_000_000.0)? as usize,
        None => 100_000,
    };
    let out = flag(&mut args, "--out")?;
    finish(&args, bommerge::MAX_BILLS)?;
    if args.is_empty() {
        return Err("bom-merge needs at least one bill of materials".into());
    }
    let mut inputs = Vec::new();
    let mut paths = Vec::new();
    for word in &args {
        let (path, boards) = match word.rsplit_once(':') {
            Some((p, n))
                if !p.is_empty() && !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) =>
            {
                (
                    p.to_string(),
                    whole(n, "the boards", 1.0, bommerge::MAX_BOARDS as f64)?,
                )
            }
            _ => (word.clone(), 1),
        };
        let name = std::path::Path::new(&path)
            .file_stem()
            .map_or_else(|| path.clone(), |s| s.to_string_lossy().into_owned());
        let bom = Bom::parse(&read_text(&path, cfg.max_file_bytes)?, max_rows)
            .map_err(|e| e.localized(lang))?;
        inputs.push(bommerge::Input { name, boards, bom });
        paths.push(path);
    }
    let lines = bommerge::merge(&inputs).map_err(|e| e.localized(lang))?;
    print!("{}", bommerge::text(&lines, &inputs));
    if let Some(o) = &out {
        refuse_same_file(&paths, o)?;
        report::write_new(o, &bommerge::csv(&lines)).map_err(|e| e.localized(lang))?;
        println!("written to {o}");
    }
    Ok(())
}
