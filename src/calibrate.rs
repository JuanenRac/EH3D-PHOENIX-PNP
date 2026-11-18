//! The calibrations that turn measurements of a real machine into settings of the configuration: the camera (how its picture lies on the machine) and the nozzle (how far the tip is from
//! the axis it turns about). Both take points that a person measured and give the values of the keys to write in the file of the machine, with how well the points agree with them.
//! Nothing here moves a machine.
//!
//! * **Camera.** The head is moved to known places, the same mark is found in the picture each time, and a line of the points file gives where it was seen (`px`, `py`: pixels from the
//!   middle of the picture, the second axis pointing up) and where the head was (`x_mm`, `y_mm`, relative to the axis of the nozzle). An affine fit gives the size of a pixel, the turn,
//!   the mirror and the place of the axis of the nozzle in the picture; the difference between the two sizes and the angle between the axes of the picture say how square it is.
//! * **Nozzle.** The nozzle is turned to several angles and the place of its tip is read each time. An eccentric tip draws a circle: its centre is the axis the nozzle turns about, its
//!   radius the runout, and the angle at which the tip is on the positive X side of the axis when the nozzle is at 0 degrees is the phase.
//!
//! Every error carries the code `PHOENIX-PNP-058` and, for a file, the number of its line.

use crate::camera::CameraModel;
use crate::error::{Code, PnpError};
use crate::fiducial::{fit_points, FitMode};
use crate::parts::{detect_delimiter, read_records_numbered};

/// The most points of a file of either calibration.
pub const MAX_POINTS: usize = 100;
const MAX_MM: f64 = 10_000.0;

fn bad(detail: impl Into<String>) -> PnpError {
    PnpError::new(Code::CalibrationBad, detail)
}

fn bad_at(line: usize, detail: impl Into<String>) -> PnpError {
    bad(format!("line {line}: {}", detail.into()))
}

/// The rows of numbers of a points file: `columns` numbers per row, a header row (the first row, when its first cell is not a number) and comment lines (`#`) are left out.
fn number_rows(text: &str, columns: usize, what: &str) -> Result<Vec<(usize, Vec<f64>)>, PnpError> {
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
        .map_err(|(line, e)| bad_at(line, e))?;
    let mut rows = Vec::new();
    for (i, (line, record)) in records.into_iter().enumerate() {
        if i == 0 && record.first().is_some_and(|c| c.parse::<f64>().is_err()) {
            continue;
        }
        if record.len() != columns {
            return Err(bad_at(
                line,
                format!(
                    "a {what} needs {columns} numbers, this row has {}",
                    record.len()
                ),
            ));
        }
        let mut numbers = Vec::with_capacity(columns);
        for cell in &record {
            let v: f64 = cell
                .parse()
                .map_err(|_| bad_at(line, format!("'{cell}' is not a number")))?;
            if !v.is_finite() || v.abs() > MAX_MM {
                return Err(bad_at(line, format!("'{cell}' is not a usable number")));
            }
            numbers.push(v);
        }
        if rows.len() >= MAX_POINTS {
            return Err(bad_at(line, format!("more than {MAX_POINTS} points")));
        }
        rows.push((line, numbers));
    }
    Ok(rows)
}

/// One sighting of a mark: where it was in the picture and where the head was.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraPoint {
    pub px: f64,
    pub py: f64,
    pub x_mm: f64,
    pub y_mm: f64,
}

/// Reads a file of sightings: the columns `px,py,x_mm,y_mm`, one row for each, with an optional header.
pub fn parse_camera_points(text: &str) -> Result<Vec<CameraPoint>, PnpError> {
    Ok(number_rows(text, 4, "sighting")?
        .into_iter()
        .map(|(_, v)| CameraPoint {
            px: v[0],
            py: v[1],
            x_mm: v[2],
            y_mm: v[3],
        })
        .collect())
}

