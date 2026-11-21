// EH3D-PHOENIX-PNP - the commands of the import and the guided work: import-openpnp, assist, align-iterate, vacuum-calibrate and test-board-gen.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//
// Each one reads its files, calls the library and prints; none of them moves a machine or writes anything but the files it is told to (a file that exists is never written over, and a file
// that is read is never written to).

use crate::cli_args::{finish, flag, num, switch};
use crate::cmd_flow::{deliver, whole};
use crate::cmd_nodes::fail;
use crate::cmd_tools::language_and_config;
use crate::commands::read_text;
use eh3d_phoenix_pnp::assist::{self, Answer, Assist, Order};
use eh3d_phoenix_pnp::error::Lang;
use eh3d_phoenix_pnp::feeders::from_csv as feeders_csv;
use eh3d_phoenix_pnp::lookmove::{self, Params, Sim};
use eh3d_phoenix_pnp::msg8::{num as show, say};
use eh3d_phoenix_pnp::openpnp::{self, Imported, Severity};
use eh3d_phoenix_pnp::parts::{parse_all, Part, Side};
use eh3d_phoenix_pnp::process::{self, Doer, Process, ProcessError, Task};
use eh3d_phoenix_pnp::testboard::{self, Kind, Spec};
use eh3d_phoenix_pnp::vacuumcal;
use std::io::BufRead;

fn pair(text: &str, what: &str) -> Result<(f64, f64), String> {
    let (x, y) = text
        .split_once(',')
        .ok_or_else(|| format!("{what} {text} is not X,Y"))?;
    Ok((num(x, what)?, num(y, what)?))
}

fn read_bytes(path: &str, max: usize) -> Result<Vec<u8>, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("{path}: {e}"))?;
    if meta.len() as usize > max {
        return Err(format!(
            "{path} has {} bytes, the limit is {max}",
            meta.len()
        ));
    }
    std::fs::read(path).map_err(|e| format!("{path}: {e}"))
}

/// `import-openpnp [--machine F] [--packages F] [--parts F] [--out-dir D]`: reads the files of another program, never writes to them, and brings the packages, the parts and the feeders into
/// the tables of this one. The findings are told with their file and line; with `--out-dir` the three tables are written there (never over a file that exists).
pub fn import_openpnp(mut args: Vec<String>) -> Result<(), String> {
    let (lang, _) = language_and_config(&mut args)?;
    let machine = flag(&mut args, "--machine")?;
    let packages = flag(&mut args, "--packages")?;
    let parts = flag(&mut args, "--parts")?;
    let out_dir = flag(&mut args, "--out-dir")?;
    finish(&args, 0)?;
    if machine.is_none() && packages.is_none() && parts.is_none() {
        return Err(
            "import-openpnp needs at least one of --machine, --packages and --parts".into(),
        );
    }
    let mut imp = Imported::default();
    let mut inputs: Vec<String> = Vec::new();
    if let Some(p) = &packages {
        imp.read_packages(&read_bytes(p, eh3d_phoenix_pnp::xmlmini::MAX_BYTES)?);
        inputs.push(p.clone());
    }
    if let Some(p) = &parts {
        imp.read_parts(&read_bytes(p, eh3d_phoenix_pnp::xmlmini::MAX_BYTES)?);
        inputs.push(p.clone());
    }
    if let Some(p) = &machine {
        imp.read_machine(&read_bytes(p, eh3d_phoenix_pnp::xmlmini::MAX_BYTES)?);
        inputs.push(p.clone());
    }
    imp.link();
    for f in &imp.findings {
        println!(
            "{}: {}",
            if f.severity == Severity::Error {
                "error"
            } else {
                "warning"
            },
            openpnp::message(f, lang)
        );
    }
    let row: [&str; 8] = [
        "{p} packages, {a} parts and {f} feeders read; {e} errors, {w} warnings",
        "{p} paquetes, {a} piezas y {f} alimentadores leídos; {e} errores, {w} avisos",
        "{p} Gehäuse, {a} Teile und {f} Zuführer gelesen; {e} Fehler, {w} Hinweise",
        "{p} boîtiers, {a} pièces et {f} alimentateurs lus ; {e} erreurs, {w} avertissements",
        "{p} package, {a} componenti e {f} alimentatori letti; {e} errori, {w} avvisi",
        "パッケージ {p}、部品 {a}、フィーダ {f} を読みました。エラー {e}、警告 {w}",
        "读取 {p} 个封装、{a} 个元件、{f} 个供料器；{e} 个错误，{w} 个警告",
        "{p} encapsulamentos, {a} peças e {f} alimentadores lidos; {e} erros, {w} avisos",
    ];
    let errors = imp.errors();
    println!(
        "{}",
        say(
            lang,
            &row,
            &[
                ("p", imp.packages.len().to_string()),
                ("a", imp.parts.len().to_string()),
                ("f", imp.feeders.len().to_string()),
                ("e", errors.to_string()),
                ("w", (imp.findings.len() - errors).to_string()),
            ]
        )
    );
    if let Some(dir) = &out_dir {
        std::fs::create_dir_all(dir).map_err(|e| format!("{dir}: {e}"))?;
        for (name, text, wanted) in [
            ("packages.csv", imp.packages_csv(), packages.is_some()),
            ("catalog.csv", imp.parts_csv(), parts.is_some()),
            ("feeders.csv", imp.feeders_csv(), machine.is_some()),
        ] {
            if wanted {
                let path = std::path::Path::new(dir).join(name);
                deliver(&text, path.to_str(), &inputs, lang)?;
            }
        }
    }
    if errors > 0 {
        fail(format!("{errors} rows could not be read"))
    } else {
        Ok(())
    }
}

