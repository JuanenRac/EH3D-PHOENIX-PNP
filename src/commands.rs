// EH3D-PHOENIX-PNP - the commands that use the configuration of the machine.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//
// `schedule`, `preflight`, `simulate`, `loadsheet` and `config-check`: they read the configuration, the tables of parts and packages and the panel, and use the library for the rest.

use crate::cli_args::{finish, flag, num, switch};
use eh3d_phoenix_pnp::catalog::Catalog;
use eh3d_phoenix_pnp::config::{MachineConfig, OrderStrategy, MAX_CONFIG_BYTES};
use eh3d_phoenix_pnp::error::{Code, Lang, PnpError};
use eh3d_phoenix_pnp::export;
use eh3d_phoenix_pnp::feeding::{self, FeederSpec, Picks};
use eh3d_phoenix_pnp::fiducial;
use eh3d_phoenix_pnp::job::{Job, Phase};
use eh3d_phoenix_pnp::loadsheet;
use eh3d_phoenix_pnp::model::Machine;
use eh3d_phoenix_pnp::panel::Panel;
use eh3d_phoenix_pnp::parts::{bottom_table, parse_with, BottomRotation, Part, Side};
use eh3d_phoenix_pnp::planner::{plan_advanced, AdvancedPlan, Options};
use eh3d_phoenix_pnp::preflight::{has_errors, preflight, to_text, Check};
use std::fs;
use std::path::Path;

/// Reads a text file no larger than `max` bytes. Text that is not UTF-8 is read as Latin-1, with a note.
pub fn read_text(path: &str, max: u64) -> Result<String, String> {
    let meta = fs::metadata(path).map_err(|e| format!("{path}: {e}"))?;
    if meta.len() > max {
        return Err(PnpError::new(
            Code::FileTooBig,
            format!("{path} has {} bytes, the limit is {max}", meta.len()),
        )
        .to_string());
    }
    let bytes = fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    match String::from_utf8(bytes) {
        Ok(text) => Ok(text),
        Err(e) => {
            // the suite reads such text as Latin-1; saying so keeps a value with an omega or a micro sign from silently not matching
            eprintln!("note: {path}: the text is not UTF-8 and was read as Latin-1");
            Ok(e.into_bytes().iter().map(|&b| char::from(b)).collect())
        }
    }
}

/// Writes beside and renames, so that a failure leaves the old file whole.
pub fn write_atomic(out: &str, text: &str) -> Result<(), String> {
    let tmp = format!("{out}.tmp");
    fs::write(&tmp, text)
        .and_then(|()| fs::rename(&tmp, out))
        .map_err(|e| {
            let _ = fs::remove_file(&tmp);
            format!("{out}: {e}")
        })
}

/// An output file is never one of the files it was made from.
pub fn refuse_same_file(inputs: &[String], out: &str) -> Result<(), String> {
    let same = |a: &str, b: &str| match (Path::new(a).canonicalize(), Path::new(b).canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    };
    if inputs.iter().any(|input| same(input, out)) {
        return Err(format!(
            "{out}: the output file is also an input; write the plan to a new file"
        ));
    }
    Ok(())
}

/// What every command that plans needs: the configuration (with the options of the command line on top), the tables, the side of the board and the panel.
pub struct Common {
    pub cfg: MachineConfig,
    pub catalog: Catalog,
    pub opts: Options,
    pub lang: Lang,
    pub panel: Option<String>,
    /// The file with the measured places of the fiducials of the panel.
    pub fiducials: Option<String>,
    pub inputs: Vec<String>,
}

