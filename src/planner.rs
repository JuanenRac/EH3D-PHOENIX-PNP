//! The planner that knows the machine configuration and the tables of parts: ranks, the order strategies, nozzle tips, the angle limits of the nozzle and the time that the moves take.
//!
//! * A **rank** is a whole number on a kind of part. The ranks are grouped in blocks of `rank_step` (with the usual 10, the ranks 0 to 9 are one block, 10 to 19 the next, and -10 to -1
//!   the one before 0). Every part of a lower block is placed before any part of a higher one, and a batch never holds parts of two blocks. Inside a block the lower rank goes first, but
//!   a batch may take parts of two ranks so that it is not left half empty: that is only a preference.
//! * The parts are planned in the order of their references, so the same board and feeders give the same plan whatever the order of the files.

use crate::catalog::Catalog;
use crate::config::{ComponentOrigin, MachineConfig, OrderStrategy, TipStrategy};
use crate::error::{Code, PnpError};
use crate::feeding::{FeederSpec, Picks};
use crate::motion::{move_time, AxisMotion};
use crate::parts::{Part, Side};
use crate::plan::{nearest_walk, Placement, Plan};
use crate::rotation::rot_path;
use crate::units::rotate_xy;
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// What the plan depends on besides the machine: the side of the board and, for the bottom side, where it is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Options {
    pub side: Side,
    /// For the bottom side: the width of the board; the x of every part is mirrored about its middle.
    pub board_width_mm: Option<f64>,
    /// For the bottom side: where the left edge of the board is on the X axis.
    pub origin_x_mm: f64,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            side: Side::Top,
            board_width_mm: None,
            origin_x_mm: 0.0,
        }
    }
}

