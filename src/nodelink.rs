// EH3D-PHOENIX-PNP - the link to the nodes of the feeders and of the head.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! The feeder node (a chain of up to 32 feeders on a bus) and the head node (the nozzles, the pump and the valves) answer over HTTP with one line of JSON, as their `CONTRACT.md` says. This is
//! the program's side of that conversation. `NodeLink` is the one thing the program needs of a link: ask for a path, with or without the token, and get the text of the answer or what went
//! wrong (silence, a refused token, a refusal with its reason). `ScNodeLink` is that over the common client of the suite (`eh3d-serverclient`: its retries, its time limit, its state of the
//! link), and `nodesim` holds nodes on a computer that speak the same contract, so everything here is tried without a node. On top of the link, `Head` and `Feeders` read the answers into
//! typed states and give the orders; both keep a `Guard`: after `node_silence_limit` questions in a row without an answer (or when the node says it is in its safe state) the node is held and
//! nothing that moves is asked of it until a person acknowledges, and a head that cannot be heard is never taken to be clear. `xy_clear_signal` is what the interlock of the axes reads: the
//! machine may move in X and Y only while the head says every nozzle is raised. The refusal of an order keeps its status and, where the link can see it, its reason. Over `ScNodeLink` that
//! reason is the `error` of the answer or, with none, its `detail`, because the released common client does not keep the `why` of a refusal and forgets a token after a `401`; with the feature
//! `common-refusal` (for a release of the common client that has `Client::why_of` and `Client::with_fixed_token`) the reason is the `why` that the node gave and the token stays. Nothing
//! here has talked to a real node.

use crate::slotbank::Seen;
use eh3d_serverclient as sc;

/// The name of the signal that the interlocks of the axes read for the head.
pub const XY_CLEAR: &str = "xy_clear";

/// The most nozzles and the most feeders that are believed.
pub const MAX_NOZZLES: usize = 6;
pub const MAX_SLOTS: usize = 32;
/// The most bytes of the body of a refusal that the link keeps when asked to (the most the common client keeps).
#[cfg(feature = "common-refusal")]
pub const MAX_EXCERPT_BYTES: u16 = 4096;

/// Why a question or an order did not give an answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkError {
    /// The node did not answer (after the tries of the client).
    Silent,
    /// The node refused the token, or there is none.
    Unauthorized,
    /// The node answered with a refusal: its status and, when it could be read, its reason.
    Refused { status: u16, why: Option<String> },
    /// The answer could not be read.
    Bad(String),
    /// The guard holds the node: nothing that moves is asked until a person acknowledges.
    Held(Hold),
}

impl std::fmt::Display for LinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LinkError::Silent => f.write_str("the node does not answer"),
            LinkError::Unauthorized => f.write_str("the node refuses the token"),
            LinkError::Refused { status, why } => match why {
                Some(w) => write!(f, "the node refuses ({status}): {w}"),
                None => write!(f, "the node refuses ({status})"),
            },
            LinkError::Bad(m) => write!(f, "the answer of the node cannot be read: {m}"),
            LinkError::Held(h) => write!(f, "the node is held: {}", h.name()),
        }
    }
}

impl std::error::Error for LinkError {}

/// Why the guard holds a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hold {
    /// Too many questions in a row went unanswered.
    Silent,
    /// The node said it is in its safe state.
    Safe,
}

impl Hold {
    pub fn name(self) -> &'static str {
        match self {
            Hold::Silent => "silent",
            Hold::Safe => "safe",
        }
    }
}

/// What the program needs of a link to a node.
pub trait NodeLink {
    /// Sends `method` (`GET` or `POST`) with the path and the query, with the token (`auth`) or without it, and gives the text of a good answer.
    fn call(&mut self, method: &str, path: &str, auth: bool) -> Result<String, LinkError>;
}

/// The link over the common client of the suite.
pub struct ScNodeLink {
    client: sc::Client,
    /// Given back to the client after a `401` (the client that is released forgets it); a client with a fixed token keeps it by itself.
    #[cfg(not(feature = "common-refusal"))]
    token: String,
}

/// A sleeper that does not wait, for a link tried against a node on a computer.
pub struct NoSleep;

