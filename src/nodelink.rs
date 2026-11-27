// EH3D-PHOENIX-PNP - the link to the nodes of the feeders and of the head.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! The feeder node (a chain of up to 32 feeders on a bus) and the head node (the nozzles, the pump and the valves) answer over HTTP with one line of JSON, as their `CONTRACT.md` says. This is
//! the program's side of that conversation. `NodeLink` is the one thing the program needs of a link: ask for a path, with or without the token, and get the text of the answer or what went
//! wrong (silence, a refused token, a refusal with its reason). `ScNodeLink` is that over the common client of the suite (`eh3d-serverclient`: its retries, its time limit, its state of the
//! link), and `nodesim` holds nodes on a computer that speak the same contract, so everything here is tried without a node. On top of the link, `Head` and `Feeders` read the answers into
//! typed states and give the orders; both keep a `Guard`: after `node_silence_limit` questions in a row without an answer (or when the node says it is in its safe state) the node is held and
//! nothing that moves is asked of it until a person acknowledges, and a head that cannot be heard is never taken to be clear. A node goes to its safe state by itself when it is not asked
//! with the token for the time it says in its status (`link_lost_ms` of the head, `server_lost_ms` of the feeder node), so the state is asked with the token, and with a second link
//! (`keep_alive`) the node is asked three times in that time while an order is waited for. `xy_clear_signal` is what the interlock of the axes reads: the
//! machine may move in X and Y only while the head says every nozzle is raised. The refusal of an order keeps its status and, where the link can see it, its reason. Over `ScNodeLink` that
//! reason is the `why` that the node gave, asked of the common client right after the error (`Client::why_of`), or, when there is no `why`, the word `safe_state` or `busy` under
//! `error` (as the feeder node answers), and a fixed token stays after a `401` (`Client::with_fixed_token`). A refusal with `safe_state` holds the node at once. An order that gets no answer is
//! followed by a stop of the node, and a feeder whose advance did not end well (no answer, `stopped`, `timeout`, `no_answer`) takes no further advance until it is acknowledged,
//! because nobody knows where its tape is. Nothing here has talked to a real node.

use crate::error::Lang;
use crate::msg8::{pick, say};
use crate::slotbank::Seen;
use eh3d_serverclient as sc;
use std::collections::BTreeSet;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

/// The name of the signal that the interlocks of the axes read for the head.
pub const XY_CLEAR: &str = "xy_clear";

/// The most nozzles and the most feeders that are believed.
pub const MAX_NOZZLES: usize = 6;
pub const MAX_SLOTS: usize = 32;
/// The most bytes of the body of a refusal that the link keeps when asked to (the most the common client keeps).
pub const MAX_EXCERPT_BYTES: u16 = 4096;

/// The words that the contract of the feeder node puts under `error`, not under `why`, in a refusal of an order.
pub const ERROR_WORDS: [&str; 2] = ["safe_state", "busy"];

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
    /// The tape of this feeder is not where it was asked to be, or nobody knows: it takes no advance until a person acknowledges the feeder.
    Unsure(u8),
}

impl std::fmt::Display for LinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.localized(Lang::En))
    }
}

impl LinkError {
    /// The reason a node gave with a refusal.
    pub fn why(&self) -> Option<&str> {
        match self {
            LinkError::Refused { why, .. } => why.as_deref(),
            _ => None,
        }
    }