/// What the camera calibration found.
#[derive(Debug, Clone, PartialEq)]
pub struct CameraCalibration {
    /// The model with the size of a pixel, the offset, the turn and the mirror; the lens keys are left at what the ideal lens has.
    pub model: CameraModel,
    /// The size of a pixel along each axis of the picture, in millimetres; the model takes their mean.
    pub size_x_mm: f64,
    pub size_y_mm: f64,
    /// How far the axes of the picture are from being at right angles, in degrees.
    pub skew_deg: f64,
    /// How far the points are from where the model puts them: root mean square and largest, in millimetres.
    pub rms_mm: f64,
    pub max_mm: f64,
    pub points: usize,
}

fn norm_deg(a: f64) -> f64 {
    let mut a = a % 360.0;
    if a > 180.0 {
        a -= 360.0;
    } else if a <= -180.0 {
        a += 360.0;
    }
    a
}

/// Fits the camera to the sightings (at least three, not on a line).
pub fn calibrate_camera(points: &[CameraPoint]) -> Result<CameraCalibration, PnpError> {
    if points.len() < 3 {
        return Err(bad("at least three sightings are needed"));
    }
    let design: Vec<(f64, f64)> = points.iter().map(|p| (p.px, p.py)).collect();
    let measured: Vec<(f64, f64)> = points.iter().map(|p| (p.x_mm, p.y_mm)).collect();
    let fit = fit_points(&design, &measured, FitMode::Affine)
        .map_err(|e| bad(format!("the sightings do not fix a camera: {}", e.detail)))?;
    let t = fit.transform;
    // the model turns after it mirrors, so a negative determinant is a mirror upside down; the columns of what is left are the two axes of the picture in the frame of the machine
    let flip_y = t.a * t.d - t.b * t.c < 0.0;
    let (b, d) = if flip_y { (-t.b, -t.d) } else { (t.b, t.d) };
    let (size_x, size_y) = (t.a.hypot(t.c), b.hypot(d));
    if size_x < 1e-12 || size_y < 1e-12 {
        return Err(bad("the sightings give a picture of no size"));
    }
    let cos = ((t.a * b + t.c * d) / (size_x * size_y)).clamp(-1.0, 1.0);
    let units = 0.5 * (size_x + size_y);
    let model = CameraModel {
        units_per_px: units,
        offset_x_mm: -t.tx,
        offset_y_mm: -t.ty,
        rotation_deg: norm_deg(t.c.atan2(t.a).to_degrees()),
        flip_x: false,
        flip_y,
        lens_scale: 1.0,
        lens_k1: 0.0,
    };
    if !(0.001..=10.0).contains(&units) {
        return Err(bad(format!(
            "a pixel of {units:.6} mm is outside the range of camera_units_per_px (0.001 to 10)"
        )));
    }
    if model.offset_x_mm.abs() > 500.0 || model.offset_y_mm.abs() > 500.0 {
        return Err(bad(format!(
            "the axis of the nozzle is at {:.3} {:.3} mm from the middle of the picture, outside the range of the offsets (-500 to 500)",
            model.offset_x_mm, model.offset_y_mm
        )));
    }
    // how far the points are from the model itself (a square pixel, no shear), which is what the settings can say
    let errors: Vec<f64> = points
        .iter()
        .map(|p| {
            let m = model.to_machine((p.px, p.py), units);
            (m.0 - p.x_mm).hypot(m.1 - p.y_mm)
        })
        .collect();
    let rms = (errors.iter().map(|e| e * e).sum::<f64>() / errors.len() as f64).sqrt();
    let max = errors.iter().copied().fold(0.0, f64::max);
    Ok(CameraCalibration {
        model,
        size_x_mm: size_x,
        size_y_mm: size_y,
        skew_deg: cos.acos().to_degrees() - 90.0,
        rms_mm: rms,
        max_mm: max,
        points: points.len(),
    })
}

impl CameraCalibration {
    /// The lines of the configuration that set what was found.
    pub fn config_lines(&self) -> String {
        let m = &self.model;
        let onoff = |b: bool| if b { "on" } else { "off" };
        format!(
            "camera_units_per_px = {:.6}\ncamera_offset_x_mm = {:.4}\ncamera_offset_y_mm = {:.4}\ncamera_rotation_deg = {:.4}\ncamera_flip_x = {}\ncamera_flip_y = {}\n",
            m.units_per_px,
            m.offset_x_mm,
            m.offset_y_mm,
            m.rotation_deg,
            onoff(m.flip_x),
            onoff(m.flip_y)
        )
    }

