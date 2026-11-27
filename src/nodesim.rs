// EH3D-PHOENIX-PNP - a head node and a feeder node on a computer.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! Two nodes that speak the contract of the head node and of the feeder node (their `CONTRACT.md`) without any hardware: the same paths, the same keys, the same refusals (`401` without the
//! token, `409` with a reason, `503` for a feeder that does not answer). They are what the link of this program is tried against. They are made up: the vacuum, the tape and the heights are
//! numbers of the simulator, and nothing here has been compared with a real node. Faults come from `Injection`: a list of faults for the next requests, or a rate for each kind drawn from a seed,
//! so a run with a seed is the same every time. A node can be given to the common client as a `Transport`, or used straight as a `NodeLink`.

use crate::nodelink::{answer, LinkError, NodeLink};
use eh3d_serverclient::errors::TransportError;
use eh3d_serverclient::transport::{Reply, Request, Transport};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// A fault put into one request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Inject {
    /// The node does not answer.
    Silence,
    /// The node answers `401`.
    Unauthorized,
    /// The node refuses an order with `409` (the interlock of the head: a nozzle is down).
    Interlock,
}

/// Where the faults come from: a script that is used up first, then a rate for each kind (per thousand requests) drawn from a seed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Injection {
    seed: u64,
    silence: u32,
    unauthorized: u32,
    interlock: u32,
    script: VecDeque<Option<Inject>>,
}

impl Injection {
    pub fn none() -> Injection {
        Injection {
            seed: 0,
            silence: 0,
            unauthorized: 0,
            interlock: 0,
            script: VecDeque::new(),
        }
    }

    /// Faults at the given rates (per thousand requests, each at most 1000 together) drawn from `seed`.
    pub fn seeded(seed: u64, silence: u32, unauthorized: u32, interlock: u32) -> Injection {
        Injection {
            seed: seed | 1,
            silence: silence.min(1000),
            unauthorized: unauthorized.min(1000),
            interlock: interlock.min(1000),
            script: VecDeque::new(),
        }
    }

    /// One entry for each of the next requests: a fault, or `None` for a request that goes through.
    pub fn scripted(script: Vec<Option<Inject>>) -> Injection {
        Injection {
            script: script.into(),
            ..Injection::none()
        }
    }

    fn next_u64(&mut self) -> u64 {
        // xorshift64*: the same numbers for the same seed on every system
        let mut x = self.seed;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.seed = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn draw(&mut self) -> Option<Inject> {
        if let Some(step) = self.script.pop_front() {
            return step;
        }
        if self.seed == 0 {
            return None;
        }
        let roll = (self.next_u64() >> 11) % 1000;
        let (s, u, i) = (
            u64::from(self.silence),
            u64::from(self.unauthorized),
            u64::from(self.interlock),
        );
        if roll < s {
            Some(Inject::Silence)
        } else if roll < s + u {
            Some(Inject::Unauthorized)
        } else if roll < s + u + i {
            Some(Inject::Interlock)
        } else {
            None
        }
    }
}

/// The parts of a request that matter: the path, its query as pairs, and the token if one came (as a bearer or as `?token=`).
struct Call {
    method: String,
    path: String,
    query: Vec<(String, String)>,
    token: Option<String>,
}

impl Call {
    fn new(method: &str, target: &str, bearer: Option<&str>) -> Call {
        let (path, q) = target.split_once('?').unwrap_or((target, ""));
        let query: Vec<(String, String)> = q
            .split('&')
            .filter(|p| !p.is_empty())
            .map(|p| {
                let (k, v) = p.split_once('=').unwrap_or((p, ""));
                (k.to_string(), v.to_string())
            })
            .collect();
        let token = bearer.map(str::to_string).or_else(|| {
            query
                .iter()
                .find(|(k, _)| k == "token")
                .map(|(_, v)| v.clone())
        });
        Call {
            method: method.to_string(),
            path: path.to_string(),
            query,
            token,
        }
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.query
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    fn number(&self, key: &str) -> Option<f64> {
        self.get(key)?.parse::<f64>().ok().filter(|v| v.is_finite())
    }
}

type Out = Result<(u16, String), TransportError>;

fn json_ok(body: String) -> Out {
    Ok((200, body))
}

fn refuse(status: u16, why: &str) -> Out {
    Ok((status, format!("{{\"ok\":false,\"why\":\"{why}\"}}")))
}

fn error(status: u16, code: &str) -> Out {
    Ok((status, format!("{{\"error\":\"{code}\"}}")))
}

fn bad_word(key: &str) -> Out {
    Ok((
        400,
        format!("{{\"error\":\"bad_request\",\"key\":\"{key}\"}}"),
    ))
}

/// Applies the fault of the injection that suits the request; `Err` is silence.
fn injected(inject: Option<Inject>, call: &Call, token_ok: bool) -> Option<Out> {
    match inject? {
        Inject::Silence => Some(Err(TransportError::Timeout)),
        Inject::Unauthorized if token_needed(call) || !token_ok => Some(error(401, "unauthorized")),
        Inject::Interlock if call.method == "POST" => Some(refuse(409, "interlock_xy_nozzle_low")),
        _ => None,
    }
}

fn token_needed(call: &Call) -> bool {
    !(call.method == "GET" && call.path == "/api/status")
}

// ---------------------------------------------------------------------------------------------------------------------------------- the head

#[derive(Debug, Clone)]
struct Nozzle {
    z: f64,
    holding: bool,
    no_part: bool,
    picks: u32,
    failed: u32,
}

/// The watch of a node on a computer: it goes to its safe state when no question with the token arrived for `limit` milliseconds. The clock only moves when it is told to, so a run is the
/// same every time; like the node, it starts to watch with the first question that carries the token.
#[derive(Debug, Clone, Copy)]
struct Watch {
    limit: u32,
    since: u32,
    watching: bool,
}

impl Watch {
    fn new(limit: u32) -> Watch {
        Watch {
            limit,
            since: 0,
            watching: false,
        }
    }

