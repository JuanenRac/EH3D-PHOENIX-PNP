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
            (Side::Bottom, Some(w)) => (w - p.x, (180.0 - p.rotation).rem_euclid(360.0)),
            _ => (p.x, p.rotation),
        };
        todo.push((a.part, a.feeder, (x, p.y), rot));
    }
    // the walk over the places, nearest first, from the corner of the board
    let mut walk: Vec<(usize, usize, (f64, f64), f64)> = Vec::with_capacity(todo.len());
    let mut here = (0.0, 0.0);
    while !todo.is_empty() {
        let i = todo
            .iter()
            .enumerate()
            .min_by(|a, b| {
                dist(here, a.1 .2)
                    .partial_cmp(&dist(here, b.1 .2))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(i, _)| i)
            .unwrap_or(0);
        let next = todo.remove(i);
        here = next.2;
        walk.push(next);
    }
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