    /// A line that says how good the calibration is.
    pub fn summary(&self) -> String {
        format!(
            "{} sightings: a pixel is {:.6} mm along X and {:.6} mm along Y, the axes are {:+.3} degrees from being square, the points are {:.4} mm (rms) and {:.4} mm (largest) from the model",
            self.points, self.size_x_mm, self.size_y_mm, self.skew_deg, self.rms_mm, self.max_mm
        )
    }
}

/// One reading of the tip: the angle the nozzle was turned to and where the tip was.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RunoutSample {
    pub angle_deg: f64,
    pub x_mm: f64,
    pub y_mm: f64,
}

/// Reads a file of readings: the columns `angle_deg,x_mm,y_mm`, one row for each, with an optional header.
pub fn parse_runout_samples(text: &str) -> Result<Vec<RunoutSample>, PnpError> {
    Ok(number_rows(text, 3, "reading")?
        .into_iter()
        .map(|(_, v)| RunoutSample {
            angle_deg: v[0],
            x_mm: v[1],
            y_mm: v[2],
        })
        .collect())
}

/// The runout of a nozzle tip.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NozzleRunout {
    /// The axis the nozzle turns about, where the readings were taken.
    pub axis: (f64, f64),
    /// How far the tip is from the axis.
    pub radius_mm: f64,
    /// The direction of the tip from the axis when the nozzle is at 0 degrees.
    pub phase_deg: f64,
    pub rms_mm: f64,
    pub max_mm: f64,
    pub samples: usize,
}

impl NozzleRunout {
    /// Where the tip is with respect to the axis when the nozzle is turned to `angle_deg`.
    pub fn offset(&self, angle_deg: f64) -> (f64, f64) {
        offset_of(self.radius_mm, self.phase_deg, angle_deg)
    }

    /// The lines of the configuration that set what was found.
    pub fn config_lines(&self) -> String {
        format!(
            "nozzle_runout_mm = {:.4}\nnozzle_runout_deg = {:.3}\n",
            self.radius_mm, self.phase_deg
        )
    }

    pub fn summary(&self) -> String {
        format!(
            "{} readings: the axis is at X {:.4} Y {:.4}, the tip is {:.4} mm from it towards {:.2} degrees at 0 degrees, the readings are {:.4} mm (rms) and {:.4} mm (largest) from the circle",
            self.samples, self.axis.0, self.axis.1, self.radius_mm, self.phase_deg, self.rms_mm, self.max_mm
        )
    }
}

/// The place of an eccentric tip with respect to the axis, for a nozzle turned `angle_deg`.
pub fn offset_of(radius_mm: f64, phase_deg: f64, angle_deg: f64) -> (f64, f64) {
    let a = (phase_deg + angle_deg).to_radians();
    (radius_mm * a.cos(), radius_mm * a.sin())
}

