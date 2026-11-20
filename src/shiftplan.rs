// EH3D-PHOENIX-PNP - a shift planned board by board, with the changes of reels that it needs.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! `shift-oee` says how well a shift is used on the average; a person who has to be at the machine at the right time wants the other answer: at which board, and so at which minute, a reel has
//! to be changed. This runs the shift board by board: a board takes the time of the plan and, from each feeder, the parts that the plan takes from it; when a feeder does not hold enough for
//! the next board it is changed for a full reel of the same size, which stops the machine for the time of a change; the shift is over when the next board would not fit in it. Nothing is
//! chance: the same plan gives the same shift.

use crate::error::{Code, PnpError};
use crate::parts::csv_field;

/// The most boards of one shift (a plan that takes a fraction of a second would otherwise be counted for ever).
pub const MAX_BOARDS: u64 = 1_000_000;

/// The most changes that the text lists (the table has them all).
pub const MAX_SHOWN: usize = 20;

/// One feeder of the shift.
#[derive(Debug, Clone, PartialEq)]
pub struct Reel {
    pub id: String,
    /// The parts that one board takes from it.
    pub per_board: u32,
    /// The parts in the reel as it is now.
    pub left: u32,
    /// The parts in a reel that replaces it.
    pub size: u32,
}

/// What the shift is made of.
#[derive(Debug, Clone, PartialEq)]
pub struct Inputs {
    /// The seconds that a board takes, from the plan.
    pub board_s: f64,
    pub shift_s: f64,
    /// The seconds that the machine is stopped to change one reel.
    pub reel_change_s: f64,
    /// Stop when this many boards are made (`None`: as many as fit).
    pub boards: Option<u64>,
    pub reels: Vec<Reel>,
}

/// A change of a reel.
#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    /// The board (from 1) that the reel is changed for.
    pub board: u64,
    /// The second of the shift at which the machine stops.
    pub at_s: f64,
    pub feeder: String,
}

/// What ended the shift.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Limit {
    /// The next board does not fit in what is left of the shift.
    Time,
    /// The number of boards that was asked is made.
    Boards,
    /// The most boards of a shift is reached.
    Cap,
}

/// The shift.
#[derive(Debug, Clone, PartialEq)]
pub struct Shift {
    pub boards: u64,
    pub changes: Vec<Change>,
    /// The seconds of the machine at work, the seconds stopped to change reels and the seconds of the shift that were left.
    pub work_s: f64,
    pub change_s: f64,
    pub left_s: f64,
    pub limit: Limit,
}

fn bad(detail: impl Into<String>) -> PnpError {
    PnpError::new(Code::PlanSetting, detail.into())
}

/// Plans the shift.
pub fn plan(inputs: &Inputs) -> Result<Shift, PnpError> {
    if !inputs.board_s.is_finite() || inputs.board_s <= 0.0 {
        return Err(bad("the plan takes no time: there is nothing to place"));
    }
    if !inputs.shift_s.is_finite() || inputs.shift_s <= 0.0 {
        return Err(bad("the shift has no time"));
    }
    if !inputs.reel_change_s.is_finite() || inputs.reel_change_s < 0.0 {
        return Err(bad("the time of a change of reel is not a time"));
    }
    if let Some(r) = inputs
        .reels
        .iter()
        .find(|r| r.per_board > r.size.max(r.left))
    {
        return Err(bad(format!(
            "the feeder {} takes {} parts for a board and a reel of it holds {}",
            r.id,
            r.per_board,
            r.size.max(r.left)
        )));
    }
    let mut reels: Vec<Reel> = inputs
        .reels
        .iter()
        .filter(|r| r.per_board > 0)
        .cloned()
        .collect();
    let goal = inputs.boards.unwrap_or(u64::MAX);
    let mut shift = Shift {
        boards: 0,
        changes: Vec::new(),
        work_s: 0.0,
        change_s: 0.0,
        left_s: inputs.shift_s,
        limit: Limit::Time,
    };
    let mut t = 0.0f64;
    loop {
        if shift.boards >= goal {
            shift.limit = Limit::Boards;
            break;
        }
        if shift.boards >= MAX_BOARDS {
            shift.limit = Limit::Cap;
            break;
        }
        let needing: Vec<usize> = (0..reels.len())
            .filter(|n| reels[*n].left < reels[*n].per_board)
            .collect();
        let stop = needing.len() as f64 * inputs.reel_change_s;
        if t + stop + inputs.board_s > inputs.shift_s + 1e-9 {
            break;
        }
        for n in needing {
            shift.changes.push(Change {
                board: shift.boards + 1,
                at_s: t,
                feeder: reels[n].id.clone(),
            });
            t += inputs.reel_change_s;
            shift.change_s += inputs.reel_change_s;
            reels[n].left = reels[n].size;
        }
        t += inputs.board_s;
        shift.work_s += inputs.board_s;
        for r in &mut reels {
            r.left -= r.per_board;
        }
        shift.boards += 1;
    }
    shift.left_s = (inputs.shift_s - t).max(0.0);
    Ok(shift)
}

