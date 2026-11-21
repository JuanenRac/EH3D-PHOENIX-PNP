// EH3D-PHOENIX-PNP - what the panels of the slots and of the nodes show, without the window.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! The panel of the slots draws the banks of feeders (a square for each slot, coloured by its state) and the panel of the nodes says whether the head and the feeder node answer and whether the
//! machine may move. Both read from one bench: a head node and a feeder node that are made up (`nodesim`), the link to them with its hold (`nodelink`) and the table of the banks
//! (`slotbank`). Nothing here talks to a real node: the panels say so. The bench has what a person needs to see the rules at work: lower a nozzle, cut the link, jam a feeder, and
//! acknowledge. All of it is a calculation on names and numbers, so it is tried without a screen; `window` only draws what these functions give.

use super::texts::Text;
use crate::config::MachineConfig;
use crate::nodelink::{Feeders, Head, Hold, LinkError};
use crate::nodesim::{Injection, SimFeeders, SimHead};
use crate::slotbank::{Bank, Geometry, Layout, State};

/// The table the panel of the slots starts with.
pub const EXAMPLE_TABLE: &str = include_str!("../../examples/slot_bank.csv");

/// How a node answers, for a light.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Light {
    /// Nobody has asked yet.
    Unknown,
    /// It answers.
    Answers,
    /// It does not answer.
    Silent,
    /// The guard holds it until a person acknowledges.
    Held,
    /// It refuses the token.
    Denied,
}

impl Light {
    pub const ALL: [Light; 5] = [
        Light::Unknown,
        Light::Answers,
        Light::Silent,
        Light::Held,
        Light::Denied,
    ];

    /// The word that says it.
    pub fn text(self) -> Text {
        match self {
            Light::Unknown => Text::NodeLinkUnknown,
            Light::Answers => Text::NodeLinkOk,
            Light::Silent => Text::NodeLinkSilent,
            Light::Held => Text::NodeLinkHeld,
            Light::Denied => Text::NodeLinkDenied,
        }
    }
}

/// The light of a node from how the last question ended and whether the guard holds it. A hold wins over the answer: a node that answers but is held is held.
pub fn light_of<T>(last: Option<&Result<T, LinkError>>, held: Option<Hold>) -> Light {
    if held.is_some() {
        return Light::Held;
    }
    match last {
        None => Light::Unknown,
        Some(Ok(_)) => Light::Answers,
        Some(Err(LinkError::Unauthorized)) => Light::Denied,
        Some(Err(LinkError::Silent)) => Light::Silent,
        // a refusal or an answer that cannot be read is an answer of a node that is there
        Some(Err(_)) => Light::Answers,
    }
}

/// A slot as the panel draws it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    /// The slot, counted from 1.
    pub slot: u32,
    pub state: State,
    /// The feeder that stands there, and whether this is the first slot of it.
    pub id: Option<String>,
    pub first: bool,
}

/// The word of a state.
pub fn state_text(state: State) -> Text {
    match state {
        State::Free => Text::SlotFree,
        State::Occupied => Text::SlotOccupied,
        State::Depleted => Text::SlotDepleted,
        State::Jammed => Text::SlotJammed,
    }
}

/// The word of a bank.
pub fn bank_text(bank: Bank) -> Text {
    match bank {
        Bank::Left => Text::SlotBankLeft,
        Bank::Right => Text::SlotBankRight,
    }
}

/// The head, the feeder node and the banks, all made up.
pub struct Bench {
    head_sim: SimHead,
    feed_sim: SimFeeders,
    head: Head<SimHead>,
    feeders: Feeders<SimFeeders>,
    pub layout: Layout,
    pub head_light: Light,
    pub feeders_light: Light,
    /// What the head said the last time it was asked: may the machine move in X and Y.
    pub xy_clear: bool,
    /// Why the table was not loaded, when it was not.
    pub message: Option<String>,
    /// The nozzle that is lowered (simulation), or the link that is cut.
    pub nozzle_down: bool,
    pub head_cut: bool,
    pub feeder_jammed: bool,
}

