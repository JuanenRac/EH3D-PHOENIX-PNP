// EH3D-PHOENIX-PNP - the composition of a panel of boards.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! A panel is a sheet with rails round it and boards on it, each board put at a place and turned. The file of the composition says the size of the sheet, the width of its four rails, the
//! outline of each board (width and height, in the file of the board's own coordinates) and the units: which board, where its origin goes and by how many degrees it is turned about that
//! origin, counter-clockwise, as the panel file does (`panel.rs`). `compose` checks that every unit lies inside the sheet once the rails are taken off, that no two units overlap and that
//! none is closer to another than `panel_clearance_mm`, and writes the panel file that the other commands read, with the units sorted by name and the numbers written one way, so the same
//! composition gives the same bytes whatever the order of the units in the file. The units may be of different boards (a different design in each). It reads the outlines as rectangles: the
//! shape of a board that is not one is the person's to say by its bounding box.
//!
//! ```json
//! { "schema_version": 1,
//!   "panel": { "width": 100, "height": 80, "rails": { "left": 5, "right": 5, "top": 5, "bottom": 5 } },
//!   "boards": { "main": { "file": "board.pos", "width": 40, "height": 30 } },
//!   "units": [ { "id": "A", "board": "main", "x": 5, "y": 5, "rotation": 0 } ],
//!   "fiducials": [ { "id": "f1", "x": 2, "y": 2 } ] }
//! ```

use crate::error::{Code, PnpError};
use crate::units::rotate_xy;
use serde_json::Value;
use std::collections::BTreeMap;

/// The most units on a panel and the most fiducials.
pub const MAX_UNITS: usize = 1000;
const MAX_FIDUCIALS: usize = 100;
/// The largest sheet and board, in millimetres.
pub const MAX_SIZE_MM: f64 = 2000.0;

fn bad(message: String) -> PnpError {
    PnpError::new(Code::PanelBad, message)
}

/// A board's outline.
#[derive(Debug, Clone, PartialEq)]
pub struct BoardOutline {
    pub file: String,
    pub width: f64,
    pub height: f64,
}

/// A unit of the panel.
#[derive(Debug, Clone, PartialEq)]
pub struct Unit {
    pub id: String,
    pub board: String,
    pub x: f64,
    pub y: f64,
    /// Degrees, counter-clockwise, from 0 up to but not including 360.
    pub rotation: f64,
}

/// The composition of a panel.
#[derive(Debug, Clone, PartialEq)]
pub struct Composition {
    pub width: f64,
    pub height: f64,
    /// Left, right, top and bottom rails.
    pub rails: [f64; 4],
    pub boards: BTreeMap<String, BoardOutline>,
    pub units: Vec<Unit>,
    pub fiducials: Vec<(String, f64, f64)>,
}

/// What is wrong with a composition.
#[derive(Debug, Clone, PartialEq)]
pub enum Finding {
    /// The unit is not wholly inside the sheet without its rails.
    Outside { unit: String, over_mm: f64 },
    /// The two units overlap.
    Overlap { a: String, b: String },
    /// The two units do not overlap but are closer than the least distance.
    TooClose { a: String, b: String, mm: f64 },
    /// A fiducial is not on the sheet.
    FiducialOff { id: String },
}

impl Finding {
    pub fn text(&self, clearance: f64) -> String {
        match self {
            Finding::Outside { unit, over_mm } => {
                format!("{unit} goes {over_mm:.3} mm out of the area between the rails")
            }
            Finding::Overlap { a, b } => format!("{a} and {b} overlap"),
            Finding::TooClose { a, b, mm } => {
                format!("{a} and {b} are {mm:.3} mm apart; the least is {clearance} mm")
            }
            Finding::FiducialOff { id } => format!("the fiducial {id} is not on the sheet"),
        }
    }
}

fn name_ok(name: &str) -> bool {
    !name.is_empty()
        && name.chars().count() <= 64
        && !name.contains('/')
        && !name.chars().any(char::is_control)
}

fn size(v: &Value, key: &str, what: &str) -> Result<f64, PnpError> {
    v.get(key)
        .and_then(Value::as_f64)
        .filter(|x| x.is_finite() && *x > 0.0 && *x <= MAX_SIZE_MM)
        .ok_or_else(|| {
            bad(format!(
                "{what}: '{key}' is not a size above 0 up to {MAX_SIZE_MM}"
            ))
        })
}

fn coord(v: &Value, key: &str, what: &str) -> Result<f64, PnpError> {
    match v.get(key) {
        None => Ok(0.0),
        Some(n) => n
            .as_f64()
            .filter(|x| x.is_finite() && x.abs() <= MAX_SIZE_MM)
            .ok_or_else(|| {
                bad(format!(
                    "{what}: '{key}' is not a number up to {MAX_SIZE_MM}"
                ))
            }),
    }
}

