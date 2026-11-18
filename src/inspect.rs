// EH3D-PHOENIX-PNP - the look at the part that hangs on the nozzle.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! The look at the part on the nozzle: what the camera measured (the sides of the part, where it hangs and how much it is turned) against what the package says it should be, with
//! limits for the size, the turn and the distance from the axis. A part that is the wrong size is most likely the wrong part (another value in the tape, a part upside down or on its
//! side); a part that hangs too far from the axis, or is turned too much, will not land on its pads even after the correction. Plain arithmetic and text: no files, no screen.

use crate::catalog::Catalog;
use crate::error::{Code, PnpError};
use crate::parts::{detect_delimiter, read_records};
use crate::rotation::glob_match;

/// The most rows of a table of sizes or of readings.
pub const MAX_ROWS: usize = 100_000;
const MAX_SIDE_MM: f64 = 1000.0;

/// The limits of the look, from the configuration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Limits {
    /// How far either side of the part may be from its size in the table, in millimetres.
    pub size_tol_mm: f64,
    /// How far the turn may be from the expected one, in degrees.
    pub angle_tol_deg: f64,
    /// How far from the axis of the nozzle the part may hang, in millimetres.
    pub offset_max_mm: f64,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            size_tol_mm: 0.3,
            angle_tol_deg: 5.0,
            offset_max_mm: 2.0,
        }
    }
}

/// The size of the body of a package, the long side first.
#[derive(Debug, Clone, PartialEq)]
pub struct SizeRow {
    /// A package name or a pattern with `*` and `?`, any case.
    pub pattern: String,
    pub long_mm: f64,
    pub short_mm: f64,
}

/// The table of sizes by package.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SizeTable {
    pub rows: Vec<SizeRow>,
}

fn bad_size(message: impl Into<String>) -> PnpError {
    PnpError::new(Code::CatalogBadRow, message)
}

fn bad_read(message: impl Into<String>) -> PnpError {
    PnpError::new(Code::CalibrationBad, message)
}

fn header_of(text: &str, empty: &str) -> Result<(Vec<Vec<String>>, Vec<String>), String> {
    let text = text.trim_start_matches('\u{feff}');
    let head = text
        .lines()
        .find(|l| !l.trim().is_empty())
        .ok_or_else(|| empty.to_string())?;
    let mut records = read_records(text, detect_delimiter(head))?.into_iter();
    let header: Vec<String> = records
        .next()
        .ok_or_else(|| empty.to_string())?
        .iter()
        .map(|h| h.trim().to_ascii_lowercase())
        .collect();
    Ok((records.collect(), header))
}

fn column(header: &[String], name: &str) -> Option<usize> {
    header.iter().position(|h| h == name)
}

fn number(cell: &str, lo: f64, hi: f64) -> Option<f64> {
    cell.trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && (lo..=hi).contains(v))
}

impl SizeTable {
    /// A CSV with the header `package,long_mm,short_mm` (the columns in any order). The two sides may be given in either order; they are kept with the long one first.
    pub fn parse(text: &str) -> Result<SizeTable, PnpError> {
        let (records, header) = header_of(text, "the table of sizes is empty").map_err(bad_size)?;
        let need = |name: &str| {
            column(&header, name).ok_or_else(|| {
                bad_size(
                    "the first row of the sizes must hold the columns package,long_mm,short_mm",
                )
            })
        };
        let (p, l, s) = (need("package")?, need("long_mm")?, need("short_mm")?);
        let mut rows: Vec<SizeRow> = Vec::new();
        for (i, r) in records.iter().enumerate() {
            let row = i + 2;
            if r.len() != header.len() {
                return Err(bad_size(format!(
                    "size row {row}: {} fields, not {}",
                    r.len(),
                    header.len()
                )));
            }
            let pattern = r[p].trim().to_ascii_lowercase();
            if pattern.is_empty() || rows.iter().any(|x| x.pattern == pattern) {
                return Err(bad_size(format!(
                    "size row {row}: the package '{}' is empty or listed twice",
                    r[p]
                )));
            }
            let side = |cell: &str| {
                number(cell, 0.01, MAX_SIDE_MM)
                    .ok_or_else(|| bad_size(format!("size row {row}: '{cell}' is not a size")))
            };
            let (a, b) = (side(&r[l])?, side(&r[s])?);
            if rows.len() >= MAX_ROWS {
                return Err(bad_size(format!("more than {MAX_ROWS} sizes")));
            }
            rows.push(SizeRow {
                pattern,
                long_mm: a.max(b),
                short_mm: a.min(b),
            });
        }
        Ok(SizeTable { rows })
    }

    /// The sizes that a table of packages gives in its columns `body_long_mm` and `body_short_mm`, one row for each package that has them.
    pub fn from_catalog(catalog: &Catalog) -> SizeTable {
        SizeTable {
            rows: catalog
                .package_bodies()
                .into_iter()
                .map(|(name, long_mm, short_mm)| SizeRow {
                    pattern: name.trim().to_ascii_lowercase(),
                    long_mm,
                    short_mm,
                })
                .collect(),
        }
    }

