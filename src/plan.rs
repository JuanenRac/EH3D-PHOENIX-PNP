//! The order in which the parts are placed. A machine with several nozzles picks a batch of parts, goes to the board and places them; the plan walks the places nearest-first
//! and cuts the walk in batches, and inside a batch the feeders are visited nearest-first too.

use crate::feeders::{Assigned, Feeder};
use crate::parts::{Part, Side};
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    /// How many parts the head carries at once, from 1 to 6.
    pub nozzles: usize,
    /// The side of the board that is placed.
    pub side: Side,
    /// For the bottom side: the width of the board; the x of every part is mirrored about its middle.
    pub board_width_mm: Option<f64>,
    /// For the bottom side: where the left edge of the board is on the X axis, in millimetres. The mirror is about the middle of the board, `origin_x_mm + board_width_mm / 2`;
    /// with the default 0 the board is taken to start at X 0, as a position file of a board placed at the origin has it.
    pub origin_x_mm: f64,
    /// The speed of the head, in mm/s.
    pub speed_mm_s: f64,
    /// The time to pick a part and to place it, in seconds.
    pub pick_s: f64,
    pub place_s: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            nozzles: 1,
            side: Side::Top,
            board_width_mm: None,
            origin_x_mm: 0.0,
            speed_mm_s: 150.0,
            pick_s: 0.4,
            place_s: 0.4,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Placement {
    pub reference: String,
    pub feeder: String,
    pub pick: (f64, f64),
    pub x: f64,
    pub y: f64,
    pub rotation: f64,
    pub nozzle: usize,
    pub batch: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub placements: Vec<Placement>,
    pub batches: usize,
    pub travel_mm: f64,
    pub seconds: f64,
}

fn dist(a: (f64, f64), b: (f64, f64)) -> f64 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

/// The order of a walk over `points` that always goes to the nearest one that is left, from `start`; of two at the same distance the first of the list goes first. The points go in
/// a grid of about one per cell and the nearest is looked for ring by ring, so a board of fifty thousand parts is walked in moments (the plain walk is quadratic).
pub fn nearest_walk(points: &[(f64, f64)], start: (f64, f64)) -> Vec<usize> {
    let n = points.len();
    let mut order = Vec::with_capacity(n);
    if n == 0 {
        return order;
    }
    let (mut x0, mut y0) = (f64::INFINITY, f64::INFINITY);
    let (mut x1, mut y1) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for p in points {
        x0 = x0.min(p.0);
        y0 = y0.min(p.1);
        x1 = x1.max(p.0);
        y1 = y1.max(p.1);
    }
    let side = ((n as f64).sqrt().ceil() as usize).clamp(1, 2048);
    let cw = ((x1 - x0) / side as f64).max(1e-9);
    let ch = ((y1 - y0) / side as f64).max(1e-9);
    let cell = |p: (f64, f64)| -> (usize, usize) {
        let cx = (((p.0 - x0) / cw).floor().max(0.0) as usize).min(side - 1);
        let cy = (((p.1 - y0) / ch).floor().max(0.0) as usize).min(side - 1);
        (cx, cy)
    };
    let mut grid: Vec<Vec<usize>> = vec![Vec::new(); side * side];
    for (k, p) in points.iter().enumerate() {
        let (cx, cy) = cell(*p);
        grid[cy * side + cx].push(k);
    }
    let smallest_cell = cw.min(ch);
    let mut at = start;
    for _ in 0..n {
        let (cx, cy) = cell(at);
        let mut best: Option<(f64, usize)> = None;
        for r in 0..=side as i64 {
            for dx in -r..=r {
                let on_edge = dx.abs() == r;
                let mut dy = -r;
                while dy <= r {
                    let (gx, gy) = (cx as i64 + dx, cy as i64 + dy);
                    if gx >= 0 && gy >= 0 && (gx as usize) < side && (gy as usize) < side {
                        for &k in &grid[gy as usize * side + gx as usize] {
                            let d = dist(at, points[k]);
                            let better = match best {
                                None => true,
                                Some((bd, bk)) => d < bd || (d == bd && k < bk),
                            };
                            if better {
                                best = Some((d, k));
                            }
                        }
                    }
                    // inside the ring only its two sides are visited; on its top and bottom rows every cell
                    dy += if on_edge || r == 0 { 1 } else { 2 * r };
                }
            }
            if let Some((bd, _)) = best {
                if bd < r as f64 * smallest_cell {
                    break;
                }
            }
        }
        let Some((_, k)) = best else { break };
        let (kx, ky) = cell(points[k]);
        let bucket = &mut grid[ky * side + kx];
        if let Some(pos) = bucket.iter().position(|&m| m == k) {
            bucket.swap_remove(pos);
        }
        at = points[k];
        order.push(k);
    }
    order
}