    /// The clock moves; true when the node has now waited too long.
    fn elapse(&mut self, ms: u32) -> bool {
        if !self.watching {
            return false;
        }
        self.since = self.since.saturating_add(ms);
        self.since > self.limit
    }

    fn heard(&mut self) {
        self.watching = true;
        self.since = 0;
    }
}

/// The watch of the head and of the feeder node on a computer, as their settings start.
pub const SIM_LINK_LOST_MS: u32 = 2000;
pub const SIM_SERVER_LOST_MS: u32 = 5000;

#[derive(Debug)]
struct HeadState {
    token: String,
    nozzles: Vec<Nozzle>,
    z_safe: f64,
    tolerance: f64,
    xy_moving: bool,
    safe: bool,
    requests: u64,
    inject: Injection,
    watch: Watch,
}

impl HeadState {
    fn raised(&self, n: usize) -> bool {
        self.nozzles[n].z >= self.z_safe - self.tolerance
    }

    fn blocking(&self) -> Vec<usize> {
        (0..self.nozzles.len())
            .filter(|&n| !self.raised(n))
            .collect()
    }

    fn nozzle_json(&self, n: usize) -> String {
        let z = &self.nozzles[n];
        let phase = if z.holding { "holding" } else { "idle" };
        format!(
            "{{\"n\":{},\"phase\":\"{phase}\",\"fault\":\"none\",\"part\":{},\"vacuum_ok\":{},\"raised\":{},\"z_mm\":{:.3},\"picks\":{},\"failed_picks\":{}}}",
            n + 1,
            z.holding,
            z.holding,
            self.raised(n),
            z.z,
            z.picks,
            z.failed
        )
    }