    /// This table with the rows of `other` for the packages it has no row for: what this table says wins.
    pub fn with_missing_from(mut self, other: &SizeTable) -> SizeTable {
        for r in &other.rows {
            if !self.rows.iter().any(|x| x.pattern == r.pattern) {
                self.rows.push(r.clone());
            }
        }
        self
    }

    /// The size of a package: an exact name wins over a pattern, and among patterns the first row does.
    pub fn lookup(&self, package: &str) -> Option<&SizeRow> {
        let key = package.trim().to_ascii_lowercase();
        self.rows
            .iter()
            .find(|r| r.pattern == key)
            .or_else(|| self.rows.iter().find(|r| glob_match(&r.pattern, &key)))
    }
}

/// What the camera saw of the part on the nozzle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measured {
    pub long_mm: f64,
    pub short_mm: f64,
    /// Where the part hangs from the axis of the nozzle.
    pub dx_mm: f64,
    pub dy_mm: f64,
    /// How much the part is turned, in degrees.
    pub rotation_deg: f64,
}

impl Measured {
    /// From a reading of the vision pipeline and the size of a pixel on the part.
    pub fn from_reading(reading: &crate::vision::Reading, mm_per_px: f64) -> Measured {
        Measured {
            long_mm: reading.long_px * mm_per_px,
            short_mm: reading.short_px * mm_per_px,
            dx_mm: reading.offset.dx_mm,
            dy_mm: reading.offset.dy_mm,
            rotation_deg: reading.offset.rotation_deg,
        }
    }
}

/// Why a part is refused.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Reason {
    /// The long side is this many millimetres off (positive: too long).
    Long(f64),
    Short(f64),
    /// The part is turned this many degrees from the expected angle (the symmetry of the part counted).
    Turn(f64),
    /// The middle of the part is this far from the axis of the nozzle.
    Offset(f64),
}

impl Reason {
    pub fn text(self) -> String {
        match self {
            Reason::Long(d) => format!("long side {d:+.3} mm off"),
            Reason::Short(d) => format!("short side {d:+.3} mm off"),
            Reason::Turn(d) => format!("turned {d:+.2} degrees off"),
            Reason::Offset(d) => format!("hangs {d:.3} mm from the axis"),
        }
    }
}

/// The result of the look at one part.
#[derive(Debug, Clone, PartialEq)]
pub struct Inspection {
    pub long_err_mm: f64,
    pub short_err_mm: f64,
    /// The turn off the expected angle, from the smallest turn that the symmetry of the part allows (a rectangle looks the same every 180 degrees, a square every 90).
    pub turn_err_deg: f64,
    pub offset_mm: f64,
    pub reasons: Vec<Reason>,
}

impl Inspection {
    pub fn accepted(&self) -> bool {
        self.reasons.is_empty()
    }
}

/// A number brought to within half a period either side of 0.
fn wrap(angle: f64, period: f64) -> f64 {
    let r = angle.rem_euclid(period);
    if r > period / 2.0 {
        r - period
    } else {
        r
    }
}

/// Compares a measurement with the size of the package and the angle the part should hang at (0 when the camera is set so that the long side lies along the axis).
pub fn inspect(
    size: &SizeRow,
    expected_rotation_deg: f64,
    seen: &Measured,
    limits: &Limits,
) -> Inspection {
    // the sides of what was seen, the long one first (a reading may give them the other way round on a nearly square part)
    let (long, short) = (
        seen.long_mm.max(seen.short_mm),
        seen.long_mm.min(seen.short_mm),
    );
    let long_err = long - size.long_mm;
    let short_err = short - size.short_mm;
    // a part whose sides differ by less than the tolerance has no long side to tell apart: it looks the same every 90 degrees
    let period = if (size.long_mm - size.short_mm).abs() <= limits.size_tol_mm {
        90.0
    } else {
        180.0
    };
    let turn = wrap(seen.rotation_deg - expected_rotation_deg, period);
    let offset = seen.dx_mm.hypot(seen.dy_mm);
    let mut reasons = Vec::new();
    if long_err.abs() > limits.size_tol_mm {
        reasons.push(Reason::Long(long_err));
    }
    if short_err.abs() > limits.size_tol_mm {
        reasons.push(Reason::Short(short_err));
    }
    if turn.abs() > limits.angle_tol_deg {
        reasons.push(Reason::Turn(turn));
    }
    if offset > limits.offset_max_mm {
        reasons.push(Reason::Offset(offset));
    }
    Inspection {
        long_err_mm: long_err,
        short_err_mm: short_err,
        turn_err_deg: turn,
        offset_mm: offset,
        reasons,
    }
}