/// Takes the shared options out of `args`.
pub fn common(args: &mut Vec<String>) -> Result<Common, String> {
    let lang_option = match flag(args, "--lang")? {
        Some(l) => Some(
            Lang::from_code(&l)
                .ok_or_else(|| format!("--lang '{l}' is not one of en es de fr it ja zh pt"))?,
        ),
        None => None,
    };
    let early = lang_option.unwrap_or(Lang::En);
    let mut inputs = Vec::new();
    let mut cfg = match flag(args, "--config")? {
        Some(p) => {
            let text = read_text(&p, MAX_CONFIG_BYTES as u64)?;
            inputs.push(p);
            MachineConfig::parse(&text).map_err(|e| e.localized(early))?
        }
        None => MachineConfig::default(),
    };
    let lang = lang_option.unwrap_or(cfg.language);
    if let Some(v) = flag(args, "--nozzles")? {
        let n = num(&v, "--nozzles")?;
        if n.fract() != 0.0 || !(1.0..=6.0).contains(&n) {
            return Err(format!("--nozzles {v} is not a whole number from 1 to 6"));
        }
        cfg.nozzles = n as usize;
    }
    if let Some(v) = flag(args, "--speed")? {
        let s = num(&v, "--speed")?;
        if !(1.0..=5000.0).contains(&s) {
            return Err(format!("--speed {v} is not from 1 to 5000"));
        }
        cfg.x_speed_mm_s = s;
        cfg.y_speed_mm_s = s;
    }
    if let Some(v) = flag(args, "--order")? {
        cfg.order = OrderStrategy::parse(&v).ok_or_else(|| {
            format!("--order '{v}' is not one of nearest part height_part pick tips none")
        })?;
    }
    cfg.validate().map_err(|e| e.localized(lang))?;
    let mut opts = Options::default();
    if let Some(v) = flag(args, "--side")? {
        opts.side = match v.as_str() {
            "top" => Side::Top,
            "bottom" => Side::Bottom,
            other => return Err(format!("--side '{other}' is not top or bottom")),
        };
    }
    if let Some(v) = flag(args, "--board-width")? {
        opts.board_width_mm = Some(num(&v, "--board-width")?);
    }
    if let Some(v) = flag(args, "--origin-x")? {
        opts.origin_x_mm = num(&v, "--origin-x")?;
    }
    let mut catalog = Catalog::new();
    if let Some(p) = flag(args, "--packages")? {
        catalog
            .add_packages(&read_text(&p, cfg.max_file_bytes)?)
            .map_err(|e| e.localized(lang))?;
        inputs.push(p);
    }
    if let Some(p) = flag(args, "--catalog")? {
        catalog
            .add_parts(&read_text(&p, cfg.max_file_bytes)?)
            .map_err(|e| e.localized(lang))?;
        inputs.push(p);
    }
    let panel = flag(args, "--panel")?;
    if let Some(p) = &panel {
        inputs.push(p.clone());
    }
    let fiducials = flag(args, "--fiducials")?;
    if let Some(p) = &fiducials {
        if panel.is_none() {
            return Err("--fiducials needs --panel: the marks are the ones of the panel".into());
        }
        inputs.push(p.clone());
    }
    Ok(Common {
        cfg,
        catalog,
        opts,
        lang,
        panel,
        fiducials,
        inputs,
    })
}

/// The parts of the board (or of the panel), with the rotation corrections of the configuration applied, and how many of the positional words they took (0 for a panel, else 1).
fn load_parts(args: &[String], c: &Common) -> Result<(Vec<Part>, usize), String> {
    let lang = c.lang;
    let l = move |e: PnpError| e.localized(lang);
    let max = c.cfg.max_file_bytes;
    let profile = &c.cfg.profile;
    let (mut parts, used) = if let Some(panel_path) = &c.panel {
        let panel = Panel::parse(&read_text(panel_path, max)?).map_err(l)?;
        let base = Path::new(panel_path)
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        let load = |file: &str| -> Result<Vec<Part>, String> {
            let text = read_text(&base.join(file).to_string_lossy(), max)?;
            parse_with(&text, profile)
        };
        let expanded = panel.expand(&load).map_err(l)?;
        let mut parts = expanded.parts;
        if let Some(path) = &c.fiducials {
            let measured = fiducial::parse_measured(&read_text(path, max)?).map_err(l)?;
            let fit = fiducial::solve(
                &expanded.fiducials,
                &measured,
                c.cfg.fiducial_mode,
                c.cfg.fiducial_max_residual_mm,
            )
            .map_err(l)?;
            fiducial::correct(&mut parts, &fit.transform);
            println!(
                "fiducials: {} marks, {} fit, turn {:.3} degrees, residual {:.3} mm (largest {:.3} mm)",
                fit.ids.len(),
                fit.mode.name(),
                fit.transform.rotation_deg(),
                fit.rms,
                fit.max
            );
        }
        (parts, 0)
    } else {
        let path = args.first().ok_or("a file of parts is needed")?;
        (parse_with(&read_text(path, max)?, profile)?, 1)
    };
    if parts.len() > c.cfg.max_parts {
        return Err(PnpError::new(
            Code::TooManyParts,
            format!("{} parts, the limit is {}", parts.len(), c.cfg.max_parts),
        )
        .localized(lang));
    }
    c.cfg.rotation_table().map_err(l)?.apply(&mut parts);
    Ok((parts, used))
}

