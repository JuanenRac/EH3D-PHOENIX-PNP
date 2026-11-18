// EH3D-PHOENIX-PNP - the calibration of the camera from a pattern of points on a grid.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! A sheet with a grid of points (the inner corners of a chequerboard, or the centres of dots) is put under the camera and the place of every point is read in the picture. Because the
//! points lie on a grid of a known pitch, the picture says two things that sightings of one mark at a few places of the head cannot: how big a pixel is on the part, and how the lens bends
//! the picture. The fit finds the size of a pixel, the radial bending (`camera_lens_k1`: a point at `d` millimetres from the middle of the picture is drawn at `d * (1 + k1 * d * d)`, the
//! convention of the model of the camera) and where the sheet lies in the picture (its turn and its shift, which are the business of the sheet and not of the camera), then says how far
//! the points are from where the model puts them, with the bending and without it. The turn of the picture with respect to the machine and the place of the axis of the nozzle come from
//! `camera-calibrate`, which moves the head. Nothing here moves a machine.
//!
//! A file has one row for each point: `col,row,px,py`, the place of the point in the grid (whole numbers, or any numbers when the points are not on a full grid) and where it was seen
//! (pixels from the middle of the picture, the second axis pointing up). Every error carries the code `PHOENIX-PNP-058`.

use crate::calibrate::MAX_POINTS;
use crate::error::{Code, PnpError};
use crate::fiducial::{fit_points, FitMode};
use crate::parts::{detect_delimiter, read_records_numbered};

/// The fewest points that fix the fit: five numbers are found and every point gives two.
pub const MIN_POINTS: usize = 6;
/// The most iterations of the fit.
const MAX_ITERATIONS: usize = 200;

fn bad(detail: impl Into<String>) -> PnpError {
    PnpError::new(Code::CalibrationBad, detail)
}

/// One point of the pattern: its place in the grid and where it was seen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PatternPoint {
    pub col: f64,
    pub row: f64,
    pub px: f64,
    pub py: f64,
}

/// Reads a file of points: the columns `col,row,px,py`, one row for each, with an optional header and comment lines (`#`).
pub fn parse_pattern(text: &str) -> Result<Vec<PatternPoint>, PnpError> {
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
    let mut out: Vec<PatternPoint> = Vec::new();
    for (i, (line, record)) in records.into_iter().enumerate() {
        if i == 0 && record.first().is_some_and(|c| c.parse::<f64>().is_err()) {
            continue;
        }
        if record.len() != 4 {
            return Err(bad(format!(
                "line {line}: a point of the pattern needs 4 numbers (col,row,px,py), this row has {}",
                record.len()
            )));
        }
        let mut v = [0.0; 4];
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
            return Err(bad(format!("line {line}: more than {MAX_POINTS} points")));
        }
        if out.iter().any(|p| p.col == v[0] && p.row == v[1]) {
            return Err(bad(format!(
                "line {line}: the point {},{} is listed twice",
                v[0], v[1]
            )));
        }
        out.push(PatternPoint {
            col: v[0],
            row: v[1],
            px: v[2],
            py: v[3],
        });
    }
    Ok(out)
}

/// What the fit of the pattern found.
#[derive(Debug, Clone, PartialEq)]
pub struct PatternCalibration {
    /// Millimetres on the part that a pixel is (`camera_units_per_px`).
    pub units_per_px: f64,
    /// The radial bending of the lens per square millimetre (`camera_lens_k1`).
    pub lens_k1: f64,
    /// How much the sheet is turned in the picture, in degrees, and whether the grid appears mirrored (rows going the other way from the second axis of the picture).
    pub pattern_turn_deg: f64,
    pub mirrored: bool,
    /// How far the axes of the grid are from being at right angles in the picture, in degrees (a lens does not make this; a sheet or a camera that is tilted does).
    pub skew_deg: f64,
    /// How far the points are from the model with the bending: root mean square and largest, in millimetres and in pixels.
    pub rms_mm: f64,
    pub max_mm: f64,
    pub rms_px: f64,
    /// The same without the bending (a plain size of a pixel, a turn and a shift), to say what the bending is worth.
    pub rms_flat_mm: f64,
    pub max_flat_mm: f64,
    /// The point that is farthest from the model, as `col,row`.
    pub worst: (f64, f64),
    pub points: usize,
    /// The farthest a point of the picture is from its middle, in millimetres: the bending matters most there.
    pub radius_mm: f64,
}