impl Shift {
    /// The boards for each hour of the shift that was used (changes included).
    pub fn per_hour(&self) -> f64 {
        let used = self.work_s + self.change_s;
        if used <= 0.0 {
            0.0
        } else {
            self.boards as f64 * 3600.0 / used
        }
    }

    /// The share of the shift that was lost to changes of reels, 0 to 1.
    pub fn lost(&self, shift_s: f64) -> f64 {
        if shift_s <= 0.0 {
            0.0
        } else {
            (self.change_s / shift_s).clamp(0.0, 1.0)
        }
    }

    /// The report for a person.
    pub fn text(&self, shift_s: f64) -> String {
        let mut out = String::new();
        for c in self.changes.iter().take(MAX_SHOWN) {
            out.push_str(&format!(
                "board {}: change the reel of feeder {} at {:.1} min\n",
                c.board,
                c.feeder,
                c.at_s / 60.0
            ));
        }
        if self.changes.len() > MAX_SHOWN {
            out.push_str(&format!(
                "... and {} more changes (all of them are in the table of --out)\n",
                self.changes.len() - MAX_SHOWN
            ));
        }
        let why = match self.limit {
            Limit::Time => "the next board does not fit in what is left of the shift",
            Limit::Boards => "the boards that were asked are made",
            Limit::Cap => "the most boards of a shift is reached",
        };
        out.push_str(&format!(
            "{} boards in a shift of {:.0} min ({why}); {} changes of reels take {:.1} min ({:.1}% of the shift); {:.1} min are left; {:.2} boards an hour\n",
            self.boards,
            shift_s / 60.0,
            self.changes.len(),
            self.change_s / 60.0,
            self.lost(shift_s) * 100.0,
            self.left_s / 60.0,
            self.per_hour()
        ));
        out
    }