    fn status(&self) -> String {
        let blocking = self.blocking();
        let clear = blocking.is_empty();
        let list: Vec<String> = blocking.iter().map(|n| (n + 1).to_string()).collect();
        let nozzles: Vec<String> = (0..self.nozzles.len())
            .map(|n| self.nozzle_json(n))
            .collect();
        let (check, codes) = if self.safe {
            ("fail", "HDN-S02")
        } else if self.token.is_empty() {
            ("warn", "HDN-S01")
        } else {
            ("ok", "")
        };
        format!(
            "{{\"node\":\"head-sim\",\"version\":\"sim\",\"nozzle_count\":{},\"safe\":{},\"safe_reason\":\"{}\",\"safe_code\":\"{}\",\"pump\":{},\"xy_clear\":{},\"xy_moving\":{},\"xy_blocking\":\"{}\",\"link_lost_ms\":{},\"z_safe_mm\":{:.3},\"selfcheck\":\"{check}\",\"selfcheck_codes\":\"{codes}\",\"nozzles\":[{}]}}",
            self.nozzles.len(),
            self.safe,
            if self.safe { "server_lost" } else { "none" },
            if self.safe { "HEAD-L01" } else { "none" },
            self.nozzles.iter().any(|z| z.holding),
            clear && !self.safe,
            self.xy_moving,
            list.join(","),
            self.watch.limit,
            self.z_safe,
            nozzles.join(",")
        )
    }

    fn nozzle_arg(&self, call: &Call) -> Result<usize, Out> {
        match call.number("n") {
            Some(v) if v.fract() == 0.0 && v >= 1.0 && (v as usize) <= self.nozzles.len() => {
                Ok(v as usize - 1)
            }
            Some(_) => Err(refuse(400, "bad_nozzle")),
            None => Err(bad_word("n")),
        }
    }

    fn route(&mut self, call: &Call) -> Out {
        match (call.method.as_str(), call.path.as_str()) {
            ("GET", "/api/status") => json_ok(self.status()),
            ("GET", "/api/nozzle") => match self.nozzle_arg(call) {
                Ok(n) => json_ok(self.nozzle_json(n)),
                Err(e) => e,
            },
            ("POST", "/api/ack") => {
                self.safe = false;
                json_ok("{\"ok\":true,\"why\":\"cleared\"}".into())
            }
            ("POST", "/api/stop") => json_ok("{\"ok\":true,\"why\":\"ok\"}".into()),
            ("POST", "/api/xy") => match call.get("moving") {
                Some("1") => {
                    if self.safe {
                        return refuse(409, "safe_state");
                    }
                    if !self.blocking().is_empty() {
                        return refuse(409, "interlock_xy_nozzle_low");
                    }
                    self.xy_moving = true;
                    json_ok("{\"ok\":true,\"why\":\"ok\"}".into())
                }
                Some("0") => {
                    self.xy_moving = false;
                    json_ok("{\"ok\":true,\"why\":\"ok\"}".into())
                }
                _ => bad_word("moving"),
            },
            ("POST", "/api/nozzle/pick") => {
                let n = match self.nozzle_arg(call) {
                    Ok(n) => n,
                    Err(e) => return e,
                };
                if self.safe {
                    return refuse(409, "safe_state");
                }
                if self.nozzles[n].holding {
                    return refuse(409, "busy");
                }
                if self.nozzles[n].no_part {
                    self.nozzles[n].failed += 1;
                    return refuse(409, "vacuum_not_reached");
                }
                self.nozzles[n].holding = true;
                self.nozzles[n].picks += 1;
                json_ok(format!("{{\"ok\":true,\"why\":\"held\",\"n\":{}}}", n + 1))
            }
            ("POST", "/api/nozzle/place") => {
                let n = match self.nozzle_arg(call) {
                    Ok(n) => n,
                    Err(e) => return e,
                };
                if !self.nozzles[n].holding {
                    return refuse(409, "not_holding");
                }
                self.nozzles[n].holding = false;
                json_ok(format!(
                    "{{\"ok\":true,\"why\":\"placed\",\"n\":{}}}",
                    n + 1
                ))
            }
            ("POST", "/api/z") => {
                let n = match self.nozzle_arg(call) {
                    Ok(n) => n,
                    Err(e) => return e,
                };
                let Some(mm) = call.number("mm") else {
                    return bad_word("mm");
                };
                if !(-50.0..=50.0).contains(&mm) {
                    return refuse(400, "out_of_range");
                }
                if self.safe && mm < self.nozzles[n].z {
                    return refuse(409, "safe_state");
                }
                if self.xy_moving && mm < self.z_safe - self.tolerance {
                    return refuse(409, "interlock_xy_moving");
                }
                self.nozzles[n].z = mm;
                json_ok(format!("{{\"ok\":true,\"why\":\"ok\",\"n\":{}}}", n + 1))
            }
            _ => error(404, "not_found"),
        }
    }
}

/// A head on a computer.
#[derive(Clone)]
pub struct SimHead(Arc<Mutex<HeadState>>);

impl SimHead {
    /// A head with `nozzles` nozzles, all raised, that answers to `token` (an empty token refuses every order).
    pub fn new(nozzles: usize, token: &str) -> SimHead {
        let nozzles = nozzles.clamp(1, 6);
        SimHead(Arc::new(Mutex::new(HeadState {
            token: token.to_string(),
            nozzles: vec![
                Nozzle {
                    z: 5.0,
                    holding: false,
                    no_part: false,
                    picks: 0,
                    failed: 0,
                };
                nozzles
            ],
            z_safe: 5.0,
            tolerance: 0.5,
            xy_moving: false,
            safe: false,
            requests: 0,
            inject: Injection::none(),
            watch: Watch::new(SIM_LINK_LOST_MS),
        })))
    }

