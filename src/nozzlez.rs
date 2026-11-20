// EH3D-PHOENIX-PNP - the height of each nozzle tip from probings of the bed.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! The tips of a head are never at exactly the same height, and a placement that is a tenth of a millimetre too high or too low drops the part or crushes it. A probing touches the bed with a tip and
//! reads the height at which it touched; this takes several probings of each tip (`nozzle,z_mm`), gives the mean height of each tip, how much its probings disagree, how far the tip is from the
//! mean of all the tips (the offset, and the correction is the offset with the other sign) and whether the tip is out of line (its offset is over the tolerance), noisy (its probings disagree by more
//! than the tolerance) or was probed once. With no machine the probings can be made up from a seed (`simulate`): every tip is given a height by chance and every probing reads it with some noise,
//! so that the number of probings and the noise that a calibration bears can be tried, and the offsets that were found are compared with the heights that were made up. It is a test of the
//! arithmetic on a simulated head, not of a real one.

use crate::error::{Code, PnpError};
use crate::parts::{csv_field, detect_delimiter, read_records_numbered};
use crate::sim::Rng;
use std::collections::BTreeMap;

/// The most tips of a head.
pub const MAX_NOZZLES: usize = 64;
/// The most probings of one tip in a simulation.
pub const MAX_PROBES: usize = 1000;
/// The most readings of a file.
pub const MAX_READINGS: usize = 100_000;

/// One probing: the tip and the height at which it touched the bed, in millimetres.
#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    pub nozzle: String,
    pub z_mm: f64,
}

fn bad(line: usize, detail: impl Into<String>) -> PnpError {
    PnpError::new(
        Code::CalibrationBad,
        format!("line {line}: {}", detail.into()),
    )
}

/// Reads the probings: a table with a column of the tip (`nozzle`, `tip` or `id`) and one of the height (`z_mm`, `z`, `touch_z_mm` or `height_mm`).
pub fn parse_readings(text: &str) -> Result<Vec<Reading>, PnpError> {
    let text = text.trim_start_matches('\u{feff}');
    let head = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let records =
        read_records_numbered(text, detect_delimiter(head)).map_err(|(line, e)| bad(line, e))?;
    let mut rows = records.into_iter();
    let (head_line, header) = rows.next().ok_or_else(|| bad(1, "the file is empty"))?;
    let find = |names: &[&str]| {
        header
            .iter()
            .position(|h| names.contains(&h.trim().to_ascii_lowercase().as_str()))
    };
    let nozzle_at = find(&["nozzle", "tip", "id"])
        .ok_or_else(|| bad(head_line, "the header has no column nozzle"))?;
    let z_at = find(&["z_mm", "z", "touch_z_mm", "height_mm"])
        .ok_or_else(|| bad(head_line, "the header has no column z_mm"))?;
    let mut out = Vec::new();
    for (line, row) in rows {
        if row.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        if out.len() >= MAX_READINGS {
            return Err(bad(line, format!("more than {MAX_READINGS} readings")));
        }
        let name = row.get(nozzle_at).map_or("", |s| s.trim());
        if name.is_empty() || name.chars().count() > 64 || name.chars().any(char::is_control) {
            return Err(bad(line, "the nozzle is a name of up to 64 characters"));
        }
        let z = row
            .get(z_at)
            .and_then(|s| s.trim().parse::<f64>().ok())
            .filter(|v| v.is_finite() && v.abs() <= 1000.0)
            .ok_or_else(|| bad(line, "z_mm is a number from -1000 to 1000"))?;
        out.push(Reading {
            nozzle: name.to_string(),
            z_mm: z,
        });
    }
    if out.is_empty() {
        return Err(bad(head_line, "the file has no readings"));
    }
    Ok(out)
}

/// The probings as a table that `parse_readings` reads back (to the four digits that are written).
pub fn readings_csv(readings: &[Reading]) -> String {
    let mut out = String::from("nozzle,z_mm\n");
    for r in readings {
        out.push_str(&format!("{},{:.4}\n", csv_field(&r.nozzle), r.z_mm));
    }
    out
}

