// EH3D-PHOENIX-PNP - the table of feeders after a reel is changed.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! When a reel is empty the operator puts a new one in, and the table of feeders has to say so: the count of the feeder is the parts of the new reel, and a strip starts again from its first
//! place. This reads a small table `id,count[,x,y]` (one row for each feeder that was refilled) and writes the table of feeders with those rows changed; everything else in the table, its
//! columns and the order of its rows, is kept as it was. A feeder that is not in the table, a count of a tray that is more than its cells, a count of 0, a place for a feeder that is not a
//! strip and a feeder named twice are refused with the code `PHOENIX-PNP-013` and the line of the refill. The place of a strip that is not given is left where it was, and the report says so.

use crate::deplete::{csv_cell, place};
use crate::error::{Code, PnpError};
use crate::feeding::{from_csv, FeederKind};
use crate::parts::{detect_delimiter, read_records};
use std::collections::BTreeSet;

/// The most rows of a table of refills.
pub const MAX_REFILLS: usize = 5000;
/// The most parts that a reel may be said to hold, as the table of feeders allows.
pub const MAX_COUNT: u32 = 1_000_000;
/// The largest place of a strip, in millimetres.
const MAX_PLACE_MM: f64 = 100_000.0;

fn bad(message: impl Into<String>) -> PnpError {
    PnpError::new(Code::FeederBadRow, message)
}

/// One feeder that was refilled.
#[derive(Debug, Clone, PartialEq)]
pub struct Refill {
    pub id: String,
    /// The parts that the new reel holds.
    pub count: u32,
    /// Where the first part of a strip is, when it is given.
    pub start: Option<(f64, f64)>,
}

/// What happened to one feeder of the table.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub id: String,
    pub before: u32,
    pub after: u32,
    /// The place of a strip before and after, when the refill gave a new one.
    pub moved: Option<((f64, f64), (f64, f64))>,
    /// The feeder is a strip and the refill did not say where it starts: the place it had was kept.
    pub kept_place: bool,
}

/// The table after the refills, and what became of each feeder that was named.
#[derive(Debug, Clone, PartialEq)]
pub struct Refilled {
    pub lines: Vec<Line>,
    /// The table of feeders with the new counts and places.
    pub csv: String,
}

/// The table of refills: a header with `id` and `count` (and `x` and `y` together, or neither) and one row for each feeder.
pub fn parse_refills(text: &str) -> Result<Vec<Refill>, PnpError> {
    let text = text.trim_start_matches('\u{feff}');
    let head_line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let records = read_records(text, detect_delimiter(head_line)).map_err(bad)?;
    let Some((header, rows)) = records.split_first() else {
        return Err(bad("the table of refills is empty"));
    };
    let header: Vec<String> = header.iter().map(|h| h.to_ascii_lowercase()).collect();
    let col = |name: &str| header.iter().position(|h| h == name);
    let (Some(c_id), Some(c_count)) = (col("id"), col("count")) else {
        return Err(bad("the table of refills needs the columns id and count"));
    };
    let (c_x, c_y) = (col("x"), col("y"));
    if c_x.is_some() != c_y.is_some() {
        return Err(bad("the table of refills has x or y but not both"));
    }
    if rows.len() > MAX_REFILLS {
        return Err(bad(format!(
            "the table of refills has {} rows, the most is {MAX_REFILLS}",
            rows.len()
        )));
    }
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for (n, row) in rows.iter().enumerate() {
        let line = n + 2;
        let cell = |c: usize| row.get(c).map_or("", |s| s.as_str());
        let id = cell(c_id);
        if id.is_empty() {
            return Err(bad(format!("refill line {line}: the feeder has no id")));
        }
        if !seen.insert(id.to_string()) {
            return Err(bad(format!(
                "refill line {line}: the feeder {id} is named twice"
            )));
        }
        let count = cell(c_count)
            .parse::<u32>()
            .ok()
            .filter(|c| (1..=MAX_COUNT).contains(c))
            .ok_or_else(|| {
                bad(format!(
                    "refill line {line}: '{}' is not a count of parts from 1 to {MAX_COUNT}",
                    cell(c_count)
                ))
            })?;
        let start = match (c_x, c_y) {
            (Some(cx), Some(cy)) if !cell(cx).is_empty() || !cell(cy).is_empty() => {
                let number = |name: &str, v: &str| {
                    v.parse::<f64>()
                        .ok()
                        .filter(|n| n.is_finite() && n.abs() <= MAX_PLACE_MM)
                        .ok_or_else(|| {
                            bad(format!(
                                "refill line {line}: '{v}' is not a {name} within {MAX_PLACE_MM} mm"
                            ))
                        })
                };
                Some((number("x", cell(cx))?, number("y", cell(cy))?))
            }
            _ => None,
        };
        out.push(Refill {
            id: id.to_string(),
            count,
            start,
        });
    }
    Ok(out)
}

