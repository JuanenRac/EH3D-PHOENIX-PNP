// EH3D-PHOENIX-PNP - a feeder that drags the tape by its sprocket holes.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! A drag feeder holds a tape and moves it with a pin that goes into its sprocket holes, which are a fixed distance apart (4 mm in the standard of tapes). One stroke moves the tape by a whole
//! number of holes, and a pocket of 2 mm means that a stroke brings two of them: the head picks one at the place of the feeder, and the next one 2 mm further along the tape, and only then
//! does the feeder drag again. This works out, for each strip of the table of feeders, the shortest stroke that is both a whole number of holes and a whole number of pockets, how many picks
//! one stroke serves, where each of them is, and how many strokes the tape takes to be emptied. A pitch that does not fit the holes in a few picks is told, and nothing is guessed. It is
//! arithmetic on a table; no feeder is driven.

use crate::feeding::{FeederKind, FeederSpec};
use crate::units::rotate_xy;

/// How the pitch of a strip fits the holes.
#[derive(Debug, Clone, PartialEq)]
pub struct Drag {
    pub id: String,
    /// The distance between two pockets (mm).
    pub pocket_pitch_mm: f64,
    /// The shortest stroke that is whole holes and whole pockets (mm), and how many holes that is.
    pub stroke_mm: f64,
    pub holes_per_stroke: u64,
    /// How many picks one stroke serves.
    pub picks_per_stroke: u64,
    /// How many strokes it takes to pick `count` parts (the first stroke brings the first pocket).
    pub strokes: u64,
    /// Where each pick of a stroke is, away from the place of the feeder (mm, along the axes of the machine).
    pub offsets: Vec<(f64, f64)>,
}

/// The result for a table of feeders.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DragPlan {
    pub rows: Vec<Drag>,
    /// The strips that cannot be dragged: id and why.
    pub refused: Vec<(String, String)>,
    /// How many feeders of the table are not strips and so were not looked at.
    pub other: usize,
}

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

/// A distance in thousandths of a millimetre.
fn units(mm: f64) -> u64 {
    (mm * 1000.0).round() as u64
}

/// The strokes for a strip: `hole_mm` is the distance between the holes and `max_picks` the most picks that one stroke may serve.
pub fn drag_of(
    f: &FeederSpec,
    pitch_mm: f64,
    rotation_deg: f64,
    hole_mm: f64,
    max_picks: u64,
) -> Result<Drag, String> {
    if !(pitch_mm.is_finite() && pitch_mm > 0.0 && pitch_mm <= 100.0) {
        return Err(format!("the pitch {pitch_mm} is not from 0 to 100 mm"));
    }
    let (p, h) = (units(pitch_mm), units(hole_mm));
    if p == 0 || h == 0 {
        return Err("a pitch or a hole distance of less than a thousandth of a millimetre".into());
    }
    let stroke = p / gcd(p, h) * h;
    let picks = stroke / p;
    if picks > max_picks {
        return Err(format!(
            "a pitch of {pitch_mm} mm needs a stroke of {} mm for {picks} picks, and the most is {max_picks}",
            stroke as f64 / 1000.0
        ));
    }
    let count = u64::from(f.base.count);
    Ok(Drag {
        id: f.base.id.clone(),
        pocket_pitch_mm: pitch_mm,
        stroke_mm: stroke as f64 / 1000.0,
        holes_per_stroke: stroke / h,
        picks_per_stroke: picks,
        strokes: count.div_ceil(picks),
        offsets: (0..picks)
            .map(|i| rotate_xy(pitch_mm * i as f64, 0.0, rotation_deg))
            .collect(),
    })
}

/// The plan of a table of feeders: every strip is a tape that a drag feeder moves.
pub fn plan(feeders: &[FeederSpec], hole_mm: f64, max_picks: u64) -> DragPlan {
    let mut out = DragPlan::default();
    for f in feeders {
        match f.kind {
            FeederKind::Strip { pitch, rotation } => {
                match drag_of(f, pitch, rotation, hole_mm, max_picks) {
                    Ok(d) => out.rows.push(d),
                    Err(why) => out.refused.push((f.base.id.clone(), why)),
                }
            }
            _ => out.other += 1,
        }
    }
    out
}

impl Drag {
    /// Where the pick number `n` (from 0) is, given the place of the feeder: the pockets of one stroke are in turn at the place and further along, and after a stroke they start again.
    pub fn pick_point(&self, origin: (f64, f64), n: u64) -> (f64, f64) {
        let o = self.offsets[(n % self.picks_per_stroke) as usize];
        (origin.0 + o.0, origin.1 + o.1)
    }

    /// How many strokes have been made once `n` parts have been picked (the first one is made before the first pick).
    pub fn strokes_after(&self, n: u64) -> u64 {
        n.div_ceil(self.picks_per_stroke)
    }
}

/// The plan as a table: `feeder,pocket_pitch_mm,stroke_mm,holes_per_stroke,picks_per_stroke,strokes`.
pub fn csv(plan: &DragPlan) -> String {
    let mut out = String::from(
        "feeder,pocket_pitch_mm,stroke_mm,holes_per_stroke,picks_per_stroke,strokes\n",
    );
    for d in &plan.rows {
        out.push_str(&format!(
            "{},{},{},{},{},{}\n",
            crate::parts::csv_field(&d.id),
            crate::msg8::num(d.pocket_pitch_mm),
            crate::msg8::num(d.stroke_mm),
            d.holes_per_stroke,
            d.picks_per_stroke,
            d.strokes
        ));
    }
    out
}