struct Prepared {
    c: Common,
    parts: Vec<Part>,
    feeders: Vec<FeederSpec>,
    picks: Picks,
    /// The positional words that are left once the board and the feeders are taken.
    rest: Vec<String>,
    inputs: Vec<String>,
}

/// Reads everything a plan needs; `extra` is how many more words the command takes.
fn prepare(mut args: Vec<String>, extra: usize) -> Result<Prepared, String> {
    let c = common(&mut args)?;
    let used = usize::from(c.panel.is_none());
    finish(&args, used + 1 + extra)?;
    let (parts, used) = load_parts(&args, &c)?;
    let lang = c.lang;
    let feeders_path = args.get(used).ok_or("a file of feeders is needed")?;
    let feeders = feeding::from_csv_with(
        &read_text(feeders_path, c.cfg.max_file_bytes)?,
        c.cfg.tray_fill,
    )
    .map_err(|e| e.localized(lang))?;
    let picks = feeding::assign(&parts, &feeders, Some(c.opts.side));
    let mut inputs = c.inputs.clone();
    if c.panel.is_none() {
        inputs.push(args[0].clone());
    }
    inputs.push(feeders_path.clone());
    let rest = args[used + 1..].to_vec();
    Ok(Prepared {
        c,
        parts,
        feeders,
        picks,
        rest,
        inputs,
    })
}

struct Planned {
    plan: AdvancedPlan,
    undefined: Vec<(String, usize)>,
    checks: Vec<Check>,
}

fn make_plan(p: &Prepared) -> Result<Planned, String> {
    let lang = p.c.lang;
    let side_parts: Vec<Part> = p
        .parts
        .iter()
        .filter(|x| x.side == p.c.opts.side)
        .cloned()
        .collect();
    let undefined = p.c.catalog.undefined(&side_parts);
    let plan = plan_advanced(
        &p.parts,
        &p.feeders,
        &p.picks,
        &p.c.catalog,
        &p.c.cfg,
        &p.c.opts,
    )
    .map_err(|e| e.localized(lang))?;
    let checks = preflight(
        &p.parts,
        p.c.opts.side,
        &p.picks,
        &p.c.catalog,
        &p.c.cfg,
        Some(&plan),
    );
    Ok(Planned {
        plan,
        undefined,
        checks,
    })
}

fn stop_on_errors(p: &Prepared, planned: &Planned, partial: bool) -> Result<(), String> {
    print!("{}", to_text(&planned.checks));
    if has_errors(&planned.checks) && !partial {
        return Err(PnpError::new(
            Code::PreflightFailed,
            "use --partial to go on with only what can be placed",
        )
        .localized(p.c.lang));
    }
    Ok(())
}

