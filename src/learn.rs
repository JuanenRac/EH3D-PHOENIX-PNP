// EH3D-PHOENIX-PNP - the offset of the placement of each package, learned from readings.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! A machine puts the parts of a package a little off in the same way every time (a nozzle that holds them off-centre, a tape that lets them sit to one side). When the place of the parts is
//! measured after the job, the mean error of a package is an offset that the next job can take out. The readings are rows `package,dx_mm,dy_mm`: how far the part ended from where it should be.
//!
//! What is learned is kept in a file of its own. Nothing moves by itself: learning is off until `place_learn` is on, and the offsets reach a job only when the person gives the file to `simulate`
//! and `place_learn_apply` is on. The learning is careful about what it takes in: a reading that lies far from the others of its package (the median and the spread of the package decide,
//! `place_learn_outlier`) is left out, a package with fewer readings than `place_learn_min` is not touched, the offsets are old measures that count for at most `place_learn_memory` readings so that
//! the machine can drift, and no offset is ever larger than `place_learn_cap_mm`. The file carries a check number, so a file that was cut or changed by hand is found out and refused, and it is
//! written beside and renamed, so that a failure leaves the old one whole. Every error carries the code `PHOENIX-PNP-058`.

use crate::config::MachineConfig;
use crate::error::{Code, PnpError};
use crate::parts::{detect_delimiter, read_records_numbered};
use serde_json::{json, Value};

/// The version of the file.
pub const SCHEMA_VERSION: u64 = 1;
/// The most readings of one file and the most packages of a state.
pub const MAX_READINGS: usize = 100_000;
pub const MAX_PACKAGES: usize = 1000;
/// The largest error of one reading that is believed, in millimetres either way.
pub const MAX_READING_MM: f64 = 5.0;
/// With fewer readings than this a package has no outliers taken out: there is nothing to tell them from.
const OUTLIER_MIN_READINGS: usize = 5;
/// The least spread (millimetres) that the outlier rule assumes, so that readings that agree to the micron do not make the rule reject a reading that is a hair away.
const SPREAD_FLOOR_MM: f64 = 0.02;

fn bad(detail: impl Into<String>) -> PnpError {
    PnpError::new(Code::CalibrationBad, detail)
}

/// Whether learning may be done: it is off until `place_learn` is on. The reason in words when it is off.
pub fn allowed(cfg: &MachineConfig) -> Result<(), String> {
    if cfg.place_learn {
        Ok(())
    } else {
        Err("learning the offsets of the placement is off: set place_learn = on in the configuration".to_string())
    }
}

/// Whether the learned offsets may be applied to a job: off until `place_learn_apply` is on.
pub fn apply_allowed(cfg: &MachineConfig) -> Result<(), String> {
    if cfg.place_learn_apply {
        Ok(())
    } else {
        Err(
            "applying the learned offsets is off: set place_learn_apply = on in the configuration"
                .to_string(),
        )
    }
}

/// One reading: how far a part of the package ended from where it should be.
#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    pub package: String,
    pub dx_mm: f64,
    pub dy_mm: f64,
}

fn clean_name(name: &str) -> bool {
    !name.is_empty() && name.chars().count() <= 64 && !name.chars().any(char::is_control)
}

/// Reads a file of readings: the columns `package,dx_mm,dy_mm`, one row for each, with an optional header and comment lines (`#`). At most `MAX_READINGS`, each within `MAX_READING_MM`.
pub fn parse_readings(text: &str) -> Result<Vec<Reading>, PnpError> {
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
    let mut out: Vec<Reading> = Vec::new();
    for (i, (line, record)) in records.into_iter().enumerate() {
        if i == 0 && record.get(1).is_some_and(|c| c.parse::<f64>().is_err()) {
            continue;
        }
        if record.len() != 3 {
            return Err(bad(format!(
                "line {line}: a reading needs 3 columns (package,dx_mm,dy_mm), this row has {}",
                record.len()
            )));
        }
        if !clean_name(&record[0]) {
            return Err(bad(format!(
                "line {line}: the name of the package is empty, longer than 64 characters or has a control character"
            )));
        }
        let mut v = [0.0; 2];
        for (slot, cell) in v.iter_mut().zip(&record[1..]) {
            let n: f64 = cell
                .parse()
                .map_err(|_| bad(format!("line {line}: '{cell}' is not a number")))?;
            if !n.is_finite() || n.abs() > MAX_READING_MM {
                return Err(bad(format!(
                    "line {line}: {cell} mm is not a believable error of a placement (within {MAX_READING_MM} mm)"
                )));
            }
            *slot = n;
        }
        if out.len() >= MAX_READINGS {
            return Err(bad(format!(
                "line {line}: more than {MAX_READINGS} readings"
            )));
        }
        out.push(Reading {
            package: record[0].clone(),
            dx_mm: v[0],
            dy_mm: v[1],
        });
    }
    Ok(out)
}

