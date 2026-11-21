// EH3D-PHOENIX-PNP - the physical occupation of the slots of the banks of feeders.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! A bank of feeders is a row of slots, each `slot_pitch_mm` wide, on the left or on the right of the work table. A tape feeder is wider than its tape: a tape of W millimetres takes the next
//! whole number of slots above (W + `slot_gap_mm`) / `slot_pitch_mm`, so an 8 mm tape takes three slots and a bank of sixty holds twenty of them. This models that occupation: where each
//! feeder stands, that no two overlap, that each one is inside the bank and inside the reach of the head, and the state of what stands in a slot: free, occupied, depleted or jammed, with the
//! moves between them (a reel is loaded, runs out, jams, is cleared or reloaded, or is taken away) written as a table that a test walks completely. A feeder can name the position it has in the
//! chain of a feeder node (1 to 32); then the state the node reports (`Seen`) moves the state of the slot by the same table. It is a calculation on numbers and names: it asks nothing of a
//! machine, and the file it reads is a table with the header `id,bank,first_slot,tape_mm,state,chain`.

use crate::config::MachineConfig;
use crate::error::{Code, PnpError};
use crate::parts::{detect_delimiter, read_records};
use std::collections::BTreeSet;

/// The most feeders in a table.
pub const MAX_ENTRIES: usize = 2000;
/// The most positions in a chain of a feeder node.
pub const MAX_CHAIN: u8 = 32;
/// The widest and the narrowest tape that is believed.
pub const MIN_TAPE_MM: f64 = 2.0;
pub const MAX_TAPE_MM: f64 = 104.0;

fn bad(message: impl Into<String>) -> PnpError {
    PnpError::new(Code::SlotsBad, message)
}

/// The side of the work table that a bank is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Bank {
    Left,
    Right,
}

impl Bank {
    pub const ALL: [Bank; 2] = [Bank::Left, Bank::Right];

    pub fn name(self) -> &'static str {
        match self {
            Bank::Left => "left",
            Bank::Right => "right",
        }
    }

    pub fn parse(text: &str) -> Option<Bank> {
        match text.trim().to_ascii_lowercase().as_str() {
            "left" | "l" => Some(Bank::Left),
            "right" | "r" => Some(Bank::Right),
            _ => None,
        }
    }
}

/// What stands in a slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum State {
    /// Nothing stands there.
    Free,
    /// A feeder with tape.
    Occupied,
    /// A feeder whose tape has run out.
    Depleted,
    /// A feeder that holds a fault and moves nothing until it is cleared.
    Jammed,
}

impl State {
    pub const ALL: [State; 4] = [State::Free, State::Occupied, State::Depleted, State::Jammed];

    pub fn name(self) -> &'static str {
        match self {
            State::Free => "free",
            State::Occupied => "occupied",
            State::Depleted => "depleted",
            State::Jammed => "jammed",
        }
    }

    pub fn parse(text: &str) -> Option<State> {
        State::ALL
            .into_iter()
            .find(|s| s.name() == text.trim().to_ascii_lowercase())
    }

    /// The letter of the state in the picture of a bank.
    pub fn glyph(self) -> char {
        match self {
            State::Free => '.',
            State::Occupied => '#',
            State::Depleted => 'o',
            State::Jammed => 'X',
        }
    }
}

/// What happens to what stands in a slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// A feeder with a reel is put in the slot.
    Load,
    /// The tape runs out.
    Exhaust,
    /// The feeder holds a fault.
    Jam,
    /// The cause of the fault is gone and a person cleared it.
    Clear,
    /// A new reel is put in a feeder whose tape ran out.
    Reload,
    /// The feeder is taken away.
    Remove,
}

impl Event {
    pub const ALL: [Event; 6] = [
        Event::Load,
        Event::Exhaust,
        Event::Jam,
        Event::Clear,
        Event::Reload,
        Event::Remove,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Event::Load => "load",
            Event::Exhaust => "exhaust",
            Event::Jam => "jam",
            Event::Clear => "clear",
            Event::Reload => "reload",
            Event::Remove => "remove",
        }
    }
}

/// The state after an event, or `None` when the event cannot happen in that state (a tape cannot run out in a slot that is free, a jam cannot be cleared twice).
pub fn next(state: State, event: Event) -> Option<State> {
    match (state, event) {
        (State::Free, Event::Load) => Some(State::Occupied),
        (State::Occupied, Event::Exhaust) => Some(State::Depleted),
        (State::Occupied, Event::Jam) => Some(State::Jammed),
        (State::Jammed, Event::Clear) => Some(State::Occupied),
        (State::Depleted, Event::Reload) => Some(State::Occupied),
        (State::Occupied | State::Depleted | State::Jammed, Event::Remove) => Some(State::Free),
        _ => None,
    }
}