/// What a tip comes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Its probings agree and it is within the tolerance of the mean of the tips.
    Ok,
    /// Its height is over the tolerance away from the mean of the tips.
    OutOfLine,
    /// Its probings disagree by more than the tolerance: the height is not to be trusted.
    Noisy,
    /// It was probed once, so nothing says how steady the reading is.
    Single,
}

impl Status {
    pub fn name(self) -> &'static str {
        match self {
            Status::Ok => "ok",
            Status::OutOfLine => "out_of_line",
            Status::Noisy => "noisy",
            Status::Single => "single",
        }
    }
}

/// One tip.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub nozzle: String,
    pub probes: usize,
    pub mean_mm: f64,
    /// The highest probing minus the lowest.
    pub spread_mm: f64,
    /// The mean height of the tip minus the mean of the tips: how much higher it is than the rest (the correction is the opposite).
    pub offset_mm: f64,
    pub status: Status,
}

/// The tips of a head.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub rows: Vec<Row>,
    /// The mean of the mean heights of the tips.
    pub reference_mm: f64,
    pub tol_mm: f64,
}

/// The mean height, the offset and the state of every tip, in the order of their names. The tolerance is from 0.001 to 1 mm.
pub fn analyse(readings: &[Reading], tol_mm: f64) -> Result<Report, PnpError> {
    let setting = |m: String| PnpError::new(Code::PlanSetting, m);
    if !tol_mm.is_finite() || !(0.001..=1.0).contains(&tol_mm) {
        return Err(setting(
            "the tolerance of the height must be from 0.001 to 1 mm".into(),
        ));
    }
    let mut by: BTreeMap<&str, Vec<f64>> = BTreeMap::new();
    for r in readings {
        by.entry(r.nozzle.as_str()).or_default().push(r.z_mm);
    }
    if by.is_empty() {
        return Err(setting("there are no readings".into()));
    }
    if by.len() > MAX_NOZZLES {
        return Err(setting(format!("a head has up to {MAX_NOZZLES} tips")));
    }
    let means: Vec<(&str, usize, f64, f64)> = by
        .iter()
        .map(|(name, zs)| {
            let mean = zs.iter().sum::<f64>() / zs.len() as f64;
            let (lo, hi) = zs
                .iter()
                .fold((f64::MAX, f64::MIN), |(lo, hi), z| (lo.min(*z), hi.max(*z)));
            (*name, zs.len(), mean, hi - lo)
        })
        .collect();
    let reference = means.iter().map(|m| m.2).sum::<f64>() / means.len() as f64;
    let rows = means
        .into_iter()
        .map(|(name, probes, mean, spread)| {
            let offset = mean - reference;
            let status = if probes > 1 && spread > tol_mm {
                Status::Noisy
            } else if offset.abs() > tol_mm {
                Status::OutOfLine
            } else if probes == 1 {
                Status::Single
            } else {
                Status::Ok
            };
            Row {
                nozzle: name.to_string(),
                probes,
                mean_mm: mean,
                spread_mm: spread,
                offset_mm: offset,
                status,
            }
        })
        .collect();
    Ok(Report {
        rows,
        reference_mm: reference,
        tol_mm,
    })
}

impl Report {
    /// The tips that are not `Ok`.
    pub fn problems(&self) -> usize {
        self.rows.iter().filter(|r| r.status != Status::Ok).count()
    }

    /// The report for a person: a line for each tip and a line that sums it up.
    pub fn text(&self) -> String {
        let mut out = format!(
            "{} nozzles, the mean height is {:.4} mm, tolerance {} mm\n",
            self.rows.len(),
            self.reference_mm,
            self.tol_mm
        );
        for r in &self.rows {
            out.push_str(&format!(
                "{}: {} probings, height {:.4} mm, they differ by {:.4} mm, offset {:+.4} mm (correction {:+.4} mm) - {}\n",
                r.nozzle,
                r.probes,
                r.mean_mm,
                r.spread_mm,
                r.offset_mm,
                -r.offset_mm,
                r.status.name()
            ));
        }
        match self.problems() {
            0 => out.push_str("every nozzle is within the tolerance\n"),
            n => out.push_str(&format!("{n} nozzles need a look\n")),
        }
        out
    }