impl sc::client::Sleeper for NoSleep {
    fn sleep(&self, _how_long: std::time::Duration) {}
}

impl ScNodeLink {
    /// A link to the node at `address` (`host:port`) over `transport` (the plain one of the client, or a node on a computer). `timeout_s` and `retries` are the time one question may take and
    /// the times a failed question is asked again.
    pub fn new(
        address: &str,
        token: &str,
        timeout_s: u64,
        retries: u32,
        transport: Box<dyn sc::Transport>,
    ) -> ScNodeLink {
        ScNodeLink::build(address, token, timeout_s, retries, 0, transport)
    }

    /// The same, keeping up to `excerpt_bytes` bytes (at most `MAX_EXCERPT_BYTES`) of the body of every refusal, with the token and the values under a name such as `token` or `password`
    /// replaced by `***`; `refused_body` gives them. With 0, as in `new`, nothing of a body is kept.
    #[cfg(feature = "common-refusal")]
    pub fn new_with_excerpt(
        address: &str,
        token: &str,
        timeout_s: u64,
        retries: u32,
        excerpt_bytes: u16,
        transport: Box<dyn sc::Transport>,
    ) -> ScNodeLink {
        ScNodeLink::build(address, token, timeout_s, retries, excerpt_bytes, transport)
    }

    /// The start of the body of the last refusal, masked, when `new_with_excerpt` asked to keep one. Read it right after the error.
    #[cfg(feature = "common-refusal")]
    pub fn refused_body(&self) -> Option<&str> {
        self.client
            .last_refusal()
            .and_then(|r| r.body_excerpt.as_deref())
    }

    // `excerpt_bytes` is used only with the feature: the settings of the released client have no such key
    #[cfg_attr(not(feature = "common-refusal"), allow(unused_variables))]
    fn build(
        address: &str,
        token: &str,
        timeout_s: u64,
        retries: u32,
        excerpt_bytes: u16,
        transport: Box<dyn sc::Transport>,
    ) -> ScNodeLink {
        #[cfg_attr(not(feature = "common-refusal"), allow(unused_mut))]
        let mut settings = sc::Settings {
            server_address: address.to_string(),
            server_token: sc::Secret::new(token),
            server_timeout_s: timeout_s,
            server_retries: retries,
            ..sc::Settings::default()
        };
        #[cfg(feature = "common-refusal")]
        {
            settings.server_error_body_bytes = u64::from(excerpt_bytes.min(MAX_EXCERPT_BYTES));
        }
        let client = sc::Client::new(settings, transport).with_auto_refresh(false);
        // the token of a node is written in the settings and does not run out: a `401` does not make the client forget it
        #[cfg(feature = "common-refusal")]
        let client = client.with_fixed_token(true);
        ScNodeLink {
            client,
            #[cfg(not(feature = "common-refusal"))]
            token: token.to_string(),
        }
    }

    /// The same without waiting between the tries, for the tests.
    pub fn without_waiting(mut self) -> ScNodeLink {
        self.client = self.client.with_sleeper(Box::new(NoSleep));
        self
    }
}

/// The `why` of a refusal, or its `error`, in the text of an answer (what a node on a computer says; the common client is asked instead where it can say it).
fn why_of(body: &str) -> Option<String> {
    let j = sc::Json::parse(body).ok()?;
    j.get("why")
        .or_else(|| j.get("error"))
        .and_then(sc::Json::as_str)
        .map(str::to_string)
}

/// A status and a body as the error or the text they are.
pub fn answer(status: u16, body: String) -> Result<String, LinkError> {
    match status {
        200..=299 => Ok(body),
        401 => Err(LinkError::Unauthorized),
        s => Err(LinkError::Refused {
            status: s,
            why: why_of(&body),
        }),
    }
}

