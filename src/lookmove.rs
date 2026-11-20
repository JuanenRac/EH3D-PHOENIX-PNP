// EH3D-PHOENIX-PNP - look, move, look again: bringing the head to a mark in small steps.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! A camera does not tell the whole way to a mark in one look: the lens bends, the scale is a little off and the head does not land where it is told. So the alignment looks, moves a part of
//! what it saw, and looks again, until what is left is under the tolerance. No step is longer than the cap of the settings, a look that gets worse halves the cap and the gain for the rest of
//! the run (they never go up again), two worse looks in a row stop it, and a look that cannot be read stops it with no move. Everything here is arithmetic over looks that are handed to it: it
//! moves nothing, and `Sim` makes up the looks (a true error, a scale error of the camera, noise) so that a setting can be tried before a machine exists.

use crate::config::MachineConfig;
use crate::error::Lang;
use crate::msg8::{num, say};

/// The figures of the alignment (the settings `lml_*`).
#[derive(Debug, Clone, PartialEq)]
pub struct Params {
    pub max_step_mm: f64,
    pub tolerance_mm: f64,
    pub max_iterations: u32,
    pub gain: f64,
    /// The constant error of the camera against the nozzle, subtracted from what is seen.
    pub offset: (f64, f64),
}

impl Params {
    pub fn from_config(c: &MachineConfig) -> Params {
        Params {
            max_step_mm: c.lml_max_step_mm,
            tolerance_mm: c.lml_tolerance_mm,
            max_iterations: c.lml_max_iterations,
            gain: c.lml_gain,
            offset: (c.lml_offset_x_mm, c.lml_offset_y_mm),
        }
    }
}

/// How the alignment ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// What is left is under the tolerance.
    Aligned,
    /// The iterations ran out.
    NotConverged,
    /// The looks got worse twice in a row: it stopped.
    Diverging,
    /// A look could not be read: it stopped with no move.
    Unreadable,
}

impl Outcome {
    pub fn name(self) -> &'static str {
        match self {
            Outcome::Aligned => "aligned",
            Outcome::NotConverged => "not_converged",
            Outcome::Diverging => "diverging",
            Outcome::Unreadable => "unreadable",
        }
    }
}

/// One look and the move that followed it.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub iteration: u32,
    /// What was seen: the error of the head against the mark (mm).
    pub seen: (f64, f64),
    /// The length of what is left after the offset is taken off (mm).
    pub residual_mm: f64,
    /// The move that was made (mm); zero when the look was inside the tolerance.
    pub moved: (f64, f64),
    /// Whether the move was cut to the cap.
    pub clipped: bool,
    /// The cap on a step at this look.
    pub cap_mm: f64,
    /// The gain at this look.
    pub gain: f64,
}

/// The whole alignment.
#[derive(Debug, Clone, PartialEq)]
pub struct Aligned {
    pub outcome: Outcome,
    pub steps: Vec<Step>,
    /// The sum of the moves (mm).
    pub total: (f64, f64),
}

fn length(v: (f64, f64)) -> f64 {
    (v.0 * v.0 + v.1 * v.1).sqrt()
}

/// Looks, moves and looks again. `look(x, y)` is handed the place of the head (the sum of the moves so far, from where it began) and gives the error that the camera sees there, or `None`
/// when it cannot read it.
pub fn align<F: FnMut(f64, f64) -> Option<(f64, f64)>>(p: &Params, mut look: F) -> Aligned {
    let mut steps: Vec<Step> = Vec::new();
    let mut at = (0.0f64, 0.0f64);
    let mut cap = p.max_step_mm;
    let mut gain = p.gain;
    let mut last: Option<f64> = None;
    let mut worse_in_a_row = 0u32;
    for iteration in 1..=p.max_iterations.saturating_add(1) {
        let Some(seen) = look(at.0, at.1) else {
            return Aligned {
                outcome: Outcome::Unreadable,
                steps,
                total: at,
            };
        };
        if !(seen.0.is_finite() && seen.1.is_finite()) {
            return Aligned {
                outcome: Outcome::Unreadable,
                steps,
                total: at,
            };
        }
        let need = (seen.0 - p.offset.0, seen.1 - p.offset.1);
        let residual = length(need);
        if let Some(before) = last {
            if residual > before {
                // worse than the look before: smaller steps from now on, and never bigger again
                worse_in_a_row += 1;
                cap *= 0.5;
                gain *= 0.5;
            } else {
                worse_in_a_row = 0;
            }
        }
        let mut step = Step {
            iteration,
            seen,
            residual_mm: residual,
            moved: (0.0, 0.0),
            clipped: false,
            cap_mm: cap,
            gain,
        };
        if residual <= p.tolerance_mm {
            steps.push(step);
            return Aligned {
                outcome: Outcome::Aligned,
                steps,
                total: at,
            };
        }
        if worse_in_a_row >= 2 {
            steps.push(step);
            return Aligned {
                outcome: Outcome::Diverging,
                steps,
                total: at,
            };
        }
        if iteration > p.max_iterations {
            steps.push(step);
            return Aligned {
                outcome: Outcome::NotConverged,
                steps,
                total: at,
            };
        }
        let mut mv = (need.0 * gain, need.1 * gain);
        let len = length(mv);
        if len > cap {
            mv = (mv.0 * cap / len, mv.1 * cap / len);
            step.clipped = true;
        }
        step.moved = mv;
        at = (at.0 + mv.0, at.1 + mv.1);
        last = Some(residual);
        steps.push(step);
    }
    Aligned {
        outcome: Outcome::NotConverged,
        steps,
        total: at,
    }
}