    /// The table `nozzle,probes,mean_mm,spread_mm,offset_mm,correction_mm,status`.
    pub fn csv(&self) -> String {
        let mut out =
            String::from("nozzle,probes,mean_mm,spread_mm,offset_mm,correction_mm,status\n");
        for r in &self.rows {
            out.push_str(&format!(
                "{},{},{:.4},{:.4},{:.4},{:.4},{}\n",
                csv_field(&r.nozzle),
                r.probes,
                r.mean_mm,
                r.spread_mm,
                r.offset_mm,
                -r.offset_mm,
                r.status.name()
            ));
        }
        out
    }
}

/// What a simulation is made of.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub nozzles: usize,
    pub probes: usize,
    /// The noise of one probing (the standard deviation), in millimetres.
    pub noise_mm: f64,
    /// The heights of the tips are made up between plus and minus half of this, in millimetres.
    pub spread_mm: f64,
    pub seed: u64,
}

impl Default for Plan {
    fn default() -> Plan {
        Plan {
            nozzles: 2,
            probes: 5,
            noise_mm: 0.005,
            spread_mm: 0.08,
            seed: 1,
        }
    }
}

/// The probings and the heights that were made up.
#[derive(Debug, Clone, PartialEq)]
pub struct Simulated {
    pub readings: Vec<Reading>,
    /// The height that each tip was given.
    pub truth: Vec<(String, f64)>,
}

fn normal(rng: &mut Rng) -> f64 {
    let u1 = rng.next_f64().max(1e-12);
    let u2 = rng.next_f64();
    (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
}

/// Makes up the heights of `plan.nozzles` tips (`N1`, `N2`, ...) and probes each one `plan.probes` times. The same plan gives the same readings.
pub fn simulate(plan: &Plan) -> Result<Simulated, PnpError> {
    let setting = |m: String| PnpError::new(Code::PlanSetting, m);
    if !(1..=MAX_NOZZLES).contains(&plan.nozzles) {
        return Err(setting(format!(
            "the number of nozzles must be from 1 to {MAX_NOZZLES}"
        )));
    }
    if !(1..=MAX_PROBES).contains(&plan.probes) {
        return Err(setting(format!(
            "the number of probings must be from 1 to {MAX_PROBES}"
        )));
    }
    if !plan.noise_mm.is_finite() || !(0.0..=1.0).contains(&plan.noise_mm) {
        return Err(setting("the noise must be from 0 to 1 mm".into()));
    }
    if !plan.spread_mm.is_finite() || !(0.0..=2.0).contains(&plan.spread_mm) {
        return Err(setting(
            "the spread of the heights must be from 0 to 2 mm".into(),
        ));
    }
    let mut rng = Rng::new(plan.seed);
    let mut readings = Vec::new();
    let mut truth = Vec::new();
    for n in 1..=plan.nozzles {
        let name = format!("N{n}");
        let height = (rng.next_f64() - 0.5) * plan.spread_mm;
        for _ in 0..plan.probes {
            readings.push(Reading {
                nozzle: name.clone(),
                z_mm: height + plan.noise_mm * normal(&mut rng),
            });
        }
        truth.push((name, height));
    }
    Ok(Simulated { readings, truth })
}

/// How far the offsets that were found are from the ones that were made up (the made-up heights minus their mean), the worst tip of all, in millimetres.
pub fn error_against(report: &Report, truth: &[(String, f64)]) -> f64 {
    let mean = truth.iter().map(|t| t.1).sum::<f64>() / truth.len().max(1) as f64;
    report
        .rows
        .iter()
        .filter_map(|r| {
            truth
                .iter()
                .find(|t| t.0 == r.nozzle)
                .map(|t| (r.offset_mm - (t.1 - mean)).abs())
        })
        .fold(0.0, f64::max)
}