/// Fits a circle to the readings of the tip (at least three, at angles that cover at least 90 degrees).
pub fn fit_runout(samples: &[RunoutSample]) -> Result<NozzleRunout, PnpError> {
    let n = samples.len();
    if n < 3 {
        return Err(bad("at least three readings are needed"));
    }
    let mut angles: Vec<f64> = samples
        .iter()
        .map(|s| s.angle_deg.rem_euclid(360.0))
        .collect();
    angles.sort_by(f64::total_cmp);
    let mut biggest_gap = angles[0] + 360.0 - angles[n - 1];
    for w in angles.windows(2) {
        biggest_gap = biggest_gap.max(w[1] - w[0]);
    }
    if 360.0 - biggest_gap < 90.0 {
        return Err(bad(
            "the readings must be taken at angles that cover at least 90 degrees",
        ));
    }
    let k = n as f64;
    let (mx, my) = (
        samples.iter().map(|s| s.x_mm).sum::<f64>() / k,
        samples.iter().map(|s| s.y_mm).sum::<f64>() / k,
    );
    let (mut suu, mut suv, mut svv, mut suuu, mut svvv, mut suvv, mut svuu) =
        (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    for s in samples {
        let (u, v) = (s.x_mm - mx, s.y_mm - my);
        suu += u * u;
        suv += u * v;
        svv += v * v;
        suuu += u * u * u;
        svvv += v * v * v;
        suvv += u * v * v;
        svuu += v * u * u;
    }
    let det = suu * svv - suv * suv;
    if det.abs() <= 1e-12 * (suu + svv) * (suu + svv) || suu + svv < 1e-18 {
        return Err(bad(
            "the readings are on one line or one point: they do not draw a circle",
        ));
    }
    let (r1, r2) = (0.5 * (suuu + suvv), 0.5 * (svvv + svuu));
    let (uc, vc) = ((r1 * svv - r2 * suv) / det, (r2 * suu - r1 * suv) / det);
    let axis = (mx + uc, my + vc);
    let radius = (uc * uc + vc * vc + (suu + svv) / k).sqrt();
    // the phase: the mean direction (a circular mean) of the tip from the axis, less the angle the nozzle was turned to
    let (mut sin, mut cos) = (0.0, 0.0);
    for s in samples {
        let a = (s.y_mm - axis.1).atan2(s.x_mm - axis.0) - s.angle_deg.to_radians();
        sin += a.sin();
        cos += a.cos();
    }
    let phase = norm_deg(sin.atan2(cos).to_degrees());
    let errors: Vec<f64> = samples
        .iter()
        .map(|s| {
            let o = offset_of(radius, phase, s.angle_deg);
            (axis.0 + o.0 - s.x_mm).hypot(axis.1 + o.1 - s.y_mm)
        })
        .collect();
    Ok(NozzleRunout {
        axis,
        radius_mm: radius,
        phase_deg: phase,
        rms_mm: (errors.iter().map(|e| e * e).sum::<f64>() / k).sqrt(),
        max_mm: errors.iter().copied().fold(0.0, f64::max),
        samples: n,
    })
}

/// What the teaching of a strip found: where the first part is and how the next ones follow.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StripTeach {
    pub x_mm: f64,
    pub y_mm: f64,
    pub count: u32,
    /// The distance between two parts along the strip.
    pub pitch_mm: f64,
    /// The angle of the strip, 0 up to 360.
    pub rotation_deg: f64,
}

const MAX_PLACE: f64 = 10_000.0;
const MIN_PITCH: f64 = 0.01;
const MAX_PITCH: f64 = 1000.0;

fn place(p: (f64, f64), what: &str) -> Result<(f64, f64), PnpError> {
    if !p.0.is_finite() || !p.1.is_finite() || p.0.abs() > MAX_PLACE || p.1.abs() > MAX_PLACE {
        return Err(bad(format!(
            "the place of {what} is not a number up to {MAX_PLACE}"
        )));
    }
    Ok(p)
}

fn pitch(value: f64, what: &str) -> Result<f64, PnpError> {
    if value.abs() < MIN_PITCH || value.abs() > MAX_PITCH {
        return Err(bad(format!(
            "the pitch {what} would be {value:.4} mm, outside {MIN_PITCH} to {MAX_PITCH} mm: check the places and the count"
        )));
    }
    Ok(value)
}

/// The pitch and the angle of a strip from the places of its first and last parts, which hold `count` parts in all (at least 2 and at most 1000000).
pub fn teach_strip(
    first: (f64, f64),
    last: (f64, f64),
    count: u32,
) -> Result<StripTeach, PnpError> {
    if !(2..=1_000_000).contains(&count) {
        return Err(bad("a strip is taught with 2 to 1000000 parts"));
    }
    let (first, last) = (
        place(first, "the first part")?,
        place(last, "the last part")?,
    );
    let (dx, dy) = (last.0 - first.0, last.1 - first.1);
    let pitch_mm = pitch(dx.hypot(dy) / f64::from(count - 1), "of the strip")?;
    Ok(StripTeach {
        x_mm: first.0,
        y_mm: first.1,
        count,
        pitch_mm,
        rotation_deg: dy.atan2(dx).to_degrees().rem_euclid(360.0),
    })
}