/// An angle in degrees brought into 0 up to but not including 360. Anything from -360 to 360 is read.
fn turn(v: &Value, what: &str) -> Result<f64, PnpError> {
    let r = coord(v, "rotation", what)?;
    if !(-360.0..=360.0).contains(&r) {
        return Err(bad(format!(
            "{what}: the rotation is not from -360 to 360 degrees"
        )));
    }
    let t = r.rem_euclid(360.0);
    // a turn that is whole in a thousandth of a degree is kept exactly, so the output has no trace of the arithmetic
    Ok((t * 1000.0).round() / 1000.0 % 360.0)
}

impl Composition {
    /// Reads the file of a composition.
    pub fn parse(text: &str) -> Result<Composition, PnpError> {
        let v: Value = serde_json::from_str(text.trim_start_matches('\u{feff}'))
            .map_err(|e| bad(format!("the composition is not JSON: {e}")))?;
        if v.get("schema_version").and_then(Value::as_u64) != Some(1) {
            return Err(bad("schema_version must be 1".into()));
        }
        let p = v
            .get("panel")
            .ok_or_else(|| bad("there is no 'panel'".into()))?;
        let width = size(p, "width", "panel")?;
        let height = size(p, "height", "panel")?;
        let mut rails = [0.0; 4];
        if let Some(r) = p.get("rails") {
            for (i, k) in ["left", "right", "top", "bottom"].iter().enumerate() {
                rails[i] = match r.get(*k) {
                    None => 0.0,
                    Some(n) => n
                        .as_f64()
                        .filter(|x| x.is_finite() && (0.0..=MAX_SIZE_MM).contains(x))
                        .ok_or_else(|| bad(format!("the rail '{k}' is not from 0 up")))?,
                };
            }
        }
        if rails[0] + rails[1] >= width || rails[2] + rails[3] >= height {
            return Err(bad("the rails leave no room on the sheet".into()));
        }
        let mut boards = BTreeMap::new();
        let list = v
            .get("boards")
            .and_then(Value::as_object)
            .ok_or_else(|| bad("there is no 'boards'".into()))?;
        for (name, b) in list {
            if !name_ok(name) {
                return Err(bad(format!("'{name}' is not a name of a board")));
            }
            let file = b
                .get("file")
                .and_then(Value::as_str)
                .filter(|f| !f.is_empty() && !f.chars().any(char::is_control))
                .ok_or_else(|| bad(format!("board {name}: there is no 'file'")))?;
            boards.insert(
                name.clone(),
                BoardOutline {
                    file: file.to_string(),
                    width: size(b, "width", &format!("board {name}"))?,
                    height: size(b, "height", &format!("board {name}"))?,
                },
            );
        }
        let mut units: Vec<Unit> = Vec::new();
        let rows = v
            .get("units")
            .and_then(Value::as_array)
            .ok_or_else(|| bad("there is no 'units'".into()))?;
        if rows.len() > MAX_UNITS {
            return Err(bad(format!("more than {MAX_UNITS} units")));
        }
        for u in rows {
            let id = u.get("id").and_then(Value::as_str).unwrap_or("");
            if !name_ok(id) || units.iter().any(|o| o.id == id) {
                return Err(bad(format!(
                    "the unit '{id}' has no name, an unfit name or a name used twice"
                )));
            }
            let what = format!("unit {id}");
            let board = u.get("board").and_then(Value::as_str).unwrap_or("");
            if !boards.contains_key(board) {
                return Err(bad(format!(
                    "{what}: the board '{board}' is not in 'boards'"
                )));
            }
            units.push(Unit {
                id: id.to_string(),
                board: board.to_string(),
                x: coord(u, "x", &what)?,
                y: coord(u, "y", &what)?,
                rotation: turn(u, &what)?,
            });
        }
        let mut fiducials: Vec<(String, f64, f64)> = Vec::new();
        if let Some(list) = v.get("fiducials").and_then(Value::as_array) {
            if list.len() > MAX_FIDUCIALS {
                return Err(bad(format!("more than {MAX_FIDUCIALS} fiducials")));
            }
            for f in list {
                let id = f.get("id").and_then(Value::as_str).unwrap_or("");
                if !name_ok(id) || fiducials.iter().any(|o| o.0 == id) {
                    return Err(bad(format!(
                        "the fiducial '{id}' has no name, an unfit name or a name used twice"
                    )));
                }
                fiducials.push((
                    id.to_string(),
                    coord(f, "x", "fiducial")?,
                    coord(f, "y", "fiducial")?,
                ));
            }
        }
        Ok(Composition {
            width,
            height,
            rails,
            boards,
            units,
            fiducials,
        })
    }