/// `schedule`: the plan with the configuration of the machine.
pub fn schedule(mut args: Vec<String>) -> Result<(), String> {
    let report = flag(&mut args, "--report-text")?;
    let gcode = flag(&mut args, "--gcode")?;
    let missing = flag(&mut args, "--missing")?;
    let partial = switch(&mut args, "--partial");
    let p = prepare(args, 1)?;
    let out = p
        .rest
        .first()
        .ok_or("schedule needs an output file")?
        .clone();
    for o in [
        Some(&out),
        report.as_ref(),
        gcode.as_ref(),
        missing.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        refuse_same_file(&p.inputs, o)?;
    }
    let planned = make_plan(&p)?;
    stop_on_errors(&p, &planned, partial)?;
    write_atomic(&out, &planned.plan.to_json())?;
    if let Some(r) = &report {
        write_atomic(
            r,
            &export::report(&planned.plan, &p.picks, &planned.undefined),
        )?;
    }
    if let Some(g) = &gcode {
        write_atomic(g, &export::gcode(&planned.plan))?;
    }
    if let Some(m) = &missing {
        write_atomic(m, &export::missing_csv(&p.picks, &planned.undefined))?;
    }
    let pl = &planned.plan.plan;
    println!(
        "{} parts in {} batches, {:.0} mm of travel, about {:.0} s, {} tip changes",
        pl.placements.len(),
        pl.batches,
        pl.travel_mm,
        pl.seconds,
        planned.plan.tip_changes
    );
    Ok(())
}

/// `preflight`: the checks, without writing anything.
pub fn run_preflight(args: Vec<String>) -> Result<(), String> {
    let p = prepare(args, 0)?;
    let planned = make_plan(&p)?;
    if planned.checks.is_empty() {
        println!("no findings");
    }
    stop_on_errors(&p, &planned, false)
}

fn phase_name(p: Phase) -> &'static str {
    match p {
        Phase::Ready => "ready",
        Phase::Paused => "paused",
        Phase::Stopped => "stopped by an error",
        Phase::Finished => "finished",
    }
}

/// `simulate`: carries the plan out on the simulated machine, and can pause, save the state and resume from it.
pub fn simulate(mut args: Vec<String>) -> Result<(), String> {
    let state_path = flag(&mut args, "--state")?;
    let report_path = flag(&mut args, "--report")?;
    let steps = match flag(&mut args, "--steps")? {
        Some(v) => {
            let n = num(&v, "--steps")?;
            if n.fract() != 0.0 || n < 1.0 {
                return Err(format!("--steps {v} is not a whole number from 1"));
            }
            Some(n as usize)
        }
        None => None,
    };
    let partial = switch(&mut args, "--partial");
    let p = prepare(args, 0)?;
    let lang = p.c.lang;
    let l = move |e: PnpError| e.localized(lang);
    if let Some(r) = &report_path {
        refuse_same_file(&p.inputs, r)?;
    }
    let planned = make_plan(&p)?;
    stop_on_errors(&p, &planned, partial)?;
    let mut job = Job::new(planned.plan, p.feeders.clone(), p.c.cfg.clone()).map_err(l)?;
    if let Some(sp) = &state_path {
        if Path::new(sp).exists() {
            job.restore_state(&read_text(sp, p.c.cfg.max_file_bytes)?)
                .map_err(l)?;
            // a saved job comes back paused or stopped by an error: running it again is asking it to go on
            job.resume();
            println!("resumed from {sp}");
        }
    }
    let outcome = job.run(steps);
    if let Some(sp) = &state_path {
        write_atomic(sp, &job.save_state())?;
    }
    if let Some(rp) = &report_path {
        write_atomic(rp, &job.stats_json())?;
    }
    let phase = outcome.map_err(l)?;
    let s = job.stats();
    println!(
        "placed {} of {}, {} put aside, {:.1} s of machine time, {}",
        s.placed,
        job.status().len(),
        s.deferred,
        job.machine().now_us() as f64 / 1e6,
        phase_name(phase)
    );
    println!("{:.0} parts per hour (simulated)", job.parts_per_hour());
    for k in &s.kinds {
        println!(
            "package {}: {} picked ({} failed), {} placed ({} failed)",
            k.package, k.picked, k.pick_failures, k.placed, k.place_failures
        );
    }
    for f in &s.feeders {
        println!(
            "feeder {}: {} picked, {} failed picks{}",
            f.id,
            f.picked,
            f.pick_failures,
            if f.disabled { ", switched off" } else { "" }
        );
    }
    for (r, why) in job.deferred() {
        println!("put aside: {r}: {why}");
    }
    Ok(())
}

