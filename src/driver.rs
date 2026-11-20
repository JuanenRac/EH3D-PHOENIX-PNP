// EH3D-PHOENIX-PNP - a driver session: the steps of a job sent to a controller with the timeouts, the retries and the stop that the profile asks for.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! `Session` sends the lines of a profile over a `Link`. A query or a switch is sent again when no answer comes (the profile says how many times), a move is not: when the wait for a
//! move times out the session asks where the head is and goes on only if it is at the target. An error from the controller, a retry that is not enough, a position that is not the one
//! asked for or a refused move stop the session after the lines of `safe_stop` (the pump off, the motors released) and nothing more is sent until a new session: it fails closed. The
//! rules of `safety` slow the moves down after each trouble and never speed them up again. The only link there is today is a simulated one: this is evidence of a simulator.

use crate::error::Lang;
use crate::gcodedriver::{DriverProfile, Op, Pos, Reply, Target};
use crate::gcodesim::{Link, LinkError};
use crate::interlock::{self, Signals, MAX_SIGNAL};
use crate::plan::Placement;
use crate::safety::{Event, Governor, Level, Rules};

/// What stopped a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// No answer, after the tries.
    Timeout,
    /// The controller answered with an error.
    Rejected,
    /// The session is stopped (an earlier failure).
    Halted,
    /// The rules hold the machine.
    Held,
    /// A step the profile cannot write or whose place is outside the limits.
    Refused,
    /// The head is not where it should be.
    Mismatch,
    /// The link is closed or the answer cannot be read.
    Link,
    /// A rule of the profile keeps this move from being made.
    Interlock,
}

/// A failure of a session.
#[derive(Debug, Clone, PartialEq)]
pub struct Failure {
    pub kind: Kind,
    pub detail: String,
}

impl Failure {
    fn new(kind: Kind, detail: impl Into<String>) -> Failure {
        Failure {
            kind,
            detail: detail.into(),
        }
    }
}

/// A line sent and what came of it.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub at_s: f64,
    pub line: String,
    pub result: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    New,
    Ready,
    Halted,
}

/// What the wait for a move came to when it did not end well.
enum Waited {
    Timeout,
    Failed(Failure),
}

/// What a run of placements gave.
#[derive(Debug, Clone, PartialEq)]
pub struct RunReport {
    pub placed: usize,
    pub failed: Option<(String, Failure)>,
}

/// A session with a controller.
pub struct Session<L: Link> {
    pub profile: DriverProfile,
    pub link: L,
    pub governor: Governor,
    pub log: Vec<Entry>,
    /// Whether every move is followed by a question about where the head is.
    pub verify_moves: bool,
    /// The signals the interlocks of the profile look at: the actuators (what was last switched) and what a sensor was reported to be.
    pub signals: Signals,
    state: State,
}

/// The most signals a session keeps.
pub const MAX_SIGNALS: usize = 64;

