// EH3D-PHOENIX-PNP - a simulated G-code controller: it speaks the dialect of Marlin or of Klipper so that a driver can be tried with no machine.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! `GcodeSim` takes a line of G-code and answers as a controller does (`ok`, the position of `M114` with its `E`, `echo:busy` lines, `Error:` lines, a controller that halts) and keeps a
//! clock: a move takes the time its distance and its feed need, `M400` answers when the moves are done, a reply that is due after the time a driver is willing to wait is lost to that
//! driver (a timeout) although the move goes on. Faults can be put in on purpose: an error at the Nth line, a reply that never comes, a halted controller, busy lines, a position that is not
//! the one asked for, a homing that lands a little off. `SimLink` is the serial line to it and `SimMoonraker` the HTTP server of Klipper in front of it. Everything here is evidence of a
//! simulator: no port is opened.

use crate::gcodedriver::{DriverProfile, Pos};
use std::collections::BTreeMap;

/// What a link to a controller can fail with.
#[derive(Debug, Clone, PartialEq)]
pub enum LinkError {
    /// The reply did not come in the time given.
    Timeout,
    /// The link is closed.
    Closed(String),
}

/// A line to a controller and what it answers: the lines of the answer, or an error of the link.
pub trait Link {
    /// Sends a line and waits at most `timeout_s` seconds for the answer.
    fn exchange(&mut self, line: &str, timeout_s: f64) -> Result<Vec<String>, LinkError>;
    /// Lets `seconds` go by (a pause between two tries).
    fn pause(&mut self, seconds: f64);
    /// The seconds on the clock of the link.
    fn now(&self) -> f64;
}

/// The faults that can be put into the simulated controller; all off by default.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Faults {
    /// The Nth line (from 1) gets an error and no `ok`.
    pub error_at: Option<(usize, String)>,
    /// The Nth line gets no answer at all.
    pub silent_at: Option<usize>,
    /// From the Nth line on, the controller is halted and answers every line with an error.
    pub halt_at: Option<usize>,
    /// How many `busy` lines come before each `ok`.
    pub busy_lines: u32,
    /// The position that `M114` reports is this far from the true one (mm).
    pub report_offset: f64,
    /// The head lands this far (mm, at most, from a seed) from where the homing says.
    pub homing_jitter: f64,
    /// The moves take this many times as long.
    pub slow_factor: f64,
    /// Marlin answers an unknown command with an echo line and `ok`; this makes it an error as Klipper does.
    pub strict_unknown: bool,
}

/// The simulated controller.
#[derive(Debug, Clone)]
pub struct GcodeSim {
    klipper: bool,
    letters: [char; 4],
    limits: ([f64; 2], [f64; 2], [f64; 2]),
    pos: [f64; 4],
    /// Where the head really is when the homing is off (mm).
    pub true_offset: (f64, f64),
    feed_mm_min: f64,
    now_s: f64,
    motion_end_s: f64,
    pins: BTreeMap<i64, i64>,
    named_pins: BTreeMap<String, f64>,
    faults: Faults,
    received: usize,
    halted: bool,
    homed: bool,
    seed: u64,
    /// Every line received, in order.
    pub log: Vec<String>,
}

fn next_unit(seed: &mut u64) -> f64 {
    *seed = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *seed;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 11) as f64 / (1u64 << 53) as f64
}

impl GcodeSim {
    /// A controller that follows the axes, limits and dialect of a profile.
    pub fn new(profile: &DriverProfile, faults: Faults, seed: u64) -> GcodeSim {
        let letter = |s: &str, d: char| s.chars().next().unwrap_or(d);
        GcodeSim {
            klipper: profile.dialect == crate::gcodedriver::Dialect::Klipper,
            letters: [
                letter(&profile.axis_x, 'X'),
                letter(&profile.axis_y, 'Y'),
                letter(&profile.axis_z, 'Z'),
                letter(&profile.axis_rot, 'E'),
            ],
            limits: (
                [profile.limit_x_min, profile.limit_x],
                [profile.limit_y_min, profile.limit_y],
                [profile.limit_z_min, profile.limit_z_max],
            ),
            pos: [0.0; 4],
            true_offset: (0.0, 0.0),
            feed_mm_min: 1000.0,
            now_s: 0.0,
            motion_end_s: 0.0,
            pins: BTreeMap::new(),
            named_pins: BTreeMap::new(),
            faults,
            received: 0,
            halted: false,
            homed: false,
            seed,
            log: Vec::new(),
        }
    }