/// How the learning is done.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LearnSettings {
    /// No offset is larger than this, in millimetres either way.
    pub cap_mm: f64,
    /// A reading farther from the median than this many spreads is left out.
    pub outlier_k: f64,
    /// The fewest readings of a package that are used.
    pub min_readings: u32,
    /// How many readings the offset that is already known counts for.
    pub memory: u32,
}

impl LearnSettings {
    /// The settings of the configuration (`place_learn_cap_mm`, `place_learn_outlier`, `place_learn_min`, `place_learn_memory`).
    pub fn from_config(cfg: &MachineConfig) -> LearnSettings {
        LearnSettings {
            cap_mm: cfg.place_learn_cap_mm,
            outlier_k: cfg.place_learn_outlier,
            min_readings: cfg.place_learn_min,
            memory: cfg.place_learn_memory,
        }
    }
}

/// What is known of one package.
#[derive(Debug, Clone, PartialEq)]
pub struct PackageOffset {
    pub package: String,
    /// The mean error of the parts of the package: the head goes to the place less this.
    pub dx_mm: f64,
    pub dy_mm: f64,
    /// The readings that were used and the ones that were left out, in all.
    pub used: u32,
    pub left_out: u32,
}

/// What a run of learning did with one package.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub package: String,
    pub readings: usize,
    pub outliers: usize,
    /// The offset before and after, and whether the cap held it.
    pub before: (f64, f64),
    pub after: (f64, f64),
    pub capped: bool,
    /// Why the package was not touched, when it was not.
    pub skipped: Option<String>,
}

impl Outcome {
    /// A line for a person.
    pub fn line(&self) -> String {
        match &self.skipped {
            Some(why) => format!("{}: not changed, {why}", self.package),
            None => format!(
                "{}: {} readings, {} left out as strays; offset X {:+.4} Y {:+.4} mm (was X {:+.4} Y {:+.4}){}",
                self.package,
                self.readings,
                self.outliers,
                self.after.0,
                self.after.1,
                self.before.0,
                self.before.1,
                if self.capped { "; held at the cap" } else { "" }
            ),
        }
    }
}

fn median(sorted: &[f64]) -> f64 {
    let n = sorted.len();
    if n == 0 {
        0.0
    } else if n % 2 == 1 {
        sorted[n / 2]
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    }
}

fn sorted(values: &[f64]) -> Vec<f64> {
    let mut v = values.to_vec();
    v.sort_by(f64::total_cmp);
    v
}

/// The median and the spread of a set of values: 1.4826 times the median of the distances from the median (which is the standard deviation of a bell), never below the floor.
fn centre_and_spread(values: &[f64]) -> (f64, f64) {
    let m = median(&sorted(values));
    let dist: Vec<f64> = values.iter().map(|v| (v - m).abs()).collect();
    (m, (1.4826 * median(&sorted(&dist))).max(SPREAD_FLOOR_MM))
}

fn round6(v: f64) -> f64 {
    (v * 1e6).round() / 1e6
}

/// What has been learned: an offset for each package.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Learned {
    pub rows: Vec<PackageOffset>,
}

impl Learned {
    pub fn new() -> Learned {
        Learned::default()
    }

    /// The offset of a package (any case), or `None` when nothing is known of it.
    pub fn offset_of(&self, package: &str) -> Option<(f64, f64)> {
        self.rows
            .iter()
            .find(|r| r.package.eq_ignore_ascii_case(package.trim()))
            .map(|r| (r.dx_mm, r.dy_mm))
    }

