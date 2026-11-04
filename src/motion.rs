//! The time a move takes. An axis speeds up to its top speed, runs, and slows down; with a jerk limit the acceleration itself is ramped (an S-curve), without it the profile is a
//! trapezoid. A short move never reaches the top speed. X and Y move at the same time, so a move takes as long as the slower of the two axes.

use crate::error::{Code, PnpError};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AxisMotion {
    /// Top speed, mm/s.
    pub vmax: f64,
    /// Acceleration, mm/s2.
    pub accel: f64,
    /// Jerk, mm/s3; 0 means no limit on the jerk (a trapezoid).
    pub jerk: f64,
}

impl AxisMotion {
    pub fn new(vmax: f64, accel: f64, jerk: f64) -> Result<AxisMotion, PnpError> {
        if !(vmax.is_finite() && vmax > 0.0 && accel.is_finite() && accel > 0.0)
            || !jerk.is_finite()
            || jerk < 0.0
        {
            return Err(PnpError::new(
                Code::PlanSetting,
                format!("speed {vmax}, acceleration {accel} and jerk {jerk} must be numbers, the first two above 0 and the last not below 0"),
            ));
        }
        Ok(AxisMotion { vmax, accel, jerk })
    }

    /// The time to go from rest to the speed `v` (and, the same, to stop from it).
    fn ramp(&self, v: f64) -> f64 {
        if self.jerk <= 0.0 {
            v / self.accel
        } else if v >= self.accel * self.accel / self.jerk {
            v / self.accel + self.accel / self.jerk
        } else {
            2.0 * (v / self.jerk).sqrt()
        }
    }

    /// The seconds to move `distance` millimetres from rest to rest along this axis.
    pub fn time(&self, distance: f64) -> f64 {
        let d = distance.abs();
        if d.is_nan() || d == 0.0 {
            return 0.0;
        }
        let ta = self.ramp(self.vmax);
        if d >= self.vmax * ta {
            return d / self.vmax + ta;
        }
        // the top speed is not reached: the peak speed vp is the one for which the distance of the speeding up and the slowing down, vp * ramp(vp), is d
        let (mut lo, mut hi) = (0.0, self.vmax);
        for _ in 0..60 {
            let mid = 0.5 * (lo + hi);
            if mid * self.ramp(mid) < d {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        2.0 * self.ramp(hi)
    }
}

/// The seconds of a move of `dx` and `dy` millimetres.
pub fn move_time(dx: f64, dy: f64, x: &AxisMotion, y: &AxisMotion) -> f64 {
    x.time(dx).max(y.time(dy))
}
