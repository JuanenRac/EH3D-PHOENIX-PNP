// EH3D-PHOENIX-PNP - the calibration of the camera from a pattern that stays still while the head moves.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! `camera-calibrate` moves the head over one mark and `camera-pattern` looks at a whole sheet from one place; this does both at once. A sheet with a grid of points of a known pitch is
//! fixed to the bed with its first point (column 0, row 0) at a place of the machine that was measured (`camera_pattern_origin_x_mm`, `camera_pattern_origin_y_mm`), the head is moved to
//! several places and the camera reads the points it sees. A line of the file is `head_x,head_y,col,row,px,py`: where the axis of the nozzle was in the machine, the place of the point in the
//! grid and where the camera saw it (pixels from the middle of the picture, the second axis pointing up).
//!
//! Because the point is where the sheet puts it, how far it was from the axis of the nozzle is known for every sighting, and the sightings of `camera-calibrate` follow: the size of a pixel,
//! the turn, the mirror and the place of the axis of the nozzle in the picture. Because a point is seen at different distances from the middle of the picture, the bending of the lens is
//! found as well. What the sheet does not fix is how much it is turned on the bed: that is found too (a sheet that is turned makes sightings that disagree with each other, and the turn
//! that makes them agree is the turn of the sheet). The head must stand at two or more places. The place of the axis of the nozzle is only as good as the measure of the origin of the
//! sheet. Nothing here moves a machine; every error carries the code `PHOENIX-PNP-058`.

use crate::calibrate::{calibrate_camera, CameraCalibration, CameraPoint, MAX_POINTS};
use crate::camera::unbend;
use crate::config::MachineConfig;
use crate::error::{Code, PnpError};
use crate::parts::{detect_delimiter, read_records_numbered};
use crate::units::rotate_xy;

/// The fewest sightings: the camera has six numbers to find and the turn of the sheet and the bending one each.
pub const MIN_POINTS: usize = 8;
/// How far the sheet may be turned on the bed, in degrees either way.
pub const MAX_TURN_DEG: f64 = 10.0;
/// How far apart the first and the farthest place of the head must be, in millimetres.
const MIN_TRAVEL_MM: f64 = 0.5;
/// The bending of the lens that `camera_lens_k1` takes, either way.
const MAX_K1: f64 = 0.01;

fn bad(detail: impl Into<String>) -> PnpError {
    PnpError::new(Code::CalibrationBad, detail)
}

/// Whether the combined calibration may be used: it is off until `camera_combined` is turned on. The reason in words when it is off.
pub fn allowed(cfg: &MachineConfig) -> Result<(), String> {
    if cfg.camera_combined {
        Ok(())
    } else {
        Err("the combined calibration of the camera is off: set camera_combined = on in the configuration (camera-calibrate and camera-pattern do not need it)".to_string())
    }
}

/// One sighting of a point of the sheet: where the head was, which point it was and where the camera saw it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CombinedPoint {
    pub head_x: f64,
    pub head_y: f64,
    pub col: f64,
    pub row: f64,
    pub px: f64,
    pub py: f64,
}