    /// The value of a pin that `M42` set.
    pub fn pin(&self, pin: i64) -> Option<i64> {
        self.pins.get(&pin).copied()
    }

    /// The value that `SET_PIN` gave a pin of Klipper by its name.
    pub fn named_pin(&self, name: &str) -> Option<f64> {
        self.named_pins.get(name).copied()
    }

    /// Whether the controller has been halted by a fault.
    pub fn is_halted(&self) -> bool {
        self.halted
    }

    /// The position the controller holds, with the turn.
    pub fn position(&self) -> Pos {
        Pos {
            x: self.pos[0],
            y: self.pos[1],
            z: self.pos[2],
            rot: Some(self.pos[3]),
        }
    }

    /// The position the head really is at: the one the controller holds and the error of the homing.
    pub fn true_position(&self) -> (f64, f64) {
        (
            self.pos[0] + self.true_offset.0,
            self.pos[1] + self.true_offset.1,
        )
    }

    /// The seconds on the clock.
    pub fn now(&self) -> f64 {
        self.now_s
    }

    /// Lets time go by.
    pub fn advance(&mut self, seconds: f64) {
        if seconds.is_finite() && seconds > 0.0 {
            self.now_s += seconds;
        }
    }

    fn words(line: &str) -> Vec<(char, f64)> {
        line.split_whitespace()
            .skip(1)
            .filter_map(|w| {
                let mut cs = w.chars();
                let c = cs.next()?.to_ascii_uppercase();
                let v: f64 = cs.as_str().parse().ok()?;
                v.is_finite().then_some((c, v))
            })
            .collect()
    }

    fn halted_line(&self) -> String {
        if self.klipper {
            "!! Printer is shutdown".to_string()
        } else {
            "Error:Printer halted. kill() called!".to_string()
        }
    }

    fn done(&self, mut lines: Vec<String>) -> Vec<String> {
        let mut out: Vec<String> = (0..self.faults.busy_lines)
            .map(|_| "echo:busy: processing".to_string())
            .collect();
        out.append(&mut lines);
        out.push("ok".to_string());
        out
    }

    /// One line in, the lines of the answer and how many seconds after now the answer is complete; `None` when no answer ever comes.
    pub fn handle(&mut self, line: &str) -> Option<(Vec<String>, f64)> {
        self.received += 1;
        self.log.push(line.trim().to_string());
        let n = self.received;
        if self.faults.halt_at == Some(n) {
            self.halted = true;
        }
        if self.faults.silent_at == Some(n) {
            // the line is taken and carried out, only the answer is lost
            self.run(line);
            return None;
        }
        if self.halted {
            return Some((vec![self.halted_line()], 0.0));
        }
        if let Some((at, text)) = &self.faults.error_at {
            if *at == n {
                let msg = if self.klipper {
                    format!("!! {text}")
                } else {
                    format!("Error:{text}")
                };
                return Some((vec![msg], 0.0));
            }
        }
        let (lines, delay) = self.run(line);
        Some((lines, delay))
    }

