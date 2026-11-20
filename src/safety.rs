// EH3D-PHOENIX-PNP - the rules that only ever make a machine safer after something goes wrong.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! After a retry, a timeout, a refusal, a position that is not where it should be or a lost vacuum, the limits of the machine move one way: slower, then held, then stopped. A good
//! answer does not give them back, and nothing but a person who says so (`acknowledge`) lowers the level, and then only from held to slow, never to normal; a stopped machine stays stopped.
//! `Governor::observe` is the only door for events and it is written so that no event can lower a level or raise a limit; the tests walk every sequence of events up to a length to prove it, and
//! they fail if a change to the code relaxes a limit after an error. The figures (how many events, how slow) are in the profile file.

use crate::error::Lang;
use crate::gcodedriver::DriverProfile;
use crate::msg8::{num, say};

/// How far the machine is held back, from free to stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Level {
    Normal,
    Careful,
    Slow,
    Hold,
    Stopped,
}

impl Level {
    pub const ALL: [Level; 5] = [
        Level::Normal,
        Level::Careful,
        Level::Slow,
        Level::Hold,
        Level::Stopped,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Level::Normal => "normal",
            Level::Careful => "careful",
            Level::Slow => "slow",
            Level::Hold => "hold",
            Level::Stopped => "stopped",
        }
    }
}

/// What happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Event {
    /// A step went well.
    Success,
    /// A step had to be tried again.
    Retry,
    /// The controller did not answer in time.
    Timeout,
    /// The controller answered with an error.
    ErrorReply,
    /// The head is not where the controller says.
    PositionMismatch,
    /// The vacuum was lost with a part on the nozzle.
    VacuumLost,
    /// A move was refused for being outside the limits.
    LimitRefused,
}

impl Event {
    pub const ALL: [Event; 7] = [
        Event::Success,
        Event::Retry,
        Event::Timeout,
        Event::ErrorReply,
        Event::PositionMismatch,
        Event::VacuumLost,
        Event::LimitRefused,
    ];

    /// The word of the event in a file of events.
    pub fn word(self) -> &'static str {
        match self {
            Event::Success => "success",
            Event::Retry => "retry",
            Event::Timeout => "timeout",
            Event::ErrorReply => "error",
            Event::PositionMismatch => "mismatch",
            Event::VacuumLost => "vacuum",
            Event::LimitRefused => "limit",
        }
    }

    pub fn from_word(w: &str) -> Option<Event> {
        Event::ALL
            .into_iter()
            .find(|e| e.word() == w.trim().to_ascii_lowercase())
    }

    /// The least level this event puts the machine in at once.
    fn floor(self) -> Level {
        match self {
            Event::Success | Event::Retry | Event::Timeout | Event::LimitRefused => Level::Normal,
            Event::PositionMismatch | Event::VacuumLost => Level::Hold,
            Event::ErrorReply => Level::Stopped,
        }
    }

    /// Whether the event counts toward the slow-down.
    fn counts(self) -> bool {
        matches!(self, Event::Retry | Event::Timeout | Event::LimitRefused)
    }
}

/// The figures of the rules.
#[derive(Debug, Clone, PartialEq)]
pub struct Rules {
    pub careful_events: u32,
    pub slow_events: u32,
    pub hold_events: u32,
    pub careful_speed_pct: f64,
    pub slow_speed_pct: f64,
}

impl Rules {
    pub fn from_profile(p: &DriverProfile) -> Rules {
        Rules {
            careful_events: p.careful_events,
            slow_events: p.slow_events,
            hold_events: p.hold_events,
            careful_speed_pct: p.careful_speed_pct,
            slow_speed_pct: p.slow_speed_pct,
        }
    }

    /// Whether the figures themselves keep the rule: the counts grow, the speeds fall and none is over 100.
    pub fn check(&self) -> Result<(), String> {
        if !(0 < self.careful_events
            && self.careful_events < self.slow_events
            && self.slow_events < self.hold_events)
        {
            return Err("the events that bring careful, slow and hold must grow".into());
        }
        if !(0.0 < self.slow_speed_pct
            && self.slow_speed_pct <= self.careful_speed_pct
            && self.careful_speed_pct <= 100.0)
        {
            return Err(
                "the speeds must fall from 100 per cent: careful at most 100, slow at most careful"
                    .into(),
            );
        }
        Ok(())
    }
}

/// The limits the level puts on the machine.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Limits {
    /// The share of the feed that a move may use.
    pub speed_pct: f64,
    /// Whether a move may be sent.
    pub moves: bool,
}

/// The state of the rules.
#[derive(Debug, Clone)]
pub struct Governor {
    rules: Rules,
    level: Level,
    counted: u32,
    speed_cap: f64,
    /// Every event with the level after it.
    pub history: Vec<(Event, Level)>,
}

impl Governor {
    pub fn new(rules: Rules) -> Governor {
        Governor {
            rules,
            level: Level::Normal,
            counted: 0,
            speed_cap: 100.0,
            history: Vec::new(),
        }
    }