impl StripTeach {
    /// The columns of the file of feeders that this sets, as a header and a row.
    pub fn csv(&self) -> String {
        format!(
            "kind,x,y,count,pitch,rotation\nstrip,{:.3},{:.3},{},{:.4},{:.3}\n",
            self.x_mm, self.y_mm, self.count, self.pitch_mm, self.rotation_deg
        )
    }

    pub fn summary(&self) -> String {
        format!(
            "a strip of {} parts from X {:.3} Y {:.3}: a pitch of {:.4} mm along {:.3} degrees",
            self.count, self.x_mm, self.y_mm, self.pitch_mm, self.rotation_deg
        )
    }
}

/// What the teaching of a tray found.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrayTeach {
    pub x_mm: f64,
    pub y_mm: f64,
    pub cols: u32,
    pub rows: u32,
    pub pitch_x_mm: f64,
    /// Negative when the rows go the other way (downwards in the frame of the tray).
    pub pitch_y_mm: f64,
    pub rotation_deg: f64,
    /// How far the columns are from being at right angles to the rows, in degrees (the feeders file has no field for it; a large value says that a place was measured badly).
    pub skew_deg: f64,
}

/// The pitches and the angle of a tray from the places of its first cell, of the last cell of the first row and of the last cell of the first column. The axes of a tray are at right
/// angles, so the third place is used for the pitch along the rows and for the check of the angle.
pub fn teach_tray(
    origin: (f64, f64),
    col_end: (f64, f64),
    row_end: (f64, f64),
    cols: u32,
    rows: u32,
) -> Result<TrayTeach, PnpError> {
    if !(2..=1000).contains(&cols) || !(2..=1000).contains(&rows) {
        return Err(bad(
            "a tray is taught with 2 to 1000 cells in each direction (a single row is a strip)",
        ));
    }
    let origin = place(origin, "the first cell")?;
    let col_end = place(col_end, "the last cell of the first row")?;
    let row_end = place(row_end, "the last cell of the first column")?;
    let (ax, ay) = (col_end.0 - origin.0, col_end.1 - origin.1);
    let pitch_x = pitch(ax.hypot(ay) / f64::from(cols - 1), "along the rows")?;
    let rotation = ay.atan2(ax);
    let (c, s) = (rotation.cos(), rotation.sin());
    let (bx, by) = (row_end.0 - origin.0, row_end.1 - origin.1);
    let along = bx * c + by * s;
    let across = -bx * s + by * c;
    let pitch_y = pitch(across / f64::from(rows - 1), "along the columns")?;
    Ok(TrayTeach {
        x_mm: origin.0,
        y_mm: origin.1,
        cols,
        rows,
        pitch_x_mm: pitch_x,
        pitch_y_mm: pitch_y,
        rotation_deg: rotation.to_degrees().rem_euclid(360.0),
        skew_deg: along.atan2(across.abs()).to_degrees(),
    })
}

impl TrayTeach {
    /// The columns of the file of feeders that this sets, as a header and a row.
    pub fn csv(&self) -> String {
        format!(
            "kind,x,y,cols,rows,pitch_x,pitch_y,rotation\ntray,{:.3},{:.3},{},{},{:.4},{:.4},{:.3}\n",
            self.x_mm,
            self.y_mm,
            self.cols,
            self.rows,
            self.pitch_x_mm,
            self.pitch_y_mm,
            self.rotation_deg
        )
    }

    pub fn summary(&self) -> String {
        format!(
            "a tray of {} by {} cells from X {:.3} Y {:.3}: pitches of {:.4} and {:.4} mm along {:.3} degrees, the columns {:+.3} degrees from square",
            self.cols,
            self.rows,
            self.x_mm,
            self.y_mm,
            self.pitch_x_mm,
            self.pitch_y_mm,
            self.rotation_deg,
            self.skew_deg
        )
    }
}

/// What a feeder of a file of measures was taught.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Taught {
    Strip(StripTeach),
    Tray(TrayTeach),
}

/// The most feeders a file of measures may hold.
pub const MAX_TEACH_ROWS: usize = 1000;