/// The pixel where the model puts a point of the grid, for the parameters `[turn (radians), shift x (mm), shift y (mm), size of a pixel (mm), k1]`.
fn predict(p: &[f64; 5], mirrored: bool, col: f64, row: f64, pitch: f64) -> (f64, f64) {
    let (gx, gy) = (
        col * pitch,
        if mirrored { -row * pitch } else { row * pitch },
    );
    let (s, c) = p[0].sin_cos();
    let (dx, dy) = (c * gx - s * gy + p[1], s * gx + c * gy + p[2]);
    let bend = 1.0 + p[4] * (dx * dx + dy * dy);
    (dx * bend / p[3], dy * bend / p[3])
}

fn residuals(
    p: &[f64; 5],
    mirrored: bool,
    points: &[PatternPoint],
    pitch: f64,
    out: &mut Vec<f64>,
) {
    out.clear();
    for q in points {
        let m = predict(p, mirrored, q.col, q.row, pitch);
        out.push(m.0 - q.px);
        out.push(m.1 - q.py);
    }
}

/// Solves `a x = b` for a small square system by elimination with the largest pivot; `None` when it is singular.
fn solve_small(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    for i in 0..n {
        let mut best = i;
        for r in i + 1..n {
            if a[r][i].abs() > a[best][i].abs() {
                best = r;
            }
        }
        if a[best][i].abs() < 1e-300 {
            return None;
        }
        a.swap(i, best);
        b.swap(i, best);
        let pivot = a[i].clone();
        for r in i + 1..n {
            let f = a[r][i] / pivot[i];
            for (cell, p) in a[r].iter_mut().zip(&pivot).skip(i) {
                *cell -= f * p;
            }
            b[r] -= f * b[i];
        }
    }
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        let behind: f64 = a[i].iter().zip(&x).skip(i + 1).map(|(p, q)| p * q).sum();
        x[i] = (b[i] - behind) / a[i][i];
    }
    Some(x)
}

/// Levenberg-Marquardt over the parameters that are free (`k1` is left at its value when `with_bend` is false). Gives the parameters.
fn fit(
    start: [f64; 5],
    mirrored: bool,
    points: &[PatternPoint],
    pitch: f64,
    with_bend: bool,
) -> [f64; 5] {
    let free: Vec<usize> = if with_bend {
        vec![0, 1, 2, 3, 4]
    } else {
        vec![0, 1, 2, 3]
    };
    // a step for the numerical derivative of each parameter, small against what the parameter is
    let steps = [1e-7, 1e-6, 1e-6, 1e-9, 1e-9];
    let mut p = start;
    let mut r = Vec::new();
    residuals(&p, mirrored, points, pitch, &mut r);
    let mut cost: f64 = r.iter().map(|v| v * v).sum();
    let mut lambda = 1e-3;
    let mut scratch = Vec::new();
    for _ in 0..MAX_ITERATIONS {
        // the Jacobian by central differences
        let mut jac = vec![vec![0.0; r.len()]; free.len()];
        for (k, &i) in free.iter().enumerate() {
            let (mut hi, mut lo) = (p, p);
            hi[i] += steps[i];
            lo[i] -= steps[i];
            let (mut rh, mut rl) = (Vec::new(), Vec::new());
            residuals(&hi, mirrored, points, pitch, &mut rh);
            residuals(&lo, mirrored, points, pitch, &mut rl);
            for (j, slot) in jac[k].iter_mut().enumerate() {
                *slot = (rh[j] - rl[j]) / (2.0 * steps[i]);
            }
        }
        let dot = |u: &[f64], v: &[f64]| -> f64 { u.iter().zip(v).map(|(x, y)| x * y).sum() };
        let jtj: Vec<Vec<f64>> = jac
            .iter()
            .map(|ja| jac.iter().map(|jb| dot(ja, jb)).collect())
            .collect();
        let jtr: Vec<f64> = jac.iter().map(|ja| dot(ja, &r)).collect();
        let mut improved = false;
        for _ in 0..30 {
            let mut m = jtj.clone();
            for (a, row) in m.iter_mut().enumerate() {
                row[a] += lambda * jtj[a][a].max(1e-12);
            }
            let neg: Vec<f64> = jtr.iter().map(|v| -v).collect();
            let Some(delta) = solve_small(m, neg) else {
                lambda *= 10.0;
                continue;
            };
            let mut trial = p;
            for (k, &i) in free.iter().enumerate() {
                trial[i] += delta[k];
            }
            if trial[3] <= 0.0 {
                lambda *= 10.0;
                continue;
            }
            residuals(&trial, mirrored, points, pitch, &mut scratch);
            let trial_cost: f64 = scratch.iter().map(|v| v * v).sum();
            if trial_cost.is_finite() && trial_cost < cost {
                let gain = cost - trial_cost;
                p = trial;
                std::mem::swap(&mut r, &mut scratch);
                cost = trial_cost;
                lambda = (lambda / 5.0).max(1e-12);
                improved = gain > 1e-18 * (1.0 + cost);
                break;
            }
            lambda *= 10.0;
        }
        if !improved {
            break;
        }
    }
    p
}

