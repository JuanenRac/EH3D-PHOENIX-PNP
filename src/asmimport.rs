// EH3D-PHOENIX-PNP - the position files that assemblers and other programs write, read into a table of parts.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! `export-assembler` writes the parts in the shapes that assemblers ask for; the files that come back, or that other programs write, are of the same few shapes with small differences: a
//! designator column that is called `Designator`, `Reference` or `RefDes`; the middle of the part as `Mid X`, `Center-X(mm)` or `PosX`; the layer as `Top`, `T`, `TopLayer` or `Bottom`; and
//! numbers that carry their unit (`12.500mm`, `0.492in`, `490mil`) or whose unit is in the name of the column (`x(mm)`, `Center-X(mil)`). This reads them, says which shape it found, and gives the
//! parts in millimetres and degrees (0 up to 360), so that every command that reads a table of parts can take the result. A row it cannot read is an error with its line; nothing is guessed.

use crate::error::{Code, PnpError};
use crate::parts::{detect_delimiter, read_records_numbered, Part, Side, MAX_PARTS};
use std::collections::HashSet;

/// What was read.
#[derive(Debug, Clone, PartialEq)]
pub struct Imported {
    /// The shape of the file: `cpl` (a middle as `Mid X`), `altium` (a middle as `Center-X`), `generic` (the unit in the name of the columns) or `table` (anything else).
    pub dialect: &'static str,
    pub parts: Vec<Part>,
}

fn bad(line: usize, detail: impl Into<String>) -> PnpError {
    PnpError::new(Code::ImportBad, format!("line {line}: {}", detail.into()))
}

/// A header cell without spaces, dashes or underscores, in lower case, and the unit that it names in parentheses: `Center-X(mil)` is `centerx` and `mil`.
fn header_key(cell: &str) -> (String, Option<f64>) {
    let lower = cell.trim().to_ascii_lowercase();
    let (base, unit) = match lower.split_once('(') {
        Some((b, rest)) => (b.to_string(), unit_mm(rest.trim_end_matches(')').trim())),
        None => (lower, None),
    };
    let base: String = base
        .chars()
        .filter(|c| !matches!(c, ' ' | '-' | '_' | '.' | '#'))
        .collect();
    (base, unit)
}

/// Millimetres in one of a unit named by a word (`mm`, `mil`, `mils`, `in`, `inch`, `inches`).
fn unit_mm(word: &str) -> Option<f64> {
    match word.trim().to_ascii_lowercase().as_str() {
        "mm" => Some(1.0),
        "mil" | "mils" | "thou" => Some(0.0254),
        "in" | "inch" | "inches" => Some(25.4),
        _ => None,
    }
}

/// A number that may carry its unit at its end: `12.500mm`, `0.492in`, `490 mil`. The unit is the one of the column when the number has none, else millimetres.
fn quantity(text: &str, column_unit: Option<f64>) -> Option<f64> {
    let t = text.trim();
    let cut = t
        .char_indices()
        .find(|(_, c)| c.is_ascii_alphabetic() && *c != 'e' && *c != 'E')
        .map_or(t.len(), |(i, _)| i);
    let (number, word) = t.split_at(cut);
    let value: f64 = number.trim().parse().ok().filter(|v: &f64| v.is_finite())?;
    let unit = if word.trim().is_empty() {
        column_unit.unwrap_or(1.0)
    } else {
        unit_mm(word)?
    };
    Some(value * unit)
}

fn side_of(text: &str) -> Option<Side> {
    match text.trim().to_ascii_lowercase().as_str() {
        "top" | "t" | "toplayer" | "top layer" | "front" | "f" | "f.cu" | "1" => Some(Side::Top),
        "bottom" | "b" | "bot" | "bottomlayer" | "bottom layer" | "back" | "b.cu" | "2" => {
            Some(Side::Bottom)
        }
        _ => None,
    }
}