/// What a feeder node says about the feeder at one position of its chain, as far as the slot is concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Seen {
    /// The node does not answer for it or has not asked yet: nothing is learned.
    Unknown,
    /// It is there and fine.
    Fine,
    /// Its tape is empty.
    Empty,
    /// It holds a fault, is disabled or is in its safe state.
    Fault,
}

/// The sizes of the banks, from the settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Geometry {
    pub pitch_mm: f64,
    pub gap_mm: f64,
    pub slots: u32,
    pub reach_mm: f64,
}

impl Geometry {
    pub fn from_config(cfg: &MachineConfig) -> Geometry {
        Geometry {
            pitch_mm: cfg.slot_pitch_mm,
            gap_mm: cfg.slot_gap_mm,
            slots: cfg.slot_bank_slots,
            reach_mm: cfg.slot_reach_mm,
        }
    }

    /// How many slots a feeder of a tape of `tape_mm` takes.
    pub fn slots_for(&self, tape_mm: f64) -> Result<u32, PnpError> {
        if !tape_mm.is_finite() || !(MIN_TAPE_MM..=MAX_TAPE_MM).contains(&tape_mm) {
            return Err(bad(format!(
                "a tape of {tape_mm} mm is not from {MIN_TAPE_MM} to {MAX_TAPE_MM} mm"
            )));
        }
        // a small amount is taken off before the rounding up, so that a width that is exact in decimals is not pushed up by the binary error
        let n = ((tape_mm + self.gap_mm) / self.pitch_mm - 1e-9).ceil();
        Ok((n as u32).max(1))
    }

    /// How far from the inner edge of the bank the end of slot `last` (counted from 1) is.
    pub fn far_end_mm(&self, last: u32) -> f64 {
        f64::from(last) * self.pitch_mm
    }

    /// How many slots, counted from the inner edge, the head can reach.
    pub fn reachable_slots(&self) -> u32 {
        (((self.reach_mm / self.pitch_mm) + 1e-9).floor() as u32).min(self.slots)
    }
}

/// A feeder in a bank.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub id: String,
    pub bank: Bank,
    /// The first slot it takes, counted from 1 at the inner edge.
    pub first: u32,
    pub tape_mm: f64,
    /// How many slots it takes.
    pub width: u32,
    pub state: State,
    /// Its position in the chain of a feeder node.
    pub chain: Option<u8>,
}

impl Entry {
    /// The last slot it takes.
    pub fn last(&self) -> u32 {
        self.first + self.width - 1
    }

    fn covers(&self, bank: Bank, slot: u32) -> bool {
        self.bank == bank && (self.first..=self.last()).contains(&slot)
    }
}

/// A move of the state of a slot that `sync` made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub id: String,
    pub from: State,
    pub to: State,
}

/// The banks with the feeders that stand in them.
#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    pub geo: Geometry,
    entries: Vec<Entry>,
}