impl NodeLink for ScNodeLink {
    fn call(&mut self, method: &str, path: &str, auth: bool) -> Result<String, LinkError> {
        let result = if auth {
            self.client.request(method, path, None)
        } else {
            self.client.request_open(method, path, None)
        };
        match result {
            Ok(reply) => Ok(reply.body),
            Err(sc::ClientError::Unreachable { .. }) => Err(LinkError::Silent),
            Err(sc::ClientError::Unauthorized { .. }) => {
                // the client forgets a token the server refused, as it should for a login that ran out; the token of a node is fixed in the settings, so it is given back and the next
                // question asks again (a node that refuses it once is not taken to refuse it for ever). With a fixed token the client does that by itself.
                #[cfg(not(feature = "common-refusal"))]
                self.client.set_token(sc::Secret::new(self.token.clone()));
                Err(LinkError::Unauthorized)
            }
            Err(sc::ClientError::Forbidden { .. }) => Err(LinkError::Refused {
                status: 403,
                why: None,
            }),
            #[cfg(feature = "common-refusal")]
            Err(error @ sc::ClientError::Server { status, .. }) => {
                // asked right after the error, as the client says: the `why` a node gives with a `409`, and none when the answer has none
                let why = self.client.why_of(&error).map(str::to_string);
                Err(LinkError::Refused { status, why })
            }
            #[cfg(not(feature = "common-refusal"))]
            Err(sc::ClientError::Server {
                status,
                code,
                detail,
            }) => {
                // the released common client keeps the `error` and the `detail` of a refusal and not its `why`: the reason is the code of the refusal, and where the node gave none, its
                // detail (see docs/COMMON_REQUESTS.md)
                let detail = detail.trim();
                let why = code.or_else(|| (!detail.is_empty()).then(|| detail.to_string()));
                Err(LinkError::Refused { status, why })
            }
            Err(sc::ClientError::Config(m) | sc::ClientError::BadReply(m)) => {
                Err(LinkError::Bad(m))
            }
        }
    }
}

/// Holds a node that cannot be heard or that says it is safe: from `limit` questions in a row without an answer on, nothing that moves is asked of it until a person acknowledges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Guard {
    limit: u32,
    silent_run: u32,
    held: Option<Hold>,
}

impl Guard {
    pub fn new(limit: u32) -> Guard {
        Guard {
            limit: limit.max(1),
            silent_run: 0,
            held: None,
        }
    }

    /// Takes note of how a question ended.
    pub fn note<T>(&mut self, result: &Result<T, LinkError>) {
        match result {
            Err(LinkError::Silent) => {
                self.silent_run = self.silent_run.saturating_add(1);
                if self.silent_run >= self.limit && self.held.is_none() {
                    self.held = Some(Hold::Silent);
                }
            }
            Ok(_) | Err(_) => self.silent_run = 0,
        }
    }

    /// The node said it is safe.
    pub fn node_is_safe(&mut self) {
        self.held = Some(Hold::Safe);
    }

    pub fn held(&self) -> Option<Hold> {
        self.held
    }

    pub fn silent_run(&self) -> u32 {
        self.silent_run
    }

    /// A person acknowledges. It takes effect only while the node answers: with the last question unanswered the hold stays. Whether the node itself is still safe is for its own
    /// acknowledgement; the guard lets go and the next answer holds it again if the node still says so.
    pub fn acknowledge(&mut self) -> bool {
        if self.silent_run > 0 {
            return false;
        }
        self.held = None;
        true
    }
}

fn parse(body: &str) -> Result<sc::Json, LinkError> {
    sc::Json::parse(body).map_err(LinkError::Bad)
}

fn text(j: &sc::Json, key: &str) -> Result<String, LinkError> {
    j.get(key)
        .and_then(sc::Json::as_str)
        .map(str::to_string)
        .ok_or_else(|| LinkError::Bad(format!("no text '{key}'")))
}

fn flag(j: &sc::Json, key: &str) -> Result<bool, LinkError> {
    j.get(key)
        .and_then(sc::Json::as_bool)
        .ok_or_else(|| LinkError::Bad(format!("no yes or no '{key}'")))
}

fn number(j: &sc::Json, key: &str) -> Result<f64, LinkError> {
    match j.get(key) {
        Some(sc::Json::Num(n)) => Ok(*n),
        _ => Err(LinkError::Bad(format!("no number '{key}'"))),
    }
}