    /// The four corners of a unit on the sheet, counter-clockwise from its origin.
    pub fn corners(&self, unit: &Unit) -> [(f64, f64); 4] {
        let b = &self.boards[&unit.board];
        [
            (0.0, 0.0),
            (b.width, 0.0),
            (b.width, b.height),
            (0.0, b.height),
        ]
        .map(|(cx, cy)| {
            let (rx, ry) = rotate_xy(cx, cy, unit.rotation);
            (unit.x + rx, unit.y + ry)
        })
    }

    /// The area that the boards may use: left, bottom, right and top of the sheet without its rails.
    pub fn usable(&self) -> (f64, f64, f64, f64) {
        (
            self.rails[0],
            self.rails[3],
            self.width - self.rails[1],
            self.height - self.rails[2],
        )
    }

    /// Everything that is wrong, with the least distance `clearance` (millimetres) between units.
    pub fn check(&self, clearance: f64) -> Vec<Finding> {
        // millionths of a millimetre are not told apart: a unit that touches the rail exactly is inside
        const EPS: f64 = 1e-6;
        let mut out = Vec::new();
        let (l, b, r, t) = self.usable();
        let shapes: Vec<[(f64, f64); 4]> = self.units.iter().map(|u| self.corners(u)).collect();
        for (u, c) in self.units.iter().zip(&shapes) {
            let over = c
                .iter()
                .map(|&(x, y)| (l - x).max(x - r).max(b - y).max(y - t))
                .fold(0.0_f64, f64::max);
            if over > EPS {
                out.push(Finding::Outside {
                    unit: u.id.clone(),
                    over_mm: over,
                });
            }
        }
        for (i, a) in self.units.iter().enumerate() {
            for (j, bb) in self.units.iter().enumerate().skip(i + 1) {
                if polygons_overlap(&shapes[i], &shapes[j]) {
                    out.push(Finding::Overlap {
                        a: a.id.clone(),
                        b: bb.id.clone(),
                    });
                } else {
                    let d = polygon_distance(&shapes[i], &shapes[j]);
                    if d < clearance - EPS {
                        out.push(Finding::TooClose {
                            a: a.id.clone(),
                            b: bb.id.clone(),
                            mm: d,
                        });
                    }
                }
            }
        }
        for (id, x, y) in &self.fiducials {
            if *x < -EPS || *y < -EPS || *x > self.width + EPS || *y > self.height + EPS {
                out.push(Finding::FiducialOff { id: id.clone() });
            }
        }
        out
    }

    /// How much of the area between the rails the boards cover, as a fraction from 0 to 1 (units that overlap are counted twice, so this is meant for a composition that passes).
    pub fn fill(&self) -> f64 {
        let (l, b, r, t) = self.usable();
        let area = (r - l) * (t - b);
        let used: f64 = self
            .units
            .iter()
            .map(|u| {
                let o = &self.boards[&u.board];
                o.width * o.height
            })
            .sum();
        if area > 0.0 {
            used / area
        } else {
            0.0
        }
    }

    /// The panel file, in the form that `Panel::parse` reads, with the units sorted by name and the numbers written one way.
    pub fn to_panel_json(&self) -> String {
        let mut units: Vec<&Unit> = self.units.iter().collect();
        units.sort_by(|a, b| a.id.cmp(&b.id));
        let mut fids: Vec<&(String, f64, f64)> = self.fiducials.iter().collect();
        fids.sort_by(|a, b| a.0.cmp(&b.0));
        // only the boards that a unit uses
        let used: Vec<(&String, &BoardOutline)> = self
            .boards
            .iter()
            .filter(|(n, _)| self.units.iter().any(|u| &u.board == *n))
            .collect();
        let mut s = String::from("{\n  \"schema_version\": 1,\n  \"boards\": {");
        let boards: Vec<String> = used
            .iter()
            .map(|(n, b)| format!("\n    {}: {}", quote(n), quote(&b.file)))
            .collect();
        s.push_str(&boards.join(","));
        s.push_str("\n  },\n  \"panels\": {\n    \"panel\": {\n      \"children\": [");
        let kids: Vec<String> = units
            .iter()
            .map(|u| {
                format!(
                    "\n        {{ \"id\": {}, \"board\": {}, \"x\": {}, \"y\": {}, \"rotation\": {} }}",
                    quote(&u.id),
                    quote(&u.board),
                    num(u.x),
                    num(u.y),
                    num(u.rotation)
                )
            })
            .collect();
        s.push_str(&kids.join(","));
        s.push_str("\n      ]");
        if !fids.is_empty() {
            s.push_str(",\n      \"fiducials\": [");
            let f: Vec<String> = fids
                .iter()
                .map(|(id, x, y)| {
                    format!(
                        "\n        {{ \"id\": {}, \"x\": {}, \"y\": {} }}",
                        quote(id),
                        num(*x),
                        num(*y)
                    )
                })
                .collect();
            s.push_str(&f.join(","));
            s.push_str("\n      ]");
        }
        s.push_str("\n    }\n  },\n  \"root\": \"panel\"\n}\n");
        s
    }