/// Reads a file of sightings: the columns `head_x,head_y,col,row,px,py`, one row for each, with an optional header and comment lines (`#`).
pub fn parse_combined(text: &str) -> Result<Vec<CombinedPoint>, PnpError> {
    let text = text.trim_start_matches('\u{feff}');
    let kept: String = text
        .lines()
        .map(|l| {
            if l.trim_start().starts_with('#') {
                ""
            } else {
                l
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let head = kept.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let records = read_records_numbered(&kept, detect_delimiter(head))
        .map_err(|(line, e)| bad(format!("line {line}: {e}")))?;
    let mut out: Vec<CombinedPoint> = Vec::new();
    for (i, (line, record)) in records.into_iter().enumerate() {
        if i == 0 && record.first().is_some_and(|c| c.parse::<f64>().is_err()) {
            continue;
        }
        if record.len() != 6 {
            return Err(bad(format!(
                "line {line}: a sighting needs 6 numbers (head_x,head_y,col,row,px,py), this row has {}",
                record.len()
            )));
        }
        let mut v = [0.0; 6];
        for (slot, cell) in v.iter_mut().zip(&record) {
            let n: f64 = cell
                .parse()
                .map_err(|_| bad(format!("line {line}: '{cell}' is not a number")))?;
            if !n.is_finite() || n.abs() > 10_000.0 {
                return Err(bad(format!("line {line}: '{cell}' is not a usable number")));
            }
            *slot = n;
        }
        if out.len() >= MAX_POINTS {
            return Err(bad(format!(
                "line {line}: more than {MAX_POINTS} sightings"
            )));
        }
        if out
            .iter()
            .any(|p| p.head_x == v[0] && p.head_y == v[1] && p.col == v[2] && p.row == v[3])
        {
            return Err(bad(format!(
                "line {line}: the point {},{} is listed twice for the head at {},{}",
                v[2], v[3], v[0], v[1]
            )));
        }
        out.push(CombinedPoint {
            head_x: v[0],
            head_y: v[1],
            col: v[2],
            row: v[3],
            px: v[4],
            py: v[5],
        });
    }
    Ok(out)
}

/// What the combined calibration found.
#[derive(Debug, Clone, PartialEq)]
pub struct CombinedCalibration {
    /// The camera: the size of a pixel, the offset, the turn and the mirror, with the bending of the lens in `model.lens_k1`.
    pub camera: CameraCalibration,
    /// How much the sheet is turned on the bed, in degrees.
    pub sheet_turn_deg: f64,
    /// The bending of the lens (`camera_lens_k1`).
    pub lens_k1: f64,
    /// How far the sightings are from the model: root mean square and largest, in millimetres, and the same when the lens is taken for a perfect one.
    pub rms_mm: f64,
    pub max_mm: f64,
    pub rms_flat_mm: f64,
    pub points: usize,
    /// At how many different places the head stood.
    pub heads: usize,
}

/// The sightings of `camera-calibrate` that the combined ones give for a turn of the sheet and a bending of the lens: the place of the point from the axis of the nozzle, and the pixel with
/// the bending taken out (`units` is the size of a pixel).
fn sightings(
    rows: &[CombinedPoint],
    pitch: f64,
    origin: (f64, f64),
    turn_deg: f64,
    k1: f64,
    units: f64,
) -> Vec<CameraPoint> {
    rows.iter()
        .map(|r| {
            let g = rotate_xy(r.col * pitch, r.row * pitch, turn_deg);
            let (px, py) = if k1 == 0.0 || units <= 0.0 {
                (r.px, r.py)
            } else {
                let d = unbend((r.px * units, r.py * units), k1);
                (d.0 / units, d.1 / units)
            };
            CameraPoint {
                px,
                py,
                x_mm: origin.0 + g.0 - r.head_x,
                y_mm: origin.1 + g.1 - r.head_y,
            }
        })
        .collect()
}

/// The golden-section search for the minimum of a function that has one in `[lo, hi]`.
fn golden(f: &dyn Fn(f64) -> f64, mut lo: f64, mut hi: f64, rounds: usize) -> f64 {
    let g = (5f64.sqrt() - 1.0) / 2.0;
    let (mut a, mut b) = (hi - g * (hi - lo), lo + g * (hi - lo));
    let (mut fa, mut fb) = (f(a), f(b));
    for _ in 0..rounds {
        if fa <= fb {
            hi = b;
            b = a;
            fb = fa;
            a = hi - g * (hi - lo);
            fa = f(a);
        } else {
            lo = a;
            a = b;
            fa = fb;
            b = lo + g * (hi - lo);
            fb = f(b);
        }
    }
    if fa <= fb {
        a
    } else {
        b
    }
}

/// Fits the camera, the turn of the sheet and the bending of the lens to the sightings; `pitch_mm` is the distance between neighbouring points of the sheet and `origin` the place of its
/// point 0,0 in the machine. At least `MIN_POINTS` sightings, with the head at two or more places.
pub fn calibrate_combined(
    rows: &[CombinedPoint],
    pitch_mm: f64,
    origin: (f64, f64),
) -> Result<CombinedCalibration, PnpError> {
    if !(pitch_mm.is_finite() && (0.01..=100.0).contains(&pitch_mm)) {
        return Err(bad(format!(
            "the pitch of the pattern is {pitch_mm} mm, outside the range of camera_pattern_pitch_mm (0.01 to 100)"
        )));
    }
    if rows.len() < MIN_POINTS {
        return Err(bad(format!(
            "at least {MIN_POINTS} sightings are needed, there are {}",
            rows.len()
        )));
    }
    let mut heads: Vec<(f64, f64)> = Vec::new();
    for r in rows {
        if !heads
            .iter()
            .any(|h| (h.0 - r.head_x).hypot(h.1 - r.head_y) < 1e-6)
        {
            heads.push((r.head_x, r.head_y));
        }
    }
    let travel = heads
        .iter()
        .map(|h| (h.0 - heads[0].0).hypot(h.1 - heads[0].1))
        .fold(0.0, f64::max);
    if heads.len() < 2 || travel < MIN_TRAVEL_MM {
        return Err(bad(
            "the head stood at one place only: move it to two or more places (at least 0.5 mm apart) so that the turn of the sheet can be told from the turn of the camera",
        ));
    }
    let cost = |turn: f64, k1: f64, units: f64| -> f64 {
        calibrate_camera(&sightings(rows, pitch_mm, origin, turn, k1, units))
            .map(|c| c.rms_mm)
            .unwrap_or(f64::INFINITY)
    };
    // a first size of a pixel from the sightings as they are
    let first = calibrate_camera(&sightings(rows, pitch_mm, origin, 0.0, 0.0, 0.0))
        .map_err(|e| bad(format!("the sightings do not fix a camera: {}", e.detail)))?;
    let mut units = first.model.units_per_px;
    let (mut turn, mut k1) = (0.0, 0.0);
    // the turn of the sheet is searched coarsely and then closely; then the bending; each round uses the size of a pixel the last one found
    let search_turn = |k1: f64, units: f64| -> f64 {
        let mut best = (f64::INFINITY, 0.0);
        let mut t = -MAX_TURN_DEG;
        while t <= MAX_TURN_DEG + 1e-9 {
            let c = cost(t, k1, units);
            if c < best.0 {
                best = (c, t);
            }
            t += 0.5;
        }
        let lo = (best.1 - 0.5).max(-MAX_TURN_DEG);
        let hi = (best.1 + 0.5).min(MAX_TURN_DEG);
        golden(&|t| cost(t, k1, units), lo, hi, 60)
    };
    for _ in 0..3 {
        turn = search_turn(k1, units);
        k1 = golden(&|k| cost(turn, k, units), -MAX_K1, MAX_K1, 60);
        if let Ok(c) = calibrate_camera(&sightings(rows, pitch_mm, origin, turn, k1, units)) {
            units = c.model.units_per_px;
        }
    }
    let mut camera = calibrate_camera(&sightings(rows, pitch_mm, origin, turn, k1, units))
        .map_err(|e| bad(format!("the sightings do not fix a camera: {}", e.detail)))?;
    if turn.abs() >= MAX_TURN_DEG - 1e-3 {
        return Err(bad(format!(
            "the sheet seems turned by {MAX_TURN_DEG} degrees or more on the bed: check the origin, the pitch and which point is which"
        )));
    }
    camera.model.lens_k1 = k1;
    // the same with a perfect lens, to say what the bending is worth
    let flat_turn = search_turn(0.0, units);
    let rms_flat = cost(flat_turn, 0.0, units);
    // the largest distance of a sighting from the model, with the bending taken out as it was fitted
    let max = sightings(rows, pitch_mm, origin, turn, k1, units)
        .iter()
        .map(|p| {
            let m = camera
                .model
                .to_machine((p.px, p.py), camera.model.units_per_px);
            (m.0 - p.x_mm).hypot(m.1 - p.y_mm)
        })
        .fold(0.0, f64::max);
    Ok(CombinedCalibration {
        sheet_turn_deg: turn,
        lens_k1: k1,
        rms_mm: camera.rms_mm,
        max_mm: max,
        rms_flat_mm: rms_flat,
        points: rows.len(),
        heads: heads.len(),
        camera,
    })
}

impl CombinedCalibration {
    /// Whether every sighting is within `limit_mm` of the model (`camera_pattern_max_residual_mm`).
    pub fn within(&self, limit_mm: f64) -> bool {
        self.max_mm <= limit_mm
    }

    /// The lines of the configuration that set what was found: the camera and the bending of the lens. `camera_correct_lens` is not touched.
    pub fn config_lines(&self) -> String {
        format!(
            "{}camera_lens_k1 = {:.6}\n",
            self.camera.config_lines(),
            self.lens_k1
        )
    }

    /// What was found, in lines for a person.
    pub fn summary(&self) -> String {
        format!(
            "{} sightings with the head at {} places: a pixel is {:.6} mm along X and {:.6} mm along Y, the axes are {:+.3} degrees from square, the lens bends the picture by k1 = {:.6}, the sheet is turned {:+.3} degrees on the bed\nthe sightings are {:.4} mm (rms) and {:.4} mm (largest) from the model; with a perfect lens {:.4} mm (rms)",
            self.points,
            self.heads,
            self.camera.size_x_mm,
            self.camera.size_y_mm,
            self.camera.skew_deg,
            self.lens_k1,
            self.sheet_turn_deg,
            self.rms_mm,
            self.max_mm,
            self.rms_flat_mm
        )
    }

    /// The warning for a fit that is over `limit_mm`, or `None` when it is within.
    pub fn warning(&self, limit_mm: f64) -> Option<String> {
        if self.within(limit_mm) {
            None
        } else {
            Some(format!(
                "a sighting is {:.4} mm from the model, over the {limit_mm} mm that camera_pattern_max_residual_mm allows: look at the sightings, the origin and the pitch of the sheet and whether it is flat",
                self.max_mm
            ))
        }
    }
}