    /// The error in a language. The word a node gives as its reason is kept as the node wrote it.
    pub fn localized(&self, lang: Lang) -> String {
        let (row, pairs): ([&'static str; 8], Vec<(&str, String)>) = match self {
            LinkError::Silent => (
                [
                    "the node does not answer",
                    "el nodo no responde",
                    "der Knoten antwortet nicht",
                    "le nœud ne répond pas",
                    "il nodo non risponde",
                    "ノードが応答しません",
                    "节点没有应答",
                    "o nó não responde",
                ],
                vec![],
            ),
            LinkError::Unauthorized => (
                [
                    "the node refuses the token",
                    "el nodo rechaza el token",
                    "der Knoten lehnt das Token ab",
                    "le nœud refuse le jeton",
                    "il nodo rifiuta il token",
                    "ノードがトークンを拒否しました",
                    "节点拒绝了令牌",
                    "o nó recusa o token",
                ],
                vec![],
            ),
            LinkError::Refused {
                status,
                why: Some(why),
            } => (
                [
                    "the node refuses ({status}): {why}",
                    "el nodo rechaza ({status}): {why}",
                    "der Knoten lehnt ab ({status}): {why}",
                    "le nœud refuse ({status}) : {why}",
                    "il nodo rifiuta ({status}): {why}",
                    "ノードが拒否しました（{status}）：{why}",
                    "节点拒绝（{status}）：{why}",
                    "o nó recusa ({status}): {why}",
                ],
                vec![("status", status.to_string()), ("why", why.clone())],
            ),
            LinkError::Refused { status, why: None } => (
                [
                    "the node refuses ({status})",
                    "el nodo rechaza ({status})",
                    "der Knoten lehnt ab ({status})",
                    "le nœud refuse ({status})",
                    "il nodo rifiuta ({status})",
                    "ノードが拒否しました（{status}）",
                    "节点拒绝（{status}）",
                    "o nó recusa ({status})",
                ],
                vec![("status", status.to_string())],
            ),
            LinkError::Bad(what) => (
                [
                    "the answer of the node cannot be read: {what}",
                    "no se puede leer la respuesta del nodo: {what}",
                    "die Antwort des Knotens ist nicht lesbar: {what}",
                    "la réponse du nœud est illisible : {what}",
                    "la risposta del nodo non è leggibile: {what}",
                    "ノードの応答を読めません：{what}",
                    "无法读取节点的应答：{what}",
                    "não é possível ler a resposta do nó: {what}",
                ],
                vec![("what", what.clone())],
            ),
            LinkError::Held(hold) => (
                [
                    "the node is held: {hold}",
                    "el nodo está retenido: {hold}",
                    "der Knoten ist gesperrt: {hold}",
                    "le nœud est retenu : {hold}",
                    "il nodo è bloccato: {hold}",
                    "ノードは保留されています：{hold}",
                    "节点处于保持状态：{hold}",
                    "o nó está retido: {hold}",
                ],
                vec![("hold", hold.localized(lang).to_string())],
            ),
            LinkError::Unsure(slot) => (
                [
                    "the tape of the feeder {slot} may not be where it was asked to be: look at it and acknowledge it",
                    "la cinta del alimentador {slot} puede no estar donde se pidió: revísela y confírmelo",
                    "der Gurt des Feeders {slot} steht vielleicht nicht, wo er sollte: ansehen und quittieren",
                    "la bande du chargeur {slot} n'est peut-être pas à la position demandée : vérifiez-la puis acquittez",
                    "il nastro dell'alimentatore {slot} potrebbe non essere dove richiesto: controllarlo e confermare",
                    "フィーダ {slot} のテープは指示の位置にない可能性があります。確認してから承認してください",
                    "供料器 {slot} 的编带可能不在要求的位置：请检查后确认",
                    "a fita do alimentador {slot} pode não estar onde foi pedido: verifique-a e confirme",
                ],
                vec![("slot", slot.to_string())],
            ),
        };
        say(lang, &row, &pairs)
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

    /// The word of the hold in a language.
    pub fn localized(self, lang: Lang) -> &'static str {
        let row: [&'static str; 8] = match self {
            Hold::Silent => [
                "silent",
                "sin respuesta",
                "stumm",
                "muet",
                "muto",
                "無応答",
                "无应答",
                "sem resposta",
            ],
            Hold::Safe => [
                "safe",
                "en estado seguro",
                "im sicheren Zustand",
                "en état sûr",
                "in stato sicuro",
                "安全状態",
                "安全状态",
                "em estado seguro",
            ],
        };
        pick(lang, &row)
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
    pub fn refused_body(&self) -> Option<&str> {
        self.client
            .last_refusal()
            .and_then(|r| r.body_excerpt.as_deref())
    }

    fn build(
        address: &str,
        token: &str,
        timeout_s: u64,
        retries: u32,
        excerpt_bytes: u16,
        transport: Box<dyn sc::Transport>,
    ) -> ScNodeLink {
        let mut settings = sc::Settings {
            server_address: address.to_string(),
            server_token: sc::Secret::new(token),
            server_timeout_s: timeout_s,
            server_retries: retries,
            ..sc::Settings::default()
        };
        settings.server_error_body_bytes = u64::from(excerpt_bytes.min(MAX_EXCERPT_BYTES));
        let client = sc::Client::new(settings, transport).with_auto_refresh(false);
        // the token of a node is written in the settings and does not run out: a `401` does not make the client forget it
        let client = client.with_fixed_token(true);
        ScNodeLink { client }
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
            // the token of a node is fixed in the settings, so the client keeps it and the next question asks again (a node that refuses it once is not taken to refuse it for ever)
            Err(sc::ClientError::Unauthorized { .. }) => Err(LinkError::Unauthorized),
            Err(sc::ClientError::Forbidden { .. }) => Err(LinkError::Refused {
                status: 403,
                why: None,
            }),
            Err(error @ sc::ClientError::Server { status, .. }) => {
                // asked right after the error, as the client says: the `why` a node gives with a `409`; the feeder node says `safe_state` and `busy` under `error` instead
                let why = self.client.why_of(&error).map(str::to_string);
                let why = why.or_else(|| match error {
                    sc::ClientError::Server { code, .. } => {
                        code.filter(|c| ERROR_WORDS.contains(&c.as_str()))
                    }
                    _ => None,
                });
                Err(LinkError::Refused { status, why })
            }
            Err(sc::ClientError::Config(m) | sc::ClientError::BadReply(m)) => {
                Err(LinkError::Bad(m))
            }
        }
    }
}

/// The longest watch of a node that is believed (the settings of the nodes go up to this).
pub const MAX_WATCH_MS: u64 = 600_000;
/// A node is asked at least this far apart while an order is waited for, whatever it says.
pub const MIN_BEAT_MS: u32 = 50;

/// How often a node that goes to its safe state after `watch_ms` without a question with the token is asked while an order is waited for: three times in that time.
pub fn beat_period_ms(watch_ms: u32) -> u32 {
    (watch_ms / 3).max(MIN_BEAT_MS)
}

/// Whether one question that gets no answer is enough for the safe state: the program waits `timeout_s` for it, and the node `watch_ms`.
pub fn one_lost_question_is_enough(watch_ms: u32, timeout_s: u64) -> bool {
    timeout_s.saturating_mul(1000) >= u64::from(watch_ms)
}

/// The watch that a status says under `key`; a firmware that does not say it, or says what makes no sense, is none.
fn watch_of(j: &sc::Json, key: &str) -> Option<u32> {
    j.get(key)
        .and_then(sc::Json::as_u64)
        .filter(|v| (1..=MAX_WATCH_MS).contains(v))
        .and_then(|v| u32::try_from(v).ok())
}

/// Asks a node for its state with the token, so that the node counts the question as the program being heard; a node that refuses the token (or a program that has none) is asked again
/// without it, so that the state is still read.
fn status_of<L: NodeLink>(link: &mut L) -> Result<String, LinkError> {
    match link.call("GET", "/api/status", true) {
        Err(LinkError::Unauthorized | LinkError::Bad(_)) => link.call("GET", "/api/status", false),
        other => other,
    }
}

/// A second link to a node, on which it is asked for its state while an order is waited for on the first, as the contract of the nodes says.
struct Beat {
    link: Box<dyn NodeLink + Send>,
    answered: u32,
    missed: u32,
}

impl Beat {
    fn new(link: Box<dyn NodeLink + Send>) -> Beat {
        Beat {
            link,
            answered: 0,
            missed: 0,
        }
    }