    pub fn level(&self) -> Level {
        self.level
    }

    fn level_of_count(&self, count: u32) -> Level {
        if count >= self.rules.hold_events {
            Level::Hold
        } else if count >= self.rules.slow_events {
            Level::Slow
        } else if count >= self.rules.careful_events {
            Level::Careful
        } else {
            Level::Normal
        }
    }

    fn speed_for(&self, level: Level) -> f64 {
        match level {
            Level::Normal => 100.0,
            Level::Careful => self.rules.careful_speed_pct,
            Level::Slow => self.rules.slow_speed_pct,
            Level::Hold | Level::Stopped => 0.0,
        }
    }

    /// Takes an event in. The level never goes down here and the cap on the speed never goes up.
    pub fn observe(&mut self, e: Event) -> Level {
        if e.counts() {
            self.counted = self.counted.saturating_add(1);
        }
        let by_count = self.level_of_count(self.counted);
        let next = self.level.max(by_count).max(e.floor());
        self.level = next;
        self.speed_cap = self.speed_cap.min(self.speed_for(next));
        self.history.push((e, next));
        next
    }

    /// What the machine may do now.
    pub fn limits(&self) -> Limits {
        Limits {
            speed_pct: self.speed_cap.min(self.speed_for(self.level)),
            moves: self.level < Level::Hold,
        }
    }

    /// A person says the cause was looked at. Only a held machine can be released, and only to slow with the speed cap of slow; a stopped one cannot.
    pub fn acknowledge(&mut self) -> Result<Level, String> {
        match self.level {
            Level::Hold => {
                self.level = Level::Slow;
                self.counted = self.counted.max(self.rules.slow_events);
                // the one place where a limit is given back, by a person and only up to slow
                self.speed_cap = self.rules.slow_speed_pct;
                Ok(self.level)
            }
            Level::Stopped => Err("the machine is stopped: start a new session".into()),
            _ => Err("nothing is held".into()),
        }
    }
}

/// What a replay of events gives.
#[derive(Debug, Clone, PartialEq)]
pub struct Replay {
    /// One row per event: the word, the level after it and the speed share.
    pub rows: Vec<(String, Level, f64, bool)>,
    /// The lines that were not an event, with their numbers.
    pub unknown: Vec<(usize, String)>,
}

/// Replays a file of events (one word per line, `ack` for an acknowledgement, `#` for a comment) against the rules.
pub fn replay(rules: &Rules, text: &str) -> Replay {
    let mut g = Governor::new(rules.clone());
    let mut rows = Vec::new();
    let mut unknown = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let w = raw
            .split('#')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        if w.is_empty() {
            continue;
        }
        if w == "ack" {
            let _ = g.acknowledge();
        } else if let Some(e) = Event::from_word(&w) {
            g.observe(e);
        } else {
            unknown.push((i + 1, w));
            continue;
        }
        let l = g.limits();
        rows.push((w, g.level(), l.speed_pct, l.moves));
    }
    Replay { rows, unknown }
}

/// The table of a replay in a language.
pub fn replay_text(r: &Replay, lang: Lang) -> String {
    let head: [&str; 8] = [
        "event          level      speed   moves",
        "evento         nivel      veloc.  movim.",
        "Ereignis       Stufe      Tempo   Fahrten",
        "événement      niveau     vitesse mouv.",
        "evento         livello    veloc.  mov.",
        "イベント       レベル     速度    移動",
        "事件           级别       速度    移动",
        "evento         nível      veloc.  mov.",
    ];
    let mut out = String::new();
    out.push_str(crate::msg8::pick(lang, &head));
    out.push('\n');
    for (w, l, s, m) in &r.rows {
        out.push_str(&format!(
            "{:<14} {:<10} {:>5}%  {}\n",
            w,
            l.name(),
            num(*s),
            if *m { "yes" } else { "no" }
        ));
    }
    for (n, w) in &r.unknown {
        let row: [&str; 8] = [
            "line {n}: '{w}' is not an event (success, retry, timeout, error, mismatch, vacuum, limit, ack)",
            "línea {n}: '{w}' no es un evento (success, retry, timeout, error, mismatch, vacuum, limit, ack)",
            "Zeile {n}: '{w}' ist kein Ereignis (success, retry, timeout, error, mismatch, vacuum, limit, ack)",
            "ligne {n} : '{w}' n'est pas un événement (success, retry, timeout, error, mismatch, vacuum, limit, ack)",
            "riga {n}: '{w}' non è un evento (success, retry, timeout, error, mismatch, vacuum, limit, ack)",
            "{n} 行目：「{w}」はイベントではありません（success, retry, timeout, error, mismatch, vacuum, limit, ack）",
            "第 {n} 行：“{w}”不是事件（success, retry, timeout, error, mismatch, vacuum, limit, ack）",
            "linha {n}: '{w}' não é um evento (success, retry, timeout, error, mismatch, vacuum, limit, ack)",
        ];
        out.push_str(&say(lang, &row, &[("n", n.to_string()), ("w", w.clone())]));
        out.push('\n');
    }
    out
}
