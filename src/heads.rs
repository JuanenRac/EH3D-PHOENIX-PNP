//! Machines with more than one head. Each head has its own nozzles, its own place with respect to the frame of the machine (an offset) and its own camera, and moves on its own; the
//! feeders are shared, and a feeder serves one head at a time. The planner splits the parts between the heads, plans each head as a machine of its own, and the clock of the machine
//! puts the plans side by side: a head that reaches a feeder that another head is using waits.
//!
//! * A head with offset `(ox, oy)` puts its first nozzle over a place when the carriage is at that place less the offset, so a place is out of reach of a head when the carriage would
//!   have to go outside the limits of the machine (`limit_*`), both to pick and to place.
//! * The parts are given out heaviest first (the time of a part is its feed, its pick, its place and the way from the feeder to the place, shared by the nozzles of the head), each to
//!   the head that would finish it first among those that reach it; a head that has not used the tip of the part yet pays the change of tip. Equal costs go to the first head.
//! * The plans of the heads are in the coordinates of the carriage of each head; `HeadPlan::board_xy` gives a place back in the coordinates of the board.

use crate::catalog::Catalog;
use crate::config::{HeadSpec, MachineConfig};
use crate::error::{Code, PnpError};
use crate::feeding::{FeederSpec, Pick, Picks};
use crate::parts::{Part, Side};
use crate::planner::{plan_advanced, AdvancedPlan, Options, PickTime};
use serde_json::{json, Value};
use std::collections::{BTreeSet, HashMap};

/// The plan of one head.
#[derive(Debug, Clone, PartialEq)]
pub struct HeadPlan {
    pub head: HeadSpec,
    /// The plan, in the coordinates of the carriage of the head.
    pub plan: AdvancedPlan,
    /// The seconds the head waits for a feeder that another head is using.
    pub wait_s: f64,
    /// When the head is done, counting the waits.
    pub finish_s: f64,
}

