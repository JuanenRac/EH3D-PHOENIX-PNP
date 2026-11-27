// EH3D-PHOENIX-PNP - what the panels of the slots and of the nodes show, without the window.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! The panel of the slots draws the banks of feeders (a square for each slot, coloured by its state) and the panel of the nodes says whether the head and the feeder node answer and whether the
//! machine may move. Both read from one bench: a head node and a feeder node that are made up (`nodesim`), the link to them with its hold (`nodelink`) and the table of the banks
//! (`slotbank`). Nothing here talks to a real node: the panels say so. The bench has what a person needs to see the rules at work: lower a nozzle, cut the link, jam a feeder, and
//! acknowledge. All of it is a calculation on names and numbers, so it is tried without a screen; `window` only draws what these functions give.

use super::texts::Text;
use crate::config::MachineConfig;
use crate::nodelink::{one_lost_question_is_enough, Feeders, Head, Hold, LinkError};
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
    /// The file the table of the banks was read from and the revision it had then; none for the example.
    pub source: Option<Source>,
    /// Whether the table on show was changed since it was read or saved.
    pub dirty: bool,
    /// The seconds one question to a node may take (`node_timeout_s`).
    timeout_s: f64,
}

/// The file a table of the banks was read from and its revision.
#[derive(Debug, Clone, PartialEq)]
pub struct Source {
    pub path: std::path::PathBuf,
    pub revision: String,
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
            source: None,
            dirty: false,
            timeout_s: cfg.node_timeout_s,
        }
    }

    /// What the nodes said of their watch the last time they were asked: for the head and the feeder node, the milliseconds without a question with the token after which they are
    /// safe, and whether one question without an answer is enough for it. A node that was not asked, or does not say it, is none.
    pub fn watches(&self) -> [Option<(u32, bool)>; 2] {
        [self.head.watch_ms(), self.feeders.watch_ms()]
            .map(|w| w.map(|ms| (ms, one_lost_question_is_enough(ms, self.timeout_s))))
    }

    /// Reads a table of the banks from a file and keeps its revision, so that a later save is held to it. A file that cannot be read or is wrong leaves the table on show where it is and says
    /// why.
    pub fn open_file(&mut self, path: &std::path::Path, cfg: &MachineConfig) -> bool {
        let shown = path.to_string_lossy().into_owned();
        match crate::revision::read(&shown, cfg.max_file_bytes) {
            Ok((text, revision)) => {
                if self.load_table(&text, cfg) {
                    self.source = Some(Source {
                        path: path.to_path_buf(),
                        revision,
                    });
                    self.dirty = false;
                    true
                } else {
                    false
                }
            }
            Err(e) => {
                self.message = Some(e.to_string());
                false
            }
        }
    }

    /// Reads the file of the table again (a change that somebody else made is taken, one that was not saved is dropped).
    pub fn read_again(&mut self, cfg: &MachineConfig) -> bool {
        match self.source.clone() {
            Some(s) => self.open_file(&s.path, cfg),
            None => false,
        }
    }

    /// Puts a feeder in another state with the event that leads there (a reel put in again or a fault cleared for `Occupied`, the tape out for `Depleted`, a fault for `Jammed`, the feeder
    /// taken away for `Free`). A change that the rules do not allow is told and nothing changes.
    pub fn set_state(&mut self, id: &str, to: State) -> bool {
        use crate::slotbank::Event;
        let Some(from) = self.layout.find(id).map(|e| e.state) else {
            self.message = Some(format!("there is no feeder '{id}' in the banks"));
            return false;
        };
        let event = match (from, to) {
            (State::Depleted, State::Occupied) => Event::Reload,
            (State::Jammed, State::Occupied) => Event::Clear,
            (State::Occupied, State::Depleted) => Event::Exhaust,
            (State::Occupied, State::Jammed) => Event::Jam,
            (_, State::Free) => Event::Remove,
            _ => {
                self.message = Some(format!(
                    "{id}: a slot that is {} cannot become {}",
                    from.name(),
                    to.name()
                ));
                return false;
            }
        };
        match self.layout.apply(id, event) {
            Ok(_) => {
                self.message = None;
                self.dirty = true;
                true
            }
            Err(e) => {
                self.message = Some(e.to_string());
                false
            }
        }
    }

    /// Saves the table to the file it was read from, but only while the file is as it was read (the revision). A file that changed is not written over: the table on show stays as it is,
    /// marked as changed, and the reason is told. Gives the new revision.
    pub fn save(&mut self, cfg: &MachineConfig) -> Result<String, String> {
        let Some(source) = self.source.clone() else {
            return Err("the table was not read from a file".to_string());
        };
        let path = source.path.to_string_lossy().into_owned();
        match crate::revision::replace(
            &path,
            &source.revision,
            &self.layout.to_csv(),
            cfg.max_file_bytes,
        ) {
            Ok(new) => {
                self.source = Some(Source {
                    path: source.path,
                    revision: new.clone(),
                });
                self.dirty = false;
                self.message = None;
                Ok(new)
            }
            Err(e) => {
                let why = e.to_string();
                self.message = Some(why.clone());
                Err(why)
            }
        }
    }

    /// Loads another table of the banks; a table that is wrong leaves the one on show where it is and says why.
    pub fn load_table(&mut self, text: &str, cfg: &MachineConfig) -> bool {
        match Layout::from_csv(text, Geometry::from_config(cfg)) {
            Ok(l) => {
                self.layout = l;
                self.message = None;
                // a table that comes from text is not the one of a file
                self.source = None;
                self.dirty = false;
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