/// Takes the answers one by one, from a file or from the keyboard.
type Answers = Box<dyn Iterator<Item = String>>;

/// Walks a person through the parts of `walk`, asking for each, until the walk is finished or the person quits. The parts that were skipped are offered once more at the end. Gives whether the
/// person asked to quit.
fn walk_parts(walk: &mut Assist, lines: &mut Answers, lang: Lang) -> bool {
    let mut revisited = false;
    loop {
        if walk.current().is_none() {
            if !revisited && !walk.skipped().is_empty() {
                revisited = true;
                walk.revisit_skipped();
                continue;
            }
            return false;
        }
        println!("{}", assist::prompt(walk, lang));
        let Some(line) = lines.next() else {
            return false;
        };
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let ans = walk.respond(&line);
        println!("{}", assist::feedback(&ans, walk, lang));
        if ans == Answer::Quit {
            return true;
        }
    }
}

/// Waits for the person to say `done` after a task that is not the placing of parts; `quit` (or the end of the answers) gives false.
fn wait_done(lines: &mut Answers, lang: Lang) -> bool {
    println!("{}", process::confirm_prompt(lang));
    loop {
        let Some(line) = lines.next() else {
            return false;
        };
        let word = line.trim().to_ascii_lowercase();
        if word.is_empty() || word.starts_with('#') {
            continue;
        }
        match word.as_str() {
            "done" => return true,
            "quit" => return false,
            _ => println!("{}", process::confirm_prompt(lang)),
        }
    }
}

/// The process the person asked for: a preset or a file.
fn load_process(name: &str, max_bytes: u64, lang: Lang) -> Result<Process, String> {
    if let Some(p) = Process::preset(name) {
        return Ok(p);
    }
    let text = read_text(name, max_bytes.min(process::MAX_PROCESS_BYTES as u64))?;
    Process::parse(&text).map_err(|e| format!("{name}: {}", e.localized(lang)))
}

/// `assist PARTS [--feeders F] [--order reel|reference] [--process pnp|manual|FILE] [--answers F] [--resume F] [--out F]`: walks a person through the board, a part at a time; the answers come
/// from the file of `--answers` or from what is typed. With `--process` the walk goes phase by phase: the parts of a side placed by hand are walked one at a time, the other tasks (loading
/// the board, the oven, soldering, looking) are told and wait for `done`, and a side that the machine places is told.
pub fn assist_cmd(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let feeders_path = flag(&mut args, "--feeders")?;
    let order = match flag(&mut args, "--order")? {
        Some(v) => {
            Order::parse(&v).ok_or_else(|| format!("--order {v} is not reel or reference"))?
        }
        None => Order::Reel,
    };
    let process_name = flag(&mut args, "--process")?;
    let answers = flag(&mut args, "--answers")?;
    let resume = flag(&mut args, "--resume")?;
    let out = flag(&mut args, "--out")?;
    finish(&args, 1)?;
    if process_name.is_some() && resume.is_some() {
        return Err("--process and --resume cannot be given together".into());
    }
    let path = args.first().ok_or("assist needs a table of parts")?.clone();
    let mut inputs = vec![path.clone()];
    let parsed = parse_all(&read_text(&path, cfg.max_file_bytes)?, &cfg.profile)?;
    let feeders = match &feeders_path {
        Some(p) => {
            inputs.push(p.clone());
            feeders_csv(&read_text(p, cfg.max_file_bytes)?)?
        }
        None => Vec::new(),
    };
    let mut lines: Answers = match &answers {
        Some(p) => {
            inputs.push(p.clone());
            let text = read_text(p, cfg.max_file_bytes)?;
            Box::new(
                text.lines()
                    .map(str::to_string)
                    .collect::<Vec<_>>()
                    .into_iter(),
            )
        }
        None => Box::new(std::io::stdin().lock().lines().map_while(Result::ok)),
    };
    if let Some(name) = &process_name {
        let mut process = load_process(name, cfg.max_file_bytes, lang)?;
        let has_top = parsed.parts.iter().any(|p| p.side == Side::Top);
        let has_bottom = parsed.parts.iter().any(|p| p.side == Side::Bottom);
        process.only_sides(has_top, has_bottom);
        if process.phases.is_empty() {
            return Err(ProcessError::Empty.localized(lang));
        }
        return walk_process(
            &mut process,
            &parsed.parts,
            &feeders,
            order,
            &cfg,
            &mut lines,
            lang,
            out.as_deref(),
            &inputs,
        );
    }
    let mut walk = Assist::new(
        &parsed.parts,
        &feeders,
        order,
        &cfg.polarity_refs,
        &cfg.polarity_packages,
    )?;
    if let Some(r) = &resume {
        inputs.push(r.clone());
        let n = walk.resume(&read_text(r, cfg.max_file_bytes)?)?;
        println!("{n} parts taken from {r}");
    }
    walk_parts(&mut walk, &mut lines, lang);
    println!("{}", assist::feedback(&Answer::Done, &walk, lang));
    if let Some(o) = &out {
        deliver(&walk.checklist_csv(), Some(o), &inputs, lang)?;
    }
    let (placed, skipped, total) = walk.progress();
    if placed + skipped < total || skipped > 0 {
        fail(format!("{} parts are not placed", total - placed))
    } else {
        Ok(())
    }
}