    /// Takes the readings in: for each package, the strays are left out, the mean of the rest is joined to the offset that is known (weighing that one as at most `memory` readings) and the
    /// result is held within the cap. With `residual` the readings were taken on parts placed with the offsets already applied, so the offset that is known is added back to each reading first.
    /// A package with too few readings is not changed. Gives what was done with each package, in the order they first appear in the readings.
    pub fn learn(
        &mut self,
        readings: &[Reading],
        settings: &LearnSettings,
        residual: bool,
    ) -> Result<Vec<Outcome>, PnpError> {
        let mut order: Vec<String> = Vec::new();
        for r in readings {
            if !order.iter().any(|p| p.eq_ignore_ascii_case(&r.package)) {
                order.push(r.package.clone());
            }
        }
        let new_packages = order.iter().filter(|p| self.offset_of(p).is_none()).count();
        if self.rows.len() + new_packages > MAX_PACKAGES {
            return Err(bad(format!("more than {MAX_PACKAGES} packages")));
        }
        let mut outcomes = Vec::new();
        for name in order {
            let known = self.offset_of(&name).unwrap_or((0.0, 0.0));
            let (xs, ys): (Vec<f64>, Vec<f64>) = readings
                .iter()
                .filter(|r| r.package.eq_ignore_ascii_case(&name))
                .map(|r| {
                    if residual {
                        (r.dx_mm + known.0, r.dy_mm + known.1)
                    } else {
                        (r.dx_mm, r.dy_mm)
                    }
                })
                .unzip();
            let n = xs.len();
            let mut outcome = Outcome {
                package: name.clone(),
                readings: n,
                outliers: 0,
                before: known,
                after: known,
                capped: false,
                skipped: None,
            };
            let keep: Vec<bool> = if n >= OUTLIER_MIN_READINGS {
                let (mx, sx) = centre_and_spread(&xs);
                let (my, sy) = centre_and_spread(&ys);
                (0..n)
                    .map(|i| {
                        (xs[i] - mx).abs() <= settings.outlier_k * sx
                            && (ys[i] - my).abs() <= settings.outlier_k * sy
                    })
                    .collect()
            } else {
                vec![true; n]
            };
            let accepted = keep.iter().filter(|k| **k).count();
            outcome.outliers = n - accepted;
            if accepted < settings.min_readings as usize {
                outcome.skipped = Some(if outcome.outliers > 0 {
                    format!(
                        "{accepted} readings are left after {} strays, at least {} are needed",
                        outcome.outliers, settings.min_readings
                    )
                } else {
                    format!(
                        "{n} readings, at least {} are needed",
                        settings.min_readings
                    )
                });
                outcomes.push(outcome);
                continue;
            }
            let (mut sum_x, mut sum_y) = (0.0, 0.0);
            for i in (0..n).filter(|i| keep[*i]) {
                sum_x += xs[i];
                sum_y += ys[i];
            }
            let (mean_x, mean_y) = (sum_x / accepted as f64, sum_y / accepted as f64);
            let (old_used, found) = match self
                .rows
                .iter()
                .find(|r| r.package.eq_ignore_ascii_case(&name))
            {
                Some(r) => (r.used.min(settings.memory), true),
                None => (0, false),
            };
            let (w_old, w_new) = (f64::from(old_used), accepted as f64);
            let joined = (
                (known.0 * w_old + mean_x * w_new) / (w_old + w_new),
                (known.1 * w_old + mean_y * w_new) / (w_old + w_new),
            );
            let held = (
                joined.0.clamp(-settings.cap_mm, settings.cap_mm),
                joined.1.clamp(-settings.cap_mm, settings.cap_mm),
            );
            outcome.capped = held != joined;
            outcome.after = (round6(held.0), round6(held.1));
            let (used, left_out) = (accepted as u32, outcome.outliers as u32);
            if found {
                if let Some(r) = self
                    .rows
                    .iter_mut()
                    .find(|r| r.package.eq_ignore_ascii_case(&name))
                {
                    r.dx_mm = outcome.after.0;
                    r.dy_mm = outcome.after.1;
                    r.used = r.used.saturating_add(used);
                    r.left_out = r.left_out.saturating_add(left_out);
                }
            } else {
                self.rows.push(PackageOffset {
                    package: name,
                    dx_mm: outcome.after.0,
                    dy_mm: outcome.after.1,
                    used,
                    left_out,
                });
            }
            outcomes.push(outcome);
        }
        Ok(outcomes)
    }

    /// Forgets what is known of a package (any case). False when nothing was known.
    pub fn forget(&mut self, package: &str) -> bool {
        let before = self.rows.len();
        self.rows
            .retain(|r| !r.package.eq_ignore_ascii_case(package.trim()));
        self.rows.len() != before
    }

    /// The offsets as a job takes them, `(package, dx, dy)`, held within `cap_mm` (the cap of the configuration now, which may be smaller than the one the file was made under).
    pub fn for_job(&self, cap_mm: f64) -> Vec<(String, f64, f64)> {
        self.rows
            .iter()
            .map(|r| {
                (
                    r.package.clone(),
                    r.dx_mm.clamp(-cap_mm, cap_mm),
                    r.dy_mm.clamp(-cap_mm, cap_mm),
                )
            })
            .collect()
    }

