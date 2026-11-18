// EH3D-PHOENIX-PNP - what the fiducials say about the panel and the board.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! A fit of the fiducials is a transformation of the plane; this module says what it is made of (a shift, a turn, a scale along each axis, how far the axes are from being square and
//! how much of the deformation a fit with one scale could not absorb), judges it against limits, and finds the one mark that does not agree when the others do. A panel that is turned a
//! quarter, stretched by a few per cent or sheared was most likely not put where the person thought, or a mark was taken for another one: the fit may be mathematically fine and the
//! job still wrong. Plain arithmetic and text: no files, no screen.

use crate::config::MachineConfig;
use crate::error::{Code, PnpError};
use crate::fiducial::{fit_points, solve, Fit, FitMode, Transform};
use crate::marks::LevelFit;

/// What a failed check does to the job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Policy {
    /// The check is not made.
    Off,
    /// The findings are printed and the job goes on.
    #[default]
    Warn,
    /// A finding stops the job.
    Refuse,
}

impl Policy {
    pub fn parse(text: &str) -> Option<Policy> {
        match text {
            "off" => Some(Policy::Off),
            "warn" => Some(Policy::Warn),
            "refuse" => Some(Policy::Refuse),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Policy::Off => "off",
            Policy::Warn => "warn",
            Policy::Refuse => "refuse",
        }
    }
}

/// What is done with a fit whose worst mark is too far from where the fit puts it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Reject {
    /// The fit is refused (what it always did).
    #[default]
    Refuse,
    /// If one mark alone explains it and the marks that are left still fix the fit more than they need, that mark is left out and said.
    DropWorst,
}

impl Reject {
    pub fn parse(text: &str) -> Option<Reject> {
        match text {
            "refuse" => Some(Reject::Refuse),
            "drop_worst" => Some(Reject::DropWorst),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Reject::Refuse => "refuse",
            Reject::DropWorst => "drop_worst",
        }
    }
}

/// The limits of the check, from the configuration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Limits {
    pub max_rotation_deg: f64,
    pub scale_tol_percent: f64,
    pub shear_tol_deg: f64,
    pub max_shift_mm: f64,
    /// How much a board may be turned in the panel beyond the turn of the panel itself.
    pub level_turn_deg: f64,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_rotation_deg: 5.0,
            scale_tol_percent: 1.0,
            shear_tol_deg: 0.5,
            max_shift_mm: 5.0,
            level_turn_deg: 2.0,
        }
    }
}

/// A transformation taken apart: first the scales along the axes and the shear, then the turn, then the shift.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Parts {
    pub rotation_deg: f64,
    /// The scale along the turned x axis and along the turned y axis (1 is the size of the design).
    pub scale_x: f64,
    pub scale_y: f64,
    /// How far the axes are from being square, in degrees (0 for a similarity).
    pub shear_deg: f64,
    /// Whether the plane is flipped over (a board that was designed for the other side).
    pub mirrored: bool,
}

/// Takes a transformation apart.
pub fn decompose(t: &Transform) -> Parts {
    let sx = t.a.hypot(t.c).max(1e-12);
    let det = t.a * t.d - t.b * t.c;
    let along = (t.b * t.a + t.d * t.c) / sx;
    let sy = det.abs() / sx;
    Parts {
        rotation_deg: t.c.atan2(t.a).to_degrees(),
        scale_x: sx,
        scale_y: sy,
        shear_deg: along.atan2(sy).to_degrees(),
        mirrored: det < 0.0,
    }
}

/// What the check found wrong.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Finding {
    Rotation(f64),
    /// The scale along an axis is this many per cent off the design.
    ScaleX(f64),
    ScaleY(f64),
    Shear(f64),
    Mirrored,
    Shift(f64),
    /// A board is turned this many degrees in the panel beyond the turn of the panel.
    LevelTurn(f64),
}

