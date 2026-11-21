// EH3D-PHOENIX-PNP - the signal tower of a simulated job: which light a tower would show at each second.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! A machine often has a tower of lights: green while it works, yellow when it waits or something takes too long, red when it has stopped on a failure. From the journal of a simulated job
//! (`busy::read_journal`) this works out what the tower would show: red during a step that did not end with `ok`, yellow during a step that took more than `tower_slow_factor` times
//! the median step of its kind (a retry shows as a long step) and during the gaps between steps longer than `tower_idle_gap_s`, green the rest (a shorter gap too). Intervals that touch and mean the same are joined.
//! It reads a journal and changes nothing; nothing is driven.

use crate::busy::Step;
use std::collections::BTreeMap;

/// The light of the tower.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Light {
    Green,
    Yellow,
    Red,
}

impl Light {
    pub fn name(self) -> &'static str {
        match self {
            Light::Green => "green",
            Light::Yellow => "yellow",
            Light::Red => "red",
        }
    }
}

/// A stretch of one light.
#[derive(Debug, Clone, PartialEq)]
pub struct Interval {
    pub from_s: f64,
    pub to_s: f64,
    pub light: Light,
    /// Why: `running`, `slow step`, `waiting` or `failed`.
    pub why: &'static str,
    /// The nozzle of the step that makes the light (from 1) and what it was doing when the step was over; 0 and empty for green and for a wait, and when the journal has no such column.
    pub nozzle: usize,
    pub nozzle_state: String,
}

/// The middle value of a list of seconds (the mean of the two in the middle for an even count).
fn median(values: &mut [f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f64::total_cmp);
    let n = values.len();
    if n % 2 == 1 {
        values[n / 2]
    } else {
        (values[n / 2 - 1] + values[n / 2]) / 2.0
    }
}

/// The intervals of the tower, in time order. `idle_gap_s` is the wait (0 or more) over which the tower turns yellow; `slow_factor` (1 or more) is how many times the median of its kind
/// a step may take.
pub fn timeline(steps: &[Step], idle_gap_s: f64, slow_factor: f64) -> Vec<Interval> {
    let mut by_action: BTreeMap<&str, Vec<f64>> = BTreeMap::new();
    for s in steps {
        by_action
            .entry(s.action.as_str())
            .or_default()
            .push(s.took_s);
    }
    let medians: BTreeMap<&str, f64> = by_action
        .into_iter()
        .map(|(k, mut v)| (k, median(&mut v)))
        .collect();
    let mut raw: Vec<Interval> = Vec::new();
    let mut previous_end: Option<f64> = None;
    for s in steps {
        if let Some(end) = previous_end {
            // a wait is yellow over the limit and green under it (the tower does not go dark between two steps)
            let wait = s.at_s - end;
            if wait > 1e-9 {
                let long = wait > idle_gap_s.max(1e-9);
                raw.push(Interval {
                    from_s: end,
                    to_s: s.at_s,
                    light: if long { Light::Yellow } else { Light::Green },
                    why: if long { "waiting" } else { "running" },
                    nozzle: 0,
                    nozzle_state: String::new(),
                });
            }
        }
        let typical = medians.get(s.action.as_str()).copied().unwrap_or(0.0);
        let (light, why) = if s.result != "ok" {
            (Light::Red, "failed")
        } else if typical > 0.0 && s.took_s > slow_factor.max(1.0) * typical {
            (Light::Yellow, "slow step")
        } else {
            (Light::Green, "running")
        };
        // green says nothing of a nozzle; a light that is not green says which nozzle made it and what it was doing
        let named = light != Light::Green && !s.nozzle_state.is_empty();
        raw.push(Interval {
            from_s: s.at_s,
            to_s: s.at_s + s.took_s,
            light,
            why,
            nozzle: if named { s.nozzle } else { 0 },
            nozzle_state: if named {
                s.nozzle_state.clone()
            } else {
                String::new()
            },
        });
        previous_end = Some(s.at_s + s.took_s);
    }
    let mut out: Vec<Interval> = Vec::new();
    for i in raw {
        match out.last_mut() {
            Some(last)
                if last.light == i.light
                    && last.why == i.why
                    && last.nozzle == i.nozzle
                    && last.nozzle_state == i.nozzle_state
                    && (i.from_s - last.to_s).abs() < 1e-9 =>
            {
                last.to_s = i.to_s;
            }
            _ => out.push(i),
        }
    }
    out
}

/// The seconds of each light, in the order green, yellow, red.
pub fn seconds(intervals: &[Interval]) -> [f64; 3] {
    let mut t = [0.0; 3];
    for i in intervals {
        let at = match i.light {
            Light::Green => 0,
            Light::Yellow => 1,
            Light::Red => 2,
        };
        t[at] += i.to_s - i.from_s;
    }
    t
}

/// The report for a person: a line for each interval that is not green and the seconds of each light.
pub fn text(intervals: &[Interval]) -> String {
    let mut out = String::new();
    for i in intervals.iter().filter(|i| i.light != Light::Green) {
        let nozzle = if i.nozzle > 0 {
            format!(" - nozzle {}: {}", i.nozzle, i.nozzle_state)
        } else {
            String::new()
        };
        out.push_str(&format!(
            "{:.3} s to {:.3} s: {} ({}){}\n",
            i.from_s,
            i.to_s,
            i.light.name(),
            i.why,
            nozzle
        ));
    }
    let t = seconds(intervals);
    out.push_str(&format!(
        "green {:.3} s, yellow {:.3} s, red {:.3} s\n",
        t[0], t[1], t[2]
    ));
    out
}

/// The intervals as a CSV: `from_s,to_s,light,why,nozzle,nozzle_state` (the nozzle is 0 and its state empty where the interval names none).
pub fn csv(intervals: &[Interval]) -> String {
    let mut out = String::from("from_s,to_s,light,why,nozzle,nozzle_state\n");
    for i in intervals {
        out.push_str(&format!(
            "{:.3},{:.3},{},{},{},{}\n",
            i.from_s,
            i.to_s,
            i.light.name(),
            i.why,
            i.nozzle,
            i.nozzle_state
        ));
    }
    out
}