/// Plans the parts that have a feeder (see `feeders::assign`), on the side of `s`.
pub fn plan(
    parts: &[Part],
    feeders: &[Feeder],
    assigned: &Assigned,
    s: &Settings,
) -> Result<Plan, String> {
    if !(1..=6).contains(&s.nozzles) {
        return Err(format!("nozzles is {}: it must be from 1 to 6", s.nozzles));
    }
    for (name, v) in [
        ("speed_mm_s", s.speed_mm_s),
        ("pick_s", s.pick_s),
        ("place_s", s.place_s),
    ] {
        if !v.is_finite() || v < 0.0 || (name == "speed_mm_s" && v == 0.0) {
            return Err(format!(
                "{name} is {v}: it cannot be negative, not a number, or zero for the speed"
            ));
        }
    }
    if s.side == Side::Bottom && !s.board_width_mm.is_some_and(|w| w.is_finite() && w > 0.0) {
        return Err(
            "the bottom side needs the width of the board (board_width_mm) to be mirrored".into(),
        );
    }
    if !s.origin_x_mm.is_finite() {
        return Err(format!(
            "origin_x_mm is {}: it must be a number",
            s.origin_x_mm
        ));
    }
    // every part of this side with its feeder, and where it goes
    let mut todo: Vec<(usize, usize, (f64, f64), f64)> = Vec::new();
    for a in &assigned.assignments {
        let p = parts.get(a.part).ok_or_else(|| {
            format!(
                "the assignment points at the part {} that does not exist",
                a.part
            )
        })?;
        feeders.get(a.feeder).ok_or_else(|| {
            format!(
                "the assignment points at the feeder {} that does not exist",
                a.feeder
            )
        })?;
        if p.side != s.side {
            continue;
        }
        let (x, rot) = match (s.side, s.board_width_mm) {
            (Side::Bottom, Some(w)) => (
                2.0 * s.origin_x_mm + w - p.x,
                (180.0 - p.rotation).rem_euclid(360.0),
            ),
            _ => (p.x, p.rotation),
        };
        todo.push((a.part, a.feeder, (x, p.y), rot));
    }
    // the walk over the places, nearest first, from the corner of the board
    let points: Vec<(f64, f64)> = todo.iter().map(|t| t.2).collect();
    let walk: Vec<(usize, usize, (f64, f64), f64)> = nearest_walk(&points, (0.0, 0.0))
        .into_iter()
        .map(|k| todo[k])
        .collect();
    let (mut placements, mut travel, mut head) = (Vec::with_capacity(walk.len()), 0.0, (0.0, 0.0));
    for (batch, chunk) in walk.chunks(s.nozzles).enumerate() {
        // the order in which the feeders of the batch are visited
        let mut order: Vec<usize> = (0..chunk.len()).collect();
        let mut picked: Vec<usize> = Vec::new();
        while !order.is_empty() {
            let at = picked
                .last()
                .map_or(head, |&k| (feeders[chunk[k].1].x, feeders[chunk[k].1].y));
            let i = order
                .iter()
                .enumerate()
                .min_by(|a, b| {
                    dist(at, (feeders[chunk[*a.1].1].x, feeders[chunk[*a.1].1].y))
                        .partial_cmp(&dist(
                            at,
                            (feeders[chunk[*b.1].1].x, feeders[chunk[*b.1].1].y),
                        ))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|(i, _)| i)
                .unwrap_or(0);
            picked.push(order.remove(i));
        }
        for &k in &picked {
            let f = &feeders[chunk[k].1];
            travel += dist(head, (f.x, f.y));
            head = (f.x, f.y);
        }
        for (nozzle, &k) in picked.iter().enumerate() {
            let (pi, fi, at, rot) = chunk[k];
            travel += dist(head, at);
            head = at;
            placements.push(Placement {
                reference: parts[pi].reference.clone(),
                feeder: feeders[fi].id.clone(),
                pick: (feeders[fi].x, feeders[fi].y),
                x: at.0,
                y: at.1,
                rotation: rot,
                nozzle,
                batch,
            });
        }
    }
    let n = placements.len() as f64;
    Ok(Plan {
        batches: walk.len().div_ceil(s.nozzles),
        seconds: travel / s.speed_mm_s + n * (s.pick_s + s.place_s),
        travel_mm: travel,
        placements,
    })
}

impl Plan {
    pub fn to_json(&self) -> String {
        let list: Vec<Value> = self
            .placements
            .iter()
            .map(|p| json!({"reference": p.reference, "feeder": p.feeder, "pick": {"x": p.pick.0, "y": p.pick.1}, "place": {"x": p.x, "y": p.y, "rotation": p.rotation}, "nozzle": p.nozzle, "batch": p.batch}))
            .collect();
        serde_json::to_string_pretty(&json!({"schema_version": 1, "batches": self.batches, "travel_mm": self.travel_mm, "seconds": self.seconds, "placements": list})).unwrap_or_default() + "\n"
    }
}