    /// Runs `order` and asks the node every `period` until it is over.
    fn during<T>(&mut self, period: Duration, order: impl FnOnce() -> T) -> T {
        let (over, waiting) = mpsc::channel::<()>();
        let link = &mut self.link;
        let (result, answered, missed) = std::thread::scope(|scope| {
            let asking = scope.spawn(move || {
                let (mut answered, mut missed) = (0u32, 0u32);
                while let Err(RecvTimeoutError::Timeout) = waiting.recv_timeout(period) {
                    match link.call("GET", "/api/status", true) {
                        Ok(_) => answered = answered.saturating_add(1),
                        Err(_) => missed = missed.saturating_add(1),
                    }
                }
                (answered, missed)
            });
            let result = order();
            drop(over);
            let (answered, missed) = asking.join().unwrap_or((0, 0));
            (result, answered, missed)
        });
        self.answered = self.answered.saturating_add(answered);
        self.missed = self.missed.saturating_add(missed);
        result
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
            Err(e) if e.why() == Some("safe_state") => {
                self.silent_run = 0;
                self.node_is_safe();
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
    /// The milliseconds without a question with the token after which the head is safe (`link_lost_ms`); none when it does not say it.
    pub watch_ms: Option<u32>,
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
            watch_ms: watch_of(&j, "link_lost_ms"),
        })
    }
}