    fn run(&mut self, line: &str) -> (Vec<String>, f64) {
        let text = line.split(';').next().unwrap_or("").trim();
        let cmd = text
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_ascii_uppercase();
        let w = Self::words(text);
        let slow = if self.faults.slow_factor > 0.0 {
            self.faults.slow_factor
        } else {
            1.0
        };
        match cmd.as_str() {
            "" => (self.done(vec![]), 0.0),
            "G90" => (self.done(vec![]), 0.0),
            "G91" | "G92" => (
                vec![self.error_text("relative and set-position modes are not simulated")],
                0.0,
            ),
            "G28" => {
                let (jx, jy) = (self.faults.homing_jitter, self.faults.homing_jitter);
                self.true_offset = (
                    (next_unit(&mut self.seed) * 2.0 - 1.0) * jx,
                    (next_unit(&mut self.seed) * 2.0 - 1.0) * jy,
                );
                let from = self.pos;
                self.pos = [0.0, 0.0, self.limits.2[1], 0.0];
                let dist = ((from[0]).powi(2) + (from[1]).powi(2)).sqrt() + 20.0;
                let t = (dist / 100.0).max(1.0) * slow;
                self.motion_end_s = self.motion_end_s.max(self.now_s) + t;
                self.homed = true;
                (self.done(vec![]), self.motion_end_s - self.now_s)
            }
            "G0" | "G1" => {
                if !self.homed {
                    return (vec![self.error_text("must home axis first")], 0.0);
                }
                let mut target = self.pos;
                for (c, v) in &w {
                    if *c == 'F' {
                        if *v > 0.0 {
                            self.feed_mm_min = *v;
                        }
                        continue;
                    }
                    if let Some(i) = self.letters.iter().position(|l| l == c) {
                        target[i] = *v;
                    }
                }
                let ok_x = (self.limits.0[0]..=self.limits.0[1]).contains(&target[0]);
                let ok_y = (self.limits.1[0]..=self.limits.1[1]).contains(&target[1]);
                let ok_z = (self.limits.2[0]..=self.limits.2[1]).contains(&target[2]);
                if !(ok_x && ok_y && ok_z) {
                    return (vec![self.error_text("move out of range")], 0.0);
                }
                let d = ((target[0] - self.pos[0]).powi(2)
                    + (target[1] - self.pos[1]).powi(2)
                    + (target[2] - self.pos[2]).powi(2))
                .sqrt();
                let t = d / (self.feed_mm_min / 60.0) * slow;
                self.motion_end_s = self.motion_end_s.max(self.now_s) + t;
                self.pos = target;
                (self.done(vec![]), 0.0)
            }
            "G4" => {
                let ms = w.iter().find(|(c, _)| *c == 'P').map_or(0.0, |(_, v)| *v);
                self.motion_end_s = self.motion_end_s.max(self.now_s) + ms.max(0.0) / 1000.0;
                (self.done(vec![]), 0.0)
            }
            "M400" => (self.done(vec![]), (self.motion_end_s - self.now_s).max(0.0)),
            "M114" => {
                let o = self.faults.report_offset;
                let p = self.pos;
                let first = format!(
                    "X:{:.2} Y:{:.2} Z:{:.2} E:{:.2} Count X:{} Y:{} Z:{}",
                    p[0] + o,
                    p[1] + o,
                    p[2],
                    p[3],
                    (p[0] * 80.0) as i64,
                    (p[1] * 80.0) as i64,
                    (p[2] * 400.0) as i64
                );
                (self.done(vec![first]), 0.0)
            }
            "M115" => (
                self.done(vec![if self.klipper {
                    "FIRMWARE_NAME:Klipper (simulated) PROTOCOL_VERSION:1.0".to_string()
                } else {
                    "FIRMWARE_NAME:Marlin 2.1 (simulated) PROTOCOL_VERSION:1.0".to_string()
                }]),
                0.0,
            ),
            "M84" => {
                self.homed = false;
                (self.done(vec![]), 0.0)
            }
            "M42" => {
                let pin = w.iter().find(|(c, _)| *c == 'P').map(|(_, v)| *v as i64);
                let val = w.iter().find(|(c, _)| *c == 'S').map(|(_, v)| *v as i64);
                match (pin, val) {
                    (Some(p), Some(v)) if (0..=255).contains(&v) => {
                        self.pins.insert(p, v);
                        (self.done(vec![]), 0.0)
                    }
                    _ => (
                        vec![self.error_text("M42 needs P and S from 0 to 255")],
                        0.0,
                    ),
                }
            }
            "SET_PIN" => {
                let value_of = |key: &str| {
                    text.split_whitespace()
                        .find_map(|w| w.strip_prefix(key))
                        .map(str::to_string)
                };
                match (
                    value_of("PIN="),
                    value_of("VALUE=").and_then(|v| v.parse::<f64>().ok()),
                ) {
                    (Some(pin), Some(v)) if v.is_finite() && (0.0..=255.0).contains(&v) => {
                        self.named_pins.insert(pin, v);
                        (self.done(vec![]), 0.0)
                    }
                    _ => (vec![self.error_text("SET_PIN needs PIN= and VALUE=")], 0.0),
                }
            }
            "M106" | "M107" => {
                let fan = w
                    .iter()
                    .find(|(c, _)| *c == 'P')
                    .map_or(0, |(_, v)| *v as i64);
                let val = if cmd == "M107" {
                    0
                } else {
                    w.iter()
                        .find(|(c, _)| *c == 'S')
                        .map_or(255, |(_, v)| *v as i64)
                };
                self.pins.insert(1000 + fan, val.clamp(0, 255));
                (self.done(vec![]), 0.0)
            }
            _ => {
                if self.faults.strict_unknown || self.klipper {
                    (
                        vec![self.error_text(&format!("Unknown command:\"{cmd}\""))],
                        0.0,
                    )
                } else {
                    (
                        self.done(vec![format!("echo:Unknown command: \"{cmd}\"")]),
                        0.0,
                    )
                }
            }
        }
    }