/// A camera that is made up: the mark is `error` away from the head at the start, the camera sees `scale` times what is there plus a noise.
#[derive(Debug, Clone, PartialEq)]
pub struct Sim {
    pub error: (f64, f64),
    pub scale: f64,
    pub noise_mm: f64,
    pub seed: u64,
    /// How many looks to leave unreadable (from the first).
    pub blind_looks: u32,
}

fn unit(seed: &mut u64) -> f64 {
    *seed = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *seed;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 11) as f64 / (1u64 << 53) as f64
}

impl Sim {
    /// Aligns against the made-up camera.
    pub fn run(&self, p: &Params) -> Aligned {
        let mut seed = self.seed;
        let mut looks = 0u32;
        align(p, |x, y| {
            looks += 1;
            if looks <= self.blind_looks {
                return None;
            }
            let n = (
                (unit(&mut seed) * 2.0 - 1.0) * self.noise_mm,
                (unit(&mut seed) * 2.0 - 1.0) * self.noise_mm,
            );
            Some((
                (self.error.0 - x) * self.scale + n.0,
                (self.error.1 - y) * self.scale + n.1,
            ))
        })
    }
}

/// The alignment as a table in a language.
pub fn report(a: &Aligned, lang: Lang) -> String {
    let head: [&str; 8] = [
        "look   seen X    seen Y   left     move X   move Y   cap",
        "mirada vista X   vista Y  queda    mov. X   mov. Y   tope",
        "Blick  gesehen X gesehen Y Rest    Fahrt X  Fahrt Y  Grenze",
        "regard vu X      vu Y     reste    dép. X   dép. Y   plafond",
        "guarda visto X   visto Y  resta    mov. X   mov. Y   tetto",
        "確認   X        Y        残り     移動 X   移動 Y   上限",
        "查看   X        Y        剩余     移动 X   移动 Y   上限",
        "olhar  visto X   visto Y  resta    mov. X   mov. Y   limite",
    ];
    let mut out = String::new();
    out.push_str(crate::msg8::pick(lang, &head));
    out.push('\n');
    for s in &a.steps {
        out.push_str(&format!(
            "{:<6} {:>8} {:>8} {:>8} {:>8} {:>8} {:>5}{}\n",
            s.iteration,
            num(round3(s.seen.0)),
            num(round3(s.seen.1)),
            num(round3(s.residual_mm)),
            num(round3(s.moved.0)),
            num(round3(s.moved.1)),
            num(round3(s.cap_mm)),
            if s.clipped { " *" } else { "" }
        ));
    }
    let row: [&str; 8] = [
        "{outcome}: {n} looks, the head moved {x} mm in X and {y} mm in Y",
        "{outcome}: {n} miradas, la cabeza se movió {x} mm en X y {y} mm en Y",
        "{outcome}: {n} Blicke, der Kopf bewegte sich {x} mm in X und {y} mm in Y",
        "{outcome} : {n} regards, la tête a bougé de {x} mm en X et de {y} mm en Y",
        "{outcome}: {n} sguardi, la testa si è mossa di {x} mm in X e di {y} mm in Y",
        "{outcome}：{n} 回確認し、ヘッドは X に {x} mm、Y に {y} mm 動きました",
        "{outcome}：查看 {n} 次，头部在 X 方向移动 {x} mm，Y 方向移动 {y} mm",
        "{outcome}: {n} olhares, a cabeça moveu-se {x} mm em X e {y} mm em Y",
    ];
    out.push_str(&say(
        lang,
        &row,
        &[
            ("outcome", a.outcome.name().to_string()),
            ("n", a.steps.len().to_string()),
            ("x", num(round3(a.total.0))),
            ("y", num(round3(a.total.1))),
        ],
    ));
    out.push('\n');
    out
}

fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}
