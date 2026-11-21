// EH3D-PHOENIX-PNP - the height of a bent board, from a grid of measured points.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! A board is never flat, and a part placed on a bent one is let go too high or pushed in too far. A person (or a sensor on the head) measures the height of the board on a regular grid of
//! points, `x_mm,y_mm,z_mm`; this reads such a table, finds the height at any place inside the grid by bilinear interpolation, and gives the change of the height of a placement: the height
//! at the place less the height at the point where the machine was set to zero (the lowest corner of the grid, or a place that is given). A place outside the measured area is told, and
//! never given a figure by guessing. The grid has to be whole (every column of X with every row of Y, once) and bounded. It reads text and calculates; it moves nothing.

use crate::parts::{detect_delimiter, read_records_numbered, Part};
use std::collections::BTreeMap;

/// The most points of a grid.
pub const MAX_POINTS: usize = 10_000;
/// The most bytes of a table of points.
pub const MAX_BYTES: u64 = 1_048_576;

/// A grid of measured heights: the columns of X and the rows of Y in millimetres, from the least to the greatest, and the height at each crossing.
#[derive(Debug, Clone, PartialEq)]
pub struct Grid {
    pub xs: Vec<f64>,
    pub ys: Vec<f64>,
    /// `z[iy][ix]` in millimetres.
    pub z: Vec<Vec<f64>>,
}

/// A thousandth of a millimetre as a whole number: two coordinates that agree to that are the same line of the grid.
fn micro(v: f64) -> i64 {
    (v * 1000.0).round() as i64
}

/// Reads the table `x_mm,y_mm,z_mm`. It has to be a whole grid of at least two columns and two rows.
pub fn parse(text: &str) -> Result<Grid, String> {
    let text = text.trim_start_matches('\u{feff}');
    let head = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let records = read_records_numbered(text, detect_delimiter(head))
        .map_err(|(line, e)| format!("line {line}: {e}"))?;
    let mut rows = records.into_iter();
    let (head_line, header) = rows.next().ok_or("the table of heights is empty")?;
    let header: Vec<String> = header
        .iter()
        .map(|h| h.trim().to_ascii_lowercase())
        .collect();
    if header != ["x_mm", "y_mm", "z_mm"] {
        return Err(format!(
            "line {head_line}: the first row must be exactly: x_mm,y_mm,z_mm"
        ));
    }
    let mut points: BTreeMap<(i64, i64), f64> = BTreeMap::new();
    for (line, row) in rows {
        if row.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        if row.len() != 3 {
            return Err(format!("line {line}: {} fields, not 3", row.len()));
        }
        if points.len() >= MAX_POINTS {
            return Err(format!("more than {MAX_POINTS} points"));
        }
        let value = |cell: &str, what: &str, limit: f64| -> Result<f64, String> {
            cell.trim()
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite() && v.abs() <= limit)
                .ok_or_else(|| {
                    format!("line {line}: {what} '{cell}' is not a number up to {limit}")
                })
        };
        let (x, y, z) = (
            value(&row[0], "x_mm", 10_000.0)?,
            value(&row[1], "y_mm", 10_000.0)?,
            value(&row[2], "z_mm", 1000.0)?,
        );
        if points.insert((micro(x), micro(y)), z).is_some() {
            return Err(format!("line {line}: the point {x},{y} is given twice"));
        }
    }
    let mut xs: Vec<i64> = points.keys().map(|k| k.0).collect();
    let mut ys: Vec<i64> = points.keys().map(|k| k.1).collect();
    xs.sort_unstable();
    xs.dedup();
    ys.sort_unstable();
    ys.dedup();
    if xs.len() < 2 || ys.len() < 2 {
        return Err("a grid has at least two columns of X and two rows of Y".into());
    }
    if xs.len() * ys.len() != points.len() {
        return Err(format!(
            "the points are not a whole grid: {} columns and {} rows need {} points and the table has {}",
            xs.len(),
            ys.len(),
            xs.len() * ys.len(),
            points.len()
        ));
    }
    let z = ys
        .iter()
        .map(|&y| xs.iter().map(|&x| points[&(x, y)]).collect())
        .collect();
    Ok(Grid {
        xs: xs.iter().map(|&v| v as f64 / 1000.0).collect(),
        ys: ys.iter().map(|&v| v as f64 / 1000.0).collect(),
        z,
    })
}