impl Layout {
    pub fn new(geo: Geometry) -> Layout {
        Layout {
            geo,
            entries: Vec::new(),
        }
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn find(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// The feeder that stands in a slot.
    pub fn occupant(&self, bank: Bank, slot: u32) -> Option<&Entry> {
        self.entries.iter().find(|e| e.covers(bank, slot))
    }

    /// The state of a slot: that of the feeder that stands in it, free when none does.
    pub fn state_of(&self, bank: Bank, slot: u32) -> State {
        self.occupant(bank, slot).map_or(State::Free, |e| e.state)
    }

    fn check(&self, e: &Entry) -> Result<(), PnpError> {
        if e.id.is_empty() || self.entries.iter().any(|o| o.id == e.id) {
            return Err(bad(format!("the id '{}' is empty or used twice", e.id)));
        }
        if e.first == 0 {
            return Err(bad(format!("{}: the first slot counts from 1", e.id)));
        }
        if e.last() > self.geo.slots {
            return Err(bad(format!(
                "{}: it takes slots {} to {} and a bank has {}",
                e.id,
                e.first,
                e.last(),
                self.geo.slots
            )));
        }
        let far = self.geo.far_end_mm(e.last());
        if far > self.geo.reach_mm + 1e-9 {
            return Err(bad(format!(
                "{}: its far end is {far:.1} mm from the edge of the bank and the head reaches {:.1} mm",
                e.id, self.geo.reach_mm
            )));
        }
        if let Some(o) = self
            .entries
            .iter()
            .find(|o| o.bank == e.bank && o.first <= e.last() && e.first <= o.last())
        {
            return Err(bad(format!(
                "{}: slots {} to {} of the {} bank are taken by {} (slots {} to {})",
                e.id,
                e.first,
                e.last(),
                e.bank.name(),
                o.id,
                o.first,
                o.last()
            )));
        }
        if let Some(c) = e.chain {
            if !(1..=MAX_CHAIN).contains(&c) {
                return Err(bad(format!(
                    "{}: the position {c} of the chain is not from 1 to {MAX_CHAIN}",
                    e.id
                )));
            }
            if let Some(o) = self.entries.iter().find(|o| o.chain == Some(c)) {
                return Err(bad(format!(
                    "{}: the position {c} of the chain is already {}'s",
                    e.id, o.id
                )));
            }
        }
        Ok(())
    }

    /// Puts a feeder in the bank (the event `Load`): it must be inside the bank and the reach of the head, on slots that are free, with a chain position that nobody has.
    pub fn place(
        &mut self,
        id: &str,
        bank: Bank,
        first: u32,
        tape_mm: f64,
        chain: Option<u8>,
    ) -> Result<(), PnpError> {
        let width = self.geo.slots_for(tape_mm)?;
        let e = Entry {
            id: id.to_string(),
            bank,
            first,
            tape_mm,
            width,
            state: next(State::Free, Event::Load).unwrap_or(State::Occupied),
            chain,
        };
        self.check(&e)?;
        if self.entries.len() >= MAX_ENTRIES {
            return Err(bad(format!("more than {MAX_ENTRIES} feeders")));
        }
        self.entries.push(e);
        Ok(())
    }

    /// Applies an event to a feeder that stands in the bank and gives its new state. `Remove` takes the feeder out and leaves its slots free.
    pub fn apply(&mut self, id: &str, event: Event) -> Result<State, PnpError> {
        let i = self
            .entries
            .iter()
            .position(|e| e.id == id)
            .ok_or_else(|| bad(format!("there is no feeder '{id}' in the banks")))?;
        let from = self.entries[i].state;
        let to = next(from, event).ok_or_else(|| {
            bad(format!(
                "{id}: a slot that is {} cannot take the event {}",
                from.name(),
                event.name()
            ))
        })?;
        if to == State::Free {
            self.entries.remove(i);
        } else {
            self.entries[i].state = to;
        }
        Ok(to)
    }

    /// The first slot from which `tape_mm` of tape fits on free slots of the bank and inside the reach of the head.
    pub fn first_fit(&self, bank: Bank, tape_mm: f64) -> Option<u32> {
        let width = self.geo.slots_for(tape_mm).ok()?;
        let top = self.geo.reachable_slots();
        (1..=top.checked_sub(width - 1)?)
            .find(|&first| (first..first + width).all(|s| self.occupant(bank, s).is_none()))
    }

    /// How many slots of the bank are free.
    pub fn free_slots(&self, bank: Bank) -> u32 {
        (1..=self.geo.slots)
            .filter(|&s| self.occupant(bank, s).is_none())
            .count() as u32
    }

    /// The picture of a bank, one letter for each slot: `.` free, `#` occupied, `o` depleted, `X` jammed.
    pub fn picture(&self, bank: Bank) -> String {
        (1..=self.geo.slots)
            .map(|s| self.state_of(bank, s).glyph())
            .collect()
    }

    /// How many feeders are in each state, as `[free slots, occupied, depleted, jammed]` counting feeders, with the free slots of both banks first.
    pub fn counts(&self) -> [u32; 4] {
        let mut out = [0u32; 4];
        out[0] = Bank::ALL.iter().map(|b| self.free_slots(*b)).sum();
        for e in &self.entries {
            match e.state {
                State::Occupied => out[1] += 1,
                State::Depleted => out[2] += 1,
                State::Jammed => out[3] += 1,
                State::Free => {}
            }
        }
        out
    }

    /// Moves the states of the feeders that have a chain position by what the node says. A feeder the node does not know (`Unknown`, or a position it did not list) keeps its state; one the
    /// node finds fine is cleared or reloaded when it was jammed or depleted; one that is empty runs out; one that holds a fault jams. A move the table of events does not allow is not made.
    pub fn sync(&mut self, seen: &[(u8, Seen)]) -> Vec<Change> {
        let mut changes = Vec::new();
        for e in &mut self.entries {
            let Some(chain) = e.chain else { continue };
            let Some((_, what)) = seen.iter().find(|(c, _)| *c == chain) else {
                continue;
            };
            let event = match (what, e.state) {
                (Seen::Unknown, _) => None,
                (Seen::Fine, State::Jammed) => Some(Event::Clear),
                (Seen::Fine, State::Depleted) => Some(Event::Reload),
                (Seen::Empty, State::Occupied) => Some(Event::Exhaust),
                (Seen::Fault, State::Occupied) => Some(Event::Jam),
                _ => None,
            };
            if let Some(to) = event.and_then(|ev| next(e.state, ev)) {
                changes.push(Change {
                    id: e.id.clone(),
                    from: e.state,
                    to,
                });
                e.state = to;
            }
        }
        changes
    }

    /// The table of feeders as a CSV that `from_csv` reads back.
    pub fn to_csv(&self) -> String {
        let mut rows: Vec<&Entry> = self.entries.iter().collect();
        rows.sort_by(|a, b| (a.bank, a.first, &a.id).cmp(&(b.bank, b.first, &b.id)));
        let mut out = String::from("id,bank,first_slot,tape_mm,state,chain\n");
        for e in rows {
            out.push_str(&format!(
                "{},{},{},{},{},{}\n",
                e.id,
                e.bank.name(),
                e.first,
                e.tape_mm,
                e.state.name(),
                e.chain.map_or(String::new(), |c| c.to_string())
            ));
        }
        out
    }

    /// The text for a person: a picture of each bank and a line for each feeder.
    pub fn text(&self) -> String {
        let mut out = String::new();
        for bank in Bank::ALL {
            out.push_str(&format!(
                "{:<5} {}  ({} of {} slots free)\n",
                bank.name(),
                self.picture(bank),
                self.free_slots(bank),
                self.geo.slots
            ));
        }
        let mut rows: Vec<&Entry> = self.entries.iter().collect();
        rows.sort_by(|a, b| (a.bank, a.first, &a.id).cmp(&(b.bank, b.first, &b.id)));
        for e in rows {
            out.push_str(&format!(
                "{}: {} bank, slots {} to {}, tape {} mm, {}{}\n",
                e.id,
                e.bank.name(),
                e.first,
                e.last(),
                e.tape_mm,
                e.state.name(),
                e.chain.map_or(String::new(), |c| format!(", chain {c}"))
            ));
        }
        let c = self.counts();
        out.push_str(&format!(
            "{} feeders: {} occupied, {} depleted, {} jammed; {} slots free\n",
            self.entries.len(),
            c[1],
            c[2],
            c[3],
            c[0]
        ));
        out
    }

    /// Reads a table with the header `id,bank,first_slot,tape_mm,state,chain` (the last two columns may be left empty: the state is then `occupied`). A feeder that is not there has no row,
    /// so `free` is not a state of a row.
    pub fn from_csv(text: &str, geo: Geometry) -> Result<Layout, PnpError> {
        let text = text.trim_start_matches('\u{feff}');
        let head_line = text
            .lines()
            .find(|l| !l.trim().is_empty())
            .ok_or_else(|| bad("the table of the banks is empty"))?;
        let mut records = read_records(text, detect_delimiter(head_line))
            .map_err(bad)?
            .into_iter();
        let head: Vec<String> = records
            .next()
            .ok_or_else(|| bad("the table of the banks is empty"))?
            .iter()
            .map(|h| h.to_ascii_lowercase())
            .collect();
        if head != ["id", "bank", "first_slot", "tape_mm", "state", "chain"] {
            return Err(bad(
                "the first row of the banks must be exactly: id,bank,first_slot,tape_mm,state,chain",
            ));
        }
        let mut layout = Layout::new(geo);
        let mut seen: BTreeSet<String> = BTreeSet::new();
        for (i, f) in records.enumerate() {
            let row = i + 2;
            let at = |m: String| bad(format!("row {row}: {m}"));
            if f.len() != 6 {
                return Err(at(format!("{} fields, not 6", f.len())));
            }
            if !seen.insert(f[0].clone()) {
                return Err(at(format!("the id '{}' is used twice", f[0])));
            }
            let bank =
                Bank::parse(&f[1]).ok_or_else(|| at(format!("'{}' is not left or right", f[1])))?;
            let first: u32 = f[2]
                .trim()
                .parse()
                .map_err(|_| at(format!("'{}' is not a slot", f[2])))?;
            let tape: f64 = f[3]
                .trim()
                .parse()
                .map_err(|_| at(format!("'{}' is not a width of tape", f[3])))?;
            let state = if f[4].trim().is_empty() {
                State::Occupied
            } else {
                State::parse(&f[4])
                    .filter(|s| *s != State::Free)
                    .ok_or_else(|| {
                        at(format!(
                            "'{}' is not occupied, depleted or jammed (a free slot has no row)",
                            f[4]
                        ))
                    })?
            };
            let chain = if f[5].trim().is_empty() {
                None
            } else {
                Some(
                    f[5].trim()
                        .parse::<u8>()
                        .map_err(|_| at(format!("'{}' is not a position of the chain", f[5])))?,
                )
            };
            layout
                .place(&f[0], bank, first, tape, chain)
                .map_err(|e| at(e.to_string()))?;
            if let Some(last) = layout.entries.last_mut() {
                last.state = state;
            }
        }
        Ok(layout)
    }
}
