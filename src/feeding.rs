//! Feeders with more than a tape at a fixed place: a priority, a tray of cells or a strip with a pitch, and a way to switch one off. The table of feeders is the same CSV as before with
//! more optional columns, and the choice of the feeder for a part does not depend on the order in which the parts or the feeders are listed.

use crate::error::{Code, PnpError};
use crate::feeders::Feeder;
use crate::parts::{detect_delimiter, read_records, Part, Side};
use crate::units::rotate_xy;
use std::collections::HashMap;

pub const MAX_FEEDERS: usize = 10_000;
const REQUIRED: [&str; 6] = ["id", "value", "package", "x", "y", "count"];
const OPTIONAL: [&str; 11] = [
    "priority",
    "kind",
    "cols",
    "rows",
    "pitch_x",
    "pitch_y",
    "pitch",
    "rotation",
    "enabled",
    "warn_below",
    "slot",
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FeederKind {
    /// A tape: the next part is always at the same place (`x`, `y`).
    Tape,
    /// A tray of `cols` by `rows` cells, filled row by row from the cell at (`x`, `y`); the cells are `pitch_x` and `pitch_y` apart, along axes turned by `rotation`.
    Tray {
        cols: u32,
        rows: u32,
        pitch_x: f64,
        pitch_y: f64,
        rotation: f64,
    },
    /// A strip: the next part is at (`x`, `y`) and each one after it a `pitch` further along a line turned by `rotation`.
    Strip { pitch: f64, rotation: f64 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct FeederSpec {
    pub base: Feeder,
    /// Higher is used first: -1 low, 0 normal, 1 high.
    pub priority: i32,
    pub kind: FeederKind,
    pub enabled: bool,
    /// The angle at which the parts lie in the tape, in degrees.
    pub tape_rotation: f64,
    /// The warning when the parts left fall below this number (0 = no warning).
    pub warn_below: u32,
    pub slot: Option<String>,
}

impl FeederSpec {
    /// A plain tape feeder: normal priority, switched on.
    pub fn from_basic(base: Feeder) -> FeederSpec {
        FeederSpec {
            base,
            priority: 0,
            kind: FeederKind::Tape,
            enabled: true,
            tape_rotation: 0.0,
            warn_below: 0,
            slot: None,
        }
    }

    /// How many parts the feeder held when it was full: the cells of a tray, else the count it was given.
    pub fn capacity(&self) -> u32 {
        match self.kind {
            FeederKind::Tray { cols, rows, .. } => cols.saturating_mul(rows),
            _ => self.base.count,
        }
    }

    /// Where the part is picked when `taken` parts have already been taken from the feeder in this job.
    pub fn pick_point(&self, taken: u32) -> (f64, f64) {
        let origin = (self.base.x, self.base.y);
        match self.kind {
            FeederKind::Tape => origin,
            FeederKind::Strip { pitch, rotation } => {
                let (dx, dy) = rotate_xy(pitch * f64::from(taken), 0.0, rotation);
                (origin.0 + dx, origin.1 + dy)
            }
            FeederKind::Tray {
                cols,
                pitch_x,
                pitch_y,
                rotation,
                ..
            } => {
                let used = self.capacity().saturating_sub(self.base.count);
                let cell = used.saturating_add(taken);
                let cols = cols.max(1);
                let (col, row) = (cell % cols, cell / cols);
                let (dx, dy) =
                    rotate_xy(pitch_x * f64::from(col), pitch_y * f64::from(row), rotation);
                (origin.0 + dx, origin.1 + dy)
            }
        }
    }
}

fn bad(row: usize, message: String) -> PnpError {
    PnpError::new(Code::FeederBadRow, format!("feeder row {row}: {message}"))
}

fn number(row: usize, name: &str, cell: &str, lo: f64, hi: f64) -> Result<f64, PnpError> {
    let v: f64 = cell
        .parse()
        .ok()
        .filter(|v: &f64| v.is_finite() && (lo..=hi).contains(v))
        .ok_or_else(|| {
            bad(
                row,
                format!("{name} '{cell}' is not a number from {lo} to {hi}"),
            )
        })?;
    Ok(v)
}

/// A CSV of feeders: the columns `id,value,package,x,y,count` in any order and, optionally, `priority` (low, normal, high), `kind` (tape, tray, strip), `cols`, `rows`, `pitch_x`,
/// `pitch_y` (for a tray), `pitch` (for a strip), `rotation` (the angle of the parts in the tape or of the axes of a tray or a strip), `enabled`, `warn_below` and `slot`.
pub fn from_csv(text: &str) -> Result<Vec<FeederSpec>, PnpError> {
    let text = text.trim_start_matches('\u{feff}');
    let head_line = text
        .lines()
        .find(|l| !l.trim().is_empty())
        .ok_or_else(|| bad(1, "the file of feeders is empty".to_string()))?;
    let mut records = read_records(text, detect_delimiter(head_line))
        .map_err(|e| bad(1, e))?
        .into_iter();
    let header: Vec<String> = records
        .next()
        .ok_or_else(|| bad(1, "the file of feeders is empty".to_string()))?
        .iter()
        .map(|h| h.to_ascii_lowercase())
        .collect();
    for (i, h) in header.iter().enumerate() {
        if !REQUIRED.contains(&h.as_str()) && !OPTIONAL.contains(&h.as_str()) {
            return Err(bad(1, format!("the column '{h}' is not known")));
        }
        if header[..i].contains(h) {
            return Err(bad(1, format!("the column '{h}' is written twice")));
        }
    }
    for r in REQUIRED {
        if !header.iter().any(|h| h == r) {
            return Err(bad(1, format!("the header has no column '{r}'")));
        }
    }
    let at = |name: &str| header.iter().position(|h| h == name);
    let mut out: Vec<FeederSpec> = Vec::new();
    let mut ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (i, f) in records.enumerate() {
        let row = i + 2;
        if out.len() >= MAX_FEEDERS {
            return Err(bad(row, format!("more than {MAX_FEEDERS} feeders")));
        }
        if f.len() != header.len() {
            return Err(bad(
                row,
                format!("{} fields, not {}", f.len(), header.len()),
            ));
        }
        let cell = |name: &str| at(name).map_or("", |c| f[c].as_str());
        let id = cell("id");
        if id.is_empty() || !ids.insert(id.to_string()) {
            return Err(bad(row, format!("the id '{id}' is empty or used twice")));
        }
        let count = cell("count")
            .parse::<u32>()
            .ok()
            .filter(|c| *c <= 1_000_000)
            .ok_or_else(|| {
                bad(
                    row,
                    format!("'{}' is not a count of parts up to 1000000", cell("count")),
                )
            })?;
        let base = Feeder {
            id: id.to_string(),
            value: cell("value").to_string(),
            package: cell("package").to_string(),
            x: number(row, "x", cell("x"), -10_000.0, 10_000.0)?,
            y: number(row, "y", cell("y"), -10_000.0, 10_000.0)?,
            count,
        };
        let priority = match cell("priority").to_ascii_lowercase().as_str() {
            "" | "normal" => 0,
            "low" => -1,
            "high" => 1,
            other => {
                return Err(bad(
                    row,
                    format!("priority '{other}' is not low, normal or high"),
                ))
            }
        };
        let rotation_cell = cell("rotation");
        let rotation = if rotation_cell.is_empty() {
            0.0
        } else {
            number(row, "rotation", rotation_cell, -3600.0, 3600.0)?.rem_euclid(360.0)
        };
        let kind = match cell("kind").to_ascii_lowercase().as_str() {
            "" | "tape" => FeederKind::Tape,
            "tray" => {
                let dim = |name: &str| -> Result<u32, PnpError> {
                    let v = number(row, name, cell(name), 1.0, 1000.0)?;
                    if v.fract() != 0.0 {
                        return Err(bad(row, format!("{name} must be a whole number")));
                    }
                    Ok(v as u32)
                };
                let pitch = |name: &str| -> Result<f64, PnpError> {
                    let v = number(row, name, cell(name), -1000.0, 1000.0)?;
                    if v == 0.0 {
                        return Err(bad(row, format!("{name} cannot be 0")));
                    }
                    Ok(v)
                };
                let (cols, rows) = (dim("cols")?, dim("rows")?);
                if count > cols * rows {
                    return Err(bad(
                        row,
                        format!(
                            "count {count} is more than the {} cells of the tray",
                            cols * rows
                        ),
                    ));
                }
                FeederKind::Tray {
                    cols,
                    rows,
                    pitch_x: pitch("pitch_x")?,
                    pitch_y: pitch("pitch_y")?,
                    rotation,
                }
            }
            "strip" => {
                let pitch = number(row, "pitch", cell("pitch"), -1000.0, 1000.0)?;
                if pitch == 0.0 {
                    return Err(bad(row, "pitch cannot be 0".to_string()));
                }
                FeederKind::Strip { pitch, rotation }
            }
            other => {
                return Err(bad(
                    row,
                    format!("kind '{other}' is not tape, tray or strip"),
                ))
            }
        };
        let enabled = match cell("enabled").to_ascii_lowercase().as_str() {
            "" | "yes" | "true" | "1" => true,
            "no" | "false" | "0" => false,
            other => return Err(bad(row, format!("enabled '{other}' is not yes or no"))),
        };
        let warn_below = if cell("warn_below").is_empty() {
            0
        } else {
            number(row, "warn_below", cell("warn_below"), 0.0, 1_000_000.0)? as u32
        };
        let tape_rotation = if kind == FeederKind::Tape {
            rotation
        } else {
            0.0
        };
        let slot = (!cell("slot").is_empty()).then(|| cell("slot").to_string());
        out.push(FeederSpec {
            base,
            priority,
            kind,
            enabled,
            tape_rotation,
            warn_below,
            slot,
        });
    }
    Ok(out)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pick {
    /// The number of the part in the list given.
    pub part: usize,
    /// The number of the feeder in the list given.
    pub feeder: usize,
    /// Where the part is picked.
    pub at: (f64, f64),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Picks {
    pub picks: Vec<Pick>,
    /// The references of the parts for which no feeder holds the value and the package.
    pub without_feeder: Vec<String>,
    /// The feeders that do not have enough parts left (or are switched off), with how many parts are missing; sorted by id.
    pub short: Vec<(String, u32)>,
    /// The feeders that will be nearly empty, with the parts that will be left (see `warn_below`); sorted by id.
    pub low: Vec<(String, u32)>,
}

/// The feeder that holds the kind `key`, is switched on and still has parts, chosen by the highest priority, then the shortest distance from its next pick point to `near`, then the
/// lowest id. `left` and `taken` say, for every feeder of the list, the parts left and the parts taken in this job.
pub fn choose(
    feeders: &[FeederSpec],
    left: &[u32],
    taken: &[u32],
    key: &str,
    near: (f64, f64),
) -> Option<usize> {
    let mut best: Option<(i32, f64, usize)> = None;
    for (n, f) in feeders.iter().enumerate() {
        if !f.enabled || left[n] == 0 || f.base.key() != key {
            continue;
        }
        let p = f.pick_point(taken[n]);
        let d = (p.0 - near.0).hypot(p.1 - near.1);
        let better = match best {
            None => true,
            Some((bp, bd, bn)) => {
                f.priority > bp
                    || (f.priority == bp
                        && (d < bd || (d == bd && f.base.id < feeders[bn].base.id)))
            }
        };
        if better {
            best = Some((f.priority, d, n));
        }
    }
    best.map(|b| b.2)
}

/// Gives every part of `side` (all the parts for `None`) a feeder. The parts are taken in the order of their references, so the same board and the same feeders give the same
/// choices whatever the order in which they are listed. Of two feeders of the same priority the one whose next pick point is nearer to the position of the part in the file is used.
pub fn assign(parts: &[Part], feeders: &[FeederSpec], side: Option<Side>) -> Picks {
    let mut order: Vec<usize> = (0..parts.len()).collect();
    order.sort_by(|a, b| parts[*a].reference.cmp(&parts[*b].reference));
    let mut left: Vec<u32> = feeders.iter().map(|f| f.base.count).collect();
    let mut taken: Vec<u32> = vec![0; feeders.len()];
    let mut by_key: HashMap<String, Vec<usize>> = HashMap::new();
    for (n, f) in feeders.iter().enumerate() {
        by_key.entry(f.base.key()).or_default().push(n);
    }
    let mut out = Picks::default();
    let mut short: HashMap<String, u32> = HashMap::new();
    for i in order {
        let p = &parts[i];
        if side.is_some_and(|s| s != p.side) {
            continue;
        }
        let key = p.key();
        let Some(same) = by_key.get(&key) else {
            out.without_feeder.push(p.reference.clone());
            continue;
        };
        match choose(feeders, &left, &taken, &key, (p.x, p.y)) {
            Some(n) => {
                out.picks.push(Pick {
                    part: i,
                    feeder: n,
                    at: feeders[n].pick_point(taken[n]),
                });
                left[n] -= 1;
                taken[n] += 1;
            }
            None => {
                let last = same
                    .iter()
                    .max_by(|a, b| feeders[**a].base.id.cmp(&feeders[**b].base.id));
                if let Some(&n) = last {
                    *short.entry(feeders[n].base.id.clone()).or_insert(0) += 1;
                }
            }
        }
    }
    out.picks.sort_by_key(|p| p.part);
    out.without_feeder.sort();
    let mut short: Vec<(String, u32)> = short.into_iter().collect();
    short.sort();
    out.short = short;
    let mut low: Vec<(String, u32)> = feeders
        .iter()
        .enumerate()
        .filter(|(n, f)| f.warn_below > 0 && left[*n] < f.warn_below)
        .map(|(n, f)| (f.base.id.clone(), left[n]))
        .collect();
    low.sort();
    out.low = low;
    out
}