/// The index of the cell that holds `v` in a sorted list, and how far across it `v` is (0 to 1); `None` outside the list.
fn cell(list: &[f64], v: f64) -> Option<(usize, f64)> {
    let (first, last) = (*list.first()?, *list.last()?);
    if !v.is_finite() || v < first - 1e-9 || v > last + 1e-9 {
        return None;
    }
    let i = list
        .windows(2)
        .position(|w| v <= w[1] + 1e-9)
        .unwrap_or(list.len() - 2);
    let t = ((v - list[i]) / (list[i + 1] - list[i])).clamp(0.0, 1.0);
    Some((i, t))
}

impl Grid {
    /// The height at a place by bilinear interpolation, and `None` outside the measured area.
    pub fn z_at(&self, x: f64, y: f64) -> Option<f64> {
        let (ix, tx) = cell(&self.xs, x)?;
        let (iy, ty) = cell(&self.ys, y)?;
        let (a, b) = (self.z[iy][ix], self.z[iy][ix + 1]);
        let (c, d) = (self.z[iy + 1][ix], self.z[iy + 1][ix + 1]);
        let low = a + (b - a) * tx;
        let high = c + (d - c) * tx;
        Some(low + (high - low) * ty)
    }

    /// The highest measured height less the lowest, in millimetres.
    pub fn warp(&self) -> f64 {
        let all = self.z.iter().flatten();
        let high = all.clone().fold(f64::MIN, |m, v| m.max(*v));
        let low = all.fold(f64::MAX, |m, v| m.min(*v));
        high - low
    }

    /// How many points the grid has.
    pub fn points(&self) -> usize {
        self.xs.len() * self.ys.len()
    }

    /// The place where the machine was set to zero when no other is given: the corner of the lowest X and the lowest Y.
    pub fn default_zero(&self) -> (f64, f64) {
        (self.xs[0], self.ys[0])
    }
}

/// The change of height of one place.
#[derive(Debug, Clone, PartialEq)]
pub struct Correction {
    pub reference: String,
    pub x: f64,
    pub y: f64,
    /// How much the height of the placement changes (mm): the board is this much higher than at the zero (negative: lower).
    pub dz_mm: Option<f64>,
}

/// The correction of each part: its height less the height at `zero`. A part outside the grid has none, and that is told.
pub fn corrections(
    grid: &Grid,
    parts: &[Part],
    zero: (f64, f64),
) -> Result<Vec<Correction>, String> {
    let base = grid.z_at(zero.0, zero.1).ok_or_else(|| {
        format!(
            "the zero {},{} is outside the measured grid",
            zero.0, zero.1
        )
    })?;
    Ok(parts
        .iter()
        .map(|p| Correction {
            reference: p.reference.clone(),
            x: p.x,
            y: p.y,
            dz_mm: grid.z_at(p.x, p.y).map(|z| z - base),
        })
        .collect())
}

/// The table of corrections: `reference,x_mm,y_mm,dz_mm`, with the correction empty for a part outside the grid, to three decimals.
pub fn csv(list: &[Correction]) -> String {
    let mut out = String::from("reference,x_mm,y_mm,dz_mm\n");
    for c in list {
        out.push_str(&format!(
            "{},{:.3},{:.3},{}\n",
            crate::parts::csv_field(&c.reference),
            c.x,
            c.y,
            // a change too small to show is 0, not -0
            c.dz_mm.map_or(String::new(), |v| format!(
                "{:.3}",
                if v.abs() < 0.0005 { 0.0 } else { v }
            ))
        ));
    }
    out
}