fn whole(j: &sc::Json, key: &str) -> Result<u64, LinkError> {
    j.get(key)
        .and_then(sc::Json::as_u64)
        .ok_or_else(|| LinkError::Bad(format!("no whole number '{key}'")))
}

/// One nozzle of the head.
#[derive(Debug, Clone, PartialEq)]
pub struct NozzleState {
    pub n: u8,
    pub phase: String,
    pub part: bool,
    pub vacuum_ok: bool,
    pub raised: bool,
    pub z_mm: f64,
}

/// What the head says of itself.
#[derive(Debug, Clone, PartialEq)]
pub struct HeadStatus {
    pub safe: bool,
    pub xy_clear: bool,
    pub xy_moving: bool,
    /// The nozzles that are not raised.
    pub blocking: Vec<u8>,
    pub z_safe_mm: f64,
    pub selfcheck: String,
    pub nozzles: Vec<NozzleState>,
}

impl HeadStatus {
    pub fn from_body(body: &str) -> Result<HeadStatus, LinkError> {
        let j = parse(body)?;
        let blocking = text(&j, "xy_blocking")?
            .split(',')
            .filter(|w| !w.trim().is_empty())
            .map(|w| {
                w.trim()
                    .parse::<u8>()
                    .map_err(|_| LinkError::Bad(format!("'{w}' is not a nozzle")))
            })
            .collect::<Result<Vec<u8>, LinkError>>()?;
        let mut nozzles = Vec::new();
        if let Some(list) = j.get("nozzles").and_then(sc::Json::as_array) {
            if list.len() > MAX_NOZZLES {
                return Err(LinkError::Bad(format!("more than {MAX_NOZZLES} nozzles")));
            }
            for e in list {
                nozzles.push(NozzleState {
                    n: u8::try_from(whole(e, "n")?).map_err(|_| LinkError::Bad("n".into()))?,
                    phase: text(e, "phase")?,
                    part: flag(e, "part")?,
                    vacuum_ok: flag(e, "vacuum_ok")?,
                    raised: flag(e, "raised")?,
                    z_mm: number(e, "z_mm")?,
                });
            }
        }
        Ok(HeadStatus {
            safe: flag(&j, "safe")?,
            xy_clear: flag(&j, "xy_clear")?,
            xy_moving: flag(&j, "xy_moving")?,
            blocking,
            z_safe_mm: number(&j, "z_safe_mm")?,
            selfcheck: text(&j, "selfcheck")?,
            nozzles,
        })
    }
}

/// The head node.
pub struct Head<L: NodeLink> {
    link: L,
    guard: Guard,
    last: Option<HeadStatus>,
}

impl<L: NodeLink> Head<L> {
    /// A head over `link` that is held after `silence_limit` questions in a row without an answer (`node_silence_limit`).
    pub fn new(link: L, silence_limit: u32) -> Head<L> {
        Head {
            link,
            guard: Guard::new(silence_limit),
            last: None,
        }
    }

    pub fn guard(&self) -> &Guard {
        &self.guard
    }

    pub fn last(&self) -> Option<&HeadStatus> {
        self.last.as_ref()
    }

    pub fn link_mut(&mut self) -> &mut L {
        &mut self.link
    }

    /// A person acknowledges the hold; true when it was let go.
    pub fn acknowledge(&mut self) -> bool {
        self.guard.acknowledge()
    }

    fn ask(&mut self, method: &str, path: &str, auth: bool) -> Result<String, LinkError> {
        let result = self.link.call(method, path, auth);
        self.guard.note(&result);
        result
    }

    fn order(&mut self, path: &str) -> Result<String, LinkError> {
        if let Some(h) = self.guard.held() {
            return Err(LinkError::Held(h));
        }
        self.ask("POST", path, true)
    }

    /// Asks the head for its state (this needs no token). A head that says it is safe is held.
    pub fn refresh(&mut self) -> Result<&HeadStatus, LinkError> {
        let body = self.ask("GET", "/api/status", false);
        let status = body.and_then(|b| HeadStatus::from_body(&b));
        match status {
            Ok(s) => {
                if s.safe {
                    self.guard.node_is_safe();
                }
                self.last = Some(s);
                Ok(self.last.as_ref().unwrap_or_else(|| unreachable!()))
            }
            Err(e) => {
                self.last = None;
                Err(e)
            }
        }
    }

