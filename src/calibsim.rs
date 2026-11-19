// EH3D-PHOENIX-PNP - a camera calibration tried on a camera that is made up.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! `camera-calibrate` fits the camera that looks at the parts under the nozzle from sightings of a mark. Before a machine exists, or before its camera is trusted, a person wants to know
//! how well the fit does for the number of sightings and the noise of the reading that they can get. This makes a camera whose size of pixel, offset, turn and mirror are known (the
//! `camera_*` keys of the settings), looks at a mark in places chosen by chance from a seed, adds the noise of a reading, fits the camera as `camera-calibrate` does and says how far the
//! camera that was found is from the one that was made up, measured where it matters: at the places of a grid over the picture, in millimetres. It changes no setting and moves nothing.

use crate::calibrate::{calibrate_camera, CameraCalibration, CameraPoint};
use crate::camera::CameraModel;
use crate::error::PnpError;
use crate::sim::Rng;

/// The size of a pixel that is used when the settings say 0 (the size of the pipeline).
pub const DEFAULT_UNITS_MM: f64 = 0.02;
/// The most sightings (the fit of the camera takes up to 100 marks).
pub const MAX_POINTS: usize = 100;

/// What the test is asked.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub points: usize,
    /// The noise of the reading of the mark, in pixels (the standard deviation of each coordinate).
    pub noise_px: f64,
    /// The half width of the picture, in pixels: the marks are seen up to this far from the middle.
    pub half_px: f64,
    pub seed: u64,
}

impl Default for Plan {
    fn default() -> Self {
        Plan {
            points: 12,
            noise_px: 0.3,
            half_px: 300.0,
            seed: 1,
        }
    }
}

/// What came out.
#[derive(Debug, Clone, PartialEq)]
pub struct Selftest {
    /// The camera that was made up, and the size of its pixel.
    pub truth: CameraModel,
    pub units_mm: f64,
    pub sightings: Vec<CameraPoint>,
    pub found: CameraCalibration,
    /// How far the camera that was found is from the true one over a grid of places, in millimetres: root mean square and largest.
    pub rms_mm: f64,
    pub max_mm: f64,
}

/// A number with the normal distribution (mean 0, deviation 1) from two of the generator (Box-Muller).
fn normal(rng: &mut Rng) -> f64 {
    let u = rng.next_f64().max(1e-12);
    let v = rng.next_f64();
    (-2.0 * u.ln()).sqrt() * (std::f64::consts::TAU * v).cos()
}

/// The sightings of a mark that the true camera gives: the mark is at a place chosen by chance in the picture, and the pixel where it is read has the noise of the reading.
pub fn sightings(truth: &CameraModel, units_mm: f64, plan: &Plan) -> Vec<CameraPoint> {
    let mut rng = Rng::new(plan.seed);
    let reach = plan.half_px * units_mm;
    (0..plan.points)
        .map(|_| {
            let place = (
                (rng.next_f64() * 2.0 - 1.0) * reach,
                (rng.next_f64() * 2.0 - 1.0) * reach,
            );
            let (px, py) = truth.to_pixels(place, units_mm);
            CameraPoint {
                px: px + normal(&mut rng) * plan.noise_px,
                py: py + normal(&mut rng) * plan.noise_px,
                x_mm: place.0,
                y_mm: place.1,
            }
        })
        .collect()
}

/// Makes the camera, looks, fits and measures the error. `truth.units_per_px` of 0 is taken as [`DEFAULT_UNITS_MM`].
pub fn selftest(truth: &CameraModel, plan: &Plan) -> Result<Selftest, PnpError> {
    let units = if truth.units_per_px > 0.0 {
        truth.units_per_px
    } else {
        DEFAULT_UNITS_MM
    };
    let seen = sightings(truth, units, plan);
    let found = calibrate_camera(&seen)?;
    // the error is measured at a grid of 5 by 5 places over the picture, with the pixel that the true camera gives for each (no noise): what a part put there would be read as
    let reach = plan.half_px * units;
    let mut sum = 0.0;
    let mut max = 0.0f64;
    let mut n = 0.0;
    for i in 0..5 {
        for j in 0..5 {
            let place = (
                (f64::from(i) / 2.0 - 1.0) * reach,
                (f64::from(j) / 2.0 - 1.0) * reach,
            );
            let px = truth.to_pixels(place, units);
            let back = found.model.to_machine(px, found.model.units_per_px);
            let e = (back.0 - place.0).hypot(back.1 - place.1);
            sum += e * e;
            max = max.max(e);
            n += 1.0;
        }
    }
    Ok(Selftest {
        truth: *truth,
        units_mm: units,
        sightings: seen,
        found,
        rms_mm: (sum / n).sqrt(),
        max_mm: max,
    })
}

impl Selftest {
    /// Whether the camera that was found is within `tol_mm` everywhere on the grid.
    pub fn within(&self, tol_mm: f64) -> bool {
        self.max_mm <= tol_mm
    }

    /// The report: the camera that was made up, the one that was found and the error.
    pub fn text(&self, tol_mm: f64) -> String {
        let t = &self.truth;
        let f = &self.found.model;
        let onoff = |b: bool| if b { "on" } else { "off" };
        let mut out = format!(
            "made up: pixel {:.6} mm, offset {:.4} {:.4} mm, turn {:.3} deg, mirror x {} y {}\n",
            self.units_mm,
            t.offset_x_mm,
            t.offset_y_mm,
            t.rotation_deg,
            onoff(t.flip_x),
            onoff(t.flip_y)
        );
        out.push_str(&format!(
            "found:   pixel {:.6} mm, offset {:.4} {:.4} mm, turn {:.3} deg, mirror x {} y {}\n",
            f.units_per_px,
            f.offset_x_mm,
            f.offset_y_mm,
            f.rotation_deg,
            onoff(f.flip_x),
            onoff(f.flip_y)
        ));
        out.push_str(&format!("{}\n", self.found.summary()));
        out.push_str(&format!(
            "error over the picture: {:.4} mm (rms), {:.4} mm (largest); {}\n",
            self.rms_mm,
            self.max_mm,
            if self.within(tol_mm) {
                format!("within {tol_mm} mm")
            } else {
                format!("over {tol_mm} mm")
            }
        ));
        out
    }

    /// The sightings as the file that `camera-calibrate` reads.
    pub fn csv(&self) -> String {
        let mut out = String::from("px,py,x_mm,y_mm\n");
        for p in &self.sightings {
            out.push_str(&format!(
                "{:.4},{:.4},{:.5},{:.5}\n",
                p.px, p.py, p.x_mm, p.y_mm
            ));
        }
        out
    }
}