/// The errors of the points from the model, in millimetres, for the parameters.
fn errors_mm(p: &[f64; 5], mirrored: bool, points: &[PatternPoint], pitch: f64) -> Vec<f64> {
    points
        .iter()
        .map(|q| {
            let m = predict(p, mirrored, q.col, q.row, pitch);
            (m.0 - q.px).hypot(m.1 - q.py) * p[3]
        })
        .collect()
}

fn rms(v: &[f64]) -> f64 {
    (v.iter().map(|e| e * e).sum::<f64>() / v.len() as f64).sqrt()
}

/// Fits the pattern: the points seen, the pitch of the grid in millimetres. At least `MIN_POINTS` points, not on a line.
pub fn calibrate_pattern(
    points: &[PatternPoint],
    pitch_mm: f64,
) -> Result<PatternCalibration, PnpError> {
    if !(pitch_mm.is_finite() && (0.01..=100.0).contains(&pitch_mm)) {
        return Err(bad(format!(
            "the pitch of the pattern is {pitch_mm} mm, outside the range of camera_pattern_pitch_mm (0.01 to 100)"
        )));
    }
    if points.len() < MIN_POINTS {
        return Err(bad(format!(
            "at least {MIN_POINTS} points of the pattern are needed, there are {}",
            points.len()
        )));
    }
    let grid: Vec<(f64, f64)> = points
        .iter()
        .map(|p| (p.col * pitch_mm, p.row * pitch_mm))
        .collect();
    let seen: Vec<(f64, f64)> = points.iter().map(|p| (p.px, p.py)).collect();
    // an affine fit of the grid to the pixels says whether the grid is mirrored and how square its axes are, and the first guess of the rest
    let affine = fit_points(&grid, &seen, FitMode::Affine)
        .map_err(|e| bad(format!("the points do not fix a pattern: {}", e.detail)))?;
    let t = affine.transform;
    let mirrored = t.a * t.d - t.b * t.c < 0.0;
    let flipped: Vec<(f64, f64)> = grid
        .iter()
        .map(|g| (g.0, if mirrored { -g.1 } else { g.1 }))
        .collect();
    let (sx, sy) = (t.a.hypot(t.c), t.b.hypot(t.d));
    if sx < 1e-9 || sy < 1e-9 {
        return Err(bad("the points give a picture of no size"));
    }
    let cos = ((t.a * t.b + t.c * t.d) / (sx * sy)).clamp(-1.0, 1.0);
    let skew = cos.acos().to_degrees() - 90.0;
    let similar = fit_points(&flipped, &seen, FitMode::Similarity)
        .map_err(|e| bad(format!("the points do not fix a pattern: {}", e.detail)))?;
    let st = similar.transform;
    let scale = st.a.hypot(st.c);
    if scale < 1e-9 {
        return Err(bad("the points give a picture of no size"));
    }
    let size = 1.0 / scale;
    let start = [st.c.atan2(st.a), st.tx * size, st.ty * size, size, 0.0];
    let flat = fit(start, mirrored, points, pitch_mm, false);
    let bent = fit(flat, mirrored, points, pitch_mm, true);
    let (size, k1) = (bent[3], bent[4]);
    if !(0.001..=10.0).contains(&size) {
        return Err(bad(format!(
            "a pixel of {size:.6} mm is outside the range of camera_units_per_px (0.001 to 10)"
        )));
    }
    if k1.abs() > 0.01 {
        return Err(bad(format!(
            "a bending of {k1:.6} is outside the range of camera_lens_k1 (-0.01 to 0.01): check the points and the pitch"
        )));
    }
    let err = errors_mm(&bent, mirrored, points, pitch_mm);
    let err_flat = errors_mm(&flat, mirrored, points, pitch_mm);
    let (mut worst_i, mut worst) = (0, 0.0);
    for (i, e) in err.iter().enumerate() {
        if *e > worst {
            worst = *e;
            worst_i = i;
        }
    }
    let radius = points
        .iter()
        .map(|q| {
            let (gx, gy) = (
                q.col * pitch_mm,
                if mirrored {
                    -q.row * pitch_mm
                } else {
                    q.row * pitch_mm
                },
            );
            let (s, c) = bent[0].sin_cos();
            (c * gx - s * gy + bent[1]).hypot(s * gx + c * gy + bent[2])
        })
        .fold(0.0, f64::max);
    Ok(PatternCalibration {
        units_per_px: size,
        lens_k1: k1,
        pattern_turn_deg: bent[0].to_degrees(),
        mirrored,
        skew_deg: skew,
        rms_mm: rms(&err),
        max_mm: worst,
        rms_px: rms(&err) / size,
        rms_flat_mm: rms(&err_flat),
        max_flat_mm: err_flat.iter().copied().fold(0.0, f64::max),
        worst: (points[worst_i].col, points[worst_i].row),
        points: points.len(),
        radius_mm: radius,
    })
}