    /// Whether the machine may move in X and Y: the head was asked just now and says every nozzle is raised, it is not held and it is not safe. Anything else (no answer, a token refused, a
    /// reading that makes no sense) is no.
    pub fn xy_clear(&mut self) -> bool {
        let held = self.guard.held().is_some();
        match self.refresh() {
            Ok(s) => s.xy_clear && !s.safe && !held,
            Err(_) => false,
        }
    }

    /// A person clears the safe state of the head itself (`POST /api/ack`), the head is asked again and, when it answers and is no longer safe, the hold is let go.
    pub fn acknowledge_node(&mut self) -> Result<(), LinkError> {
        self.ask("POST", "/api/ack", true)?;
        let safe = self.refresh()?.safe;
        if safe || !self.guard.acknowledge() {
            return Err(LinkError::Held(self.guard.held().unwrap_or(Hold::Safe)));
        }
        Ok(())
    }

    /// Asks the head for the permission to move in X and Y (`true`) or tells it the move is over (`false`). The permission is refused while a nozzle is down or while the guard holds the head;
    /// letting it go is always tried.
    pub fn set_xy_moving(&mut self, moving: bool) -> Result<(), LinkError> {
        let path = format!("/api/xy?moving={}", u8::from(moving));
        if moving {
            self.order(&path).map(|_| ())
        } else {
            self.ask("POST", &path, true).map(|_| ())
        }
    }

    fn nozzle_order(&mut self, what: &str, n: u8) -> Result<(), LinkError> {
        if n == 0 || usize::from(n) > MAX_NOZZLES {
            return Err(LinkError::Bad(format!("the nozzle {n} does not exist")));
        }
        self.order(&format!("/api/nozzle/{what}?n={n}")).map(|_| ())
    }

    /// Picks with a nozzle: the vacuum says whether a part is held.
    pub fn pick(&mut self, n: u8) -> Result<(), LinkError> {
        self.nozzle_order("pick", n)
    }

    /// Releases the part of a nozzle.
    pub fn place(&mut self, n: u8) -> Result<(), LinkError> {
        self.nozzle_order("place", n)
    }

    /// Moves a nozzle to a height.
    pub fn move_z(&mut self, n: u8, mm: f64) -> Result<(), LinkError> {
        if !mm.is_finite() || n == 0 || usize::from(n) > MAX_NOZZLES {
            return Err(LinkError::Bad(
                "the nozzle or the height is not valid".into(),
            ));
        }
        self.order(&format!("/api/z?n={n}&mm={mm:.3}")).map(|_| ())
    }
}

/// One feeder of the chain.
#[derive(Debug, Clone, PartialEq)]
pub struct FeederState {
    pub slot: u8,
    pub online: bool,
    pub state: String,
    pub fault: String,
    pub remaining: Option<u32>,
    pub low: bool,
}

impl FeederState {
    /// What the slot model makes of it.
    pub fn seen(&self) -> Seen {
        if !self.online || self.state == "unknown" {
            return Seen::Unknown;
        }
        match self.state.as_str() {
            "fault" | "disabled" | "safe" => Seen::Fault,
            "empty" => Seen::Empty,
            _ => Seen::Fine,
        }
    }
}

/// What the feeder node says of its chain.
#[derive(Debug, Clone, PartialEq)]
pub struct ChainStatus {
    pub slots: u8,
    pub online: u8,
    pub safe: bool,
    pub selfcheck: String,
    pub feeders: Vec<FeederState>,
}

impl ChainStatus {
    pub fn from_body(body: &str) -> Result<ChainStatus, LinkError> {
        let j = parse(body)?;
        let mut feeders = Vec::new();
        if let Some(list) = j.get("feeders").and_then(sc::Json::as_array) {
            if list.len() > MAX_SLOTS {
                return Err(LinkError::Bad(format!("more than {MAX_SLOTS} feeders")));
            }
            for e in list {
                feeders.push(feeder_from(e)?);
            }
        }
        Ok(ChainStatus {
            slots: u8::try_from(whole(&j, "slots")?).map_err(|_| LinkError::Bad("slots".into()))?,
            online: u8::try_from(whole(&j, "online")?)
                .map_err(|_| LinkError::Bad("online".into()))?,
            safe: flag(&j, "safe")?,
            selfcheck: text(&j, "selfcheck")?,
            feeders,
        })
    }

