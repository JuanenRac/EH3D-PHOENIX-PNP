// EH3D-PHOENIX-PNP - the table of feeders as it stands after a job.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! A job takes parts from the tapes, the strips and the trays. When it ends (or when it is stopped and the board is taken off), the table of feeders still says what it said before the
//! first pick, so the next job would count on parts that are gone. This reads the saved state of a job (`simulate --state`) and writes the table as it is now: the count of each feeder
//! is what is left, and the next place of a strip is moved along by the parts taken (a tray keeps its place: its next cell comes from its count). A feeder whose count falls under its
//! warning, or to nothing, is said. Everything else in the table, its columns and the order of its rows, is kept as it was.

use crate::error::{Code, PnpError};
use crate::feeding::{from_csv, FeederKind};
use crate::parts::{csv_field, detect_delimiter, formula_guard, read_records};

/// What happened to one feeder.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub id: String,
    pub before: u32,
    pub taken: u32,
    pub after: u32,
    /// The next place of a strip that moved, from and to.
    pub moved: Option<((f64, f64), (f64, f64))>,
    /// The parts left are under the warning of the feeder, or none are left.
    pub low: bool,
}

/// The table after the job, and what became of each feeder.
#[derive(Debug, Clone, PartialEq)]
pub struct Depleted {
    pub lines: Vec<Line>,
    /// The table of feeders with the new counts and places.
    pub csv: String,
}

fn bad(message: impl Into<String>) -> PnpError {
    PnpError::new(Code::JobStateBad, message)
}

/// A place with no more than six decimals and no trailing zeros.
pub(crate) fn place(v: f64) -> String {
    let s = format!("{v:.6}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-0" {
        "0".to_string()
    } else {
        s.to_string()
    }
}

/// Takes the parts that the saved state says were taken out of the table of feeders. The state has to be of a job made with this table: the number of feeders is the same, and no feeder
/// has more parts left than it had.
pub fn deplete(feeders_text: &str, state_text: &str) -> Result<Depleted, PnpError> {
    let specs = from_csv(feeders_text)?;
    let state: serde_json::Value =
        serde_json::from_str(state_text).map_err(|e| bad(format!("the state is not JSON: {e}")))?;
    let devices = state
        .get("devices")
        .and_then(|d| d.as_array())
        .ok_or_else(|| bad("the state has no list of feeders"))?;
    if devices.len() != specs.len() {
        return Err(PnpError::new(
            Code::JobStateMismatch,
            format!(
                "the state has {} feeders and the table has {}",
                devices.len(),
                specs.len()
            ),
        ));
    }
    let text = feeders_text.trim_start_matches('\u{feff}');
    let head_line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let delim = detect_delimiter(head_line);
    let mut records = read_records(text, delim).map_err(bad)?;
    let header: Vec<String> = records[0].iter().map(|h| h.to_ascii_lowercase()).collect();
    let col = |name: &str| header.iter().position(|h| h == name);
    let (Some(c_count), c_x, c_y) = (col("count"), col("x"), col("y")) else {
        return Err(bad("the table of feeders has no column count"));
    };
    let mut lines = Vec::new();
    for (i, spec) in specs.iter().enumerate() {
        let left = devices[i]
            .get("remaining")
            .and_then(|r| r.as_u64())
            .and_then(|r| u32::try_from(r).ok())
            .ok_or_else(|| {
                bad(format!(
                    "the feeder {} has no count of parts left",
                    spec.base.id
                ))
            })?;
        let before = spec.base.count;
        if left > before {
            return Err(PnpError::new(
                Code::JobStateMismatch,
                format!(
                    "the feeder {} held {before} parts and the state says {left} are left",
                    spec.base.id
                ),
            ));
        }
        let taken = before - left;
        let moved = match spec.kind {
            FeederKind::Strip { .. } if taken > 0 => {
                Some(((spec.base.x, spec.base.y), spec.pick_point(taken)))
            }
            _ => None,
        };
        let row = &mut records[i + 1];
        row[c_count] = left.to_string();
        if let (Some(((_, _), (nx, ny))), Some(cx), Some(cy)) = (moved, c_x, c_y) {
            row[cx] = place(nx);
            row[cy] = place(ny);
        }
        lines.push(Line {
            id: spec.base.id.clone(),
            before,
            taken,
            after: left,
            moved,
            low: left == 0 || (spec.warn_below > 0 && left < spec.warn_below),
        });
    }
    let sep = delim.to_string();
    let mut csv = String::new();
    for r in &records {
        let cells: Vec<String> = r.iter().map(|c| csv_cell(c, delim)).collect();
        csv.push_str(&cells.join(&sep));
        csv.push('\n');
    }
    Ok(Depleted { lines, csv })
}

/// A field of the table, in quotes when it holds the delimiter.
pub(crate) fn csv_cell(text: &str, delim: char) -> String {
    if delim == ',' {
        return csv_field(text);
    }
    let text = formula_guard(text);
    if text.contains([delim, '"', '\n', '\r']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.into_owned()
    }
}

impl Depleted {
    /// The parts taken, all feeders together.
    pub fn taken(&self) -> u64 {
        self.lines.iter().map(|l| u64::from(l.taken)).sum()
    }

    /// The feeders that are low or empty.
    pub fn low(&self) -> usize {
        self.lines.iter().filter(|l| l.low).count()
    }

    /// One line for each feeder that gave parts or is low, then a line that sums it up.
    pub fn text(&self) -> String {
        let mut out = String::new();
        for l in self.lines.iter().filter(|l| l.taken > 0 || l.low) {
            out.push_str(&format!(
                "{}: {} -> {} ({} taken)",
                l.id, l.before, l.after, l.taken
            ));
            if let Some((from, to)) = l.moved {
                out.push_str(&format!(
                    ", next place {:.3},{:.3} -> {:.3},{:.3}",
                    from.0, from.1, to.0, to.1
                ));
            }
            if l.after == 0 {
                out.push_str(", EMPTY");
            } else if l.low {
                out.push_str(", LOW");
            }
            out.push('\n');
        }
        out.push_str(&format!(
            "{} parts taken from {} feeders of {}; {} low or empty\n",
            self.taken(),
            self.lines.iter().filter(|l| l.taken > 0).count(),
            self.lines.len(),
            self.low()
        ));
        out
    }
}