    /// The changes as a table: `board,at_s,feeder`.
    pub fn csv(&self) -> String {
        let mut out = String::from("board,at_s,feeder\n");
        for c in &self.changes {
            out.push_str(&format!(
                "{},{:.3},{}\n",
                c.board,
                c.at_s,
                csv_field(&c.feeder)
            ));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reel(id: &str, per_board: u32, left: u32, size: u32) -> Reel {
        Reel {
            id: id.into(),
            per_board,
            left,
            size,
        }
    }

    fn inputs(reels: Vec<Reel>) -> Inputs {
        Inputs {
            board_s: 60.0,
            shift_s: 3600.0,
            reel_change_s: 30.0,
            boards: None,
            reels,
        }
    }

    #[test]
    fn with_enough_parts_every_board_fits_and_nothing_is_changed() {
        let s = plan(&inputs(vec![reel("1", 2, 1000, 1000)])).unwrap();
        assert_eq!(s.boards, 60);
        assert!(s.changes.is_empty());
        assert_eq!(s.change_s, 0.0);
        assert_eq!(s.limit, Limit::Time);
        assert!((s.per_hour() - 60.0).abs() < 1e-9);
    }

    #[test]
    fn a_reel_is_changed_for_the_board_that_it_cannot_supply() {
        // 10 parts, 4 for a board: boards 1 and 2 take 8, the third needs a change
        let s = plan(&inputs(vec![reel("A", 4, 10, 10)])).unwrap();
        assert_eq!(s.changes[0].board, 3);
        assert_eq!(s.changes[0].feeder, "A");
        // two boards take 120 s before the stop
        assert!((s.changes[0].at_s - 120.0).abs() < 1e-9);
        // 2 left, +10 -> boards 3,4 take 8 -> 2 left; change at board 5
        assert_eq!(s.changes[1].board, 5);
        assert!(s.change_s > 0.0);
        assert!(s.boards < 60);
    }

    #[test]
    fn the_time_of_the_changes_comes_out_of_the_shift() {
        let mut i = inputs(vec![reel("A", 1, 1, 1)]);
        i.board_s = 100.0;
        i.shift_s = 1000.0;
        i.reel_change_s = 100.0;
        // a change for every board but the first: 100 + 100 + ... boards at t=0, 200, 400, 600, 800 (the fifth ends at 900)
        let s = plan(&i).unwrap();
        assert_eq!(s.boards, 5);
        assert_eq!(s.changes.len(), 4);
        assert!((s.change_s - 400.0).abs() < 1e-9);
        assert!((s.work_s - 500.0).abs() < 1e-9);
        assert!((s.left_s - 100.0).abs() < 1e-9);
        assert!((s.lost(1000.0) - 0.4).abs() < 1e-9);
    }

    #[test]
    fn the_boards_asked_end_the_shift_early() {
        let mut i = inputs(vec![reel("A", 1, 100, 100)]);
        i.boards = Some(7);
        let s = plan(&i).unwrap();
        assert_eq!((s.boards, s.limit), (7, Limit::Boards));
        assert!((s.left_s - (3600.0 - 420.0)).abs() < 1e-9);
        i.boards = Some(0);
        assert_eq!(plan(&i).unwrap().boards, 0);
    }

    #[test]
    fn several_reels_that_run_out_together_each_take_a_change() {
        let i = inputs(vec![reel("A", 1, 0, 5), reel("B", 1, 0, 5)]);
        let s = plan(&i).unwrap();
        assert_eq!(
            s.changes
                .iter()
                .take(2)
                .map(|c| (c.board, c.feeder.as_str()))
                .collect::<Vec<_>>(),
            vec![(1, "A"), (1, "B")]
        );
        assert!((s.changes[1].at_s - 30.0).abs() < 1e-9);
    }

    #[test]
    fn a_feeder_that_the_plan_does_not_use_never_stops_the_machine() {
        let s = plan(&inputs(vec![reel("A", 0, 0, 0)])).unwrap();
        assert_eq!(s.boards, 60);
        assert!(s.changes.is_empty());
    }

    #[test]
    fn a_plan_that_cannot_be_run_is_refused() {
        let mut i = inputs(vec![reel("A", 5, 3, 4)]);
        assert!(plan(&i).is_err());
        i.reels = vec![];
        i.board_s = 0.0;
        assert!(plan(&i).is_err());
        i.board_s = f64::NAN;
        assert!(plan(&i).is_err());
        i.board_s = 1.0;
        i.shift_s = 0.0;
        assert!(plan(&i).is_err());
        i.shift_s = 10.0;
        i.reel_change_s = -1.0;
        assert!(plan(&i).is_err());
    }

    #[test]
    fn a_very_short_board_stops_at_the_cap() {
        let mut i = inputs(vec![]);
        i.board_s = 0.0001;
        i.shift_s = 86_400.0;
        let s = plan(&i).unwrap();
        assert_eq!((s.boards, s.limit), (MAX_BOARDS, Limit::Cap));
    }

    #[test]
    fn a_shift_that_cannot_hold_one_board_makes_none() {
        let mut i = inputs(vec![]);
        i.shift_s = 59.0;
        let s = plan(&i).unwrap();
        assert_eq!(s.boards, 0);
        assert_eq!(s.per_hour(), 0.0);
        assert!((s.left_s - 59.0).abs() < 1e-9);
    }

    #[test]
    fn the_text_and_the_table_name_the_changes() {
        let s = plan(&inputs(vec![reel("A", 4, 10, 10)])).unwrap();
        let text = s.text(3600.0);
        assert!(text.contains("board 3: change the reel of feeder A at 2.0 min"));
        assert!(text.contains("boards in a shift of 60 min"));
        let csv = s.csv();
        assert!(csv.starts_with("board,at_s,feeder\n3,120.000,A\n"));
        assert_eq!(csv.lines().count(), s.changes.len() + 1);
    }

    #[test]
    fn the_text_lists_the_first_changes_and_counts_the_rest() {
        let s = plan(&inputs(vec![reel("A", 1, 1, 1)])).unwrap();
        assert!(s.changes.len() > MAX_SHOWN);
        let text = s.text(3600.0);
        assert_eq!(text.matches("change the reel").count(), MAX_SHOWN);
        assert!(text.contains(&format!(
            "... and {} more changes",
            s.changes.len() - MAX_SHOWN
        )));
        assert_eq!(s.csv().lines().count(), s.changes.len() + 1);
    }

    #[test]
    fn the_same_inputs_give_the_same_shift() {
        let i = inputs(vec![reel("A", 3, 17, 40), reel("B", 2, 9, 30)]);
        assert_eq!(plan(&i).unwrap(), plan(&i).unwrap());
    }
}