    /// What the node sees at each position of the chain, for `Layout::sync`.
    pub fn seen(&self) -> Vec<(u8, Seen)> {
        self.feeders.iter().map(|f| (f.slot, f.seen())).collect()
    }
}

fn feeder_from(e: &sc::Json) -> Result<FeederState, LinkError> {
    let remaining = match e.get("remaining") {
        Some(sc::Json::Null) | None => None,
        Some(_) => Some(
            u32::try_from(whole(e, "remaining")?)
                .map_err(|_| LinkError::Bad("remaining".into()))?,
        ),
    };
    Ok(FeederState {
        slot: u8::try_from(whole(e, "slot")?).map_err(|_| LinkError::Bad("slot".into()))?,
        online: flag(e, "online")?,
        state: text(e, "state")?,
        fault: text(e, "fault")?,
        remaining,
        low: flag(e, "low")?,
    })
}

/// The feeder node.
pub struct Feeders<L: NodeLink> {
    link: L,
    guard: Guard,
    last: Option<ChainStatus>,
}

impl<L: NodeLink> Feeders<L> {
    pub fn new(link: L, silence_limit: u32) -> Feeders<L> {
        Feeders {
            link,
            guard: Guard::new(silence_limit),
            last: None,
        }
    }

    pub fn guard(&self) -> &Guard {
        &self.guard
    }

    pub fn last(&self) -> Option<&ChainStatus> {
        self.last.as_ref()
    }

    pub fn link_mut(&mut self) -> &mut L {
        &mut self.link
    }

    pub fn acknowledge(&mut self) -> bool {
        self.guard.acknowledge()
    }

    fn ask(&mut self, method: &str, path: &str, auth: bool) -> Result<String, LinkError> {
        let result = self.link.call(method, path, auth);
        self.guard.note(&result);
        result
    }

    /// Asks the node for the state of its chain. A node that says it is safe is held.
    pub fn refresh(&mut self) -> Result<&ChainStatus, LinkError> {
        let body = self.ask("GET", "/api/status", false);
        match body.and_then(|b| ChainStatus::from_body(&b)) {
            Ok(s) => {
                if s.safe {
                    self.guard.node_is_safe();
                }
                self.last = Some(s);
                Ok(self.last.as_ref().unwrap_or_else(|| unreachable!()))
            }
            Err(e) => {
                self.last = None;
                Err(e)
            }
        }
    }

    fn check_slot(slot: u8) -> Result<(), LinkError> {
        if slot == 0 || usize::from(slot) > MAX_SLOTS {
            return Err(LinkError::Bad(format!(
                "the position {slot} is not from 1 to {MAX_SLOTS}"
            )));
        }
        Ok(())
    }

    /// Moves the tape of one feeder by `count` pitches (1 to 8).
    pub fn advance(&mut self, slot: u8, count: u8) -> Result<(), LinkError> {
        Feeders::<L>::check_slot(slot)?;
        if !(1..=8).contains(&count) {
            return Err(LinkError::Bad(format!(
                "{count} pitches are not from 1 to 8"
            )));
        }
        if let Some(h) = self.guard.held() {
            return Err(LinkError::Held(h));
        }
        self.ask(
            "POST",
            &format!("/api/feeder/advance?slot={slot}&count={count}"),
            true,
        )
        .map(|_| ())
    }

    /// Clears the fault of one feeder whose cause is gone.
    pub fn ack(&mut self, slot: u8) -> Result<(), LinkError> {
        Feeders::<L>::check_slot(slot)?;
        self.ask("POST", &format!("/api/feeder/ack?slot={slot}"), true)
            .map(|_| ())
    }
}

/// The value the interlock of the axes reads for the head: yes only when the head has just said that every nozzle is raised.
pub fn xy_clear_signal<L: NodeLink>(head: &mut Head<L>) -> bool {
    head.xy_clear()
}