    fn error_text(&self, text: &str) -> String {
        if self.klipper {
            format!("!! {text}")
        } else {
            format!("Error:{text}")
        }
    }
}

/// A serial line to a simulated controller.
#[derive(Debug, Clone)]
pub struct SimLink {
    pub sim: GcodeSim,
}

impl SimLink {
    pub fn new(sim: GcodeSim) -> SimLink {
        SimLink { sim }
    }
}

impl Link for SimLink {
    fn exchange(&mut self, line: &str, timeout_s: f64) -> Result<Vec<String>, LinkError> {
        match self.sim.handle(line) {
            None => {
                self.sim.advance(timeout_s);
                Err(LinkError::Timeout)
            }
            Some((lines, delay)) => {
                if delay > timeout_s {
                    self.sim.advance(timeout_s);
                    Err(LinkError::Timeout)
                } else {
                    self.sim.advance(delay);
                    Ok(lines)
                }
            }
        }
    }

    fn pause(&mut self, seconds: f64) {
        self.sim.advance(seconds);
    }

    fn now(&self) -> f64 {
        self.sim.now()
    }
}

/// What a link to the HTTP server of Klipper does.
pub trait Http {
    /// A `POST` with a JSON body: the status and the body of the answer.
    fn post(&mut self, path: &str, body: &str, timeout_s: f64) -> Result<(u16, String), LinkError>;
    /// A `GET`: the status and the body of the answer.
    fn get(&mut self, path: &str, timeout_s: f64) -> Result<(u16, String), LinkError>;
    /// Lets time go by.
    fn pause(&mut self, seconds: f64);
    /// The seconds on the clock.
    fn now(&self) -> f64;
}

/// The Moonraker of a simulated Klipper: scripts run line by line, an error line makes a 400 with its message, the toolhead can be asked.
#[derive(Debug, Clone)]
pub struct SimMoonraker {
    pub sim: GcodeSim,
    script_path: String,
    query_path: String,
}

impl SimMoonraker {
    pub fn new(profile: &DriverProfile, faults: Faults, seed: u64) -> SimMoonraker {
        SimMoonraker {
            sim: GcodeSim::new(profile, faults, seed),
            script_path: profile.moonraker_script_path.clone(),
            query_path: profile.moonraker_query_path.clone(),
        }
    }
}

impl Http for SimMoonraker {
    fn post(&mut self, path: &str, body: &str, timeout_s: f64) -> Result<(u16, String), LinkError> {
        if path != self.script_path {
            return Ok((
                404,
                "{\"error\":{\"code\":404,\"message\":\"Not Found\"}}".into(),
            ));
        }
        let v: serde_json::Value = match serde_json::from_str(body) {
            Ok(v) => v,
            Err(_) => {
                return Ok((
                    400,
                    "{\"error\":{\"code\":400,\"message\":\"bad body\"}}".into(),
                ))
            }
        };
        let script = v
            .get("script")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let mut total = 0.0f64;
        for line in script.lines() {
            match self.sim.handle(line) {
                None => {
                    self.sim.advance(timeout_s);
                    return Err(LinkError::Timeout);
                }
                Some((lines, delay)) => {
                    total += delay;
                    if let Some(bad) = lines
                        .iter()
                        .find(|l| l.starts_with("!!") || l.starts_with("Error"))
                    {
                        let message = bad.trim_start_matches("!!").trim().to_string();
                        let e = serde_json::json!({"error": {"code": 400, "message": message}});
                        return Ok((400, e.to_string()));
                    }
                }
            }
        }
        if total > timeout_s {
            self.sim.advance(timeout_s);
            return Err(LinkError::Timeout);
        }
        self.sim.advance(total);
        Ok((200, "{\"result\":\"ok\"}".into()))
    }

