//! The command `phoenix-pnp project ...`: the work kept in the branch `pnp/` of an `.eh3d` project. The exit code is 2 when the project holds something out of date or wrong.

use std::path::{Path, PathBuf};

use crate::cli_args::{finish, flag, switch};
use crate::cmd_nodes::not_passed;
use crate::commands::read_text;
use eh3d_phoenix_pnp::config::{MachineConfig, MAX_CONFIG_BYTES};
use eh3d_phoenix_pnp::eh3dfile::{self, Identity, Inputs, Kind, PositionOptions};
use eh3d_phoenix_pnp::error::Lang;
use eh3d_phoenix_pnp::import::{ColumnMap, Format};

const ACTIONS: &str = "project needs list, save, import, open, take or extract (see help)";

fn config_of(path: Option<&str>) -> Result<MachineConfig, String> {
    Ok(match path {
        Some(p) => MachineConfig::parse(&read_text(p, MAX_CONFIG_BYTES as u64)?)
            .map_err(|e| e.localized(Lang::En))?,
        None => MachineConfig::default(),
    })
}

fn identity(args: &mut Vec<String>) -> Result<Identity, String> {
    Ok(Identity {
        name: flag(args, "--name")?,
        id: flag(args, "--id")?,
    })
}

fn kind_flag(args: &mut Vec<String>) -> Result<Option<Kind>, String> {
    flag(args, "--kind")?
        .map(|k| {
            Kind::parse(&k).ok_or_else(|| format!("--kind '{k}' is not one of parts bom feeders"))
        })
        .transpose()
}

fn words<const N: usize>(args: &[String], what: &str) -> Result<[String; N], String> {
    finish(args, N)?;
    <[String; N]>::try_from(args.to_vec()).map_err(|_| format!("project {what} (see help)"))
}

fn notes(list: &[String]) {
    for note in list {
        eprintln!("note: {note}");
    }
}

/// What is out of date or wrong is told and the check is not passed.
fn verdict(warnings: &[String]) -> Result<(), String> {
    notes(warnings);
    if warnings.is_empty() {
        Ok(())
    } else {
        Err(not_passed(format!(
            "the project has {} things out of date or wrong",
            warnings.len()
        )))
    }
}

/// `phoenix-pnp project list|save|import|open|take|extract FILE.eh3d ...`
pub fn project(mut args: Vec<String>) -> Result<(), String> {
    if args.is_empty() {
        return Err(ACTIONS.into());
    }
    let what = args.remove(0);
    match what.as_str() {
        "list" => list(args),
        "save" => save(args),
        "import" => import(args),
        "open" => open(args),
        "take" => take(args),
        "extract" => {
            let [file, branch, dir] = words::<3>(&args, "extract needs FILE.eh3d BRANCH DIR")?;
            let n = eh3dfile::extract(Path::new(&file), &branch, Path::new(&dir))?;
            println!("{n} files written to {dir}");
            Ok(())
        }
        _ => Err(ACTIONS.into()),
    }
}

fn list(mut args: Vec<String>) -> Result<(), String> {
    let json = switch(&mut args, "--json");
    let [file] = words::<1>(&args, "list needs FILE.eh3d")?;
    let listing = eh3dfile::list(Path::new(&file))?;
    if json {
        let rows: Vec<serde_json::Value> = listing
            .rows
            .iter()
            .map(|r| {
                serde_json::json!({
                    "id": r.id, "app": r.app, "format": r.format, "app_version": r.app_version,
                    "files": r.files, "hash": r.hash, "notes": r.notes,
                })
            })
            .collect();
        let document = serde_json::json!({
            "name": listing.name, "id": listing.id, "branches": rows, "warnings": listing.warnings,
        });
        println!("{document:#}");
    } else {
        println!("project {} ({})", listing.name, listing.id);
        for r in &listing.rows {
            println!(
                "  {:<16} {:>4} files  format {:<6} {} {}  {}",
                r.id,
                r.files,
                r.format,
                r.app,
                r.app_version,
                r.notes.join(", ")
            );
        }
        println!("{} branch(es)", listing.rows.len());
    }
    verdict(&listing.warnings)
}

