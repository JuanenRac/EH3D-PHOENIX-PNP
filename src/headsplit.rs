// EH3D-PHOENIX-PNP - several heads on one board, taking from the same row of feeders.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! A machine with two heads or more on one gantry works the same board at the same time, but the feeders are shared: a reel can be picked from by one head at a time, so a head that comes to
//! a feeder that another is using waits. This plays that out with the pick time and the placement time of the settings (the moves of the heads are left out, as in `split-job`, so the
//! times are for comparing the ways of sharing the work with each other). Two ways are tried: *strips*, where the board is cut across into as many bands as there are heads with as many parts
//! each, and *by feeder*, where whole feeders are given to a head so that no reel is shared. The result says, for each way, how long each head works and waits, when the board is done and how
//! much faster that is than one head. It is arithmetic on a list: the same board gives the same answer.

use crate::parts::csv_field;
use std::collections::{BTreeMap, BTreeSet};

/// The most heads.
pub const MAX_HEADS: usize = 8;

/// A part to place: the feeder it comes from and where it goes.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub reference: String,
    pub feeder: String,
    pub x: f64,
}

/// The seconds of a pick and of a placement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Timing {
    pub pick_s: f64,
    pub place_s: f64,
}

/// The way the parts are given to the heads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Way {
    Strips,
    ByFeeder,
}

impl Way {
    pub fn name(self) -> &'static str {
        match self {
            Way::Strips => "strips",
            Way::ByFeeder => "by-feeder",
        }
    }
}

/// What one head did.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Head {
    pub parts: usize,
    /// The seconds picking and placing.
    pub busy_s: f64,
    /// The seconds waiting for a feeder that another head was using.
    pub wait_s: f64,
    /// The second at which the head put its last part down.
    pub finish_s: f64,
}

/// A way tried.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub way: Way,
    pub heads: Vec<Head>,
    /// The second at which the board is done.
    pub makespan_s: f64,
    /// The feeders that more than one head picks from.
    pub shared: Vec<String>,
}

impl Outcome {
    /// The seconds the heads wait, all together.
    pub fn waited_s(&self) -> f64 {
        self.heads.iter().map(|h| h.wait_s).sum()
    }
}

/// The groups of parts, one for each head, in the order in which a head places them.
fn assign(items: &[Item], heads: usize, way: Way) -> Vec<Vec<&Item>> {
    let mut groups: Vec<Vec<&Item>> = vec![Vec::new(); heads];
    match way {
        Way::Strips => {
            let mut sorted: Vec<&Item> = items.iter().collect();
            sorted.sort_by(|a, b| {
                a.x.total_cmp(&b.x)
                    .then_with(|| a.reference.cmp(&b.reference))
            });
            let total = sorted.len();
            for (n, item) in sorted.into_iter().enumerate() {
                // the first bands get the parts that do not divide evenly
                groups[n * heads / total.max(1)].push(item);
            }
        }
        Way::ByFeeder => {
            let mut by_feeder: BTreeMap<&str, Vec<&Item>> = BTreeMap::new();
            for item in items {
                by_feeder
                    .entry(item.feeder.as_str())
                    .or_default()
                    .push(item);
            }
            let mut feeders: Vec<(&str, Vec<&Item>)> = by_feeder.into_iter().collect();
            feeders.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(b.0)));
            for (_, mut list) in feeders {
                list.sort_by(|a, b| a.reference.cmp(&b.reference));
                let at = groups
                    .iter()
                    .enumerate()
                    .min_by_key(|(n, g)| (g.len(), *n))
                    .map_or(0, |m| m.0);
                groups[at].extend(list);
            }
        }
    }
    groups
}