impl PatternCalibration {
    /// Whether every point is within `limit_mm` of the model (`camera_pattern_max_residual_mm`).
    pub fn within(&self, limit_mm: f64) -> bool {
        self.max_mm <= limit_mm
    }

    /// The lines of the configuration that set what was found: the size of a pixel and the bending of the lens.
    pub fn config_lines(&self) -> String {
        format!(
            "camera_units_per_px = {:.6}\ncamera_lens_k1 = {:.6}\n",
            self.units_per_px, self.lens_k1
        )
    }

    /// What was found, in lines for a person.
    pub fn summary(&self) -> String {
        format!(
            "{} points: a pixel is {:.6} mm, the lens bends the picture by k1 = {:.6} (up to {:.2} per cent at {:.2} mm from the middle), the sheet is turned {:+.3} degrees{}, its axes are {:+.3} degrees from square\nthe points are {:.4} mm (rms, {:.2} px) and {:.4} mm (largest, the point {},{}) from the model; without the bending {:.4} mm (rms) and {:.4} mm (largest)",
            self.points,
            self.units_per_px,
            self.lens_k1,
            self.lens_k1 * self.radius_mm * self.radius_mm * 100.0,
            self.radius_mm,
            self.pattern_turn_deg,
            if self.mirrored { " and mirrored" } else { "" },
            self.skew_deg,
            self.rms_mm,
            self.rms_px,
            self.max_mm,
            self.worst.0,
            self.worst.1,
            self.rms_flat_mm,
            self.max_flat_mm
        )
    }

    /// The warning for a fit that is over `limit_mm`, or `None` when it is within.
    pub fn warning(&self, limit_mm: f64) -> Option<String> {
        if self.within(limit_mm) {
            None
        } else {
            Some(format!(
                "the point {},{} is {:.4} mm from the model, over the {limit_mm} mm that camera_pattern_max_residual_mm allows: look at the points that were read, the pitch and whether the sheet is flat",
                self.worst.0, self.worst.1, self.max_mm
            ))
        }
    }
}