/// One line of a file of readings.
#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    pub reference: String,
    pub package: String,
    pub seen: Measured,
    pub expected_rotation_deg: f64,
}

/// A CSV of readings with the header `ref,package,long_mm,short_mm,dx_mm,dy_mm,rot_deg` and, optionally, `exp_rot_deg` (0 when it is missing); the columns in any order.
pub fn parse_readings(text: &str) -> Result<Vec<Reading>, PnpError> {
    let (records, header) = header_of(text, "the file of readings is empty").map_err(bad_read)?;
    let need = |name: &str| {
        column(&header, name).ok_or_else(|| {
            bad_read("the first row of the readings must hold ref,package,long_mm,short_mm,dx_mm,dy_mm,rot_deg")
        })
    };
    let (rf, pk) = (need("ref")?, need("package")?);
    let (lg, sh) = (need("long_mm")?, need("short_mm")?);
    let (dx, dy, rt) = (need("dx_mm")?, need("dy_mm")?, need("rot_deg")?);
    let er = column(&header, "exp_rot_deg");
    let mut out: Vec<Reading> = Vec::new();
    for (i, r) in records.iter().enumerate() {
        let row = i + 2;
        if r.len() != header.len() {
            return Err(bad_read(format!(
                "reading row {row}: {} fields, not {}",
                r.len(),
                header.len()
            )));
        }
        if r[rf].trim().is_empty() || out.iter().any(|x| x.reference == r[rf].trim()) {
            return Err(bad_read(format!(
                "reading row {row}: the reference '{}' is empty or listed twice",
                r[rf]
            )));
        }
        if out.len() >= MAX_ROWS {
            return Err(bad_read(format!("more than {MAX_ROWS} readings")));
        }
        let n = |c: usize, lo: f64, hi: f64| {
            number(&r[c], lo, hi).ok_or_else(|| {
                bad_read(format!(
                    "reading row {row}: '{}' is not a number from {lo} to {hi}",
                    r[c]
                ))
            })
        };
        out.push(Reading {
            reference: r[rf].trim().to_string(),
            package: r[pk].trim().to_string(),
            seen: Measured {
                long_mm: n(lg, 0.0, MAX_SIDE_MM)?,
                short_mm: n(sh, 0.0, MAX_SIDE_MM)?,
                dx_mm: n(dx, -MAX_SIDE_MM, MAX_SIDE_MM)?,
                dy_mm: n(dy, -MAX_SIDE_MM, MAX_SIDE_MM)?,
                rotation_deg: n(rt, -3600.0, 3600.0)?,
            },
            expected_rotation_deg: match er {
                Some(c) => n(c, -3600.0, 3600.0)?,
                None => 0.0,
            },
        });
    }
    Ok(out)
}

/// The result for one reading: `None` when the table has no size for its package (the part cannot be judged, which is not the same as accepted).
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub reference: String,
    pub package: String,
    pub inspection: Option<Inspection>,
}

/// Judges every reading against the table.
pub fn judge(readings: &[Reading], table: &SizeTable, limits: &Limits) -> Vec<Row> {
    readings
        .iter()
        .map(|r| Row {
            reference: r.reference.clone(),
            package: r.package.clone(),
            inspection: table
                .lookup(&r.package)
                .map(|s| inspect(s, r.expected_rotation_deg, &r.seen, limits)),
        })
        .collect()
}

/// How many rows were accepted, refused and could not be judged.
pub fn tally(rows: &[Row]) -> (usize, usize, usize) {
    let mut t = (0, 0, 0);
    for r in rows {
        match &r.inspection {
            Some(i) if i.accepted() => t.0 += 1,
            Some(_) => t.1 += 1,
            None => t.2 += 1,
        }
    }
    t
}

/// The rows as a CSV: `ref,package,verdict,long_err_mm,short_err_mm,turn_err_deg,offset_mm,reasons` (the verdict is `ok`, `reject` or `unknown`).
pub fn rows_csv(rows: &[Row]) -> String {
    let mut out = String::from(
        "ref,package,verdict,long_err_mm,short_err_mm,turn_err_deg,offset_mm,reasons\n",
    );
    for r in rows {
        let field = |t: &str| crate::parts::csv_field(t);
        match &r.inspection {
            Some(i) => out.push_str(&format!(
                "{},{},{},{:.3},{:.3},{:.2},{:.3},{}\n",
                field(&r.reference),
                field(&r.package),
                if i.accepted() { "ok" } else { "reject" },
                i.long_err_mm,
                i.short_err_mm,
                i.turn_err_deg,
                i.offset_mm,
                field(
                    &i.reasons
                        .iter()
                        .map(|x| x.text())
                        .collect::<Vec<_>>()
                        .join(" | ")
                )
            )),
            None => out.push_str(&format!(
                "{},{},unknown,,,,,{}\n",
                field(&r.reference),
                field(&r.package),
                field("no size for this package")
            )),
        }
    }
    out
}