impl Finding {
    pub fn text(self) -> String {
        match self {
            Finding::Rotation(d) => format!("turned {d:+.2} degrees from the design"),
            Finding::ScaleX(p) => format!("scale along x {p:+.3} per cent"),
            Finding::ScaleY(p) => format!("scale along y {p:+.3} per cent"),
            Finding::Shear(d) => format!("axes {d:+.3} degrees from square"),
            Finding::Mirrored => "the plane is flipped over (mirrored)".to_string(),
            Finding::Shift(mm) => format!("moved {mm:.2} mm from the design"),
            Finding::LevelTurn(d) => format!("turned {d:+.2} degrees in the panel"),
        }
    }
}

/// The check of one fit.
#[derive(Debug, Clone, PartialEq)]
pub struct Check {
    pub parts: Parts,
    /// How far the middle of the marks moved, when the places of the design are known.
    pub shift_mm: Option<f64>,
    pub findings: Vec<Finding>,
}

impl Check {
    pub fn ok(&self) -> bool {
        self.findings.is_empty()
    }

    pub fn text(&self) -> String {
        self.findings
            .iter()
            .map(|f| f.text())
            .collect::<Vec<_>>()
            .join("; ")
    }
}

/// Judges a transformation. `centroid` is the middle of the marks of the design, to tell how far the whole moved; without it the shift is not judged.
pub fn check(t: &Transform, centroid: Option<(f64, f64)>, limits: &Limits) -> Check {
    let parts = decompose(t);
    let shift = centroid.map(|(x, y)| {
        let (u, v) = t.apply(x, y);
        (u - x).hypot(v - y)
    });
    let mut findings = Vec::new();
    if parts.mirrored {
        findings.push(Finding::Mirrored);
    }
    if parts.rotation_deg.abs() > limits.max_rotation_deg {
        findings.push(Finding::Rotation(parts.rotation_deg));
    }
    let (px, py) = ((parts.scale_x - 1.0) * 100.0, (parts.scale_y - 1.0) * 100.0);
    if px.abs() > limits.scale_tol_percent {
        findings.push(Finding::ScaleX(px));
    }
    if py.abs() > limits.scale_tol_percent {
        findings.push(Finding::ScaleY(py));
    }
    if parts.shear_deg.abs() > limits.shear_tol_deg {
        findings.push(Finding::Shear(parts.shear_deg));
    }
    if let Some(s) = shift {
        if s > limits.max_shift_mm {
            findings.push(Finding::Shift(s));
        }
    }
    Check {
        parts,
        shift_mm: shift,
        findings,
    }
}

/// Judges the fit of every level of a panel. `design` holds the places of the design of every mark of the panel and of its boards (by name), for the shift. A board is also judged
/// against the turn of the panel: a board that is turned in the panel more than the limit is a finding of its own.
pub fn check_levels(
    levels: &[LevelFit],
    design: &[(String, f64, f64)],
    limits: &Limits,
) -> Vec<(String, Check)> {
    let panel_turn = levels
        .iter()
        .find(|l| l.group.is_empty())
        .map(|l| decompose(&l.fit.transform).rotation_deg);
    levels
        .iter()
        .map(|l| {
            let places: Vec<(f64, f64)> = l
                .fit
                .ids
                .iter()
                .filter_map(|id| design.iter().find(|d| &d.0 == id).map(|d| (d.1, d.2)))
                .collect();
            let centroid = if places.is_empty() {
                None
            } else {
                let k = places.len() as f64;
                Some((
                    places.iter().map(|p| p.0).sum::<f64>() / k,
                    places.iter().map(|p| p.1).sum::<f64>() / k,
                ))
            };
            let mut c = check(&l.fit.transform, centroid, limits);
            if let (false, Some(pt)) = (l.group.is_empty(), panel_turn) {
                let rel = wrap180(c.parts.rotation_deg - pt);
                if rel.abs() > limits.level_turn_deg {
                    c.findings.push(Finding::LevelTurn(rel));
                }
            }
            (l.group.clone(), c)
        })
        .collect()
}