/// What the planner knows about each placement besides where it goes; `AdvancedPlan::details[i]` goes with `plan.placements[i]`.
#[derive(Debug, Clone, PartialEq)]
pub struct Detail {
    pub rank: i32,
    pub tier: i64,
    pub tip: String,
    pub bin: u8,
    pub height_mm: f64,
    pub pick_retries: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AdvancedPlan {
    pub plan: Plan,
    pub details: Vec<Detail>,
    /// How many times the nozzle tip is changed.
    pub tip_changes: usize,
    /// How many groups of ranks separated by a hard order there are.
    pub tiers: usize,
    /// The seconds of the plan that the nozzles spend turning when they do not turn while the head moves.
    pub rotation_s: f64,
    pub order: OrderStrategy,
}

#[derive(Debug, Clone)]
struct Item {
    pick: (f64, f64),
    at: (f64, f64),
    rot: f64,
    tape_rot: f64,
    rank: i32,
    tier: i64,
    tip: String,
    bin: u8,
    height: f64,
    pick_retries: u32,
    kind: String,
    reference: String,
    feeder_id: String,
}

fn dist(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

/// The length of the path that starts at `start` and visits the points in `order`.
pub fn path_length(points: &[(f64, f64)], order: &[usize], start: (f64, f64)) -> f64 {
    let mut at = start;
    let mut total = 0.0;
    for &k in order {
        total += dist(at, points[k]);
        at = points[k];
    }
    total
}

/// Shortens an open path with 2-opt moves: a stretch of the path is turned round when that makes it shorter. The first point is joined to `start`, the path has no end to come back to.
/// A pass looks at every stretch; the passes stop when one finds nothing. The path is never made longer.
pub fn two_opt(points: &[(f64, f64)], order: &mut [usize], start: (f64, f64), passes: usize) {
    let n = order.len();
    if n < 2 {
        return;
    }
    for _ in 0..passes {
        let mut improved = false;
        let mut i = 0;
        while i + 1 < n {
            let mut j = i + 1;
            while j < n {
                let prev = if i == 0 { start } else { points[order[i - 1]] };
                let (first, last) = (points[order[i]], points[order[j]]);
                let (mut before, mut after) = (dist(prev, first), dist(prev, last));
                if j + 1 < n {
                    let next = points[order[j + 1]];
                    before += dist(last, next);
                    after += dist(first, next);
                }
                if after + 1e-9 < before {
                    order[i..=j].reverse();
                    improved = true;
                }
                j += 1;
            }
            i += 1;
        }
        if !improved {
            break;
        }
    }
}

fn walk(items: &[Item], idx: &[usize], start: (f64, f64), cfg: &MachineConfig) -> Vec<usize> {
    let points: Vec<(f64, f64)> = idx.iter().map(|&k| items[k].at).collect();
    let mut order = nearest_walk(&points, start);
    if cfg.two_opt && points.len() <= cfg.two_opt_max_points {
        two_opt(&points, &mut order, start, cfg.two_opt_passes);
    }
    order.into_iter().map(|o| idx[o]).collect()
}

/// The order of the items of one rank, from the point `start`, by the strategy of the configuration.
fn order_group(
    items: &[Item],
    idx: &[usize],
    start: (f64, f64),
    cfg: &MachineConfig,
) -> Vec<usize> {
    match cfg.order {
        OrderStrategy::None => idx.to_vec(),
        OrderStrategy::Nearest => walk(items, idx, start, cfg),
        other => {
            let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
            for &k in idx {
                let key = match other {
                    OrderStrategy::Part => items[k].kind.clone(),
                    OrderStrategy::HeightPart => {
                        format!("{:09.3}|{}", items[k].height, items[k].kind)
                    }
                    OrderStrategy::Pick => items[k].feeder_id.clone(),
                    _ => items[k].tip.clone(),
                };
                groups.entry(key).or_default().push(k);
            }
            let mut out = Vec::with_capacity(idx.len());
            let mut at = start;
            for g in groups.values() {
                let w = walk(items, g, at, cfg);
                if let Some(&last) = w.last() {
                    at = items[last].at;
                }
                out.extend(w);
            }
            out
        }
    }
}

/// Brings the items that need the same nozzle tip together, keeping the order inside each tip and putting the tips in the order in which they first appear.
fn regroup_by_tip(seq: &mut [usize], items: &[Item]) {
    let mut tips: Vec<&str> = Vec::new();
    for &k in seq.iter() {
        if !tips.contains(&items[k].tip.as_str()) {
            tips.push(items[k].tip.as_str());
        }
    }
    if tips.len() < 2 {
        return;
    }
    let mut out: Vec<usize> = Vec::with_capacity(seq.len());
    for t in &tips {
        out.extend(seq.iter().copied().filter(|&k| items[k].tip == *t));
    }
    seq.copy_from_slice(&out);
}

/// The parts of a block that need one nozzle tip, in the order they have in the sequence.
struct TipGroup {
    tip: String,
    items: Vec<usize>,
    first: (f64, f64),
    last: (f64, f64),
    /// The length of the way through the group.
    inner: f64,
}

fn tip_groups(seq: &[usize], items: &[Item]) -> Vec<TipGroup> {
    let mut groups: Vec<TipGroup> = Vec::new();
    for &k in seq {
        match groups.iter().position(|g| g.tip == items[k].tip) {
            Some(g) => {
                let group = &mut groups[g];
                group.inner += dist(group.last, items[k].at);
                group.last = items[k].at;
                group.items.push(k);
            }
            None => groups.push(TipGroup {
                tip: items[k].tip.clone(),
                items: vec![k],
                first: items[k].at,
                last: items[k].at,
                inner: 0.0,
            }),
        }
    }
    groups
}

/// The seconds that visiting the groups in `order` takes, from `start` with the tip `mounted` on the nozzle: the way at the speed of the head, and the change of the tip with its trip to
/// the changer.
fn order_cost(
    groups: &[TipGroup],
    order: &[usize],
    start: (f64, f64),
    mounted: Option<&str>,
    cfg: &MachineConfig,
) -> f64 {
    let per_mm = 2.0 / (cfg.x_speed_mm_s + cfg.y_speed_mm_s);
    let mut at = start;
    let mut tip = mounted;
    let mut cost = 0.0;
    for &g in order {
        let group = &groups[g];
        let change = tip.is_some_and(|t| t != group.tip);
        let to_first = match cfg.tip_changer {
            Some(changer) if change => dist(at, changer) + dist(changer, group.first),
            _ => dist(at, group.first),
        };
        cost += (to_first + group.inner) * per_mm;
        if change {
            cost += cfg.tip_change_s;
        }
        at = group.last;
        tip = Some(group.tip.as_str());
    }
    cost
}

/// The next order of `order` in alphabetical order; false when it was the last.
fn next_permutation(order: &mut [usize]) -> bool {
    let n = order.len();
    if n < 2 {
        return false;
    }
    let mut i = n - 1;
    while i > 0 && order[i - 1] >= order[i] {
        i -= 1;
    }
    if i == 0 {
        return false;
    }
    let mut j = n - 1;
    while order[j] <= order[i - 1] {
        j -= 1;
    }
    order.swap(i - 1, j);
    order[i..].reverse();
    true
}

/// The order of the groups with the lowest cost that the search finds within its limits. The first order tried is the order in which the tips appear, and an order replaces the best
/// only when it is cheaper, so the result is never worse than that one.
fn best_group_order(
    groups: &[TipGroup],
    start: (f64, f64),
    mounted: Option<&str>,
    cfg: &MachineConfig,
) -> Vec<usize> {
    let n = groups.len();
    let mut best: Vec<usize> = (0..n).collect();
    if n < 2 {
        return best;
    }
    if n > 8 {
        // too many orders to try: the group that is cheapest to go to next, again and again
        let mut order: Vec<usize> = Vec::with_capacity(n);
        let mut left: Vec<usize> = (0..n).collect();
        while !left.is_empty() {
            let mut pick = 0;
            let mut pick_cost = f64::MAX;
            for (m, &g) in left.iter().enumerate() {
                order.push(g);
                let c = order_cost(groups, &order, start, mounted, cfg);
                order.pop();
                if c < pick_cost {
                    pick = m;
                    pick_cost = c;
                }
            }
            order.push(left.remove(pick));
        }
        return order;
    }
    let started = std::time::Instant::now();
    let mut best_cost = order_cost(groups, &best, start, mounted, cfg);
    let mut order: Vec<usize> = best.clone();
    let mut tried = 1usize;
    while tried < cfg.tip_opt_max_evals && next_permutation(&mut order) {
        tried += 1;
        let c = order_cost(groups, &order, start, mounted, cfg);
        if c + 1e-9 < best_cost {
            best_cost = c;
            best.clone_from(&order);
        }
        if tried.is_multiple_of(256) && started.elapsed().as_secs_f64() >= cfg.tip_opt_time_limit_s
        {
            break;
        }
    }
    best
}

/// Brings the parts of one tip together like `regroup_by_tip` and puts the tips in the order that is cheapest.
fn regroup_optimized(
    seq: &mut [usize],
    items: &[Item],
    start: (f64, f64),
    mounted: Option<&str>,
    cfg: &MachineConfig,
) {
    let groups = tip_groups(seq, items);
    if groups.len() < 2 {
        return;
    }
    let order = best_group_order(&groups, start, mounted, cfg);
    let mut out: Vec<usize> = Vec::with_capacity(seq.len());
    for g in order {
        out.extend(groups[g].items.iter().copied());
    }
    seq.copy_from_slice(&out);
}

/// One leg of the way: the distance, the seconds it takes (the nozzle turns `turn_deg` meanwhile, or after the move when the configuration says so) and the seconds of turning.
fn leg(
    from: (f64, f64),
    to: (f64, f64),
    turn_deg: f64,
    cfg: &MachineConfig,
    motion: &(AxisMotion, AxisMotion),
) -> (f64, f64, f64) {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let moving = move_time(dx, dy, &motion.0, &motion.1);
    let turning = turn_deg.abs() / cfg.rotation_deg_s;
    let seconds = if cfg.pre_rotate {
        moving.max(turning)
    } else {
        moving + turning
    };
    (
        dx.hypot(dy),
        seconds,
        if cfg.pre_rotate { 0.0 } else { turning },
    )
}

/// Plans the parts that have a feeder (see `feeding::assign`) for one side of the board.
pub fn plan_advanced(
    parts: &[Part],
    feeders: &[FeederSpec],
    picks: &Picks,
    catalog: &Catalog,
    cfg: &MachineConfig,
    opts: &Options,
) -> Result<AdvancedPlan, PnpError> {
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
    if !opts.origin_x_mm.is_finite() {
        return Err(setting(format!(
            "origin_x_mm is {}: it must be a number",
            opts.origin_x_mm
        )));
    }
    let motion = (
        AxisMotion::new(cfg.x_speed_mm_s, cfg.x_accel_mm_s2, cfg.x_jerk_mm_s3)?,
        AxisMotion::new(cfg.y_speed_mm_s, cfg.y_accel_mm_s2, cfg.y_jerk_mm_s3)?,
    );
    let defaults = cfg.part_defaults();
    let mut items: Vec<Item> = Vec::new();
    for pick in &picks.picks {
        let p = parts.get(pick.part).ok_or_else(|| {
            setting(format!(
                "the pick points at the part {} that does not exist",
                pick.part
            ))
        })?;
        let f = feeders.get(pick.feeder).ok_or_else(|| {
            setting(format!(
                "the pick points at the feeder {} that does not exist",
                pick.feeder
            ))
        })?;
        if p.side != opts.side {
            continue;
        }
        let (x, rot) = match (opts.side, opts.board_width_mm) {
            (Side::Bottom, Some(w)) => (
                2.0 * opts.origin_x_mm + w - p.x,
                (180.0 - p.rotation).rem_euclid(360.0),
            ),
            _ => (p.x, p.rotation),
        };
        let props = catalog.resolve(p, &defaults);
        let mut at = (x, p.y);
        if cfg.component_origin == ComponentOrigin::PadsCenter {
            let dx = if opts.side == Side::Bottom {
                -props.origin_dx_mm
            } else {
                props.origin_dx_mm
            };
            let (ox, oy) = rotate_xy(dx, props.origin_dy_mm, rot);
            at = (at.0 + ox, at.1 + oy);
        }
        items.push(Item {
            pick: pick.at,
            at,
            rot,
            tape_rot: f.tape_rotation,
            rank: props.rank,
            tier: i64::from(props.rank).div_euclid(i64::from(cfg.rank_step)),
            tip: props.nozzle_tip.clone(),
            bin: props.bin,
            height: props.height_mm,
            pick_retries: props.pick_retries,
            kind: p.key(),
            reference: p.reference.clone(),
            feeder_id: f.base.id.clone(),
        });
    }
    // the canonical order: by reference, so that the order of the files does not change the plan
    items.sort_by(|a, b| a.reference.cmp(&b.reference));
    let mut sorted: Vec<usize> = (0..items.len()).collect();
    sorted.sort_by_key(|&k| (items[k].tier, items[k].rank, k));
    let mut sequence: Vec<usize> = Vec::with_capacity(items.len());
    let mut tier_ranges: Vec<(usize, usize)> = Vec::new();
    let mut at = (0.0, 0.0);
    let mut mounted: Option<String> = None;
    let mut i = 0;
    while i < sorted.len() {
        let tier = items[sorted[i]].tier;
        let tier_start = sequence.len();
        let tier_entry = at;
        while i < sorted.len() && items[sorted[i]].tier == tier {
            let rank = items[sorted[i]].rank;
            let mut j = i;
            while j < sorted.len() && items[sorted[j]].tier == tier && items[sorted[j]].rank == rank
            {
                j += 1;
            }
            let ordered = order_group(&items, &sorted[i..j], at, cfg);
            if let Some(&last) = ordered.last() {
                at = items[last].at;
            }
            sequence.extend(ordered);
            i = j;
        }
        match cfg.tip_strategy {
            TipStrategy::Minimize => regroup_by_tip(&mut sequence[tier_start..], &items),
            TipStrategy::Optimize => {
                regroup_optimized(
                    &mut sequence[tier_start..],
                    &items,
                    tier_entry,
                    mounted.as_deref(),
                    cfg,
                );
                // the next block starts where this one ends, with the tip that is on the nozzle then
                if let Some(&last) = sequence[tier_start..].last() {
                    at = items[last].at;
                    mounted = Some(items[last].tip.clone());
                }
            }
            TipStrategy::AsPlanned => {}
        }
        tier_ranges.push((tier_start, sequence.len()));
    }
    // the batches: up to as many parts as nozzles, all for the same nozzle tip, never across a hard order
    let mut batches: Vec<Vec<usize>> = Vec::new();
    for &(a, b) in &tier_ranges {
        let mut cur: Vec<usize> = Vec::new();
        for &k in &sequence[a..b] {
            if !cur.is_empty() && (cur.len() >= cfg.nozzles || items[cur[0]].tip != items[k].tip) {
                batches.push(std::mem::take(&mut cur));
            }
            cur.push(k);
        }
        if !cur.is_empty() {
            batches.push(cur);
        }
    }
    // the walk of the head
    let mut head = (0.0, 0.0);
    let (mut travel, mut seconds, mut rotation_s) = (0.0, 0.0, 0.0);
    let mut angles = [0.0f64; 6];
    let mut tip_changes = 0usize;
    let mut current_tip: Option<&str> = None;
    let mut placements: Vec<Placement> = Vec::with_capacity(items.len());
    let mut details: Vec<Detail> = Vec::with_capacity(items.len());
    for (batch_no, batch) in batches.iter().enumerate() {
        let tip = items[batch[0]].tip.as_str();
        if let Some(cur) = current_tip {
            if cur != tip {
                tip_changes += 1;
                if let Some(changer) = cfg.tip_changer {
                    let (d, t, r) = leg(head, changer, 0.0, cfg, &motion);
                    travel += d;
                    seconds += t;
                    rotation_s += r;
                    head = changer;
                }
                seconds += cfg.tip_change_s;
            }
        }
        current_tip = Some(tip);
        // the feeders of the batch are visited nearest-first
        let mut left: Vec<usize> = batch.clone();
        let mut order: Vec<usize> = Vec::with_capacity(batch.len());
        while !left.is_empty() {
            let from = order.last().map_or(head, |&k| items[k].pick);
            let mut best = 0;
            for (n, &k) in left.iter().enumerate() {
                if dist(from, items[k].pick) < dist(from, items[left[best]].pick) {
                    best = n;
                }
            }
            order.push(left.remove(best));
        }
        for (nozzle, &k) in order.iter().enumerate() {
            let it = &items[k];
            let new = rot_path(
                angles[nozzle],
                it.tape_rot,
                cfg.nozzle_angle_min_deg,
                cfg.nozzle_angle_max_deg,
            )?;
            let (d, t, r) = leg(head, it.pick, new - angles[nozzle], cfg, &motion);
            travel += d;
            seconds += t + cfg.feed_s + cfg.pick_s;
            rotation_s += r;
            angles[nozzle] = new;
            head = it.pick;
        }
        for (nozzle, &k) in order.iter().enumerate() {
            let it = &items[k];
            let relative = (it.rot - it.tape_rot + 180.0).rem_euclid(360.0) - 180.0;
            let new = rot_path(
                angles[nozzle],
                angles[nozzle] + relative,
                cfg.nozzle_angle_min_deg,
                cfg.nozzle_angle_max_deg,
            )?;
            let (d, t, r) = leg(head, it.at, new - angles[nozzle], cfg, &motion);
            travel += d;
            seconds += t + cfg.place_s;
            rotation_s += r;
            angles[nozzle] = new;
            head = it.at;
            placements.push(Placement {
                reference: it.reference.clone(),
                feeder: it.feeder_id.clone(),
                pick: it.pick,
                x: it.at.0,
                y: it.at.1,
                rotation: it.rot,
                nozzle,
                batch: batch_no,
            });
            details.push(Detail {
                rank: it.rank,
                tier: it.tier,
                tip: it.tip.clone(),
                bin: it.bin,
                height_mm: it.height,
                pick_retries: it.pick_retries,
            });
        }
    }
    Ok(AdvancedPlan {
        plan: Plan {
            placements,
            batches: batches.len(),
            travel_mm: travel,
            seconds,
        },
        details,
        tip_changes,
        tiers: tier_ranges.len(),
        rotation_s,
        order: cfg.order,
    })
}

impl AdvancedPlan {
    pub fn to_json(&self) -> String {
        let list: Vec<Value> = self
            .plan
            .placements
            .iter()
            .zip(&self.details)
            .map(|(p, d)| {
                json!({
                    "reference": p.reference, "feeder": p.feeder,
                    "pick": {"x": p.pick.0, "y": p.pick.1},
                    "place": {"x": p.x, "y": p.y, "rotation": p.rotation},
                    "nozzle": p.nozzle, "batch": p.batch,
                    "rank": d.rank, "tier": d.tier, "tip": d.tip, "bin": d.bin
                })
            })
            .collect();
        serde_json::to_string_pretty(&json!({
            "schema_version": 2,
            "order": self.order.name(),
            "batches": self.plan.batches,
            "tip_changes": self.tip_changes,
            "tiers": self.tiers,
            "travel_mm": self.plan.travel_mm,
            "seconds": self.plan.seconds,
            "rotation_s": self.rotation_s,
            "placements": list
        }))
        .unwrap_or_default()
            + "\n"
    }
}