    /// The text for a person: the sheet, the area between the rails, each unit with its box, how full the sheet is and what is wrong.
    pub fn text(&self, clearance: f64) -> String {
        let (l, b, r, t) = self.usable();
        let mut out = format!(
            "sheet {} x {} mm, rails left {} right {} top {} bottom {}; boards may use x {l} to {r}, y {b} to {t}\n",
            num(self.width),
            num(self.height),
            num(self.rails[0]),
            num(self.rails[1]),
            num(self.rails[2]),
            num(self.rails[3])
        );
        let mut units: Vec<&Unit> = self.units.iter().collect();
        units.sort_by(|a, b| a.id.cmp(&b.id));
        for u in units {
            let c = self.corners(u);
            let xs = c.map(|p| p.0);
            let ys = c.map(|p| p.1);
            let min = |v: [f64; 4]| v.into_iter().fold(f64::INFINITY, f64::min);
            let max = |v: [f64; 4]| v.into_iter().fold(f64::NEG_INFINITY, f64::max);
            out.push_str(&format!(
                "{}: board {} at {:.3},{:.3} turned {} degrees, box x {:.3} to {:.3}, y {:.3} to {:.3}\n",
                u.id,
                u.board,
                u.x,
                u.y,
                num(u.rotation),
                min(xs),
                max(xs),
                min(ys),
                max(ys)
            ));
        }
        out.push_str(&format!(
            "{} units cover {:.1} per cent of the area between the rails\n",
            self.units.len(),
            self.fill() * 100.0
        ));
        let findings = self.check(clearance);
        for f in &findings {
            out.push_str(&format!("PROBLEM: {}\n", f.text(clearance)));
        }
        if findings.is_empty() {
            out.push_str("the composition is sound\n");
        }
        out
    }
}

fn quote(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| "\"\"".to_string())
}

/// A number written one way: at most four decimals, no trailing zeros, no `-0`.
pub fn num(v: f64) -> String {
    let r = (v * 10_000.0).round() / 10_000.0;
    if r == 0.0 {
        return "0".to_string();
    }
    let s = format!("{r:.4}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn sub(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    (a.0 - b.0, a.1 - b.1)
}

fn dot(a: (f64, f64), b: (f64, f64)) -> f64 {
    a.0 * b.0 + a.1 * b.1
}

/// Whether the interiors of two convex polygons meet, by the separating axis: touching along an edge is not meeting.
fn polygons_overlap(a: &[(f64, f64); 4], b: &[(f64, f64); 4]) -> bool {
    const EPS: f64 = 1e-9;
    for poly in [a, b] {
        for i in 0..4 {
            let e = sub(poly[(i + 1) % 4], poly[i]);
            let axis = (-e.1, e.0);
            let project = |p: &[(f64, f64); 4]| {
                p.iter()
                    .map(|&q| dot(q, axis))
                    .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
                        (lo.min(v), hi.max(v))
                    })
            };
            let (a_lo, a_hi) = project(a);
            let (b_lo, b_hi) = project(b);
            let scale = (axis.0.hypot(axis.1)).max(1.0);
            if a_hi <= b_lo + EPS * scale || b_hi <= a_lo + EPS * scale {
                return false;
            }
        }
    }
    true
}

fn point_segment(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let ab = sub(b, a);
    let len2 = dot(ab, ab);
    let t = if len2 > 0.0 {
        (dot(sub(p, a), ab) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let q = (a.0 + ab.0 * t, a.1 + ab.1 * t);
    (p.0 - q.0).hypot(p.1 - q.1)
}

/// The least distance between the outlines of two convex polygons that do not overlap.
fn polygon_distance(a: &[(f64, f64); 4], b: &[(f64, f64); 4]) -> f64 {
    let mut best = f64::INFINITY;
    for (p, q) in [(a, b), (b, a)] {
        for &v in p {
            for i in 0..4 {
                best = best.min(point_segment(v, q[i], q[(i + 1) % 4]));
            }
        }
    }
    best
}
