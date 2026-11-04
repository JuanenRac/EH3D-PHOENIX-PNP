//! Lengths and locations that carry their unit: a position is built from millimetres, mils or inches once, and from then on it is a length in millimetres, so the units cannot be mixed.

use crate::error::{Code, PnpError};
use crate::parts::length_with;

/// A length in millimetres. The largest one that a board or a machine can have is `Length::MAX_MM`.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Length(f64);

impl Length {
    pub const MAX_MM: f64 = 10_000.0;
    pub const MM_PER_MIL: f64 = 0.0254;
    pub const MM_PER_INCH: f64 = 25.4;

    pub fn from_mm(v: f64) -> Result<Length, PnpError> {
        if !v.is_finite() || v.abs() > Length::MAX_MM {
            return Err(PnpError::new(
                Code::UnitBad,
                format!(
                    "{v} mm is not a length on a board (beyond {} mm)",
                    Length::MAX_MM
                ),
            ));
        }
        Ok(Length(v))
    }

    pub fn from_mil(v: f64) -> Result<Length, PnpError> {
        Length::from_mm(v * Length::MM_PER_MIL)
    }

    pub fn from_inch(v: f64) -> Result<Length, PnpError> {
        Length::from_mm(v * Length::MM_PER_INCH)
    }

    /// A number with an optional unit (`12.5`, `12.5mm`, `492mil`, `0.5in`); a bare number is in millimetres.
    pub fn parse(text: &str) -> Result<Length, PnpError> {
        let v = length_with(text, 1.0).map_err(|e| PnpError::new(Code::UnitBad, e))?;
        Length::from_mm(v)
    }

    pub fn mm(self) -> f64 {
        self.0
    }

    pub fn mil(self) -> f64 {
        self.0 / Length::MM_PER_MIL
    }

    pub fn inch(self) -> f64 {
        self.0 / Length::MM_PER_INCH
    }
}

/// A place of the machine: X, Y and Z as lengths and the rotation in degrees, from 0 up to 360.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Location {
    pub x: Length,
    pub y: Length,
    pub z: Length,
    rotation: f64,
}

impl Location {
    pub fn new(x: Length, y: Length, z: Length, rotation_deg: f64) -> Result<Location, PnpError> {
        if !rotation_deg.is_finite() {
            return Err(PnpError::new(
                Code::UnitBad,
                format!("{rotation_deg} is not an angle"),
            ));
        }
        let rotation = rotation_deg.rem_euclid(360.0);
        Ok(Location { x, y, z, rotation })
    }

    pub fn from_mm(x: f64, y: f64, z: f64, rotation_deg: f64) -> Result<Location, PnpError> {
        Location::new(
            Length::from_mm(x)?,
            Length::from_mm(y)?,
            Length::from_mm(z)?,
            rotation_deg,
        )
    }

    /// The rotation in degrees, from 0 up to 360.
    pub fn rotation(&self) -> f64 {
        self.rotation
    }

    /// The distance in the XY plane, in millimetres.
    pub fn distance_xy(&self, other: &Location) -> f64 {
        (self.x.mm() - other.x.mm()).hypot(self.y.mm() - other.y.mm())
    }

    /// The same place moved by `dx` and `dy` millimetres.
    pub fn translated(&self, dx: f64, dy: f64) -> Result<Location, PnpError> {
        Ok(Location {
            x: Length::from_mm(self.x.mm() + dx)?,
            y: Length::from_mm(self.y.mm() + dy)?,
            z: self.z,
            rotation: self.rotation,
        })
    }

    /// The place turned by `deg` degrees (counter-clockwise) about the origin of the XY plane; its own rotation turns by the same angle.
    pub fn rotated_about_origin(&self, deg: f64) -> Result<Location, PnpError> {
        let (x, y) = rotate_xy(self.x.mm(), self.y.mm(), deg);
        Location::new(
            Length::from_mm(x)?,
            Length::from_mm(y)?,
            self.z,
            self.rotation + deg,
        )
    }
}

/// A point turned by `deg` degrees counter-clockwise about the origin.
pub fn rotate_xy(x: f64, y: f64, deg: f64) -> (f64, f64) {
    let r = deg.to_radians();
    let (s, c) = r.sin_cos();
    (x * c - y * s, x * s + y * c)
}