/// `loadsheet`: which kind of part goes in which slot.
pub fn run_loadsheet(mut args: Vec<String>) -> Result<(), String> {
    let feeders_out = flag(&mut args, "--feeders-out")?;
    let c = common(&mut args)?;
    let used = usize::from(c.panel.is_none());
    finish(&args, used + 2)?;
    let (parts, used) = load_parts(&args, &c)?;
    let lang = c.lang;
    let slots_path = args.get(used).ok_or("a file of slots is needed")?;
    let out = args.get(used + 1).ok_or("loadsheet needs an output file")?;
    let slots = loadsheet::slots_from_csv(&read_text(slots_path, c.cfg.max_file_bytes)?)
        .map_err(|e| e.localized(lang))?;
    let (needs, middle) = loadsheet::needs(&parts, Some(c.opts.side), &c.catalog, &c.cfg);
    let sheet = loadsheet::assign_slots(&needs, &slots, middle);
    let mut inputs = c.inputs.clone();
    if c.panel.is_none() {
        inputs.push(args[0].clone());
    }
    inputs.push(slots_path.clone());
    refuse_same_file(&inputs, out)?;
    write_atomic(out, &sheet.to_csv())?;
    if let Some(f) = &feeders_out {
        refuse_same_file(&inputs, f)?;
        write_atomic(f, &sheet.to_feeders_csv())?;
    }
    println!(
        "{} kinds of part in slots, {} without a slot",
        sheet.rows.len(),
        sheet.unassigned.len()
    );
    for n in &sheet.unassigned {
        println!(
            "no slot for {} / {} ({} parts)",
            n.value, n.package, n.count
        );
    }
    if sheet.unassigned.is_empty() {
        Ok(())
    } else {
        Err("some kinds of part have no slot that is wide enough".into())
    }
}

/// `config-check`: reads a configuration and says whether it is valid.
pub fn config_check(mut args: Vec<String>) -> Result<(), String> {
    let lang = match flag(&mut args, "--lang")? {
        Some(l) => Lang::from_code(&l)
            .ok_or_else(|| format!("--lang '{l}' is not one of en es de fr it ja zh pt"))?,
        None => Lang::En,
    };
    finish(&args, 1)?;
    let path = args.first().ok_or("config-check needs a file")?;
    let cfg = MachineConfig::parse(&read_text(path, MAX_CONFIG_BYTES as u64)?)
        .map_err(|e| e.localized(lang))?;
    println!(
        "ok: schema {}, {} nozzles, order {}, {} rotation rules",
        cfg.schema_version,
        cfg.nozzles,
        cfg.order.name(),
        cfg.rotation_rules.len()
    );
    Ok(())
}

/// `check-bottom`: the angles that the rules of the bottom side give to the parts of a position file, to compare with the real files.
pub fn check_bottom(mut args: Vec<String>) -> Result<(), String> {
    let lang = match flag(&mut args, "--lang")? {
        Some(l) => Lang::from_code(&l)
            .ok_or_else(|| format!("--lang '{l}' is not one of en es de fr it ja zh pt"))?,
        None => Lang::En,
    };
    let cfg = match flag(&mut args, "--config")? {
        Some(p) => MachineConfig::parse(&read_text(&p, MAX_CONFIG_BYTES as u64)?)
            .map_err(|e| e.localized(lang))?,
        None => MachineConfig::default(),
    };
    finish(&args, 1)?;
    let path = args
        .first()
        .ok_or("check-bottom needs a file of positions")?;
    // the file is read with the rule `keep`, so that the angles are the ones the file gives
    let mut profile = cfg.profile.clone();
    profile.bottom_rotation = BottomRotation::Keep;
    let parts = parse_with(&read_text(path, cfg.max_file_bytes)?, &profile)?;
    print!("{}", bottom_table(&parts, cfg.profile.bottom_rotation));
    Ok(())
}