    fn get(&mut self, path: &str, _timeout_s: f64) -> Result<(u16, String), LinkError> {
        if path != self.query_path {
            return Ok((
                404,
                "{\"error\":{\"code\":404,\"message\":\"Not Found\"}}".into(),
            ));
        }
        let p = self.sim.position();
        let o = self.sim.faults.report_offset;
        let body = serde_json::json!({"result": {"status": {"toolhead": {"position": [p.x + o, p.y + o, p.z, p.rot.unwrap_or(0.0)]}}}});
        Ok((200, body.to_string()))
    }

    fn pause(&mut self, seconds: f64) {
        self.sim.advance(seconds);
    }

    fn now(&self) -> f64 {
        self.sim.now()
    }
}

/// A link that talks to Moonraker: the lines become scripts, the answers become the lines of the dialect the patterns of the profile know (`ok`, `!! message`, a position line).
pub struct MoonrakerLink<H: Http> {
    profile: DriverProfile,
    pub http: H,
}

impl<H: Http> MoonrakerLink<H> {
    pub fn new(profile: &DriverProfile, http: H) -> MoonrakerLink<H> {
        MoonrakerLink {
            profile: profile.clone(),
            http,
        }
    }
}

impl<H: Http> Link for MoonrakerLink<H> {
    fn exchange(&mut self, line: &str, timeout_s: f64) -> Result<Vec<String>, LinkError> {
        if line.trim() == self.profile.position_request {
            let (status, body) = self.http.get(&self.profile.query_request(), timeout_s)?;
            if status != 200 {
                return Ok(vec![format!("!! HTTP {status}")]);
            }
            return Ok(match self.profile.read_position_answer(&body) {
                Ok(p) => vec![
                    format!(
                        "X:{:.3} Y:{:.3} Z:{:.3} E:{:.3}",
                        p.x,
                        p.y,
                        p.z,
                        p.rot.unwrap_or(0.0)
                    ),
                    "ok".to_string(),
                ],
                Err(e) => vec![format!("!! {e}")],
            });
        }
        let (path, body) = self.profile.script_request(&[line.to_string()]);
        let (status, answer) = self.http.post(&path, &body, timeout_s)?;
        Ok(match self.profile.read_script_answer(status, &answer) {
            crate::gcodedriver::Reply::Ok => vec!["ok".to_string()],
            crate::gcodedriver::Reply::Error(m) => vec![format!("!! {m}")],
            _ => vec!["!! unreadable answer".to_string()],
        })
    }

    fn pause(&mut self, seconds: f64) {
        self.http.pause(seconds);
    }

    fn now(&self) -> f64 {
        self.http.now()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sim() -> GcodeSim {
        GcodeSim::new(&DriverProfile::default(), Faults::default(), 1)
    }

    #[test]
    fn a_move_before_the_homing_is_refused_as_the_controller_does() {
        let mut s = sim();
        let (lines, _) = s.handle("G1 X10 F6000").unwrap();
        assert!(lines[0].starts_with("Error:"));
    }

    #[test]
    fn m114_reports_the_turn_in_the_e_field() {
        let mut s = sim();
        s.handle("G28");
        s.handle("G1 X10 Y20 Z5 E90 F6000");
        let (lines, _) = s.handle("M114").unwrap();
        assert!(lines[0].starts_with("X:10.00 Y:20.00 Z:5.00 E:90.00 Count"));
        assert_eq!(lines.last().map(String::as_str), Some("ok"));
    }
}