impl HeadPlan {
    /// The place of the placement number `index` of the plan in the coordinates of the board.
    pub fn board_xy(&self, index: usize) -> Option<(f64, f64)> {
        let p = self.plan.plan.placements.get(index)?;
        Some((p.x + self.head.offset_x_mm, p.y + self.head.offset_y_mm))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct HeadsPlan {
    pub heads: Vec<HeadPlan>,
    /// The seconds until the last head is done.
    pub seconds: f64,
    /// The seconds the heads wait, all together.
    pub wait_s: f64,
}

/// When every head is done and how long it waited, given for each head the seconds its plan takes alone and the arrival at the feeders of its picks. The heads are served in the order of
/// their arrivals (the first head among equal times); a pick that arrives while its feeder is busy waits for it, and everything after it in that head moves later by the same time.
pub fn schedule(heads: &[(f64, &[PickTime])]) -> Vec<(f64, f64)> {
    let n = heads.len();
    let mut next = vec![0usize; n];
    let mut shift = vec![0.0f64; n];
    let mut free: HashMap<&str, f64> = HashMap::new();
    loop {
        let mut best: Option<(usize, f64)> = None;
        for (h, (_, picks)) in heads.iter().enumerate() {
            if let Some(pick) = picks.get(next[h]) {
                let t = shift[h] + pick.at_s;
                if best.is_none_or(|(_, b)| t < b) {
                    best = Some((h, t));
                }
            }
        }
        let Some((h, t)) = best else { break };
        let pick = &heads[h].1[next[h]];
        let start = t.max(free.get(pick.feeder.as_str()).copied().unwrap_or(0.0));
        shift[h] += start - t;
        free.insert(pick.feeder.as_str(), start + pick.busy_s);
        next[h] += 1;
    }
    (0..n).map(|h| (heads[h].0 + shift[h], shift[h])).collect()
}

/// Whether the carriage can put the first nozzle of the head over `place` within the limits of the machine.
pub fn reaches(cfg: &MachineConfig, head: &HeadSpec, place: (f64, f64)) -> bool {
    let (x, y) = (place.0 - head.offset_x_mm, place.1 - head.offset_y_mm);
    x >= cfg.limit_x_min_mm
        && x <= cfg.limit_x_max_mm
        && y >= cfg.limit_y_min_mm
        && y <= cfg.limit_y_max_mm
}

fn dist(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

/// Plans the parts of one side of the board for the heads of the configuration (one head with `nozzles` nozzles when it declares none, which gives the plan of `plan_advanced`).
pub fn plan_heads(
    parts: &[Part],
    feeders: &[FeederSpec],
    picks: &Picks,
    catalog: &Catalog,
    cfg: &MachineConfig,
    opts: &Options,
) -> Result<HeadsPlan, PnpError> {
    let setting = |m: String| PnpError::new(Code::PlanSetting, m);
    if opts.side == Side::Bottom
        && !opts
            .board_width_mm
            .is_some_and(|w| w.is_finite() && w > 0.0)
    {
        return Err(setting(
            "the bottom side needs the width of the board to be mirrored".to_string(),
        ));
    }
    let heads = cfg.head_list();
    let tip_of = crate::tips::resolve(parts, picks, opts.side, catalog, cfg);
    let defaults = cfg.part_defaults();
    let mean_speed = (cfg.x_speed_mm_s + cfg.y_speed_mm_s) / 2.0;
    // what each pick of this side weighs and where it can go
    struct Entry {
        pick: usize,
        reference: String,
        tip: String,
        weight: f64,
        reach: Vec<bool>,
    }
    let mut entries: Vec<Entry> = Vec::new();
    for (i, pick) in picks.picks.iter().enumerate() {
        let p = parts.get(pick.part).ok_or_else(|| {
            setting(format!(
                "the pick points at the part {} that does not exist",
                pick.part
            ))
        })?;
        if p.side != opts.side {
            continue;
        }
        let x = match (opts.side, opts.board_width_mm) {
            (Side::Bottom, Some(w)) => 2.0 * opts.origin_x_mm + w - p.x,
            _ => p.x,
        };
        let place = (x, p.y);
        let reach: Vec<bool> = heads
            .iter()
            .map(|h| reaches(cfg, h, place) && reaches(cfg, h, pick.at))
            .collect();
        if !reach.iter().any(|&r| r) {
            return Err(PnpError::new(
                Code::HeadUnreachable,
                format!(
                    "part {} (place {:.3}, {:.3}; pick {:.3}, {:.3})",
                    p.reference, place.0, place.1, pick.at.0, pick.at.1
                ),
            ));
        }
        entries.push(Entry {
            pick: i,
            reference: p.reference.clone(),
            tip: tip_of
                .get(&p.key())
                .cloned()
                .unwrap_or_else(|| catalog.resolve(p, &defaults).nozzle_tip),
            weight: cfg.feed_s + cfg.pick_s + cfg.place_s + dist(pick.at, place) / mean_speed,
            reach,
        });
    }
    // heaviest first; equal weights by reference, then by pick, so that the files do not change the result
    entries.sort_by(|a, b| {
        b.weight
            .total_cmp(&a.weight)
            .then_with(|| a.reference.cmp(&b.reference))
            .then_with(|| a.pick.cmp(&b.pick))
    });
    let mut load = vec![0.0f64; heads.len()];
    let mut tips: Vec<BTreeSet<String>> = vec![BTreeSet::new(); heads.len()];
    let mut given: Vec<Vec<usize>> = vec![Vec::new(); heads.len()];
    for e in &entries {
        let mut best: Option<(usize, f64)> = None;
        for (h, head) in heads.iter().enumerate() {
            if !e.reach[h] {
                continue;
            }
            let change = if tips[h].is_empty() || tips[h].contains(&e.tip) {
                0.0
            } else {
                cfg.tip_change_s
            };
            let finish = load[h] + e.weight / head.nozzles as f64 + change;
            if best.is_none_or(|(_, b)| finish < b) {
                best = Some((h, finish));
            }
        }
        if let Some((h, finish)) = best {
            load[h] = finish;
            tips[h].insert(e.tip.clone());
            given[h].push(e.pick);
        }
    }
    // each head is a machine of its own, in the coordinates of its carriage
    let mut planned: Vec<(HeadSpec, AdvancedPlan)> = Vec::with_capacity(heads.len());
    for (h, head) in heads.iter().enumerate() {
        let (ox, oy) = (head.offset_x_mm, head.offset_y_mm);
        let mut head_cfg = cfg.clone();
        head_cfg.nozzles = head.nozzles;
        head_cfg.heads = Vec::new();
        head_cfg.tip_changer = cfg.tip_changer.map(|(x, y)| (x - ox, y - oy));
        let moved: Vec<Part> = parts
            .iter()
            .map(|p| Part {
                x: p.x - ox,
                y: p.y - oy,
                ..p.clone()
            })
            .collect();
        given[h].sort_unstable();
        let head_picks = Picks {
            picks: given[h]
                .iter()
                .map(|&i| {
                    let pick = &picks.picks[i];
                    Pick {
                        at: (pick.at.0 - ox, pick.at.1 - oy),
                        ..pick.clone()
                    }
                })
                .collect(),
            ..Picks::default()
        };
        let head_opts = Options {
            origin_x_mm: opts.origin_x_mm - ox,
            ..*opts
        };
        let plan = plan_advanced(&moved, feeders, &head_picks, catalog, &head_cfg, &head_opts)?;
        planned.push((head.clone(), plan));
    }
    let timelines: Vec<(f64, &[PickTime])> = planned
        .iter()
        .map(|(_, p)| (p.plan.seconds, p.pick_times.as_slice()))
        .collect();
    let times = schedule(&timelines);
    let seconds = times.iter().map(|t| t.0).fold(0.0, f64::max);
    let wait_s = times.iter().map(|t| t.1).sum();
    let heads = planned
        .into_iter()
        .zip(times)
        .map(|((head, plan), (finish_s, wait_s))| HeadPlan {
            head,
            plan,
            wait_s,
            finish_s,
        })
        .collect();
    Ok(HeadsPlan {
        heads,
        seconds,
        wait_s,
    })
}

impl HeadsPlan {
    /// How many parts the heads place, all together.
    pub fn parts(&self) -> usize {
        self.heads
            .iter()
            .map(|h| h.plan.plan.placements.len())
            .sum()
    }

    /// The plan of the heads as JSON: for every head its name, nozzles, offset, parts, the seconds of its plan alone, its waits and when it is done, and its placements in the coordinates
    /// of the board.
    pub fn to_value(&self) -> Value {
        let heads: Vec<Value> = self
            .heads
            .iter()
            .map(|h| {
                let placements: Vec<Value> = h
                    .plan
                    .plan
                    .placements
                    .iter()
                    .enumerate()
                    .map(|(i, p)| {
                        let (x, y) = h.board_xy(i).unwrap_or((p.x, p.y));
                        json!({
                            "reference": p.reference, "feeder": p.feeder,
                            "nozzle": p.nozzle, "batch": p.batch,
                            "x": x, "y": y, "rotation": p.rotation
                        })
                    })
                    .collect();
                json!({
                    "name": h.head.name,
                    "nozzles": h.head.nozzles,
                    "offset": {"x": h.head.offset_x_mm, "y": h.head.offset_y_mm},
                    "parts": placements.len(),
                    "alone_s": h.plan.plan.seconds,
                    "wait_s": h.wait_s,
                    "finish_s": h.finish_s,
                    "tip_changes": h.plan.tip_changes,
                    "placements": placements
                })
            })
            .collect();
        json!({"seconds": self.seconds, "wait_s": self.wait_s, "heads": heads})
    }

    /// One line for each head, and the total.
    pub fn summary(&self) -> String {
        let mut out = String::new();
        for h in &self.heads {
            out.push_str(&format!(
                "head {}: {} parts, {} nozzles, about {:.1} s ({:.1} s waiting for a feeder)\n",
                h.head.name,
                h.plan.plan.placements.len(),
                h.head.nozzles,
                h.finish_s,
                h.wait_s
            ));
        }
        out.push_str(&format!(
            "heads together: {} parts in about {:.1} s\n",
            self.parts(),
            self.seconds
        ));
        out
    }
}
