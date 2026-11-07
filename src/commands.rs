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
use eh3d_phoenix_pnp::marks;
use eh3d_phoenix_pnp::model::Machine;
use eh3d_phoenix_pnp::model::Offset;
use eh3d_phoenix_pnp::panel::Panel;
use eh3d_phoenix_pnp::parts::{
    bottom_table, parse_all, parse_with, take_references, BottomRotation, Part, Side,
};
use eh3d_phoenix_pnp::planner::{plan_advanced, AdvancedPlan, Options};
use eh3d_phoenix_pnp::preflight::{has_errors, preflight, to_text, Check, Severity, Source};
use eh3d_phoenix_pnp::report::{self, ReportCheck, ReportInput};
use eh3d_phoenix_pnp::sim::Rng;
use eh3d_phoenix_pnp::vision::{measure_image, render, wrap, VisionConfig, MAX_VISION_BYTES};
use std::cell::RefCell;
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
    /// The references that `--dnp` leaves out of the plan.
    pub dnp: Vec<String>,
    /// The boards of the panel that `--x-out` marks as bad, by their path (`A`, `B/C`).
    pub x_out: Vec<String>,
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
    let x_out = match flag(args, "--x-out")? {
        Some(v) => {
            if panel.is_none() {
                return Err("--x-out needs --panel: it names boards of the panel".into());
            }
            marks::parse_list(&v)
        }
        None => Vec::new(),
    };
    let dnp: Vec<String> = match flag(args, "--dnp")? {
        Some(v) => v
            .split(',')
            .map(|r| r.trim().to_string())
            .filter(|r| !r.is_empty())
            .collect(),
        None => Vec::new(),
    };
    Ok(Common {
        cfg,
        catalog,
        opts,
        lang,
        panel,
        fiducials,
        dnp,
        x_out,
        inputs,
    })
}

/// What `load_parts` gives: the parts to mount, how many of the positional words they took (0 for a panel, else 1) and the parts left out as not to be mounted.
type Loaded = (Vec<Part>, usize, Vec<Part>);

/// The parts of the board (or of the panel), with the rotation corrections of the configuration applied; the parts that a column of mounting or `--dnp` leaves out come apart.
fn load_parts(args: &[String], c: &Common) -> Result<Loaded, String> {
    let lang = c.lang;
    let l = move |e: PnpError| e.localized(lang);
    let max = c.cfg.max_file_bytes;
    let profile = &c.cfg.profile;
    let left_out: RefCell<Vec<Part>> = RefCell::new(Vec::new());
    let (mut parts, used) = if let Some(panel_path) = &c.panel {
        let panel = Panel::parse(&read_text(panel_path, max)?).map_err(l)?;
        let base = Path::new(panel_path)
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        let load = |file: &str| -> Result<Vec<Part>, String> {
            let text = read_text(&base.join(file).to_string_lossy(), max)?;
            let parsed = parse_all(&text, profile)?;
            left_out.borrow_mut().extend(parsed.dnp);
            Ok(parsed.parts)
        };
        let expanded = panel.expand_with(&load, &c.x_out).map_err(l)?;
        for path in &expanded.x_out {
            println!("x-out: {path} is marked as bad and is left out");
        }
        let mut parts = expanded.parts;
        let marks_as_parts =
            marks::take_fiducial_parts(&mut parts, &c.cfg.fiducial_words, &c.cfg.fiducial_refs);
        if !marks_as_parts.is_empty() {
            let names: Vec<&str> = marks_as_parts
                .iter()
                .map(|p| p.reference.as_str())
                .collect();
            println!(
                "fiducial parts: {} are marks of their boards and are not placed ({})",
                names.len(),
                names.join(", ")
            );
        }
        if let Some(path) = &c.fiducials {
            let measured = fiducial::parse_measured(&read_text(path, max)?).map_err(l)?;
            let fits = marks::fit_levels(
                &expanded.fiducials,
                &marks::marks_of(&marks_as_parts),
                &measured,
                c.cfg.fiducial_mode,
                c.cfg.fiducial_max_residual_mm,
            )
            .map_err(l)?;
            marks::correct_levels(&mut parts, &fits);
            for level in &fits {
                let label = if level.group.is_empty() {
                    "fiducials".to_string()
                } else {
                    format!("fiducials of {}", level.group)
                };
                println!(
                    "{label}: {} marks, {} fit, turn {:.3} degrees, residual {:.3} mm (largest {:.3} mm)",
                    level.fit.ids.len(),
                    level.fit.mode.name(),
                    level.fit.transform.rotation_deg(),
                    level.fit.rms,
                    level.fit.max
                );
            }
        }
        (parts, 0)
    } else {
        let path = args.first().ok_or("a file of parts is needed")?;
        let parsed = parse_all(&read_text(path, max)?, profile)?;
        left_out.borrow_mut().extend(parsed.dnp);
        let mut parts = parsed.parts;
        let marks_as_parts =
            marks::take_fiducial_parts(&mut parts, &c.cfg.fiducial_words, &c.cfg.fiducial_refs);
        if !marks_as_parts.is_empty() {
            let names: Vec<&str> = marks_as_parts
                .iter()
                .map(|p| p.reference.as_str())
                .collect();
            println!(
                "fiducial parts: {} are marks of the board and are not placed ({})",
                names.len(),
                names.join(", ")
            );
        }
        (parts, 1)
    };
    let mut dnp = left_out.into_inner();
    if !c.dnp.is_empty() {
        dnp.extend(take_references(&mut parts, &c.dnp)?);
    }
    if parts.len() > c.cfg.max_parts {
        return Err(PnpError::new(
            Code::TooManyParts,
            format!("{} parts, the limit is {}", parts.len(), c.cfg.max_parts),
        )
        .localized(lang));
    }
    c.cfg.rotation_table().map_err(l)?.apply(&mut parts);
    Ok((parts, used, dnp))
}

