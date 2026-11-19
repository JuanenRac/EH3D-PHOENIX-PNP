// EH3D-PHOENIX-PNP - a table of parts turned, flipped or moved as a whole.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! The table of parts of a board is written for the board as it was drawn. A board that lies on the machine turned a quarter, a board that is placed from its other side, a panel whose
//! origin is not where the CAD put it: each needs the whole table moved the same way. This turns the places counter-clockwise about the origin, mirrors them left to right (a board seen from
//! its other side: the sides swap and the angles mirror), moves them, and can move them so that the lowest left corner of the parts is the origin. The order is always the same: flip, turn,
//! move, zero. The distances between the parts do not change, and a turn and its opposite, or two flips, give back the table that was given (to the digits that are written).

use crate::parts::{Part, Side};

/// The largest place that the result may have, in millimetres.
const MAX_MM: f64 = 1_000_000.0;

/// What is done to the table.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Transform {
    /// Degrees, counter-clockwise about the origin.
    pub rotate_deg: f64,
    /// Mirror left to right (x becomes -x) and swap the sides.
    pub flip: bool,
    pub shift: (f64, f64),
    /// Move the lowest left corner of the parts to the origin, after the rest.
    pub zero: bool,
}

/// A turn as a number from 0 up to, not including, 360.
fn norm(deg: f64) -> f64 {
    let r = deg.rem_euclid(360.0);
    if r >= 360.0 - 1e-9 {
        0.0
    } else {
        r
    }
}

/// The table as it is after the transform.
pub fn apply(parts: &[Part], t: &Transform) -> Result<Vec<Part>, String> {
    if !t.rotate_deg.is_finite() || !t.shift.0.is_finite() || !t.shift.1.is_finite() {
        return Err("a turn and a shift are numbers".into());
    }
    let (sin, cos) = t.rotate_deg.to_radians().sin_cos();
    let mut out: Vec<Part> = parts
        .iter()
        .map(|p| {
            let (mut x, y, mut rot, mut side) = (p.x, p.y, p.rotation, p.side);
            if t.flip {
                x = -x;
                rot = 180.0 - rot;
                side = match side {
                    Side::Top => Side::Bottom,
                    Side::Bottom => Side::Top,
                };
            }
            let (tx, ty) = (x * cos - y * sin, x * sin + y * cos);
            Part {
                reference: p.reference.clone(),
                value: p.value.clone(),
                package: p.package.clone(),
                x: tx + t.shift.0,
                y: ty + t.shift.1,
                rotation: norm(rot + t.rotate_deg),
                side,
            }
        })
        .collect();
    if t.zero && !out.is_empty() {
        let lowest_x = out.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
        let lowest_y = out.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
        for p in &mut out {
            p.x -= lowest_x;
            p.y -= lowest_y;
        }
    }
    if let Some(p) = out
        .iter()
        .find(|p| !(p.x.abs() <= MAX_MM && p.y.abs() <= MAX_MM))
    {
        return Err(format!(
            "{} would be at {:.1} {:.1} mm, beyond {MAX_MM} mm",
            p.reference, p.x, p.y
        ));
    }
    Ok(out)
}

/// A line that says what was done and where the parts are now: how many, and the box of their centres.
pub fn summary(parts: &[Part]) -> String {
    if parts.is_empty() {
        return "no parts".to_string();
    }
    let min = |f: fn(&Part) -> f64| parts.iter().map(f).fold(f64::INFINITY, f64::min);
    let max = |f: fn(&Part) -> f64| parts.iter().map(f).fold(f64::NEG_INFINITY, f64::max);
    format!(
        "{} parts, centres from X{:.3} Y{:.3} to X{:.3} Y{:.3}",
        parts.len(),
        min(|p| p.x),
        min(|p| p.y),
        max(|p| p.x),
        max(|p| p.y)
    )
}