    /// The check number of the rows: FNV-1a over a plain text of them.
    fn check(&self) -> String {
        let mut text = String::new();
        for r in &self.rows {
            text.push_str(&format!(
                "{}|{:.6}|{:.6}|{}|{};",
                r.package.to_ascii_lowercase(),
                r.dx_mm,
                r.dy_mm,
                r.used,
                r.left_out
            ));
        }
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in text.bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        format!("{h:016x}")
    }

    /// The file: the offsets as JSON with the check number.
    pub fn to_text(&self) -> String {
        let rows: Vec<Value> = self
            .rows
            .iter()
            .map(|r| {
                json!({"package": r.package, "dx_mm": r.dx_mm, "dy_mm": r.dy_mm, "used": r.used, "left_out": r.left_out})
            })
            .collect();
        let doc = json!({
            "schema_version": SCHEMA_VERSION,
            "kind": "place-offsets",
            "packages": rows,
            "check": self.check()
        });
        serde_json::to_string_pretty(&doc).unwrap_or_default() + "\n"
    }

    /// Reads the file back. One that is not JSON, is of another kind or a newer version, has a package twice, a number that is not finite or beyond `MAX_READING_MM`, or a check number that
    /// does not match the rows is refused with its reason, and so is one that is too big.
    pub fn parse(text: &str) -> Result<Learned, PnpError> {
        if text.len() > 4 * 1024 * 1024 {
            return Err(bad("the file of the learned offsets is bigger than 4 MiB"));
        }
        let v: Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| {
            bad(format!(
                "the file of the learned offsets is not valid JSON: {e}"
            ))
        })?;
        match v.get("schema_version").and_then(Value::as_u64) {
            Some(SCHEMA_VERSION) => {}
            Some(n) if n > SCHEMA_VERSION => {
                return Err(bad(format!(
                    "the file of the learned offsets is of version {n}, newer than this program reads ({SCHEMA_VERSION})"
                )))
            }
            _ => return Err(bad("schema_version of the learned offsets is not 1")),
        }
        if v.get("kind").and_then(Value::as_str) != Some("place-offsets") {
            return Err(bad(
                "this is not a file of learned offsets (kind is not place-offsets)",
            ));
        }
        let packages = v
            .get("packages")
            .and_then(Value::as_array)
            .ok_or_else(|| bad("the file of the learned offsets has no list of packages"))?;
        if packages.len() > MAX_PACKAGES {
            return Err(bad(format!("more than {MAX_PACKAGES} packages")));
        }
        let mut rows: Vec<PackageOffset> = Vec::new();
        for p in packages {
            let name = p
                .get("package")
                .and_then(Value::as_str)
                .filter(|n| clean_name(n))
                .ok_or_else(|| bad("a package of the learned offsets has no usable name"))?;
            let num = |key: &str| -> Result<f64, PnpError> {
                p.get(key)
                    .and_then(Value::as_f64)
                    .filter(|n| n.is_finite() && n.abs() <= MAX_READING_MM)
                    .ok_or_else(|| {
                        bad(format!(
                            "{name}: {key} is not a number within {MAX_READING_MM} mm"
                        ))
                    })
            };
            let count = |key: &str| -> Result<u32, PnpError> {
                p.get(key)
                    .and_then(Value::as_u64)
                    .and_then(|n| u32::try_from(n).ok())
                    .ok_or_else(|| bad(format!("{name}: {key} is not a count")))
            };
            if rows.iter().any(|r| r.package.eq_ignore_ascii_case(name)) {
                return Err(bad(format!("the package '{name}' is in the file twice")));
            }
            rows.push(PackageOffset {
                package: name.to_string(),
                dx_mm: num("dx_mm")?,
                dy_mm: num("dy_mm")?,
                used: count("used")?,
                left_out: count("left_out")?,
            });
        }
        let learned = Learned { rows };
        if v.get("check").and_then(Value::as_str) != Some(learned.check().as_str()) {
            return Err(bad(
                "the check number of the learned offsets does not match: the file was cut or changed by hand; move it away and learn again",
            ));
        }
        Ok(learned)
    }

    /// The offsets in lines for a person.
    pub fn summary(&self) -> String {
        if self.rows.is_empty() {
            return "nothing is learned yet".to_string();
        }
        self.rows
            .iter()
            .map(|r| {
                format!(
                    "{}: X {:+.4} Y {:+.4} mm from {} readings ({} left out)",
                    r.package, r.dx_mm, r.dy_mm, r.used, r.left_out
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}
