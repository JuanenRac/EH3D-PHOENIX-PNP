//! Angles: the correction of the rotation of the parts by package (a footprint drawn at another angle than the tape holds it), and the way a nozzle turns from one angle to another
//! when its turn is limited.

use crate::error::{Code, PnpError};
use crate::parts::Part;

/// Whether `text` matches `pattern`, without regard to case. `*` stands for any run of characters (also none) and `?` for exactly one character.
pub fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let t: Vec<char> = text.to_lowercase().chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    // where the last `*` was and how much of the text it has taken so far
    let mut star: Option<(usize, usize)> = None;
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) && p[pi] != '*' {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some((pi, ti));
            pi += 1;
        } else if let Some((sp, st)) = star {
            pi = sp + 1;
            ti = st + 1;
            star = Some((sp, st + 1));
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

#[derive(Debug, Clone, PartialEq)]
struct RotationRule {
    pattern: String,
    offset: f64,
}

/// The rules of rotation by package, in the order of the configuration: the first rule that matches the package of a part is the one that is used.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RotationTable {
    rules: Vec<RotationRule>,
}

impl RotationTable {
    pub const MAX_RULES: usize = 1000;

    /// `rules` are pairs of a pattern and an offset in degrees (from -360 to 360).
    pub fn new(rules: &[(String, f64)]) -> Result<RotationTable, PnpError> {
        if rules.len() > RotationTable::MAX_RULES {
            return Err(PnpError::new(
                Code::RotationRule,
                format!("more than {} rules", RotationTable::MAX_RULES),
            ));
        }
        let mut out = Vec::with_capacity(rules.len());
        for (pattern, offset) in rules {
            let pattern = pattern.trim();
            if pattern.is_empty() || pattern.chars().count() > 128 {
                return Err(PnpError::new(
                    Code::RotationRule,
                    "a pattern is empty or longer than 128 characters",
                ));
            }
            if !offset.is_finite() || offset.abs() > 360.0 {
                return Err(PnpError::new(
                    Code::RotationRule,
                    format!("the offset {offset} of '{pattern}' is not from -360 to 360 degrees"),
                ));
            }
            out.push(RotationRule {
                pattern: pattern.to_string(),
                offset: *offset,
            });
        }
        Ok(RotationTable { rules: out })
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    pub fn len(&self) -> usize {
        self.rules.len()
    }

    /// The offset for a package: that of the first rule that matches, or 0.
    pub fn offset_for(&self, package: &str) -> f64 {
        self.rules
            .iter()
            .find(|r| glob_match(&r.pattern, package.trim()))
            .map_or(0.0, |r| r.offset)
    }

    /// Adds the offset of the package to the rotation of every part (the result stays from 0 up to 360). Applied once to a list, not twice.
    pub fn apply(&self, parts: &mut [Part]) {
        if self.rules.is_empty() {
            return;
        }
        for p in parts.iter_mut() {
            p.rotation = (p.rotation + self.offset_for(&p.package)).rem_euclid(360.0);
        }
    }
}

/// The angle a nozzle turns to, to stand at `target` degrees when its turn is limited to the range from `min` to `max` (absolute angles, which may be beyond a full turn): of all the
/// angles in the range that are the same direction as `target`, the one nearest to the angle where the nozzle is now (the lower one when two are equally near).
pub fn rot_path(from: f64, target: f64, min: f64, max: f64) -> Result<f64, PnpError> {
    if !(from.is_finite() && target.is_finite() && min.is_finite() && max.is_finite()) || min > max
    {
        return Err(PnpError::new(
            Code::RotationUnreachable,
            format!("from {from} to {target} within {min}..{max}: not numbers or an empty range"),
        ));
    }
    let base = target.rem_euclid(360.0);
    let first = ((min - base) / 360.0).ceil() as i64;
    let last = ((max - base) / 360.0).floor() as i64;
    let near = ((from - base) / 360.0).round() as i64;
    let mut best: Option<f64> = None;
    for k in [first, last, near - 1, near, near + 1] {
        if k < first || k > last {
            continue;
        }
        let c = base + 360.0 * k as f64;
        best = match best {
            Some(b)
                if (b - from).abs() < (c - from).abs()
                    || ((b - from).abs() == (c - from).abs() && b <= c) =>
            {
                Some(b)
            }
            _ => Some(c),
        };
    }
    best.ok_or_else(|| {
        PnpError::new(
            Code::RotationUnreachable,
            format!("{target} degrees is not reachable within {min}..{max}"),
        )
    })
}