/// Plays the work of `heads` heads (1 to `MAX_HEADS`) given the parts the way `way` says.
pub fn simulate(items: &[Item], heads: usize, way: Way, t: Timing) -> Outcome {
    let heads = heads.clamp(1, MAX_HEADS);
    let groups = assign(items, heads, way);
    let mut clock = vec![0.0f64; heads];
    let mut next = vec![0usize; heads];
    let mut result = vec![Head::default(); heads];
    let mut free_at: BTreeMap<&str, f64> = BTreeMap::new();
    loop {
        // the head that is ready first, the lowest number first
        let ready = (0..heads)
            .filter(|h| next[*h] < groups[*h].len())
            .min_by(|a, b| clock[*a].total_cmp(&clock[*b]).then(a.cmp(b)));
        let Some(h) = ready else { break };
        let item = groups[h][next[h]];
        next[h] += 1;
        let free = free_at.get(item.feeder.as_str()).copied().unwrap_or(0.0);
        let start = clock[h].max(free);
        result[h].wait_s += start - clock[h];
        let picked = start + t.pick_s;
        free_at.insert(item.feeder.as_str(), picked);
        clock[h] = picked + t.place_s;
        result[h].parts += 1;
        result[h].busy_s += t.pick_s + t.place_s;
        result[h].finish_s = clock[h];
    }
    let mut users: BTreeMap<&str, BTreeSet<usize>> = BTreeMap::new();
    for (h, g) in groups.iter().enumerate() {
        for item in g {
            users.entry(item.feeder.as_str()).or_default().insert(h);
        }
    }
    Outcome {
        way,
        makespan_s: result.iter().map(|h| h.finish_s).fold(0.0, f64::max),
        shared: users
            .into_iter()
            .filter(|(_, hs)| hs.len() > 1)
            .map(|(f, _)| f.to_string())
            .collect(),
        heads: result,
    }
}

/// The seconds one head takes for all the parts.
pub fn single_head_s(items: &[Item], t: Timing) -> f64 {
    items.len() as f64 * (t.pick_s + t.place_s)
}

/// Both ways, strips first.
pub fn compare(items: &[Item], heads: usize, t: Timing) -> Vec<Outcome> {
    [Way::Strips, Way::ByFeeder]
        .into_iter()
        .map(|w| simulate(items, heads, w, t))
        .collect()
}

/// The way that finishes first (strips when two finish together).
pub fn best(outcomes: &[Outcome]) -> Option<&Outcome> {
    outcomes.iter().min_by(|a, b| {
        a.makespan_s
            .total_cmp(&b.makespan_s)
            .then((a.way as u8).cmp(&(b.way as u8)))
    })
}

/// The report for a person.
pub fn text(outcomes: &[Outcome], single_s: f64) -> String {
    let mut out = String::new();
    for o in outcomes {
        out.push_str(&format!("{}:\n", o.way.name()));
        for (n, h) in o.heads.iter().enumerate() {
            out.push_str(&format!(
                "  head {}: {} parts, {:.1} s at work, {:.1} s waiting, done at {:.1} s\n",
                n + 1,
                h.parts,
                h.busy_s,
                h.wait_s,
                h.finish_s
            ));
        }
        let faster = if o.makespan_s > 0.0 {
            single_s / o.makespan_s
        } else {
            1.0
        };
        out.push_str(&format!(
            "  the board is done at {:.1} s, {:.2} times as fast as one head ({:.1} s); {} feeders are shared{}\n",
            o.makespan_s,
            faster,
            single_s,
            o.shared.len(),
            if o.shared.is_empty() {
                String::new()
            } else {
                format!(" ({})", o.shared.join(", "))
            }
        ));
    }
    if let Some(b) = best(outcomes) {
        out.push_str(&format!("the quicker way is {}\n", b.way.name()));
    }
    out
}