/// The walk of a process: each phase is started, its tasks are done one after the other and it is finished. The checklists of the sides placed by hand are written together with `--out`.
#[allow(clippy::too_many_arguments)]
fn walk_process(
    process: &mut Process,
    parts: &[Part],
    feeders: &[eh3d_phoenix_pnp::feeders::Feeder],
    order: Order,
    cfg: &eh3d_phoenix_pnp::config::MachineConfig,
    lines: &mut Answers,
    lang: Lang,
    out: Option<&str>,
    inputs: &[String],
) -> Result<(), String> {
    let total = process.phases.len();
    let mut checklist = String::new();
    let mut left = 0usize;
    let mut quit = false;
    'phases: for i in 0..total {
        process.start(i).map_err(|e| e.localized(lang))?;
        let name = process.phases[i].name.clone();
        println!("{}", process::phase_header(lang, i + 1, total, &name));
        for task in process.phases[i].tasks.clone() {
            match task {
                Task::Place(side, Doer::Hand) => {
                    let of_side: Vec<Part> =
                        parts.iter().filter(|p| p.side == side).cloned().collect();
                    println!("{}", task.describe(lang));
                    let mut walk = Assist::new(
                        &of_side,
                        feeders,
                        order,
                        &cfg.polarity_refs,
                        &cfg.polarity_packages,
                    )?;
                    let stopped = walk_parts(&mut walk, lines, lang);
                    println!("{}", assist::feedback(&Answer::Done, &walk, lang));
                    let sheet = walk.checklist_csv();
                    let mut rows = sheet.lines();
                    let header = rows.next().unwrap_or("");
                    if checklist.is_empty() {
                        checklist.push_str(header);
                        checklist.push('\n');
                    }
                    for row in rows {
                        checklist.push_str(row);
                        checklist.push('\n');
                    }
                    let (placed, _, all) = walk.progress();
                    left += all - placed;
                    if stopped {
                        quit = true;
                        break 'phases;
                    }
                }
                other => {
                    println!("{}", other.describe(lang));
                    if !wait_done(lines, lang) {
                        quit = true;
                        break 'phases;
                    }
                }
            }
        }
        process.finish(i).map_err(|e| e.localized(lang))?;
    }
    print!("{}", process.summary(lang));
    if let Some(o) = out {
        if !checklist.is_empty() {
            deliver(&checklist, Some(o), inputs, lang)?;
        }
    }
    if quit || left > 0 || !process.finished() {
        fail(format!(
            "the process is not finished: {} of {} phases are done, {left} parts are not placed",
            process.progress().0,
            total
        ))
    } else {
        Ok(())
    }
}

