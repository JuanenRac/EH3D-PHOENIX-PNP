// EH3D-PHOENIX-PNP - readings made by the simulated camera, to try the checks of the parts and of the fiducials.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! `part-inspect` and `fiducial-check` judge readings that a camera gave. Without a machine there are no readings, so this makes them: for every part it draws the picture that a camera
//! under the nozzle would give (`vision::render`, with the noise, the blur, the uneven light, the pixels that turn black or white and the spoilt pictures that `vision.cfg` asks for), measures
//! it with the pipeline of the same file and gives the reading in the files those checks read. A part hangs a little off the axis and a little turned, by chance within the spread that is
//! asked for; a mark of a panel is looked at from where the design puts it, and the panel may lie somewhere else than the design says. A picture in which the pipeline finds no part (a
//! spoilt one, a part that hangs out of the picture) gives no reading and is told, with its reason, so that what the checks do with a missing reading can be tried too.
//!
//! Every picture is drawn from a seed and the number of the part, so the same files give the same readings, and a part does not change when another is added.

use crate::inspect::{Measured, SizeRow};
use crate::model::Offset;
use crate::parts::csv_field;
use crate::sim::Rng;
use crate::vision::{measure_image, render, VisionConfig};

/// How far, by chance, a part hangs from the axis of the nozzle and how much it is turned, and which parts are the wrong ones.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spread {
    /// Either way, in millimetres, along each axis.
    pub offset_mm: f64,
    /// Either way, in degrees.
    pub turn_deg: f64,
    /// Every this many parts one is a part of another size (half as long again), as when another value is in the tape; 0 for none.
    pub wrong_every: usize,
}

impl Default for Spread {
    fn default() -> Self {
        Spread {
            offset_mm: 0.2,
            turn_deg: 2.0,
            wrong_every: 0,
        }
    }
}

/// What the camera made of one part.
#[derive(Debug, Clone, PartialEq)]
pub struct PartSeen {
    pub reference: String,
    pub package: String,
    /// The reading, or why the pipeline found no part.
    pub outcome: Result<Measured, String>,
}

/// The seed of the picture of the `index`-th item: it depends on the number and not on the others.
fn seed_of(seed: u64, index: usize) -> u64 {
    seed.wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add((index as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9))
}

/// A number from -1 to 1.
fn spread_draw(rng: &mut Rng) -> f64 {
    2.0 * rng.next_f64() - 1.0
}

/// The readings of the parts `items` (reference, package and the size of the package) that the simulated camera gives, with the settings `cfg` and the `spread`; `seed` makes them repeat.
pub fn read_parts(
    items: &[(String, String, SizeRow)],
    cfg: &VisionConfig,
    spread: &Spread,
    seed: u64,
) -> Vec<PartSeen> {
    items
        .iter()
        .enumerate()
        .map(|(i, (reference, package, size))| {
            let mut truth_rng = Rng::new(seed_of(seed, i));
            let truth = Offset {
                dx_mm: spread_draw(&mut truth_rng) * spread.offset_mm,
                dy_mm: spread_draw(&mut truth_rng) * spread.offset_mm,
                rotation_deg: spread_draw(&mut truth_rng) * spread.turn_deg,
            };
            let wrong = spread.wrong_every > 0 && (i + 1) % spread.wrong_every == 0;
            let grow = if wrong { 1.5 } else { 1.0 };
            let mut part_cfg = cfg.clone();
            part_cfg.sim_part_long_mm = (size.long_mm * grow).clamp(0.05, 50.0);
            part_cfg.sim_part_short_mm = (size.short_mm * grow)
                .clamp(0.05, 50.0)
                .min(part_cfg.sim_part_long_mm);
            let mut picture_rng = Rng::new(seed_of(seed, i) ^ 0x5555_5555_5555_5555);
            let picture = render(&part_cfg, truth, &mut picture_rng);
            let outcome = measure_image(&part_cfg, &picture)
                .map(|r| Measured::from_reading(&r, part_cfg.mm_per_px))
                .map_err(|e| e.detail);
            PartSeen {
                reference: reference.clone(),
                package: package.clone(),
                outcome,
            }
        })
        .collect()
}