struct Prepared {
    c: Common,
    parts: Vec<Part>,
    /// The parts left out as not to be mounted.
    dnp: Vec<Part>,
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
    let (parts, used, dnp) = load_parts(&args, &c)?;
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
        dnp,
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
    let mut checks = preflight(
        &p.parts,
        p.c.opts.side,
        &p.picks,
        &p.c.catalog,
        &p.c.cfg,
        Some(&plan),
    );
    // the parts that are not to be mounted are told apart, so that nobody looks for them on the board
    for d in p.dnp.iter().filter(|d| d.side == p.c.opts.side) {
        checks.push(Check {
            severity: Severity::Info,
            source: Source::Part(d.reference.clone()),
            message: "marked as not to be mounted: it is left out of the plan".to_string(),
        });
    }
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
        let mut text = export::report(&planned.plan, &p.picks, &planned.undefined);
        text.push_str(&export::dnp_text(&p.dnp));
        write_atomic(r, &text)?;
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
    let csv_path = flag(&mut args, "--report-csv")?;
    let text_path = flag(&mut args, "--report-text")?;
    let html_path = flag(&mut args, "--report-html")?;
    let vision_path = flag(&mut args, "--vision")?;
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
    let mut inputs = p.inputs.clone();
    let vision = match &vision_path {
        Some(vp) => {
            let settings =
                VisionConfig::parse(&read_text(vp, MAX_VISION_BYTES as u64)?).map_err(l)?;
            inputs.push(vp.clone());
            Some(settings)
        }
        None => None,
    };
    for r in [&report_path, &csv_path, &text_path, &html_path]
        .into_iter()
        .flatten()
    {
        refuse_same_file(&inputs, r)?;
    }
    // the page is never written over a file: that is told before the job runs, not after
    if let Some(h) = &html_path {
        if Path::new(h).exists() {
            return Err(PnpError::new(Code::ReportExists, h.as_str()).localized(lang));
        }
    }
    let planned = make_plan(&p)?;
    stop_on_errors(&p, &planned, partial)?;
    let input = ReportInput {
        dnp: p
            .dnp
            .iter()
            .filter(|d| d.side == p.c.opts.side)
            .map(|d| d.reference.clone())
            .collect(),
        checks: planned
            .checks
            .iter()
            .map(|c| ReportCheck {
                severity: c.severity.name().to_string(),
                source: match &c.source {
                    Source::Part(x) | Source::Feeder(x) | Source::Setting(x) => x.clone(),
                    Source::Board => "board".to_string(),
                },
                message: c.message.clone(),
            })
            .collect(),
    };
    let mut job = Job::new(planned.plan, p.feeders.clone(), p.c.cfg.clone()).map_err(l)?;
    if let Some(settings) = vision {
        job.set_vision(settings);
    }
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
        write_atomic(rp, &report::json(&job, &input))?;
    }
    if let Some(cp) = &csv_path {
        write_atomic(cp, &report::csv(&job, &input))?;
    }
    if text_path.is_some() || html_path.is_some() {
        let work = report::build(&job, &input);
        if let Some(tp) = &text_path {
            write_atomic(tp, &work.text(lang))?;
        }
        if let Some(hp) = &html_path {
            report::write_new(hp, &work.html(lang)).map_err(l)?;
        }
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
    if p.c.cfg.align {
        let a = &s.align;
        println!(
            "align: {} parts corrected (mean {:.3} mm, largest {:.3} mm, largest turn {:.2} degrees), {} thrown away over {} mm, {} looks without a reading",
            a.corrected,
            a.mean_mm(),
            a.max_mm,
            a.max_deg,
            a.rejected,
            p.c.cfg.pick_tolerance_mm,
            a.camera_failures
        );
    }
    if p.c.cfg.vacuum {
        let v = &s.vacuum;
        println!(
            "vacuum: {} of {} picks found no part on the nozzle, {} of {} places left the part stuck",
            v.pick_misses, v.pick_checks, v.place_stuck, v.place_checks
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
    let (parts, used, dnp) = load_parts(&args, &c)?;
    print!("{}", export::dnp_text(&dnp));
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

/// `tables-check`: reads each table that is named (`--parts`, `--packages`, `--catalog`, `--feeders`, `--slots`) and says whether it is valid; an error shows its stable code and the row.
/// The exit code is 1 when any table is not valid, after all of them were read.
pub fn tables_check(mut args: Vec<String>) -> Result<(), String> {
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
    let mut named: Vec<(&str, String)> = Vec::new();
    for kind in ["parts", "packages", "catalog", "feeders", "slots"] {
        if let Some(path) = flag(&mut args, &format!("--{kind}"))? {
            named.push((kind, path));
        }
    }
    finish(&args, 0)?;
    if named.is_empty() {
        return Err("tables-check needs at least one of --parts, --packages, --catalog, --feeders and --slots".into());
    }
    let mut bad = 0usize;
    for (kind, path) in &named {
        let text = read_text(path, cfg.max_file_bytes)?;
        let result: Result<usize, String> = match *kind {
            "parts" => parse_all(&text, &cfg.profile).map(|p| p.parts.len() + p.dnp.len()),
            "packages" => {
                let mut c = Catalog::new();
                c.add_packages(&text)
                    .map(|()| c.package_names().len())
                    .map_err(|e| e.localized(lang))
            }
            "catalog" => Catalog::new()
                .add_parts(&text)
                .map(|()| 0)
                .map_err(|e| e.localized(lang)),
            "feeders" => feeding::from_csv_with(&text, cfg.tray_fill)
                .map(|f| f.len())
                .map_err(|e| e.localized(lang)),
            _ => loadsheet::slots_from_csv(&text)
                .map(|s| s.len())
                .map_err(|e| e.localized(lang)),
        };
        match result {
            Ok(_) if *kind == "catalog" => println!("ok: {kind} {path}"),
            Ok(rows) => println!("ok: {kind} {path} ({rows} rows)"),
            Err(e) => {
                bad += 1;
                println!("not valid: {kind} {path}: {e}");
            }
        }
    }
    if bad > 0 {
        return Err(format!("{bad} of {} tables are not valid", named.len()));
    }
    Ok(())
}

/// Two positive numbers written as `AxB` (a size in pixels or in millimetres).
fn pair(text: &str, what: &str) -> Result<(f64, f64), String> {
    let (a, b) = text
        .split_once(['x', 'X'])
        .ok_or_else(|| format!("{what} '{text}' is not two numbers written as AxB"))?;
    Ok((num(a, what)?, num(b, what)?))
}

/// A whole number from `lo` to `hi` given to an option.
fn whole(text: &str, what: &str, lo: f64, hi: f64) -> Result<u64, String> {
    let n = num(text, what)?;
    if n.fract() != 0.0 || n < lo || n > hi {
        return Err(format!(
            "{what} {text} is not a whole number from {lo} to {hi}"
        ));
    }
    Ok(n as u64)
}

/// `vision-test`: draws the picture of a part from the size, the turn, the offset and the noise that the options say, measures it with the pipeline of the vision settings
/// (`--vision FILE`, else the defaults) and prints what was measured with its error against what was true. The exit code is 1 when the pipeline finds no part.
pub fn vision_test(mut args: Vec<String>) -> Result<(), String> {
    let lang = match flag(&mut args, "--lang")? {
        Some(l) => Lang::from_code(&l)
            .ok_or_else(|| format!("--lang '{l}' is not one of en es de fr it ja zh pt"))?,
        None => Lang::En,
    };
    let mut cfg = match flag(&mut args, "--vision")? {
        Some(p) => VisionConfig::parse(&read_text(&p, MAX_VISION_BYTES as u64)?)
            .map_err(|e| e.localized(lang))?,
        None => VisionConfig::default(),
    };
    if let Some(v) = flag(&mut args, "--size")? {
        let (w, h) = pair(&v, "--size")?;
        for side in [w, h] {
            if side.fract() != 0.0 || !(16.0..=1024.0).contains(&side) {
                return Err(format!(
                    "--size {v} is not two whole numbers from 16 to 1024"
                ));
            }
        }
        cfg.image_width = w as usize;
        cfg.image_height = h as usize;
    }
    if let Some(v) = flag(&mut args, "--part")? {
        let (long, short) = pair(&v, "--part")?;
        cfg.sim_part_long_mm = long;
        cfg.sim_part_short_mm = short;
    }
    if let Some(v) = flag(&mut args, "--mm-per-px")? {
        cfg.mm_per_px = num(&v, "--mm-per-px")?;
    }
    if let Some(v) = flag(&mut args, "--noise")? {
        cfg.sim_noise_levels = whole(&v, "--noise", 0.0, 127.0)? as u32;
    }
    let seed = match flag(&mut args, "--seed")? {
        Some(v) => whole(&v, "--seed", 0.0, 9_007_199_254_740_991.0)?,
        None => 1,
    };
    let mut truth = Offset::default();
    if let Some(v) = flag(&mut args, "--dx")? {
        truth.dx_mm = num(&v, "--dx")?;
    }
    if let Some(v) = flag(&mut args, "--dy")? {
        truth.dy_mm = num(&v, "--dy")?;
    }
    if let Some(v) = flag(&mut args, "--turn")? {
        truth.rotation_deg = num(&v, "--turn")?;
    }
    finish(&args, 0)?;
    if !(0.001..=10.0).contains(&cfg.mm_per_px) {
        return Err(format!(
            "--mm-per-px {} is not from 0.001 to 10",
            cfg.mm_per_px
        ));
    }
    if !(0.05..=50.0).contains(&cfg.sim_part_long_mm)
        || !(0.05..=50.0).contains(&cfg.sim_part_short_mm)
    {
        return Err("--part wants sides from 0.05 to 50 millimetres".into());
    }
    cfg.validate().map_err(|e| e.localized(lang))?;
    let picture = render(&cfg, truth, &mut Rng::new(seed));
    let reading = measure_image(&cfg, &picture).map_err(|e| e.localized(lang))?;
    // a number that is a hair below zero must not print as -0.000
    let tidy = |x: f64| -> f64 { (x * 1000.0).round() / 1000.0 + 0.0 };
    let seen = Offset {
        dx_mm: tidy(reading.offset.dx_mm),
        dy_mm: tidy(reading.offset.dy_mm),
        rotation_deg: tidy(reading.offset.rotation_deg),
    };
    // a part that is nearly a square is known up to a quarter turn, any other up to a half turn
    let period = if cfg.sim_part_long_mm / cfg.sim_part_short_mm < cfg.square_ratio {
        90.0
    } else {
        180.0
    };
    let (ex, ey, et): (f64, f64, f64) = (
        tidy(reading.offset.dx_mm - truth.dx_mm),
        tidy(reading.offset.dy_mm - truth.dy_mm),
        tidy(wrap(
            reading.offset.rotation_deg - truth.rotation_deg,
            period,
        )),
    );
    println!(
        "picture {}x{} pixels, {:.3} mm per pixel, threshold {}",
        cfg.image_width, cfg.image_height, cfg.mm_per_px, reading.threshold
    );
    println!(
        "true:     dx {:.3} mm, dy {:.3} mm, turn {:.2} degrees",
        truth.dx_mm, truth.dy_mm, truth.rotation_deg
    );
    println!(
        "measured: dx {:.3} mm, dy {:.3} mm, turn {:.2} degrees (spot of {} pixels, rectangle of {:.1} x {:.1} pixels)",
        seen.dx_mm, seen.dy_mm, seen.rotation_deg, reading.area_px, reading.long_px, reading.short_px
    );
    println!(
        "error:    dx {:+.3} mm, dy {:+.3} mm, turn {:+.2} degrees, distance {:.3} mm",
        ex,
        ey,
        et,
        ex.hypot(ey)
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