/// `align-iterate [--error X,Y] [--noise MM] [--scale K] [--seed S] [--blind N]`: looks, moves and looks again against a camera that is made up, with the settings `lml_*` (options override them).
pub fn align_iterate(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let mut p = Params::from_config(&cfg);
    let error = match flag(&mut args, "--error")? {
        Some(v) => pair(&v, "--error")?,
        None => (1.2, -0.8),
    };
    let noise = match flag(&mut args, "--noise")? {
        Some(v) => num(&v, "--noise")?,
        None => 0.0,
    };
    let scale = match flag(&mut args, "--scale")? {
        Some(v) => num(&v, "--scale")?,
        None => 1.0,
    };
    let seed = match flag(&mut args, "--seed")? {
        Some(v) => whole(&v, "--seed", 0.0, 1e12)?,
        None => 1,
    };
    let blind = match flag(&mut args, "--blind")? {
        Some(v) => whole(&v, "--blind", 0.0, 100.0)? as u32,
        None => 0,
    };
    if let Some(v) = flag(&mut args, "--max-step")? {
        p.max_step_mm = num(&v, "--max-step")?;
    }
    if let Some(v) = flag(&mut args, "--tolerance")? {
        p.tolerance_mm = num(&v, "--tolerance")?;
    }
    if let Some(v) = flag(&mut args, "--iterations")? {
        p.max_iterations = whole(&v, "--iterations", 1.0, 20.0)? as u32;
    }
    if let Some(v) = flag(&mut args, "--gain")? {
        p.gain = num(&v, "--gain")?;
    }
    finish(&args, 0)?;
    if !(0.01..=20.0).contains(&p.max_step_mm)
        || !(0.001..=5.0).contains(&p.tolerance_mm)
        || !(0.05..=1.0).contains(&p.gain)
    {
        return Err("the step is from 0.01 to 20 mm, the tolerance from 0.001 to 5 mm and the gain from 0.05 to 1".into());
    }
    if !(0.0..=10.0).contains(&noise)
        || !(0.1..=10.0).contains(&scale)
        || error.0.abs() > 1000.0
        || error.1.abs() > 1000.0
    {
        return Err(
            "--noise is from 0 to 10 mm, --scale from 0.1 to 10 and --error is under 1000 mm"
                .into(),
        );
    }
    let result = Sim {
        error,
        scale,
        noise_mm: noise,
        seed,
        blind_looks: blind,
    }
    .run(&p);
    print!("{}", lookmove::report(&result, lang));
    println!("evidence=simulator");
    if result.outcome == lookmove::Outcome::Aligned {
        Ok(())
    } else {
        fail(format!("the head did not align: {}", result.outcome.name()))
    }
}

/// `vacuum-calibrate READINGS [--out F]`: the thresholds of the vacuum from readings (`state,kpa`).
pub fn vacuum_calibrate(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let out = flag(&mut args, "--out")?;
    finish(&args, 1)?;
    let path = args
        .first()
        .ok_or("vacuum-calibrate needs a table of readings")?
        .clone();
    let readings = vacuumcal::parse(&read_text(&path, cfg.max_file_bytes)?)?;
    let proposal = vacuumcal::calibrate(&readings, &vacuumcal::Params::from_config(&cfg))?;
    let text = vacuumcal::report(&proposal, lang);
    match &out {
        Some(o) => deliver(&text, Some(o), std::slice::from_ref(&path), lang)?,
        None => print!("{text}"),
    }
    if proposal.thresholds.is_none() {
        fail("the readings with and without a part overlap".into())
    } else {
        Ok(())
    }
}

/// `test-board-gen KIND PARTS_OUT FEEDERS_OUT [--seed S] [--count N] [--pitch MM]`: the tables of a board made up from a seed (`grid`, `mixed` or `dense`).
pub fn test_board_gen(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let seed = match flag(&mut args, "--seed")? {
        Some(v) => whole(&v, "--seed", 0.0, 1e12)?,
        None => 1,
    };
    let count = match flag(&mut args, "--count")? {
        Some(v) => whole(&v, "--count", 1.0, testboard::MAX_PARTS as f64)? as usize,
        None => cfg.testboard_count as usize,
    };
    let pitch = match flag(&mut args, "--pitch")? {
        Some(v) => num(&v, "--pitch")?,
        None => cfg.testboard_pitch_mm,
    };
    let dry = switch(&mut args, "--print");
    finish(&args, 3)?;
    if args.len() < 3 && !dry {
        return Err(
            "test-board-gen needs the kind, the file of parts and the file of feeders".into(),
        );
    }
    let kind = Kind::parse(args.first().ok_or("test-board-gen needs the kind")?)
        .ok_or("the kind is grid, mixed or dense")?;
    let board = testboard::make(&Spec {
        kind,
        seed,
        count,
        pitch_mm: pitch,
    })?;
    if dry {
        print!("{}{}", board.parts_csv, board.feeders_csv);
    } else {
        deliver(&board.parts_csv, Some(&args[1]), &[], lang)?;
        deliver(&board.feeders_csv, Some(&args[2]), &[], lang)?;
    }
    println!(
        "{} board: {} parts, {} feeders, {} x {} mm",
        kind.name(),
        board.parts,
        board.feeders,
        show(board.size.0),
        show(board.size.1)
    );
    Ok(())
}
