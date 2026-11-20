// EH3D-PHOENIX-PNP - how much of a shift the machine makes good boards, as the overall equipment effectiveness.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! The overall equipment effectiveness of a shift is the product of three shares: availability (how much of the shift the machine is not stopped), performance (how close to the ideal time of the
//! plan a board is made) and quality (how many boards are good). With no machine these are worked out on the simulated one: the plan is run many times from different seeds, the mean time of
//! the runs that went to the end is the actual time of a board, the share of the runs that placed every part is the quality, the ideal time is the seconds the plan itself counts, and the stops are
//! the changes of reels that the shift needs (a reel is changed when its parts are used up, and a person takes `reel_change_s` seconds for each). The number of boards that the shift holds is the
//! most that fit, counting those stops. It is a figure for the plan and the settings on a simulated machine, not for a real shift.

use crate::error::{Code, PnpError};
use crate::runs::Runs;

/// The most boards that a shift is counted for.
pub const MAX_BOARDS: u64 = 1_000_000;

/// What the figures are worked out from.
#[derive(Debug, Clone, PartialEq)]
pub struct Inputs {
    /// The seconds that the plan counts for one board, with nothing going wrong.
    pub ideal_s: f64,
    /// The runs of the simulated job.
    pub runs: Runs,
    /// For each feeder the plan takes from: the parts one board takes and the parts of a full reel.
    pub reels: Vec<(u32, u32)>,
    pub shift_s: f64,
    pub reel_change_s: f64,
}

/// The shift.
#[derive(Debug, Clone, PartialEq)]
pub struct Oee {
    /// The boards that fit in the shift, counting the stops for the reels.
    pub boards: u64,
    /// The boards expected to be good.
    pub good_boards: f64,
    pub reel_changes: u64,
    pub stopped_s: f64,
    pub ideal_s: f64,
    pub actual_s: f64,
    pub availability: f64,
    pub performance: f64,
    pub quality: f64,
    pub oee: f64,
    pub runs: usize,
}

/// The changes of reels that making `boards` boards needs: each feeder is loaded with a full reel at the start of the shift.
fn changes_for(boards: u64, reels: &[(u32, u32)]) -> u64 {
    reels
        .iter()
        .filter(|(per, count)| *per > 0 && *count > 0)
        .map(|(per, count)| {
            (boards * u64::from(*per))
                .div_ceil(u64::from(*count))
                .saturating_sub(1)
        })
        .sum()
}

/// Works the shift out. The runs must have at least one that went to the end (every part placed or put aside).
pub fn compute(i: &Inputs) -> Result<Oee, PnpError> {
    let setting = |m: &str| PnpError::new(Code::PlanSetting, m.to_string());
    if !i.shift_s.is_finite() || i.shift_s <= 0.0 {
        return Err(setting("the shift must last more than 0 seconds"));
    }
    if !i.reel_change_s.is_finite() || i.reel_change_s < 0.0 {
        return Err(setting("a change of reel takes 0 seconds or more"));
    }
    if !i.ideal_s.is_finite() || i.ideal_s <= 0.0 {
        return Err(setting(
            "the plan takes no time: there is nothing to measure",
        ));
    }
    let finished: Vec<f64> = i
        .runs
        .rows
        .iter()
        .filter(|r| r.pending == 0)
        .map(|r| r.seconds)
        .collect();
    if finished.is_empty() {
        return Err(setting(
            "no run went to the end: there is no actual time of a board",
        ));
    }
    let actual = finished.iter().sum::<f64>() / finished.len() as f64;
    if actual <= 0.0 {
        return Err(setting("the runs took no time"));
    }
    let mut boards = 0u64;
    while boards < MAX_BOARDS {
        let next = boards + 1;
        let used = next as f64 * actual + changes_for(next, &i.reels) as f64 * i.reel_change_s;
        if used > i.shift_s {
            break;
        }
        boards = next;
    }
    let reel_changes = changes_for(boards, &i.reels);
    let stopped = reel_changes as f64 * i.reel_change_s;
    let availability = ((i.shift_s - stopped) / i.shift_s).clamp(0.0, 1.0);
    let performance = (i.ideal_s / actual).clamp(0.0, 1.0);
    let quality = i.runs.complete() as f64 / i.runs.rows.len().max(1) as f64;
    Ok(Oee {
        boards,
        good_boards: boards as f64 * quality,
        reel_changes,
        stopped_s: stopped,
        ideal_s: i.ideal_s,
        actual_s: actual,
        availability,
        performance,
        quality,
        oee: availability * performance * quality,
        runs: i.runs.rows.len(),
    })
}

impl Oee {
    /// The report for a person.
    pub fn text(&self, shift_minutes: u32) -> String {
        format!(
            "a shift of {shift_minutes} minutes, from {} runs on the simulated machine\n\
             a board takes {:.1} s in the plan and {:.1} s in the runs; {} boards fit, about {:.1} of them good\n\
             {} changes of reels, {:.0} s stopped\n\
             availability {:.1}%, performance {:.1}%, quality {:.1}%: effectiveness {:.1}%\n",
            self.runs,
            self.ideal_s,
            self.actual_s,
            self.boards,
            self.good_boards,
            self.reel_changes,
            self.stopped_s,
            self.availability * 100.0,
            self.performance * 100.0,
            self.quality * 100.0,
            self.oee * 100.0
        )
    }

    /// The figures as a table `metric,value`.
    pub fn csv(&self) -> String {
        format!(
            "metric,value\nruns,{}\nboards,{}\ngood_boards,{:.2}\nreel_changes,{}\nstopped_s,{:.1}\nideal_s,{:.3}\nactual_s,{:.3}\navailability,{:.4}\nperformance,{:.4}\nquality,{:.4}\noee,{:.4}\n",
            self.runs,
            self.boards,
            self.good_boards,
            self.reel_changes,
            self.stopped_s,
            self.ideal_s,
            self.actual_s,
            self.availability,
            self.performance,
            self.quality,
            self.oee
        )
    }
}