/// A row for each head of each way: `way,head,parts,busy_s,wait_s,finish_s`.
pub fn csv(outcomes: &[Outcome]) -> String {
    let mut out = String::from("way,head,parts,busy_s,wait_s,finish_s\n");
    for o in outcomes {
        for (n, h) in o.heads.iter().enumerate() {
            out.push_str(&format!(
                "{},{},{},{:.3},{:.3},{:.3}\n",
                csv_field(o.way.name()),
                n + 1,
                h.parts,
                h.busy_s,
                h.wait_s,
                h.finish_s
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: Timing = Timing {
        pick_s: 1.0,
        place_s: 1.0,
    };

    fn item(r: &str, f: &str, x: f64) -> Item {
        Item {
            reference: r.into(),
            feeder: f.into(),
            x,
        }
    }

    fn board(n: usize, feeders: usize) -> Vec<Item> {
        (0..n)
            .map(|i| item(&format!("R{i:03}"), &format!("F{}", i % feeders), i as f64))
            .collect()
    }

    #[test]
    fn one_head_takes_the_time_of_a_pick_and_a_placement_for_every_part() {
        let items = board(10, 3);
        for way in [Way::Strips, Way::ByFeeder] {
            let o = simulate(&items, 1, way, T);
            assert_eq!(o.heads.len(), 1);
            assert!((o.makespan_s - single_head_s(&items, T)).abs() < 1e-9);
            assert_eq!(o.waited_s(), 0.0);
            assert!(o.shared.is_empty());
        }
    }

    #[test]
    fn feeders_given_whole_never_make_a_head_wait() {
        let items = board(12, 4);
        let o = simulate(&items, 2, Way::ByFeeder, T);
        assert_eq!(o.waited_s(), 0.0);
        assert!(o.shared.is_empty());
        assert_eq!(o.heads[0].parts, 6);
        assert_eq!(o.heads[1].parts, 6);
        assert!((o.makespan_s - 12.0).abs() < 1e-9);
    }

    #[test]
    fn strips_across_one_feeder_make_the_heads_wait_for_the_reel() {
        // one feeder for every part: the picks cannot overlap
        let items = board(8, 1);
        let o = simulate(&items, 2, Way::Strips, T);
        assert_eq!(o.shared, vec!["F0".to_string()]);
        assert!(o.waited_s() > 0.0);
        // the picks follow each other (1 s each), the placements overlap: 8 picks + the last placement
        assert!((o.makespan_s - 9.0).abs() < 1e-9);
        let faster = single_head_s(&items, T) / o.makespan_s;
        assert!(faster > 1.0 && faster < 2.0);
    }

    #[test]
    fn a_way_never_beats_the_work_divided_among_the_heads() {
        let mut seed = 0x1234_5678_u64;
        for _ in 0..60 {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let n = 1 + (seed >> 33) as usize % 40;
            let feeders = 1 + (seed >> 20) as usize % 6;
            let heads = 1 + (seed >> 10) as usize % MAX_HEADS;
            let items = board(n, feeders);
            let single = single_head_s(&items, T);
            for o in compare(&items, heads, T) {
                let busy: f64 = o.heads.iter().map(|h| h.busy_s).sum();
                assert!((busy - single).abs() < 1e-6, "the work is the same");
                assert_eq!(o.heads.iter().map(|h| h.parts).sum::<usize>(), n);
                assert!(o.makespan_s + 1e-9 >= single / heads.min(MAX_HEADS) as f64);
                assert!(o.makespan_s <= single + 1e-9, "never slower than one head");
                for h in &o.heads {
                    assert!(h.finish_s + 1e-9 >= h.busy_s + h.wait_s - 1e-9);
                }
            }
        }
    }

    #[test]
    fn the_same_board_gives_the_same_outcome() {
        let items = board(25, 5);
        assert_eq!(compare(&items, 3, T), compare(&items, 3, T));
    }

    #[test]
    fn the_number_of_heads_is_kept_in_range_and_an_empty_board_is_done_at_once() {
        let o = simulate(&[], 0, Way::Strips, T);
        assert_eq!(o.heads.len(), 1);
        assert_eq!(o.makespan_s, 0.0);
        let o = simulate(&board(3, 1), 99, Way::ByFeeder, T);
        assert_eq!(o.heads.len(), MAX_HEADS);
        // the feeder goes whole to the first head
        assert_eq!(o.heads[0].parts, 3);
        assert_eq!(
            o.heads.iter().filter(|h| h.parts == 0).count(),
            MAX_HEADS - 1
        );
    }

    #[test]
    fn strips_keep_the_parts_in_bands_by_their_place() {
        let items = vec![
            item("A", "F1", 50.0),
            item("B", "F1", 10.0),
            item("C", "F1", 30.0),
            item("D", "F1", 20.0),
            item("E", "F1", 40.0),
        ];
        let groups = assign(&items, 2, Way::Strips);
        let refs = |g: &Vec<&Item>| g.iter().map(|i| i.reference.clone()).collect::<Vec<_>>();
        assert_eq!(refs(&groups[0]), ["B", "D", "C"]);
        assert_eq!(refs(&groups[1]), ["E", "A"]);
    }

    #[test]
    fn the_best_way_and_the_report_name_themselves() {
        let items = board(8, 1);
        let outcomes = compare(&items, 2, T);
        let b = best(&outcomes).unwrap();
        // one feeder: by feeder gives all to one head (the same time as one head), strips are quicker
        assert_eq!(b.way, Way::Strips);
        let said = text(&outcomes, single_head_s(&items, T));
        assert!(said.contains("strips:") && said.contains("by-feeder:"));
        assert!(said.contains("the quicker way is strips"));
        let csv = csv(&outcomes);
        assert!(csv.starts_with("way,head,parts,busy_s,wait_s,finish_s\nstrips,1,"));
        assert_eq!(csv.lines().count(), 1 + 4);
        assert!(best(&[]).is_none());
    }
}