/// The readings as the file that `part-inspect` reads: `ref,package,long_mm,short_mm,dx_mm,dy_mm,rot_deg,exp_rot_deg`. A part with no reading is left out.
pub fn readings_csv(seen: &[PartSeen]) -> String {
    let mut out = String::from("ref,package,long_mm,short_mm,dx_mm,dy_mm,rot_deg,exp_rot_deg\n");
    for s in seen {
        if let Ok(m) = &s.outcome {
            out.push_str(&format!(
                "{},{},{:.3},{:.3},{:.3},{:.3},{:.2},0\n",
                csv_field(&s.reference),
                csv_field(&s.package),
                m.long_mm,
                m.short_mm,
                m.dx_mm,
                m.dy_mm,
                m.rotation_deg
            ));
        }
    }
    out
}

/// Which parts got no reading and why, one per line, as `REF: reason`.
pub fn part_failures(seen: &[PartSeen]) -> Vec<String> {
    seen.iter()
        .filter_map(|s| {
            s.outcome
                .as_ref()
                .err()
                .map(|e| format!("{}: {e}", s.reference))
        })
        .collect()
}

/// How the panel really lies, against where the design puts it: the design is scaled, turned about the origin and shifted.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PanelTruth {
    pub shift: (f64, f64),
    pub turn_deg: f64,
    pub scale: f64,
}

impl Default for PanelTruth {
    fn default() -> Self {
        PanelTruth {
            shift: (0.0, 0.0),
            turn_deg: 0.0,
            scale: 1.0,
        }
    }
}

impl PanelTruth {
    /// Where a place of the design really is.
    pub fn apply(&self, x: f64, y: f64) -> (f64, f64) {
        let (s, c) = self.turn_deg.to_radians().sin_cos();
        (
            self.scale * (c * x - s * y) + self.shift.0,
            self.scale * (s * x + c * y) + self.shift.1,
        )
    }
}

/// What the camera made of one mark.
#[derive(Debug, Clone, PartialEq)]
pub struct MarkSeen {
    pub id: String,
    /// The place measured, or why the pipeline found no mark.
    pub outcome: Result<(f64, f64), String>,
}

/// The measured places of the marks `design` (id, x, y) of a panel that lies as `truth` says. The camera goes to the place that the design gives and sees the mark off the middle of the
/// picture by as much as the panel is off the design; a mark is a square of `dot_mm` on a side. The measured place is the one of the design plus what the pipeline read.
pub fn read_marks(
    design: &[(String, f64, f64)],
    truth: &PanelTruth,
    cfg: &VisionConfig,
    dot_mm: f64,
    seed: u64,
) -> Vec<MarkSeen> {
    design
        .iter()
        .enumerate()
        .map(|(i, (id, x, y))| {
            let real = truth.apply(*x, *y);
            let off = Offset {
                dx_mm: real.0 - x,
                dy_mm: real.1 - y,
                rotation_deg: 0.0,
            };
            let mut mark_cfg = cfg.clone();
            mark_cfg.sim_part_long_mm = dot_mm.clamp(0.05, 50.0);
            mark_cfg.sim_part_short_mm = mark_cfg.sim_part_long_mm;
            let mut rng = Rng::new(seed_of(seed, i));
            let picture = render(&mark_cfg, off, &mut rng);
            let outcome = measure_image(&mark_cfg, &picture)
                .map(|r| (x + r.offset.dx_mm, y + r.offset.dy_mm))
                .map_err(|e| e.detail);
            MarkSeen {
                id: id.clone(),
                outcome,
            }
        })
        .collect()
}

/// The marks as the file that `fiducial-check` reads: `id,x,y`. A mark with no reading is left out.
pub fn marks_csv(seen: &[MarkSeen]) -> String {
    let mut out = String::from("id,x,y\n");
    for s in seen {
        if let Ok((x, y)) = &s.outcome {
            out.push_str(&format!("{},{:.4},{:.4}\n", csv_field(&s.id), x, y));
        }
    }
    out
}

/// Which marks got no reading and why, one per line, as `ID: reason`.
pub fn mark_failures(seen: &[MarkSeen]) -> Vec<String> {
    seen.iter()
        .filter_map(|s| s.outcome.as_ref().err().map(|e| format!("{}: {e}", s.id)))
        .collect()
}
