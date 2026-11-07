//! The model of a camera: how a picture relates to the machine. A pixel is `units_per_px` millimetres on the part, the picture can be turned and mirrored with respect to the axes of the
//! machine, the axis of the nozzle can be somewhere other than the middle of the picture (`offset`), and the lens can differ from what the software believes (`lens_scale`, `lens_k1`).
//! Nothing here draws or measures: the functions turn places into pixels and pixels into places, and the simulated camera uses them around the vision pipeline.
//!
//! Conventions. Pixel coordinates in this module are measured from the middle of the picture with the second axis pointing up, as the machine does: a pixel `(10, 5)` is ten pixels to the
//! right of and five above the middle. A place is in millimetres, in the frame of the machine, relative to the axis of the nozzle.
//!
//! * The software believes that a pixel is `units` millimetres (`units_per_px` of the model, or the size of the pixel of the pipeline when that is 0). The lens makes the true size of a
//!   pixel `units * lens_scale`, so a camera with `lens_scale` 1.02 under-reports every distance by that factor, and `lens_k1` bends the picture: a point at `d` millimetres from the middle
//!   of the picture (in the frame of the picture) is drawn at `d * (1 + lens_k1 * d * d)`. The software does not know about the bending.
//! * `to_machine` is what the software does (the believed place of a pixel); `to_pixels` is what the optics do (the pixel where a true place appears).

use crate::units::rotate_xy;

/// How a camera is set on the machine and how its lens differs from the ideal.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraModel {
    /// Millimetres on the part that the software believes a pixel is; 0 means the size the vision pipeline is set to.
    pub units_per_px: f64,
    /// Where the axis of the nozzle is in the picture, in millimetres from its middle, in the frame of the machine.
    pub offset_x_mm: f64,
    pub offset_y_mm: f64,
    /// How much the picture is turned counter-clockwise with respect to the axes of the machine, in degrees.
    pub rotation_deg: f64,
    /// The picture is mirrored left to right / upside down (after the turn).
    pub flip_x: bool,
    pub flip_y: bool,
    /// The true size of a pixel over the believed one.
    pub lens_scale: f64,
    /// The radial bending of the lens, per square millimetre.
    pub lens_k1: f64,
}

impl Default for CameraModel {
    fn default() -> Self {
        CameraModel {
            units_per_px: 0.0,
            offset_x_mm: 0.0,
            offset_y_mm: 0.0,
            rotation_deg: 0.0,
            flip_x: false,
            flip_y: false,
            lens_scale: 1.0,
            lens_k1: 0.0,
        }
    }
}

impl CameraModel {
    /// Whether the model changes nothing: the picture is in the frame of the machine, centred on the nozzle, with a perfect lens and the pixel size of the pipeline.
    pub fn is_default(&self) -> bool {
        *self == CameraModel::default()
    }

    /// The size of a pixel the software uses: the one of the model, or `pipeline_mm_per_px` when the model says 0.
    pub fn units(&self, pipeline_mm_per_px: f64) -> f64 {
        if self.units_per_px > 0.0 {
            self.units_per_px
        } else {
            pipeline_mm_per_px
        }
    }

    /// The frame of the picture to the frame of the machine for a vector: turn, then mirror.
    fn image_to_machine(&self, v: (f64, f64)) -> (f64, f64) {
        let v = (
            if self.flip_x { -v.0 } else { v.0 },
            if self.flip_y { -v.1 } else { v.1 },
        );
        rotate_xy(v.0, v.1, self.rotation_deg)
    }

    /// The frame of the machine to the frame of the picture for a vector (the inverse of `image_to_machine`).
    fn machine_to_image(&self, v: (f64, f64)) -> (f64, f64) {
        let v = rotate_xy(v.0, v.1, -self.rotation_deg);
        (
            if self.flip_x { -v.0 } else { v.0 },
            if self.flip_y { -v.1 } else { v.1 },
        )
    }

    /// The place, in millimetres from the axis of the nozzle in the frame of the machine, that the software believes a pixel is.
    pub fn to_machine(&self, px: (f64, f64), units: f64) -> (f64, f64) {
        let m = self.image_to_machine((px.0 * units, px.1 * units));
        (m.0 - self.offset_x_mm, m.1 - self.offset_y_mm)
    }

    /// The pixel where a true place (millimetres from the axis of the nozzle, in the frame of the machine) appears, with the lens as it is.
    pub fn to_pixels(&self, place: (f64, f64), units: f64) -> (f64, f64) {
        let d = self.machine_to_image((place.0 + self.offset_x_mm, place.1 + self.offset_y_mm));
        let bend = 1.0 + self.lens_k1 * (d.0 * d.0 + d.1 * d.1);
        let size = units * self.lens_scale;
        (d.0 * bend / size, d.1 * bend / size)
    }

    /// What the software reports for a part that is truly at `place`: the place goes through the optics and back through the software.
    pub fn read(&self, place: (f64, f64), units: f64) -> (f64, f64) {
        self.to_machine(self.to_pixels(place, units), units)
    }

    /// The turn of a part in the frame of the picture, for a part turned `deg` degrees in the frame of the machine.
    pub fn angle_to_image(&self, deg: f64) -> f64 {
        let mut a = deg - self.rotation_deg;
        if self.flip_x {
            a = 180.0 - a;
        }
        if self.flip_y {
            a = -a;
        }
        a
    }

    /// The turn of a part in the frame of the machine, for a part turned `deg` degrees in the frame of the picture (the inverse of `angle_to_image`).
    pub fn angle_to_machine(&self, deg: f64) -> f64 {
        let mut a = deg;
        if self.flip_y {
            a = -a;
        }
        if self.flip_x {
            a = 180.0 - a;
        }
        a + self.rotation_deg
    }
}