fn near(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

impl<L: Link> Session<L> {
    pub fn new(profile: &DriverProfile, link: L) -> Session<L> {
        Session {
            profile: profile.clone(),
            link,
            governor: Governor::new(Rules::from_profile(profile)),
            log: Vec::new(),
            verify_moves: true,
            signals: profile
                .actuators
                .iter()
                .map(|a| (a.name.clone(), false))
                .collect(),
            state: State::New,
        }
    }

    /// Tells the session what a sensor says (a door closed, the nozzles lifted). A name that is not a name (letters, digits and `_`, at most `MAX_SIGNAL`) and a new name when `MAX_SIGNALS`
    /// are kept are refused, and the answer is false.
    pub fn set_signal(&mut self, name: &str, on: bool) -> bool {
        let valid = !name.is_empty()
            && name.len() <= MAX_SIGNAL
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        if !valid || (self.signals.len() >= MAX_SIGNALS && !self.signals.contains_key(name)) {
            return false;
        }
        self.signals.insert(name.to_string(), on);
        true
    }

    /// Whether the session was stopped by a failure.
    pub fn is_halted(&self) -> bool {
        self.state == State::Halted
    }

    fn note(&mut self, line: &str, result: &str) {
        let at_s = self.link.now();
        self.log.push(Entry {
            at_s,
            line: line.to_string(),
            result: result.to_string(),
        });
    }

    /// Takes an event into the rules (the vacuum that was lost, for one, is known to the caller and not to the controller).
    pub fn event(&mut self, e: Event) -> Level {
        self.governor.observe(e)
    }

    /// Stops the session after sending the lines that leave the machine safe. A line that does not get through is noted and the others are still sent.
    fn halt(&mut self, why: &str) {
        if self.state == State::Halted {
            return;
        }
        self.state = State::Halted;
        self.note("(halt)", why);
        let lines = self.profile.safe_stop.clone();
        let timeout = self.profile.command_timeout_s;
        for l in lines {
            let r = match self.link.exchange(&l, timeout) {
                Ok(_) => "sent".to_string(),
                Err(e) => format!("not sent: {e:?}"),
            };
            self.note(&l, &r);
        }
    }

    fn fail(&mut self, kind: Kind, detail: String, event: Event) -> Failure {
        self.governor.observe(event);
        self.halt(&detail);
        Failure::new(kind, detail)
    }

    /// Sends a line. With `again` a missing answer is asked for again up to `retries` times, with the pause of the profile between them.
    fn send(&mut self, line: &str, timeout_s: f64, again: bool) -> Result<Vec<Reply>, Failure> {
        if self.state == State::Halted {
            return Err(Failure::new(Kind::Halted, "the session is stopped"));
        }
        let mut tries = 0u32;
        loop {
            let answer = self.link.exchange(line, timeout_s);
            let lines = match answer {
                Ok(l) => l,
                Err(LinkError::Closed(why)) => {
                    self.note(line, "link closed");
                    return Err(self.fail(
                        Kind::Link,
                        format!("the link is closed: {why}"),
                        Event::Timeout,
                    ));
                }
                Err(LinkError::Timeout) => {
                    self.note(line, "timeout");
                    self.governor.observe(Event::Timeout);
                    if again && tries < self.profile.retries {
                        tries += 1;
                        self.link.pause(self.profile.retry_wait_s);
                        self.governor.observe(Event::Retry);
                        continue;
                    }
                    return Err(self.fail(
                        Kind::Timeout,
                        format!(
                            "no answer to '{line}' in {} s after {} tries",
                            timeout_s,
                            tries + 1
                        ),
                        Event::Timeout,
                    ));
                }
            };
            let replies: Vec<Reply> = lines.iter().map(|l| self.profile.classify(l)).collect();
            if let Some(Reply::Error(m)) = replies.iter().find(|r| matches!(r, Reply::Error(_))) {
                let m = m.clone();
                self.note(line, &format!("error: {m}"));
                return Err(self.fail(
                    Kind::Rejected,
                    format!("'{line}' was refused: {m}"),
                    Event::ErrorReply,
                ));
            }
            if !replies.contains(&Reply::Ok) {
                let busy = replies.contains(&Reply::Busy);
                self.note(line, if busy { "busy" } else { "no ok" });
                self.governor.observe(Event::Timeout);
                if again && tries < self.profile.retries {
                    tries += 1;
                    self.link.pause(self.profile.retry_wait_s);
                    self.governor.observe(Event::Retry);
                    continue;
                }
                return Err(self.fail(
                    Kind::Timeout,
                    format!("'{line}' got no ok after {} tries", tries + 1),
                    Event::Timeout,
                ));
            }
            if !self.profile.tolerate_unknown_echo {
                if let Some(Reply::Other(t)) = replies.iter().find(|r| matches!(r, Reply::Other(_)))
                {
                    let t = t.clone();
                    self.note(line, &format!("unreadable: {t}"));
                    return Err(self.fail(
                        Kind::Link,
                        format!("'{line}' got a line that is not known: {t}"),
                        Event::ErrorReply,
                    ));
                }
            }
            self.note(line, "ok");
            self.governor.observe(Event::Success);
            return Ok(replies);
        }
    }

    /// Opens the session: the lines of `connect`, then the absolute mode.
    pub fn connect(&mut self) -> Result<(), Failure> {
        if self.state == State::Halted {
            return Err(Failure::new(Kind::Halted, "the session is stopped"));
        }
        let t = self.profile.connect_timeout_s;
        for l in self.profile.connect.clone() {
            self.send(&l, t, true)?;
        }
        let abs = self.profile.absolute_mode.clone();
        self.send(&abs, t, true)?;
        self.state = State::Ready;
        Ok(())
    }

    fn ready(&self) -> Result<(), Failure> {
        match self.state {
            State::Ready => Ok(()),
            State::Halted => Err(Failure::new(Kind::Halted, "the session is stopped")),
            State::New => Err(Failure::new(Kind::Refused, "the session is not connected")),
        }
    }

    /// Homes the machine.
    pub fn home(&mut self) -> Result<(), Failure> {
        self.ready()?;
        let t = self.profile.move_timeout_s;
        for l in self.profile.home.clone() {
            self.send(&l, t, false)?;
        }
        Ok(())
    }

    /// Asks where the head is.
    pub fn position(&mut self) -> Result<Pos, Failure> {
        self.ready()?;
        let line = self.profile.position_request.clone();
        let t = self.profile.command_timeout_s;
        let replies = self.send(&line, t, true)?;
        replies
            .into_iter()
            .find_map(|r| match r {
                Reply::Position(p) => Some(p),
                _ => None,
            })
            .ok_or_else(|| {
                let f = Failure::new(
                    Kind::Link,
                    "the answer to the question about the position has none",
                );
                self.halt(&f.detail);
                f
            })
    }

    fn at_target(&self, p: &Pos, t: &Target) -> bool {
        let tol = self.profile.position_tol_mm;
        t.x.is_none_or(|v| near(p.x, v, tol))
            && t.y.is_none_or(|v| near(p.y, v, tol))
            && t.z.is_none_or(|v| near(p.z, v, tol))
            && match (t.rot, p.rot) {
                (Some(want), Some(got)) => {
                    let d = (want - got).rem_euclid(360.0);
                    d.min(360.0 - d) <= tol.max(0.01)
                }
                _ => true,
            }
    }

    /// Moves the head. The feed is cut to the share the rules allow, the move is refused when they hold the machine, and the head is asked for its place afterward (`verify_moves`).
    pub fn move_to(&mut self, target: &Target, feed: f64) -> Result<(), Failure> {
        self.ready()?;
        if let Err(violation) = interlock::check(&self.profile.interlocks, &self.signals, target) {
            let why = violation.localized(Lang::En);
            return Err(self.fail(Kind::Interlock, why, Event::LimitRefused));
        }
        let limits = self.governor.limits();
        if !limits.moves {
            return Err(Failure::new(
                Kind::Held,
                format!("the machine is held ({})", self.governor.level().name()),
            ));
        }
        let used = (feed * limits.speed_pct / 100.0).max(1.0);
        let line = match self.profile.move_line(target, used) {
            Ok(l) => l,
            Err(e) => {
                return Err(self.fail(Kind::Refused, e, Event::LimitRefused));
            }
        };
        let tc = self.profile.command_timeout_s;
        self.send(&line, tc, false)?;
        let wait = self.profile.wait.clone();
        let tm = self.profile.move_timeout_s;
        let mut tries = 0u32;
        loop {
            match self.send_wait(&wait, tm) {
                Ok(()) => break,
                Err(Waited::Timeout) => {
                    // the move may be done and the answer lost: ask where the head is before anything else
                    let pos = self.position_quiet()?;
                    if self.at_target(&pos, target) {
                        self.governor.observe(Event::Retry);
                        self.note("(recovered)", "the head is at the target");
                        break;
                    }
                    if tries >= self.profile.retries {
                        return Err(self.fail(
                            Kind::Timeout,
                            "the move did not end in time and the head is not at the target"
                                .to_string(),
                            Event::Timeout,
                        ));
                    }
                    tries += 1;
                    self.governor.observe(Event::Retry);
                    self.link.pause(self.profile.retry_wait_s);
                }
                Err(Waited::Failed(f)) => return Err(f),
            }
        }
        if self.verify_moves {
            let pos = self.position()?;
            if !self.at_target(&pos, target) {
                return Err(self.fail(
                    Kind::Mismatch,
                    format!(
                        "the head reports ({:.3}, {:.3}, {:.3}) and not the target",
                        pos.x, pos.y, pos.z
                    ),
                    Event::PositionMismatch,
                ));
            }
        }
        Ok(())
    }

    /// The line that waits for a move: one try. A timeout is given back to the caller to settle, and is not yet a failure.
    fn send_wait(&mut self, line: &str, timeout_s: f64) -> Result<(), Waited> {
        if self.state == State::Halted {
            return Err(Waited::Failed(Failure::new(
                Kind::Halted,
                "the session is stopped",
            )));
        }
        match self.link.exchange(line, timeout_s) {
            Err(LinkError::Timeout) => {
                self.note(line, "timeout");
                Err(Waited::Timeout)
            }
            Err(LinkError::Closed(why)) => {
                self.note(line, "link closed");
                Err(Waited::Failed(self.fail(
                    Kind::Link,
                    format!("the link is closed: {why}"),
                    Event::Timeout,
                )))
            }
            Ok(lines) => {
                let replies: Vec<Reply> = lines.iter().map(|l| self.profile.classify(l)).collect();
                if let Some(Reply::Error(m)) = replies.iter().find(|r| matches!(r, Reply::Error(_)))
                {
                    let m = m.clone();
                    self.note(line, &format!("error: {m}"));
                    return Err(Waited::Failed(self.fail(
                        Kind::Rejected,
                        format!("'{line}' was refused: {m}"),
                        Event::ErrorReply,
                    )));
                }
                self.note(line, "ok");
                self.governor.observe(Event::Success);
                Ok(())
            }
        }
    }

    /// The question about the position that does not stop the session at its first failure (the caller decides).
    fn position_quiet(&mut self) -> Result<Pos, Failure> {
        self.position()
    }

    /// Switches an actuator and waits for the controller to say it is done.
    pub fn set_actuator(&mut self, name: &str, on: bool) -> Result<(), Failure> {
        self.ready()?;
        let line = match self.profile.actuator_line(name, on) {
            Ok(l) => l,
            Err(e) => return Err(self.fail(Kind::Refused, e, Event::LimitRefused)),
        };
        let t = self.profile.command_timeout_s;
        self.send(&line, t, true)?;
        let wait = self.profile.wait.clone();
        self.send(&wait, t, true)?;
        self.signals.insert(name.to_string(), on);
        Ok(())
    }

    /// Does one step.
    pub fn run_op(&mut self, op: &Op) -> Result<(), Failure> {
        match op {
            Op::Move(t, feed) => self.move_to(t, *feed),
            Op::Actuator(n, on) => self.set_actuator(n, *on),
            Op::Dwell(ms) => {
                self.ready()?;
                let line = self.profile.dwell_line(*ms);
                let t = self.profile.command_timeout_s + f64::from(*ms) / 1000.0;
                self.send(&line, t, false).map(|_| ())
            }
            Op::Wait => {
                self.ready()?;
                let w = self.profile.wait.clone();
                let t = self.profile.move_timeout_s;
                self.send(&w, t, true).map(|_| ())
            }
            Op::Query => self.position().map(|_| ()),
        }
    }

    /// Places the parts one after the other. The first failure ends the run and says which part it was.
    pub fn run_placements(&mut self, placements: &[Placement]) -> RunReport {
        let mut placed = 0usize;
        for p in placements {
            let ops = match self.profile.placement_ops(p) {
                Ok(o) => o,
                Err(e) => {
                    let f = self.fail(Kind::Refused, e, Event::LimitRefused);
                    return RunReport {
                        placed,
                        failed: Some((p.reference.clone(), f)),
                    };
                }
            };
            for op in &ops {
                if let Err(f) = self.run_op(op) {
                    return RunReport {
                        placed,
                        failed: Some((p.reference.clone(), f)),
                    };
                }
            }
            placed += 1;
        }
        RunReport {
            placed,
            failed: None,
        }
    }

    /// Ends the session the proper way: the motors are released.
    pub fn close(&mut self) -> Result<(), Failure> {
        if self.state == State::Halted {
            return Ok(());
        }
        let t = self.profile.command_timeout_s;
        for l in self.profile.disable.clone() {
            self.send(&l, t, true)?;
        }
        self.state = State::New;
        Ok(())
    }
}