impl std::fmt::Debug for Bench {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bench")
            .field("head_light", &self.head_light)
            .field("feeders_light", &self.feeders_light)
            .field("xy_clear", &self.xy_clear)
            .field("feeders", &self.layout.entries().len())
            .finish()
    }
}

impl Bench {
    /// A bench with the example table. The sizes of the slots, and how many unanswered questions hold a node, are the settings.
    pub fn new(cfg: &MachineConfig) -> Bench {
        let head_sim = SimHead::new(2, "bench");
        let feed_sim = SimFeeders::new(8, Some(500), "bench");
        let layout = Layout::from_csv(EXAMPLE_TABLE, Geometry::from_config(cfg))
            .unwrap_or_else(|_| Layout::new(Geometry::from_config(cfg)));
        Bench {
            head: Head::new(head_sim.clone(), cfg.node_silence_limit),
            feeders: Feeders::new(feed_sim.clone(), cfg.node_silence_limit),
            head_sim,
            feed_sim,
            layout,
            head_light: Light::Unknown,
            feeders_light: Light::Unknown,
            xy_clear: false,
            message: None,
            nozzle_down: false,
            head_cut: false,
            feeder_jammed: false,
        }
    }

    /// Loads another table of the banks; a table that is wrong leaves the one on show where it is and says why.
    pub fn load_table(&mut self, text: &str, cfg: &MachineConfig) -> bool {
        match Layout::from_csv(text, Geometry::from_config(cfg)) {
            Ok(l) => {
                self.layout = l;
                self.message = None;
                true
            }
            Err(e) => {
                self.message = Some(e.to_string());
                false
            }
        }
    }

    /// The slots of a bank, one cell for each.
    pub fn cells(&self, bank: Bank) -> Vec<Cell> {
        (1..=self.layout.geo.slots)
            .map(|slot| {
                let e = self.layout.occupant(bank, slot);
                Cell {
                    slot,
                    state: e.map_or(State::Free, |e| e.state),
                    id: e.map(|e| e.id.clone()),
                    first: e.is_some_and(|e| e.first == slot),
                }
            })
            .collect()
    }

    /// Asks both nodes, takes what the feeder node says into the slots and says whether the machine may move. Gives how many slots changed state.
    pub fn ask(&mut self) -> usize {
        let head = self.head.refresh().map(|_| ());
        self.xy_clear = self.head.guard().held().is_none()
            && self.head.last().is_some_and(|s| s.xy_clear && !s.safe);
        self.head_light = light_of(Some(&head), self.head.guard().held());
        let chain = self.feeders.refresh().map(|c| c.seen());
        self.feeders_light = light_of(Some(&chain), self.feeders.guard().held());
        match chain {
            Ok(seen) => self.layout.sync(&seen).len(),
            Err(_) => 0,
        }
    }

    /// A person acknowledges the holds of both nodes (and the safe state of the head).
    pub fn acknowledge(&mut self) {
        let _ = self.head.acknowledge_node();
        let _ = self.feeders.acknowledge();
        self.ask();
    }

    /// Lowers (or raises) the second nozzle of the head, as if the machine had.
    pub fn lower_nozzle(&mut self, down: bool) {
        self.head_sim.set_z(2, if down { 0.0 } else { 5.0 });
        self.nozzle_down = down;
    }

    /// Cuts the link to the head (it never answers) or restores it.
    pub fn cut_head(&mut self, cut: bool) {
        self.head_sim.set_injection(if cut {
            Injection::seeded(1, 1000, 0, 0)
        } else {
            Injection::none()
        });
        self.head_cut = cut;
    }

    /// Jams the second feeder of the chain, or gives the cause away and clears it.
    pub fn jam_feeder(&mut self, jam: bool) {
        if jam {
            self.feed_sim.jam(2);
        } else {
            self.feed_sim.clear_cause(2);
            let _ = self.feeders.ack(2);
        }
        self.feeder_jammed = jam;
    }

    /// Whether the guard holds either node.
    pub fn held(&self) -> bool {
        self.head.guard().held().is_some() || self.feeders.guard().held().is_some()
    }
}
