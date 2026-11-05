//! Fiducials of a panel: marks whose designed places are in the panel file and whose real places are measured on the machine. From the pairs a transformation of the plane is fitted
//! by least squares, and the parts are moved with it, so that a panel that was cut, glued or put in the fixture a little crooked is placed where it really is. The fit says how well
//! the marks agree with it (the residual of each mark), and one that does not agree is refused, because a wrong mark would move every part.
//!
//! Two kinds of fit: a similarity (a turn, one scale and a shift; two marks are enough) and an affine fit (a turn, two scales, a shear and a shift; three marks that are not on a line).

use crate::error::{Code, PnpError};
use crate::parts::{detect_delimiter, read_records, Part};
use std::collections::HashSet;

pub const MAX_FIDUCIALS: usize = 100;
const MAX_COORDINATE: f64 = 10_000.0;

/// `x' = a x + b y + tx`, `y' = c x + d y + ty`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub tx: f64,
    pub ty: f64,
}

impl Transform {
    pub const IDENTITY: Transform = Transform {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        tx: 0.0,
        ty: 0.0,
    };

    pub fn apply(&self, x: f64, y: f64) -> (f64, f64) {
        (
            self.a * x + self.b * y + self.tx,
            self.c * x + self.d * y + self.ty,
        )
    }

    /// The angle the x axis is turned by, in degrees, counter-clockwise.
    pub fn rotation_deg(&self) -> f64 {
        self.c.atan2(self.a).to_degrees()
    }

