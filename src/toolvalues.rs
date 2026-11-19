// EH3D-PHOENIX-PNP - the values that the forms of the tools start from, and how they are kept in the configuration.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! The tolerances of the parts diff, the distance and the number of pairs of the board statistics, the most plans of the search for the places of the feeders, the number of runs and the steps of
//! the journal are settings of the machine file (`parts_diff_tol_mm`, `parts_diff_rot_tol_deg`, `board_stats_near_mm`, `board_stats_limit`, `feeder_slots_max_plans`, `simulate_runs_count`,
//! `journal_steps`), each with its range, and not numbers of the program. The commands take them as the value to use when the option is not given; the panels of the Tools tab start from them
//! and can keep what was typed in the file again. Keeping a value changes only its line (or adds one at the end): the other lines, the comments and the line endings stay as they are, and
//! the file is written to a file beside it and moved into place, so a cut never leaves half a file.

use crate::config::{MachineConfig, MAX_CONFIG_BYTES};
use crate::error::{Code, PnpError};
use std::path::Path;

/// The values of the forms, as the configuration has them.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolValues {
    pub parts_diff_tol_mm: f64,
    pub parts_diff_rot_tol_deg: f64,
    pub board_stats_near_mm: f64,
    pub board_stats_limit: u32,
    pub feeder_slots_max_plans: u32,
    pub simulate_runs_count: u32,
    pub journal_steps: u32,
}

impl ToolValues {
    /// The values of a configuration.
    pub fn from_config(cfg: &MachineConfig) -> ToolValues {
        ToolValues {
            parts_diff_tol_mm: cfg.parts_diff_tol_mm,
            parts_diff_rot_tol_deg: cfg.parts_diff_rot_tol_deg,
            board_stats_near_mm: cfg.board_stats_near_mm,
            board_stats_limit: cfg.board_stats_limit,
            feeder_slots_max_plans: cfg.feeder_slots_max_plans,
            simulate_runs_count: cfg.simulate_runs_count,
            journal_steps: cfg.journal_steps,
        }
    }

    /// Puts the values in a configuration (the ranges are not checked here: `merge` and `keep` check the file they write).
    pub fn apply(&self, cfg: &mut MachineConfig) {
        cfg.parts_diff_tol_mm = self.parts_diff_tol_mm;
        cfg.parts_diff_rot_tol_deg = self.parts_diff_rot_tol_deg;
        cfg.board_stats_near_mm = self.board_stats_near_mm;
        cfg.board_stats_limit = self.board_stats_limit;
        cfg.feeder_slots_max_plans = self.feeder_slots_max_plans;
        cfg.simulate_runs_count = self.simulate_runs_count;
        cfg.journal_steps = self.journal_steps;
    }

    /// The `key = value` lines of the values, in the order of `config::KEYS`.
    pub fn lines(&self) -> Vec<(&'static str, String)> {
        vec![
            ("parts_diff_tol_mm", number(self.parts_diff_tol_mm)),
            (
                "parts_diff_rot_tol_deg",
                number(self.parts_diff_rot_tol_deg),
            ),
            ("board_stats_near_mm", number(self.board_stats_near_mm)),
            ("board_stats_limit", self.board_stats_limit.to_string()),
            (
                "feeder_slots_max_plans",
                self.feeder_slots_max_plans.to_string(),
            ),
            ("simulate_runs_count", self.simulate_runs_count.to_string()),
            ("journal_steps", self.journal_steps.to_string()),
        ]
    }
}

/// A number the way the file writes it: no trailing zeros, no exponent.
fn number(x: f64) -> String {
    let mut s = format!("{x:.6}");
    while s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    if s.is_empty() || s == "-" {
        s = "0".to_string();
    }
    s
}

/// Whether `line` is the line that sets `key` (not a comment).
fn sets(line: &str, key: &str) -> bool {
    let t = line.trim_start();
    match t.strip_prefix(key) {
        Some(rest) => rest.trim_start().starts_with('='),
        None => false,
    }
}

/// `text` with each of `lines` set: the line of the key is replaced where it stands, and a key that has no line is added at the end. Nothing else changes, not even the line endings.
pub fn set_lines(text: &str, lines: &[(&str, String)]) -> String {
    let crlf = text.contains("\r\n");
    let eol = if crlf { "\r\n" } else { "\n" };
    let mut out: Vec<String> = text
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l).to_string())
        .collect();
    // the text ends with a line end: the last piece is empty and is put back at the end
    let ended = out.last().is_some_and(String::is_empty);
    if ended {
        out.pop();
    }
    for (key, value) in lines {
        let new = format!("{key} = {value}");
        match out.iter().position(|l| sets(l, key)) {
            Some(i) => out[i] = new,
            None => out.push(new),
        }
    }
    let mut joined = out.join(eol);
    joined.push_str(eol);
    joined
}

/// The text of a configuration file with the values kept in it. The result is read again as a whole, so a value out of its range, or a file that was not valid, is refused with the
/// code and the line and nothing is changed.
pub fn merge(text: &str, values: &ToolValues) -> Result<String, PnpError> {
    if text.len() > MAX_CONFIG_BYTES {
        return Err(PnpError::new(
            Code::FileTooBig,
            format!("the configuration is larger than {MAX_CONFIG_BYTES} bytes"),
        ));
    }
    let out = set_lines(text, &values.lines());
    MachineConfig::parse(&out)?;
    Ok(out)
}

/// Keeps the values in the configuration file `path`: the file is read, the values merged, and the result written to `PATH.tmp` and moved into place. A file that is not there is not
/// created (the values go to a file the person loaded).
pub fn keep(path: &Path, values: &ToolValues) -> Result<(), PnpError> {
    let shown = path.display();
    let bytes = std::fs::read(path)
        .map_err(|e| PnpError::new(Code::DriverError, format!("{shown}: {e}")))?;
    if bytes.len() > MAX_CONFIG_BYTES {
        return Err(PnpError::new(
            Code::FileTooBig,
            format!("{shown} is larger than {MAX_CONFIG_BYTES} bytes"),
        ));
    }
    let text = String::from_utf8(bytes)
        .map_err(|_| PnpError::new(Code::DriverError, format!("{shown} is not text in UTF-8")))?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let merged = merge(text, values)?;
    let mut tmp = path.as_os_str().to_os_string();
    tmp.push(".tmp");
    let tmp = std::path::PathBuf::from(tmp);
    std::fs::write(&tmp, merged.as_bytes())
        .and_then(|()| std::fs::rename(&tmp, path))
        .map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            PnpError::new(Code::DriverError, format!("{shown}: {e}"))
        })
}