fn wrap180(angle: f64) -> f64 {
    let r = angle.rem_euclid(360.0);
    if r > 180.0 {
        r - 360.0
    } else {
        r
    }
}

/// What the marks say, taken apart and judged, with the mark that does not agree when there is one.
#[derive(Debug, Clone, PartialEq)]
pub struct Analysis {
    pub fit: Fit,
    pub check: Check,
    /// The largest distance left by a similarity fit (a turn, one scale and a shift) of the same marks: how much of the deformation (different scales along the axes, shear)
    /// that fit could not absorb. `None` when that fit cannot be made.
    pub similarity_max_mm: Option<f64>,
    /// For each mark, the largest residual of the fit made without it (`None` when the marks left are too few to fit).
    pub without: Vec<Option<f64>>,
    /// The only mark that, left out, brings every other mark within `max_residual_mm`, when the fit is over it and the marks left fix the fit more than they need.
    pub suspect: Option<usize>,
}

/// The least number of marks that fix a fit: an affine fit needs three, a similarity two.
fn needed(mode: FitMode) -> usize {
    if mode == FitMode::Affine {
        3
    } else {
        2
    }
}

fn mode_for(mode: FitMode, count: usize) -> FitMode {
    match mode {
        FitMode::Auto if count >= 3 => FitMode::Affine,
        FitMode::Auto => FitMode::Similarity,
        other => other,
    }
}

fn leave_one_out(
    design: &[(f64, f64)],
    measured: &[(f64, f64)],
    mode: FitMode,
) -> Vec<Option<f64>> {
    (0..design.len())
        .map(|skip| {
            let keep = |v: &[(f64, f64)]| -> Vec<(f64, f64)> {
                v.iter()
                    .enumerate()
                    .filter(|(i, _)| *i != skip)
                    .map(|(_, p)| *p)
                    .collect()
            };
            let (d, m) = (keep(design), keep(measured));
            // a fit that has just as many marks as it needs has no residual: it cannot say which mark is wrong
            let used = mode_for(mode, d.len());
            if d.len() <= needed(used) {
                return None;
            }
            fit_points(&d, &m, used).ok().map(|f| f.max)
        })
        .collect()
}

/// Fits the marks, takes the fit apart and judges it; `max_residual_mm` is the limit of the residual of a mark, used to find the mark that does not agree.
pub fn analyze_points(
    design: &[(f64, f64)],
    measured: &[(f64, f64)],
    mode: FitMode,
    max_residual_mm: f64,
    limits: &Limits,
) -> Result<Analysis, PnpError> {
    let fit = fit_points(design, measured, mode)?;
    let k = design.len() as f64;
    let centroid = (
        design.iter().map(|p| p.0).sum::<f64>() / k,
        design.iter().map(|p| p.1).sum::<f64>() / k,
    );
    let check = check(&fit.transform, Some(centroid), limits);
    let similarity_max_mm = fit_points(design, measured, FitMode::Similarity)
        .ok()
        .map(|f| f.max);
    let without = leave_one_out(design, measured, mode);
    let suspect = unique_below(&without, fit.max, max_residual_mm);
    Ok(Analysis {
        fit,
        check,
        similarity_max_mm,
        without,
        suspect,
    })
}

/// The index of the only mark whose absence brings the largest residual to the limit or under it, when the fit with all of them is over the limit.
fn unique_below(without: &[Option<f64>], max: f64, limit: f64) -> Option<usize> {
    if max <= limit {
        return None;
    }
    let mut ok = without
        .iter()
        .enumerate()
        .filter(|(_, w)| w.is_some_and(|v| v <= limit));
    match (ok.next(), ok.next()) {
        (Some((i, _)), None) => Some(i),
        _ => None,
    }
}