fn save(mut args: Vec<String>) -> Result<(), String> {
    let inputs = Inputs {
        parts: flag(&mut args, "--parts")?,
        bom: flag(&mut args, "--bom")?,
        feeders: flag(&mut args, "--feeders")?,
        config: flag(&mut args, "--config")?,
    };
    let who = identity(&mut args)?;
    let [file] = words::<1>(&args, "save needs FILE.eh3d")?;
    let cfg = config_of(inputs.config.as_deref())?;
    notes(&eh3dfile::save(Path::new(&file), &inputs, &who, &cfg)?);
    println!("saved: the branch pnp of {file}");
    Ok(())
}

fn import(mut args: Vec<String>) -> Result<(), String> {
    let cfg = config_of(flag(&mut args, "--config")?.as_deref())?;
    let kind = kind_flag(&mut args)?;
    let format = match flag(&mut args, "--format")? {
        Some(f) => Format::parse(&f)
            .ok_or_else(|| format!("--format '{f}' is not one of auto csv eagle kicad-pos"))?,
        None => Format::Auto,
    };
    let map = match flag(&mut args, "--map")? {
        Some(p) => Some(
            ColumnMap::parse(&read_text(&p, MAX_CONFIG_BYTES as u64)?)
                .map_err(|e| e.localized(Lang::En))?,
        ),
        None => None,
    };
    let who = identity(&mut args)?;
    let [file, source] = words::<2>(&args, "import needs FILE.eh3d SOURCE --kind KIND")?;
    let kind = kind.ok_or("project import needs --kind parts, bom or feeders")?;
    let positions = PositionOptions { format, map };
    let (report, said) = eh3dfile::import_file(
        Path::new(&file),
        Path::new(&source),
        kind,
        &who,
        &cfg,
        &positions,
    )?;
    notes(&said);
    println!(
        "imported {} as {} ({})",
        report.source.name,
        kind.name(),
        report.source.format
    );
    for c in &report.read {
        println!("  read: {} {}", c.count, c.item);
    }
    for c in &report.export_losses {
        println!(
            "  not kept in the table: {} ({})",
            c.item,
            c.reason.as_deref().unwrap_or("-")
        );
    }
    Ok(())
}

fn open(mut args: Vec<String>) -> Result<(), String> {
    let cfg = config_of(flag(&mut args, "--config")?.as_deref())?;
    let into = flag(&mut args, "--into")?.map(PathBuf::from);
    let [file] = words::<1>(&args, "open needs FILE.eh3d")?;
    let opened = eh3dfile::open(Path::new(&file), into.as_deref(), &cfg)?;
    match (&opened.parts, opened.part_count) {
        (Some(rel), Some((mount, dnp))) => {
            println!("parts: {rel} ({mount} to mount, {dnp} not to be mounted)")
        }
        _ => println!("parts: none"),
    }
    match (&opened.bom, opened.bom_count) {
        (Some(rel), Some((rows, references))) => {
            println!("bom: {rel} ({rows} rows, {references} references)")
        }
        _ => println!("bom: none"),
    }
    match (&opened.feeders, opened.feeder_count) {
        (Some(rel), Some(n)) => println!("feeders: {rel} ({n} feeders)"),
        _ => println!("feeders: none"),
    }
    if let Some(rel) = &opened.config {
        println!("configuration: {rel}");
    }
    for (kind, report) in &opened.reports {
        println!(
            "imported {}: {} ({})",
            kind.name(),
            report.source.name,
            report.source.format
        );
    }
    if let (Some(n), Some(dir)) = (opened.written, &into) {
        println!("{n} files written to {}", dir.display());
    }
    verdict(&opened.warnings)
}

fn take(mut args: Vec<String>) -> Result<(), String> {
    let cfg = config_of(flag(&mut args, "--config")?.as_deref())?;
    let kind = kind_flag(&mut args)?;
    let [file, branch, name] = words::<3>(&args, "take needs FILE.eh3d BRANCH NAME")?;
    let (target, found, said) = eh3dfile::take(Path::new(&file), &branch, &name, kind, &cfg)?;
    notes(&said);
    match found {
        Some(k) => println!("taken: {target} is now the {} of the work", k.name()),
        None => println!("taken: {target} is kept as a reference"),
    }
    Ok(())
}
