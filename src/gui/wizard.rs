// EH3D-PHOENIX-PNP - the guide of five steps.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! The guide: five steps from nothing to a simulated job (the parts, the feeders, the machine, the checks, the simulation). It only says where the person is and whether the step can be
//! left; the window shows the hint of the step and the tab where the work of the step is done.

use super::texts::Text;
use super::workspace::Workspace;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Parts,
    Feeders,
    Machine,
    Check,
    Simulate,
}

impl Step {
    pub const ALL: [Step; 5] = [
        Step::Parts,
        Step::Feeders,
        Step::Machine,
        Step::Check,
        Step::Simulate,
    ];

    /// The place of the step, from 0.
    pub fn index(self) -> usize {
        Step::ALL.iter().position(|s| *s == self).unwrap_or(0)
    }

    pub fn title(self) -> Text {
        match self {
            Step::Parts => Text::Guide1,
            Step::Feeders => Text::Guide2,
            Step::Machine => Text::Guide3,
            Step::Check => Text::Guide4,
            Step::Simulate => Text::Guide5,
        }
    }

    pub fn hint(self) -> Text {
        match self {
            Step::Parts => Text::Guide1Hint,
            Step::Feeders => Text::Guide2Hint,
            Step::Machine => Text::Guide3Hint,
            Step::Check => Text::Guide4Hint,
            Step::Simulate => Text::Guide5Hint,
        }
    }

    pub fn next(self) -> Option<Step> {
        Step::ALL.get(self.index() + 1).copied()
    }

    pub fn prev(self) -> Option<Step> {
        self.index().checked_sub(1).map(|i| Step::ALL[i])
    }
}

/// What the guide needs to know about the plan to let the person leave the step of the checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanState {
    NoPlan,
    WithErrors,
    Clean,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Wizard {
    pub step: Step,
    pub open: bool,
}

impl Default for Wizard {
    fn default() -> Self {
        Wizard {
            step: Step::Parts,
            open: false,
        }
    }
}

impl Wizard {
    /// Whether the work of the step is done, so that the person may go on.
    pub fn can_continue(&self, ws: &Workspace, plan: PlanState) -> bool {
        match self.step {
            Step::Parts => !ws.parts.is_empty(),
            Step::Feeders => !ws.feeders.is_empty(),
            Step::Machine => true,
            Step::Check => plan == PlanState::Clean,
            Step::Simulate => false,
        }
    }

    /// Goes to the next step when the work of this one is done; gives whether it moved.
    pub fn next(&mut self, ws: &Workspace, plan: PlanState) -> bool {
        match self.step.next() {
            Some(n) if self.can_continue(ws, plan) => {
                self.step = n;
                true
            }
            _ => false,
        }
    }

    pub fn back(&mut self) -> bool {
        match self.step.prev() {
            Some(p) => {
                self.step = p;
                true
            }
            None => false,
        }
    }
}