fn teach_number(cell: &str, line: usize, name: &str) -> Result<f64, PnpError> {
    let v: f64 = cell
        .trim()
        .parse()
        .map_err(|_| bad_at(line, format!("{name} '{cell}' is not a number")))?;
    if !v.is_finite() || v.abs() > MAX_MM {
        return Err(bad_at(
            line,
            format!("{name} '{cell}' is not a usable number"),
        ));
    }
    Ok(v)
}

fn teach_count(cell: &str, line: usize, name: &str) -> Result<u32, PnpError> {
    cell.trim()
        .parse::<u32>()
        .map_err(|_| bad_at(line, format!("{name} '{cell}' is not a whole number")))
}

/// Reads a file of measures for several feeders: the columns `id,kind,x1,y1,x2,y2,x3,y3,count,rows`, one row for each feeder, with an optional header and comment lines (`#`). For a
/// `strip` the places 1 and 2 are the first and the last part and `count` the number of parts; for a `tray` they are the first cell, the last cell of the first row and (place 3) the last
/// cell of the first column, and `count` and `rows` are the cells of a row and the rows. Each row is taught as `teach_strip` or `teach_tray` do, and the id is kept to name the feeder.
pub fn teach_file(text: &str) -> Result<Vec<(String, Taught)>, PnpError> {
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
        .map_err(|(line, e)| bad_at(line, e))?;
    let mut out: Vec<(String, Taught)> = Vec::new();
    for (i, (line, record)) in records.into_iter().enumerate() {
        if i == 0
            && record
                .first()
                .is_some_and(|c| c.trim().eq_ignore_ascii_case("id"))
        {
            continue;
        }
        if record.len() != 10 {
            return Err(bad_at(
                line,
                format!("a feeder needs 10 columns, this row has {}", record.len()),
            ));
        }
        if out.len() >= MAX_TEACH_ROWS {
            return Err(bad_at(line, format!("more than {MAX_TEACH_ROWS} feeders")));
        }
        let id = record[0].trim().to_string();
        if id.is_empty() || id.chars().any(char::is_control) {
            return Err(bad_at(line, "the id of the feeder is empty"));
        }
        if out.iter().any(|(other, _)| *other == id) {
            return Err(bad_at(line, format!("the id '{id}' is used twice")));
        }
        let at = |k: usize, name: &str| teach_number(&record[k], line, name);
        let p1 = (at(2, "x1")?, at(3, "y1")?);
        let p2 = (at(4, "x2")?, at(5, "y2")?);
        let count = teach_count(&record[8], line, "count")?;
        let taught = match record[1].trim().to_ascii_lowercase().as_str() {
            "strip" => {
                Taught::Strip(teach_strip(p1, p2, count).map_err(|e| bad_at(line, e.detail))?)
            }
            "tray" => {
                let p3 = (at(6, "x3")?, at(7, "y3")?);
                let rows = teach_count(&record[9], line, "rows")?;
                Taught::Tray(
                    teach_tray(p1, p2, p3, count, rows).map_err(|e| bad_at(line, e.detail))?,
                )
            }
            other => {
                return Err(bad_at(
                    line,
                    format!("the kind '{other}' is not strip or tray"),
                ))
            }
        };
        out.push((id, taught));
    }
    if out.is_empty() {
        return Err(bad("the file of measures has no feeder"));
    }
    Ok(out)
}

/// The columns of the file of feeders that the taught feeders set (`id,kind,x,y,count,cols,rows,pitch_x,pitch_y,pitch,rotation`), as a header and a row for each.
pub fn taught_csv(rows: &[(String, Taught)]) -> String {
    let mut out = String::from("id,kind,x,y,count,cols,rows,pitch_x,pitch_y,pitch,rotation\n");
    for (id, t) in rows {
        match t {
            Taught::Strip(s) => out.push_str(&format!(
                "{id},strip,{:.3},{:.3},{},,,,,{:.4},{:.3}\n",
                s.x_mm, s.y_mm, s.count, s.pitch_mm, s.rotation_deg
            )),
            Taught::Tray(t) => out.push_str(&format!(
                "{id},tray,{:.3},{:.3},,{},{},{:.4},{:.4},,{:.3}\n",
                t.x_mm, t.y_mm, t.cols, t.rows, t.pitch_x_mm, t.pitch_y_mm, t.rotation_deg
            )),
        }
    }
    out
}
