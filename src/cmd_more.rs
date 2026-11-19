// EH3D-PHOENIX-PNP - the commands that compare and count: parts-diff, config-diff, board-stats, queue-stock and feeder-deplete.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//
// Each one reads its files, calls the library and prints; none of them moves a machine or writes anything but the file named by --out, and a file that exists is never written over.

use crate::cli_args::{finish, flag, num};
use crate::cmd_tools::language_and_config;
use crate::commands::{read_text, refuse_same_file};
use eh3d_phoenix_pnp::error::PnpError;
use eh3d_phoenix_pnp::parts::parse_all;
use eh3d_phoenix_pnp::partsdiff;
use eh3d_phoenix_pnp::report;

/// `parts-diff FIRST SECOND [--tol MM] [--rot-tol DEG] [--out F]`: what changed between two tables of parts, part by part (added, removed, moved, turned, another value, another package,
/// the other side). The report goes to the screen, or the changes as a CSV to `--out` (never over a file that exists). The exit code is 1 when the tables differ.
pub fn parts_diff(mut args: Vec<String>) -> Result<(), String> {
    let (lang, cfg) = language_and_config(&mut args)?;
    let mut settings = partsdiff::Settings::default();
    if let Some(v) = flag(&mut args, "--tol")? {
        settings.tol_mm = num(&v, "--tol")?;
    }
    if let Some(v) = flag(&mut args, "--rot-tol")? {
        settings.rot_tol_deg = num(&v, "--rot-tol")?;
    }
    let out = flag(&mut args, "--out")?;
    finish(&args, 2)?;
    let (a, b) = match (args.first(), args.get(1)) {
        (Some(a), Some(b)) => (a.clone(), b.clone()),
        _ => return Err("parts-diff needs two tables of parts".into()),
    };
    let l = |e: PnpError| e.localized(lang);
    let first = parse_all(&read_text(&a, cfg.max_file_bytes)?, &cfg.profile)?;
    let second = parse_all(&read_text(&b, cfg.max_file_bytes)?, &cfg.profile)?;
    let diff = partsdiff::diff(&first.parts, &second.parts, &settings).map_err(l)?;
    print!("{}", diff.text());
    if let Some(o) = &out {
        refuse_same_file(&[a, b], o)?;
        report::write_new(o, &diff.csv()).map_err(l)?;
        println!("written to {o}");
    }
    if diff.is_same() {
        Ok(())
    } else {
        Err(format!("{} parts differ", diff.parts_changed()))
    }
}