/// Like `fiducial::solve`, but with `Reject::DropWorst` a fit that is over the limit because of one mark is made again without it, as long as the marks that are left still fix the
/// fit more than they need. The name of the mark that was left out comes back with the fit; `Reject::Refuse` is `fiducial::solve` itself.
pub fn solve_robust(
    design: &[(String, f64, f64)],
    measured: &[(String, f64, f64)],
    mode: FitMode,
    max_residual_mm: f64,
    reject: Reject,
) -> Result<(Fit, Option<String>), PnpError> {
    let first = solve(design, measured, mode, max_residual_mm);
    if reject == Reject::Refuse || first.is_ok() {
        return first.map(|f| (f, None));
    }
    let (mut ids, mut from, mut to) = (Vec::new(), Vec::new(), Vec::new());
    for d in design {
        if let Some(m) = measured.iter().find(|m| m.0 == d.0) {
            ids.push(d.0.clone());
            from.push((d.1, d.2));
            to.push((m.1, m.2));
        }
    }
    let attempt = fit_points(&from, &to, mode)
        .ok()
        .and_then(|fit| unique_below(&leave_one_out(&from, &to, mode), fit.max, max_residual_mm));
    let Some(skip) = attempt else {
        return first.map(|f| (f, None));
    };
    let name = ids.remove(skip);
    let (mut d2, mut m2) = (from.clone(), to.clone());
    d2.remove(skip);
    m2.remove(skip);
    let mut fit = fit_points(&d2, &m2, mode_for(mode, d2.len()))?;
    if fit.max > max_residual_mm {
        return first.map(|f| (f, None));
    }
    fit.ids = ids;
    Ok((fit, Some(name)))
}

/// A short report of an analysis, one line for each thing it found, for the command and the window.
pub fn report(a: &Analysis, ids: &[String]) -> String {
    let p = &a.check.parts;
    let mut out = format!(
        "fit: {} with {} marks, rms {:.3} mm, largest {:.3} mm\nturn {:+.3} degrees, scale x {:.5} y {:.5}, axes {:+.3} degrees from square{}\n",
        a.fit.mode.name(),
        a.fit.residuals.len(),
        a.fit.rms,
        a.fit.max,
        p.rotation_deg,
        p.scale_x,
        p.scale_y,
        p.shear_deg,
        if p.mirrored { ", mirrored" } else { "" }
    );
    if let Some(s) = a.check.shift_mm {
        out.push_str(&format!("the middle of the marks moved {s:.3} mm\n"));
    }
    if let Some(w) = a.similarity_max_mm {
        out.push_str(&format!(
            "a fit with one scale and no shear would leave up to {w:.3} mm\n"
        ));
    }
    for (i, r) in a.fit.residuals.iter().enumerate() {
        let name = ids.get(i).map_or("", String::as_str);
        out.push_str(&format!("mark {name}: {r:.3} mm"));
        if a.suspect == Some(i) {
            out.push_str(" (the only one that does not agree: without it every mark does)");
        }
        out.push('\n');
    }
    if a.check.ok() {
        out.push_str("check: ok\n");
    } else {
        for f in &a.check.findings {
            out.push_str(&format!("check: {}\n", f.text()));
        }
    }
    out
}

/// The error for a fit that the check refuses.
pub fn refusal(group: &str, c: &Check) -> PnpError {
    let who = if group.is_empty() {
        "the panel".to_string()
    } else {
        format!("the board {group}")
    };
    PnpError::new(
        Code::FiducialBad,
        format!("the marks put {who} somewhere unexpected: {}", c.text()),
    )
}

impl Limits {
    /// The limits that the configuration sets.
    pub fn from_config(cfg: &MachineConfig) -> Limits {
        Limits {
            max_rotation_deg: cfg.fiducial_max_rotation_deg,
            scale_tol_percent: cfg.fiducial_scale_tol_percent,
            shear_tol_deg: cfg.fiducial_shear_tol_deg,
            max_shift_mm: cfg.fiducial_max_shift_mm,
            level_turn_deg: cfg.fiducial_level_turn_deg,
        }
    }
}