    /// Sets the watch of the head (`link_lost_ms`).
    pub fn set_link_lost_ms(&self, ms: u32) {
        self.state().watch.limit = ms;
    }

    /// Time passes: a head that has heard a question with the token and then none for longer than its watch is safe.
    pub fn elapse(&self, ms: u32) {
        let mut s = self.state();
        if s.watch.elapse(ms) {
            s.safe = true;
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, HeadState> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn set_injection(&self, inject: Injection) {
        self.state().inject = inject;
    }

    /// Sets the height of a nozzle (counted from 1), as if it had been moved.
    pub fn set_z(&self, n: usize, mm: f64) {
        if let Some(z) = self.state().nozzles.get_mut(n.wrapping_sub(1)) {
            z.z = mm;
        }
    }

    /// A nozzle that finds no part: its picks do not reach the vacuum.
    pub fn set_no_part(&self, n: usize, on: bool) {
        if let Some(z) = self.state().nozzles.get_mut(n.wrapping_sub(1)) {
            z.no_part = on;
        }
    }

    /// The head loses the server and goes to its safe state.
    pub fn set_safe(&self, on: bool) {
        self.state().safe = on;
    }

    pub fn xy_moving(&self) -> bool {
        self.state().xy_moving
    }

    pub fn requests(&self) -> u64 {
        self.state().requests
    }

    pub fn picks(&self, n: usize) -> u32 {
        self.state()
            .nozzles
            .get(n.wrapping_sub(1))
            .map_or(0, |z| z.picks)
    }

    /// Answers a request as the node does.
    pub fn handle(&self, method: &str, target: &str, bearer: Option<&str>) -> Out {
        let mut s = self.state();
        s.requests += 1;
        let call = Call::new(method, target, bearer);
        let token_ok = !s.token.is_empty() && call.token.as_deref() == Some(s.token.as_str());
        let inject = s.inject.draw();
        if let Some(out) = injected(inject, &call, token_ok) {
            return out;
        }
        if token_needed(&call) && !token_ok {
            return error(401, "unauthorized");
        }
        if token_ok {
            s.watch.heard();
        }
        s.route(&call)
    }
}

impl Transport for SimHead {
    fn send(&self, request: &Request) -> Result<Reply, TransportError> {
        self.handle(&request.method, &request.path, request.bearer())
            .map(|(status, body)| Reply::new(status, body))
    }
}

impl NodeLink for SimHead {
    fn call(&mut self, method: &str, path: &str, auth: bool) -> Result<String, LinkError> {
        let token = if auth {
            Some(self.state().token.clone())
        } else {
            None
        };
        match self.handle(method, path, token.as_deref()) {
            Ok((status, body)) => answer(status, body),
            Err(_) => Err(LinkError::Silent),
        }
    }
}

// ------------------------------------------------------------------------------------------------------------------------------ the feeders

#[derive(Debug, Clone)]
struct Unit {
    online: bool,
    remaining: Option<u32>,
    fault: Option<&'static str>,
    cause_gone: bool,
    advances: u32,
}

#[derive(Debug)]
struct ChainState {
    token: String,
    units: Vec<Unit>,
    safe: bool,
    requests: u64,
    inject: Injection,
    watch: Watch,
}

impl ChainState {
    fn state_of(&self, i: usize) -> &'static str {
        let u = &self.units[i];
        if !u.online {
            "unknown"
        } else if u.fault.is_some() {
            "fault"
        } else if u.remaining == Some(0) {
            "empty"
        } else {
            "idle"
        }
    }

    fn feeder_json(&self, i: usize) -> String {
        let u = &self.units[i];
        let remaining = u.remaining.map_or("null".to_string(), |r| r.to_string());
        format!(
            "{{\"slot\":{},\"online\":{},\"state\":\"{}\",\"fault\":\"{}\",\"remaining\":{remaining},\"low\":{},\"pitch_mm\":4,\"tape\":true,\"pocket\":true,\"tension_ok\":true,\"film_clear\":true}}",
            i + 1,
            u.online,
            self.state_of(i),
            u.fault.unwrap_or("none"),
            u.remaining.is_some_and(|r| r <= 5)
        )
    }

    fn status(&self) -> String {
        let feeders: Vec<String> = (0..self.units.len()).map(|i| self.feeder_json(i)).collect();
        let online = self.units.iter().filter(|u| u.online).count();
        let check = if self.safe || online == 0 {
            "fail"
        } else if online < self.units.len() || self.units.iter().any(|u| u.fault.is_some()) {
            "warn"
        } else {
            "ok"
        };
        format!(
            "{{\"node\":\"feeders-sim\",\"version\":\"sim\",\"slots\":{},\"online\":{online},\"safe\":{},\"safe_reason\":\"{}\",\"server_lost_ms\":{},\"bus\":\"rs485\",\"selfcheck\":\"{check}\",\"feeders\":[{}]}}",
            self.units.len(),
            self.safe,
            if self.safe { "server_lost" } else { "none" },
            self.watch.limit,
            feeders.join(",")
        )
    }

    fn slot_arg(&self, call: &Call) -> Result<usize, Out> {
        match call.number("slot") {
            Some(v) if v.fract() == 0.0 && v >= 1.0 && (v as usize) <= self.units.len() => {
                Ok(v as usize - 1)
            }
            Some(_) => Err(refuse(400, "bad_argument")),
            None => Err(bad_word("slot")),
        }
    }

    fn route(&mut self, call: &Call) -> Out {
        match (call.method.as_str(), call.path.as_str()) {
            ("GET", "/api/status") => json_ok(self.status()),
            ("GET", "/api/feeder") => match self.slot_arg(call) {
                Ok(i) => json_ok(self.feeder_json(i)),
                Err(e) => e,
            },
            ("POST", "/api/feeder/ack") => match call.get("slot") {
                None => {
                    self.safe = false;
                    json_ok("{\"ok\":true,\"why\":\"cleared\"}".into())
                }
                Some(_) => {
                    let i = match self.slot_arg(call) {
                        Ok(i) => i,
                        Err(e) => return e,
                    };
                    let u = &mut self.units[i];
                    if u.fault.is_some() && !u.cause_gone {
                        return refuse(409, "still_active");
                    }
                    u.fault = None;
                    json_ok("{\"ok\":true,\"why\":\"cleared\"}".into())
                }
            },
            ("POST", "/api/feeder/advance") => {
                let i = match self.slot_arg(call) {
                    Ok(i) => i,
                    Err(e) => return e,
                };
                if self.safe {
                    return error(409, "safe_state");
                }
                let count = match call.get("count") {
                    None => 1,
                    Some(_) => match call.number("count") {
                        Some(c) if c.fract() == 0.0 && (1.0..=8.0).contains(&c) => c as u32,
                        _ => return refuse(409, "bad_argument"),
                    },
                };
                let u = &mut self.units[i];
                if !u.online {
                    return refuse(503, "offline");
                }
                if u.fault.is_some() {
                    return refuse(409, "fault");
                }
                if u.remaining == Some(0) {
                    return refuse(409, "empty");
                }
                u.advances += count;
                if let Some(r) = u.remaining.as_mut() {
                    *r = r.saturating_sub(count);
                }
                json_ok(format!(
                    "{{\"ok\":true,\"why\":\"ok\",\"slot\":{},\"feeder\":{}}}",
                    i + 1,
                    self.feeder_json(i)
                ))
            }
            _ => error(404, "not_found"),
        }
    }
}

/// A chain of feeders on a computer.
#[derive(Clone)]
pub struct SimFeeders(Arc<Mutex<ChainState>>);

impl SimFeeders {
    /// A chain of `slots` feeders (1 to 32), each with `tape` advances in its tape (`None`: not counted), answering to `token`.
    pub fn new(slots: usize, tape: Option<u32>, token: &str) -> SimFeeders {
        SimFeeders(Arc::new(Mutex::new(ChainState {
            token: token.to_string(),
            units: vec![
                Unit {
                    online: true,
                    remaining: tape,
                    fault: None,
                    cause_gone: false,
                    advances: 0,
                };
                slots.clamp(1, 32)
            ],
            safe: false,
            requests: 0,
            inject: Injection::none(),
            watch: Watch::new(SIM_SERVER_LOST_MS),
        })))
    }