/// Reads a position file of an assembler or of another program. The header is the first row; the columns are found by name. A part must have a designator, a place and a layer; the value, the
/// package and the turn are 0 or empty when the file has no column for them.
pub fn parse(text: &str) -> Result<Imported, PnpError> {
    let text = text.trim_start_matches('\u{feff}');
    let head = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let records =
        read_records_numbered(text, detect_delimiter(head)).map_err(|(line, e)| bad(line, e))?;
    let mut rows = records.into_iter();
    let (head_line, header) = rows.next().ok_or_else(|| bad(1, "the file is empty"))?;
    let keys: Vec<(String, Option<f64>)> = header.iter().map(|h| header_key(h)).collect();
    let find = |names: &[&str]| -> Option<usize> {
        names
            .iter()
            .find_map(|n| keys.iter().position(|k| k.0 == *n))
    };
    let reference_at = find(&["designator", "reference", "ref", "refdes", "part"])
        .ok_or_else(|| bad(head_line, "the header has no column of designators"))?;
    let x_at = find(&["midx", "centerx", "posx", "x", "refx"])
        .ok_or_else(|| bad(head_line, "the header has no column of the place along X"))?;
    let y_at = find(&["midy", "centery", "posy", "y", "refy"])
        .ok_or_else(|| bad(head_line, "the header has no column of the place along Y"))?;
    let side_at = find(&["layer", "side", "tb", "topbottom"])
        .ok_or_else(|| bad(head_line, "the header has no column of the layer"))?;
    let rotation_at = find(&["rotation", "rot", "angle"]);
    let value_at = find(&["val", "value", "comment"]);
    let package_at = find(&["package", "footprint", "pattern"]);
    let dialect = if keys[x_at].0 == "midx" {
        "cpl"
    } else if keys[x_at].0 == "centerx" || keys[x_at].0 == "refx" {
        "altium"
    } else if keys[x_at].1.is_some() {
        "generic"
    } else {
        "table"
    };
    let cell = |row: &[String], at: Option<usize>| -> String {
        at.and_then(|i| row.get(i))
            .map_or(String::new(), |s| s.trim().to_string())
    };
    let mut parts = Vec::new();
    let mut seen = HashSet::new();
    for (line, row) in rows {
        if row.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        if parts.len() >= MAX_PARTS {
            return Err(bad(line, format!("more than {MAX_PARTS} parts")));
        }
        let reference = cell(&row, Some(reference_at));
        if reference.is_empty() {
            return Err(bad(line, "the designator is empty"));
        }
        if !seen.insert(reference.clone()) {
            return Err(bad(
                line,
                format!("the designator {reference} is in the file twice"),
            ));
        }
        let place = |at: usize, what: &str| -> Result<f64, PnpError> {
            quantity(&cell(&row, Some(at)), keys[at].1).ok_or_else(|| {
                bad(
                    line,
                    format!(
                        "the place along {what} is a number with an optional unit (mm, mil or in)"
                    ),
                )
            })
        };
        let x = place(x_at, "X")?;
        let y = place(y_at, "Y")?;
        let rotation = match rotation_at {
            Some(at) if !cell(&row, Some(at)).is_empty() => cell(&row, Some(at))
                .trim_end_matches(|c: char| c.is_ascii_alphabetic() || c == '°')
                .trim()
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite())
                .ok_or_else(|| bad(line, "the rotation is a number of degrees"))?,
            _ => 0.0,
        };
        let side = side_of(&cell(&row, Some(side_at)))
            .ok_or_else(|| bad(line, "the layer is top or bottom"))?;
        parts.push(Part {
            reference,
            value: cell(&row, value_at),
            package: cell(&row, package_at),
            x,
            y,
            rotation: rotation.rem_euclid(360.0),
            side,
        });
    }
    if parts.is_empty() {
        return Err(bad(head_line, "the file has no parts"));
    }
    Ok(Imported { dialect, parts })
}