/// The marks that were both designed and measured: their names, their places in the design and the places they were measured at.
pub type Paired = (Vec<String>, Vec<(f64, f64)>, Vec<(f64, f64)>);

/// A mark found by its name: the places of the marks of the design that were also measured, in the order of the design, with their names. A measured mark that is not in the design is
/// an error, and so is one that is measured twice.
pub fn pair_marks(
    design: &[(String, f64, f64)],
    measured: &[(String, f64, f64)],
) -> Result<Paired, PnpError> {
    if let Some(m) = measured.iter().find(|m| !design.iter().any(|d| d.0 == m.0)) {
        return Err(PnpError::new(
            Code::FiducialBad,
            format!("the mark '{}' is not in the design", m.0),
        ));
    }
    for (i, m) in measured.iter().enumerate() {
        if measured[..i].iter().any(|o| o.0 == m.0) {
            return Err(PnpError::new(
                Code::FiducialBad,
                format!("the mark '{}' is measured more than once", m.0),
            ));
        }
    }
    let (mut ids, mut from, mut to) = (Vec::new(), Vec::new(), Vec::new());
    for d in design {
        if let Some(m) = measured.iter().find(|m| m.0 == d.0) {
            ids.push(d.0.clone());
            from.push((d.1, d.2));
            to.push((m.1, m.2));
        }
    }
    Ok((ids, from, to))
}

/// Fits and judges the measured marks against the design with the limits and the kind of fit of the configuration. Gives the analysis and the names of the marks it used.
pub fn analyze_marks(
    design: &[(String, f64, f64)],
    measured: &[(String, f64, f64)],
    cfg: &MachineConfig,
) -> Result<(Analysis, Vec<String>), PnpError> {
    let (ids, from, to) = pair_marks(design, measured)?;
    let a = analyze_points(
        &from,
        &to,
        cfg.fiducial_mode,
        cfg.fiducial_max_residual_mm,
        &Limits::from_config(cfg),
    )?;
    Ok((a, ids))
}

/// What is wrong with an analysis, one line each: a mark farther from the fit than `max_residual_mm` (with the one that does not agree named when there is one) and the findings of the
/// check. Empty when all is well.
pub fn warnings(a: &Analysis, ids: &[String], max_residual_mm: f64) -> Vec<String> {
    let mut out = Vec::new();
    for (i, r) in a.fit.residuals.iter().enumerate() {
        if *r > max_residual_mm {
            let name = ids.get(i).map_or("", String::as_str);
            out.push(format!(
                "mark {name} is {r:.3} mm from where the fit puts it, over the {max_residual_mm} mm that fiducial_max_residual_mm allows"
            ));
        }
    }
    if let Some(i) = a.suspect {
        let name = ids.get(i).map_or("", String::as_str);
        out.push(format!(
            "the mark {name} is the only one that does not agree: look at where it was taken"
        ));
    }
    for f in &a.check.findings {
        out.push(f.text());
    }
    out
}

/// The residual of each mark as a table for the window: the name, the distance from the fit in millimetres, how far the fit would be without the mark and whether the mark is over the limit.
pub fn residual_table(a: &Analysis, ids: &[String], max_residual_mm: f64) -> String {
    let mut out = String::from("mark,residual_mm,without_mark_mm,state\n");
    for (i, r) in a.fit.residuals.iter().enumerate() {
        let name = ids.get(i).map_or("", String::as_str);
        let without = a
            .without
            .get(i)
            .copied()
            .flatten()
            .map_or(String::new(), |w| format!("{w:.3}"));
        let state = if a.suspect == Some(i) {
            "suspect"
        } else if *r > max_residual_mm {
            "over"
        } else {
            "ok"
        };
        out.push_str(&format!(
            "{},{r:.3},{without},{state}\n",
            crate::parts::csv_field(name)
        ));
    }
    out
}