/// The head node.
pub struct Head<L: NodeLink> {
    link: L,
    guard: Guard,
    last: Option<HeadStatus>,
    /// The watch the head said the last time it was read.
    watch_ms: Option<u32>,
    beat: Option<Beat>,
}

impl<L: NodeLink> Head<L> {
    /// A head over `link` that is held after `silence_limit` questions in a row without an answer (`node_silence_limit`).
    pub fn new(link: L, silence_limit: u32) -> Head<L> {
        Head {
            link,
            guard: Guard::new(silence_limit),
            last: None,
            watch_ms: None,
            beat: None,
        }
    }

    /// Keeps the head hearing the program while an order is waited for: `link` is a second link to the same head, asked for the state every third of the watch that the head says. A
    /// head that does not say its watch is not asked, and nothing is asked before the state was read once.
    pub fn keep_alive(mut self, link: Box<dyn NodeLink + Send>) -> Head<L> {
        self.beat = Some(Beat::new(link));
        self
    }

    /// The questions of the second link that were answered and that were not.
    pub fn beats(&self) -> (u32, u32) {
        self.beat
            .as_ref()
            .map_or((0, 0), |b| (b.answered, b.missed))
    }

    /// The watch the head said the last time it was read.
    pub fn watch_ms(&self) -> Option<u32> {
        self.watch_ms
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
        let result = match (self.beat.take(), self.watch_ms) {
            (Some(mut beat), Some(watch)) => {
                let period = Duration::from_millis(u64::from(beat_period_ms(watch)));
                let result = beat.during(period, || self.ask("POST", path, true));
                self.beat = Some(beat);
                result
            }
            (beat, _) => {
                self.beat = beat;
                self.ask("POST", path, true)
            }
        };
        if matches!(result, Err(LinkError::Silent)) {
            // the program gave up waiting and the head may still be moving: ask it to stop (a stop never drops a part)
            let _ = self.link.call("POST", "/api/stop", true);
        }
        result
    }

    /// Stops every move of the head where it is (the vacuum stays as it is). It is asked even while the node is held.
    pub fn stop(&mut self) -> Result<(), LinkError> {
        self.ask("POST", "/api/stop", true).map(|_| ())
    }

