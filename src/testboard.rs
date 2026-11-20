// EH3D-PHOENIX-PNP - boards made up from a seed, to compare machines and settings.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! A table of parts and a table of feeders for a board that does not exist: a grid of 0402 resistors, a mix of passives, LEDs and chips on both sides turned in the four directions, or a dense
//! grid with the parts close together. The same seed gives the same bytes, so two machines, two plans or two settings can be compared on the same board, and the speed of a plan
//! (parts per hour in the simulation) has a board to be measured on. The figures (how many parts, how far apart) are in the settings.

use crate::msg8::num;

/// The most parts of a made-up board.
pub const MAX_PARTS: usize = 5000;

/// The kind of board.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// 0402 resistors on a grid.
    Grid,
    /// Passives, LEDs and chips on both sides, turned in the four directions.
    Mixed,
    /// A grid with the parts half a pitch apart (at least 0.8 mm).
    Dense,
}

impl Kind {
    pub fn parse(text: &str) -> Option<Kind> {
        match text.trim().to_ascii_lowercase().as_str() {
            "grid" => Some(Kind::Grid),
            "mixed" => Some(Kind::Mixed),
            "dense" => Some(Kind::Dense),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Kind::Grid => "grid",
            Kind::Mixed => "mixed",
            Kind::Dense => "dense",
        }
    }
}

/// What to make.
#[derive(Debug, Clone, PartialEq)]
pub struct Spec {
    pub kind: Kind,
    pub seed: u64,
    pub count: usize,
    pub pitch_mm: f64,
}

/// The tables.
#[derive(Debug, Clone, PartialEq)]
pub struct Board {
    pub parts_csv: String,
    pub feeders_csv: String,
    pub parts: usize,
    pub feeders: usize,
    /// The size of the board (mm).
    pub size: (f64, f64),
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

/// The kinds of part a mixed board draws from: value, package and the share of the parts (per thousand).
const MIX: [(&str, &str, usize); 7] = [
    ("10k", "R_0402", 300),
    ("100n", "C_0402", 250),
    ("4k7", "R_0603", 150),
    ("10u", "C_0805", 100),
    ("LED", "LED_0603", 120),
    ("MCU", "QFN-32", 40),
    ("REG", "SOT-23", 40),
];

/// Makes the tables. A count over `MAX_PARTS` or under 1, and a pitch that is not above 0.2 mm, are refused.
pub fn make(spec: &Spec) -> Result<Board, String> {
    if spec.count == 0 || spec.count > MAX_PARTS {
        return Err(format!("the count of parts is from 1 to {MAX_PARTS}"));
    }
    if !spec.pitch_mm.is_finite() || !(0.2..=100.0).contains(&spec.pitch_mm) {
        return Err("the pitch is from 0.2 to 100 mm".into());
    }
    let mut rng = Rng(spec.seed);
    let pitch = match spec.kind {
        Kind::Dense => (spec.pitch_mm / 2.0).max(0.8),
        _ => spec.pitch_mm,
    };
    // the places: cells of a grid, taken in an order that the seed shuffles for a mixed board
    let per_side = if spec.kind == Kind::Mixed {
        spec.count.max(2)
    } else {
        spec.count
    };
    let cols = (per_side as f64).sqrt().ceil() as usize;
    let rows = per_side.div_ceil(cols);
    let mut cells: Vec<usize> = (0..cols * rows).collect();
    if spec.kind == Kind::Mixed {
        for i in (1..cells.len()).rev() {
            let j = rng.below(i + 1);
            cells.swap(i, j);
        }
    }
    let origin = 10.0;
    let at = |cell: usize| {
        (
            origin + (cell % cols) as f64 * pitch,
            origin + (cell / cols) as f64 * pitch,
        )
    };
    let mut parts = String::from("ref,val,package,x,y,rot,side\n");
    let mut kinds: Vec<(String, String)> = Vec::new();
    let mut counts: Vec<usize> = Vec::new();
    for (i, cell) in cells.iter().copied().enumerate().take(spec.count) {
        let (value, package, rotation, side) = match spec.kind {
            Kind::Grid | Kind::Dense => ("10k", "R_0402", 0, "top"),
            Kind::Mixed => {
                let mut roll = rng.below(1000);
                let mut pick = MIX[0];
                for m in MIX {
                    if roll < m.2 {
                        pick = m;
                        break;
                    }
                    roll -= m.2;
                }
                let rot = [0, 90, 180, 270][rng.below(4)];
                let side = if rng.below(4) == 0 { "bottom" } else { "top" };
                (pick.0, pick.1, rot, side)
            }
        };
        let letter = match package {
            p if p.starts_with("R_") => 'R',
            p if p.starts_with("C_") => 'C',
            p if p.starts_with("LED") => 'D',
            p if p.starts_with("QFN") => 'U',
            _ => 'Q',
        };
        let (x, y) = at(cell);
        parts.push_str(&format!(
            "{letter}{},{value},{package},{},{},{rotation},{side}\n",
            i + 1,
            num(x),
            num(y)
        ));
        match kinds.iter().position(|k| k.0 == value && k.1 == package) {
            Some(k) => counts[k] += 1,
            None => {
                kinds.push((value.to_string(), package.to_string()));
                counts.push(1);
            }
        }
    }
    let mut feeders = String::from("id,value,package,x,y,count\n");
    for (n, ((value, package), c)) in kinds.iter().zip(&counts).enumerate() {
        feeders.push_str(&format!(
            "F{},{value},{package},-20,{},{}\n",
            n + 1,
            10 + 10 * n,
            c + 10
        ));
    }
    Ok(Board {
        parts_csv: parts,
        feeders_csv: feeders,
        parts: spec.count,
        feeders: kinds.len(),
        size: (
            origin * 2.0 + cols as f64 * pitch,
            origin * 2.0 + rows as f64 * pitch,
        ),
    })
}