    /// Sets the watch of the feeder node over the program (`server_lost_ms`).
    pub fn set_server_lost_ms(&self, ms: u32) {
        self.state().watch.limit = ms;
    }

    /// Time passes: a node that has heard a question with the token and then none for longer than its watch is safe.
    pub fn elapse(&self, ms: u32) {
        let mut s = self.state();
        if s.watch.elapse(ms) {
            s.safe = true;
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, ChainState> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn set_injection(&self, inject: Injection) {
        self.state().inject = inject;
    }

    /// A feeder that does not answer (or answers again).
    pub fn set_online(&self, slot: usize, on: bool) {
        if let Some(u) = self.state().units.get_mut(slot.wrapping_sub(1)) {
            u.online = on;
        }
    }

    /// A feeder with a jam; its cause stays until `clear_cause`.
    pub fn jam(&self, slot: usize) {
        if let Some(u) = self.state().units.get_mut(slot.wrapping_sub(1)) {
            u.fault = Some("FDR-J01");
            u.cause_gone = false;
        }
    }

    /// The cause of the fault is gone: an acknowledgement now clears it.
    pub fn clear_cause(&self, slot: usize) {
        if let Some(u) = self.state().units.get_mut(slot.wrapping_sub(1)) {
            u.cause_gone = true;
        }
    }

    /// How many advances are left in a tape.
    pub fn set_remaining(&self, slot: usize, left: Option<u32>) {
        if let Some(u) = self.state().units.get_mut(slot.wrapping_sub(1)) {
            u.remaining = left;
        }
    }

    pub fn set_safe(&self, on: bool) {
        self.state().safe = on;
    }

    pub fn requests(&self) -> u64 {
        self.state().requests
    }

    /// The pitches a feeder has advanced since the start.
    pub fn advances(&self, slot: usize) -> u32 {
        self.state()
            .units
            .get(slot.wrapping_sub(1))
            .map_or(0, |u| u.advances)
    }

    pub fn handle(&self, method: &str, target: &str, bearer: Option<&str>) -> Out {
        let mut s = self.state();
        s.requests += 1;
        let call = Call::new(method, target, bearer);
        let token_ok = !s.token.is_empty() && call.token.as_deref() == Some(s.token.as_str());
        let inject = s.inject.draw();
        if let Some(out) = injected(inject, &call, token_ok) {
            return out;
        }
        if token_needed(&call) && !token_ok {
            return error(401, "unauthorized");
        }
        if token_ok {
            s.watch.heard();
        }
        s.route(&call)
    }
}

impl Transport for SimFeeders {
    fn send(&self, request: &Request) -> Result<Reply, TransportError> {
        self.handle(&request.method, &request.path, request.bearer())
            .map(|(status, body)| Reply::new(status, body))
    }
}

impl NodeLink for SimFeeders {
    fn call(&mut self, method: &str, path: &str, auth: bool) -> Result<String, LinkError> {
        let token = if auth {
            Some(self.state().token.clone())
        } else {
            None
        };
        match self.handle(method, path, token.as_deref()) {
            Ok((status, body)) => answer(status, body),
            Err(_) => Err(LinkError::Silent),
        }
    }
}