    /// The scale along the turned x axis and along the turned y axis.
    pub fn scale(&self) -> (f64, f64) {
        (self.a.hypot(self.c), self.b.hypot(self.d))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FitMode {
    /// A similarity with two marks, an affine fit with three or more.
    #[default]
    Auto,
    Similarity,
    Affine,
}

impl FitMode {
    pub fn parse(text: &str) -> Option<FitMode> {
        match text {
            "auto" => Some(FitMode::Auto),
            "similarity" => Some(FitMode::Similarity),
            "affine" => Some(FitMode::Affine),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            FitMode::Auto => "auto",
            FitMode::Similarity => "similarity",
            FitMode::Affine => "affine",
        }
    }
}

/// A fit and how well the marks agree with it.
#[derive(Debug, Clone, PartialEq)]
pub struct Fit {
    pub transform: Transform,
    /// The kind that was used: never `Auto`.
    pub mode: FitMode,
    /// The names of the marks that were used (empty for `fit_points`).
    pub ids: Vec<String>,
    /// For each mark, the distance in millimetres between its measured place and the place the fit gives it.
    pub residuals: Vec<f64>,
    pub rms: f64,
    pub max: f64,
}

fn bad(message: impl Into<String>) -> PnpError {
    PnpError::new(Code::FiducialBad, message)
}

/// The least-squares fit that takes the `design` places to the `measured` ones.
pub fn fit_points(
    design: &[(f64, f64)],
    measured: &[(f64, f64)],
    mode: FitMode,
) -> Result<Fit, PnpError> {
    let n = design.len();
    if n != measured.len() {
        return Err(bad(
            "the marks of the design and the measured ones differ in number",
        ));
    }
    if n < 2 {
        return Err(bad("at least two marks are needed"));
    }
    if n > MAX_FIDUCIALS {
        return Err(bad(format!("more than {MAX_FIDUCIALS} marks")));
    }
    if design
        .iter()
        .chain(measured)
        .any(|p| !p.0.is_finite() || !p.1.is_finite())
    {
        return Err(bad("a mark has a place that is not a number"));
    }
    let used = match mode {
        FitMode::Auto if n >= 3 => FitMode::Affine,
        FitMode::Auto => FitMode::Similarity,
        other => other,
    };
    if used == FitMode::Affine && n < 3 {
        return Err(bad("an affine fit needs three marks"));
    }
    let k = n as f64;
    let (mx, my) = (
        design.iter().map(|p| p.0).sum::<f64>() / k,
        design.iter().map(|p| p.1).sum::<f64>() / k,
    );
    let (mu, mv) = (
        measured.iter().map(|p| p.0).sum::<f64>() / k,
        measured.iter().map(|p| p.1).sum::<f64>() / k,
    );
    // sums over the places taken from their centres
    let (mut sxx, mut sxy, mut syy) = (0.0, 0.0, 0.0);
    let (mut sxu, mut syu, mut sxv, mut syv) = (0.0, 0.0, 0.0, 0.0);
    for (d, m) in design.iter().zip(measured) {
        let (x, y, u, v) = (d.0 - mx, d.1 - my, m.0 - mu, m.1 - mv);
        sxx += x * x;
        sxy += x * y;
        syy += y * y;
        sxu += x * u;
        syu += y * u;
        sxv += x * v;
        syv += y * v;
    }
    let (a, b, c, d) = if used == FitMode::Affine {
        let det = sxx * syy - sxy * sxy;
        if det <= 1e-9 * (sxx + syy) * (sxx + syy) {
            return Err(bad("the marks are on one line: they do not fix a position"));
        }
        (
            (sxu * syy - syu * sxy) / det,
            (syu * sxx - sxu * sxy) / det,
            (sxv * syy - syv * sxy) / det,
            (syv * sxx - sxv * sxy) / det,
        )
    } else {
        let total = sxx + syy;
        if total <= 1e-12 {
            return Err(bad(
                "the marks are on one point: they do not fix a position",
            ));
        }
        let a = (sxu + syv) / total;
        let s = (sxv - syu) / total;
        (a, -s, s, a)
    };
    let transform = Transform {
        a,
        b,
        c,
        d,
        tx: mu - (a * mx + b * my),
        ty: mv - (c * mx + d * my),
    };
    let residuals: Vec<f64> = design
        .iter()
        .zip(measured)
        .map(|(p, m)| {
            let q = transform.apply(p.0, p.1);
            (q.0 - m.0).hypot(q.1 - m.1)
        })
        .collect();
    let rms = (residuals.iter().map(|r| r * r).sum::<f64>() / k).sqrt();
    let max = residuals.iter().copied().fold(0.0, f64::max);
    Ok(Fit {
        transform,
        mode: used,
        ids: Vec::new(),
        residuals,
        rms,
        max,
    })
}

/// Fits the marks that the design and the measurements have in common (by name). A measured mark that the design does not have is an error (it is most likely a typing mistake); a
/// designed mark that was not measured is not used. The fit is refused when a mark is further than `max_residual_mm` from where the fit puts it.
pub fn solve(
    design: &[(String, f64, f64)],
    measured: &[(String, f64, f64)],
    mode: FitMode,
    max_residual_mm: f64,
) -> Result<Fit, PnpError> {
    let known: HashSet<&str> = design.iter().map(|d| d.0.as_str()).collect();
    if let Some(m) = measured.iter().find(|m| !known.contains(m.0.as_str())) {
        return Err(bad(format!("the mark '{}' is not in the panel", m.0)));
    }
    let (mut ids, mut from, mut to) = (Vec::new(), Vec::new(), Vec::new());
    for d in design {
        if let Some(m) = measured.iter().find(|m| m.0 == d.0) {
            ids.push(d.0.clone());
            from.push((d.1, d.2));
            to.push((m.1, m.2));
        }
    }
    let mut fit = fit_points(&from, &to, mode)?;
    if fit.max > max_residual_mm {
        let worst = fit
            .residuals
            .iter()
            .position(|r| *r == fit.max)
            .map_or("", |i| ids[i].as_str());
        return Err(bad(format!(
            "the mark '{worst}' is {:.3} mm from where the fit puts it, the limit is {max_residual_mm} mm",
            fit.max
        )));
    }
    fit.ids = ids;
    Ok(fit)
}

/// Moves the parts with the fit and turns them by its angle.
pub fn correct(parts: &mut [Part], transform: &Transform) {
    // a shift alone leaves the angles as they are, whatever the last digits of the fit say
    let turn = transform.rotation_deg();
    let turn = if turn.abs() < 1e-9 { 0.0 } else { turn };
    for p in parts {
        let (x, y) = transform.apply(p.x, p.y);
        p.x = x;
        p.y = y;
        p.rotation = (p.rotation + turn).rem_euclid(360.0);
    }
}

/// The measured marks of a file: a CSV with the header `id,x,y` (comma or semicolon).
pub fn parse_measured(text: &str) -> Result<Vec<(String, f64, f64)>, PnpError> {
    let text = text.trim_start_matches('\u{feff}');
    let head = text
        .lines()
        .find(|l| !l.trim().is_empty())
        .ok_or_else(|| bad("the file of measured marks is empty"))?;
    let mut records = read_records(text, detect_delimiter(head))
        .map_err(bad)?
        .into_iter();
    let header: Vec<String> = records
        .next()
        .ok_or_else(|| bad("the file of measured marks is empty"))?
        .iter()
        .map(|h| h.to_ascii_lowercase())
        .collect();
    if header != ["id", "x", "y"] {
        return Err(bad(
            "the first row of the measured marks must be exactly: id,x,y",
        ));
    }
    let mut out: Vec<(String, f64, f64)> = Vec::new();
    for (i, r) in records.enumerate() {
        let row = i + 2;
        if r.len() != 3 {
            return Err(bad(format!(
                "measured mark row {row}: {} fields, not 3",
                r.len()
            )));
        }
        if r[0].is_empty() || out.iter().any(|m| m.0 == r[0]) {
            return Err(bad(format!(
                "measured mark row {row}: the id '{}' is empty or used twice",
                r[0]
            )));
        }
        if out.len() >= MAX_FIDUCIALS {
            return Err(bad(format!("more than {MAX_FIDUCIALS} measured marks")));
        }
        let num = |s: &str| {
            s.parse::<f64>()
                .ok()
                .filter(|v| v.is_finite() && v.abs() <= MAX_COORDINATE)
                .ok_or_else(|| bad(format!("measured mark row {row}: '{s}' is not a position")))
        };
        out.push((r[0].clone(), num(&r[1])?, num(&r[2])?));
    }
    Ok(out)
}

/// The measured marks as the text of a file that `parse_measured` reads.
pub fn measured_csv(marks: &[(String, f64, f64)]) -> String {
    let mut out = String::from("id,x,y\n");
    for (id, x, y) in marks {
        out.push_str(&format!("{id},{x},{y}\n"));
    }
    out
}