/// The table of feeders with the refills put in. The refills are checked against the table as a whole before anything is written, so a refill that is refused changes nothing.
pub fn refill(feeders_text: &str, refills: &[Refill]) -> Result<Refilled, PnpError> {
    let specs = from_csv(feeders_text)?;
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
    for r in refills {
        let Some(i) = specs.iter().position(|s| s.base.id == r.id) else {
            return Err(bad(format!(
                "the feeder {} is not in the table of feeders",
                r.id
            )));
        };
        let spec = &specs[i];
        if let FeederKind::Tray { cols, rows, .. } = spec.kind {
            if r.count > cols * rows {
                return Err(bad(format!(
                    "the feeder {} is a tray of {} cells and the refill says {} parts",
                    r.id,
                    cols * rows,
                    r.count
                )));
            }
        }
        let is_strip = matches!(spec.kind, FeederKind::Strip { .. });
        if r.start.is_some() && !is_strip {
            return Err(bad(format!(
                "the feeder {} is not a strip: only a strip has a first place that moves",
                r.id
            )));
        }
        let row = &mut records[i + 1];
        row[c_count] = r.count.to_string();
        let moved = match (r.start, c_x, c_y) {
            (Some((x, y)), Some(cx), Some(cy)) => {
                row[cx] = place(x);
                row[cy] = place(y);
                Some(((spec.base.x, spec.base.y), (x, y)))
            }
            _ => None,
        };
        lines.push(Line {
            id: r.id.clone(),
            before: spec.base.count,
            after: r.count,
            moved,
            kept_place: is_strip && r.start.is_none(),
        });
    }
    let sep = delim.to_string();
    let mut csv = String::new();
    for r in &records {
        let cells: Vec<String> = r.iter().map(|c| csv_cell(c, delim)).collect();
        csv.push_str(&cells.join(&sep));
        csv.push('\n');
    }
    Ok(Refilled { lines, csv })
}

impl Refilled {
    /// The parts that the refills put in, all feeders together (a refill to fewer parts than the feeder held counts as 0).
    pub fn added(&self) -> u64 {
        self.lines
            .iter()
            .map(|l| u64::from(l.after.saturating_sub(l.before)))
            .sum()
    }

    /// One line for each feeder that was refilled, then a line that sums it up.
    pub fn text(&self) -> String {
        let mut out = String::new();
        for l in &self.lines {
            out.push_str(&format!("{}: {} -> {} (refilled)", l.id, l.before, l.after));
            if let Some((from, to)) = l.moved {
                out.push_str(&format!(
                    ", first place {:.3},{:.3} -> {:.3},{:.3}",
                    from.0, from.1, to.0, to.1
                ));
            } else if l.kept_place {
                out.push_str(", the first place of the strip was kept");
            }
            out.push('\n');
        }
        out.push_str(&format!(
            "{} feeders refilled, {} parts more\n",
            self.lines.len(),
            self.added()
        ));
        out
    }
}