    /// Asks the head for its state, with the token so that the question counts as the program being heard (the state is still read when the token is refused). A head that says it is
    /// safe is held.
    pub fn refresh(&mut self) -> Result<&HeadStatus, LinkError> {
        let body = status_of(&mut self.link);
        self.guard.note(&body);
        let status = body.and_then(|b| HeadStatus::from_body(&b));
        match status {
            Ok(s) => {
                if s.safe {
                    self.guard.node_is_safe();
                }
                self.watch_ms = s.watch_ms;
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
    /// The milliseconds without a question with the token after which the node is safe (`server_lost_ms`); none when it does not say it.
    pub watch_ms: Option<u32>,
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
            watch_ms: watch_of(&j, "server_lost_ms"),
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
    watch_ms: Option<u32>,
    beat: Option<Beat>,
    /// The positions whose tape is not known to be where it was asked to be.
    unsure: BTreeSet<u8>,
}

impl<L: NodeLink> Feeders<L> {
    pub fn new(link: L, silence_limit: u32) -> Feeders<L> {
        Feeders {
            link,
            guard: Guard::new(silence_limit),
            last: None,
            watch_ms: None,
            beat: None,
            unsure: BTreeSet::new(),
        }
    }

    /// The positions that take no advance until they are acknowledged with `ack`.
    pub fn unsure(&self) -> Vec<u8> {
        self.unsure.iter().copied().collect()
    }

    /// Stops every feeder at once. It is asked even while the node is held.
    pub fn stop(&mut self) -> Result<(), LinkError> {
        self.ask("POST", "/api/feeder/stop", true).map(|_| ())
    }

    /// Keeps the feeder node hearing the program while an advance is waited for, as `Head::keep_alive` does for the head.
    pub fn keep_alive(mut self, link: Box<dyn NodeLink + Send>) -> Feeders<L> {
        self.beat = Some(Beat::new(link));
        self
    }

    /// The questions of the second link that were answered and that were not.
    pub fn beats(&self) -> (u32, u32) {
        self.beat
            .as_ref()
            .map_or((0, 0), |b| (b.answered, b.missed))
    }

    /// The watch the node said the last time it was read.
    pub fn watch_ms(&self) -> Option<u32> {
        self.watch_ms
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

    /// Asks the node for the state of its chain, with the token as `Head::refresh` does. A node that says it is safe is held.
    pub fn refresh(&mut self) -> Result<&ChainStatus, LinkError> {
        let body = status_of(&mut self.link);
        self.guard.note(&body);
        match body.and_then(|b| ChainStatus::from_body(&b)) {
            Ok(s) => {
                if s.safe {
                    self.guard.node_is_safe();
                }
                self.watch_ms = s.watch_ms;
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
        if self.unsure.contains(&slot) {
            return Err(LinkError::Unsure(slot));
        }
        let path = format!("/api/feeder/advance?slot={slot}&count={count}");
        let result = match (self.beat.take(), self.watch_ms) {
            (Some(mut beat), Some(watch)) => {
                let period = Duration::from_millis(u64::from(beat_period_ms(watch)));
                let result = beat.during(period, || self.ask("POST", &path, true));
                self.beat = Some(beat);
                result
            }
            (beat, _) => {
                self.beat = beat;
                self.ask("POST", &path, true)
            }
        };
        let unsure = match &result {
            Ok(_) => false,
            Err(LinkError::Silent) => {
                // the program gave up waiting and the tape may still be moving: ask the node to stop
                let _ = self.link.call("POST", "/api/feeder/stop", true);
                true
            }
            Err(LinkError::Bad(_)) => true,
            Err(e) => matches!(e.why(), Some("stopped" | "timeout" | "no_answer")),
        };
        if unsure {
            self.unsure.insert(slot);
        }
        result.map(|_| ())
    }

    /// Clears the fault of one feeder whose cause is gone, and lets it be advanced again when its tape was not known to be in place: a person has looked at it.
    pub fn ack(&mut self, slot: u8) -> Result<(), LinkError> {
        Feeders::<L>::check_slot(slot)?;
        self.ask("POST", &format!("/api/feeder/ack?slot={slot}"), true)?;
        self.unsure.remove(&slot);
        Ok(())
    }
}

/// The value the interlock of the axes reads for the head: yes only when the head has just said that every nozzle is raised.
pub fn xy_clear_signal<L: NodeLink>(head: &mut Head<L>) -> bool {
    head.xy_clear()
}
