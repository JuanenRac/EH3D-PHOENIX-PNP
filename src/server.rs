// EH3D-PHOENIX-PNP - the way to the EH3D server.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! The server of the suite runs machines for the programs that talk to it, and the `pnp` plugin of the server is the pick-and-place machine behind it. With `mode = server` in the settings
//! this program sends its table of feeders and its placements there as jobs (the same `/api/v1/work` routes that the other programs of the suite use), starts them, follows them and
//! holds or stops them; the machine, its feeders and the head are the server's. Where the server is, the token of this program and the id of the machine are keys of the settings
//! (`server_address`, `server_token`, `server_machine`), never in the code. The local mode is not touched: nothing here runs unless a command of the server is given.
//!
//! The requests go through [`Transport`]: [`ScLink`] is the one client of the suite (`eh3d-serverclient` of EH3D-COMMON: retries with growing pauses, a state of the link, the refusals of the
//! server by their code in eight languages, a token that is never printed), and [`SimServer`] is a server in memory that keeps the same rules for the requests (the slots, the names, the
//! stock, the bed, the dry run, the calibrations) so that everything can be tried without a network. What the real plugin does with a job on a real machine has not been tried: the
//! requests follow its documented contract and the simulated server follows the requests.

use crate::config::MachineConfig;
use crate::feeding::FeederSpec;
use crate::parts::{Part, Side};
use eh3d_serverclient as sc;
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// The most slots of feeders that a machine of the server has.
pub const MAX_SLOTS: u32 = 200;
/// The most parts a feeder may be said to hold.
pub const MAX_COUNT: u64 = 100_000;
/// The most placements of one job.
pub const MAX_PLACEMENTS: usize = 200_000;
/// The name this program gives itself on the server.
pub const APP: &str = "phoenix-pnp";
/// The most that a calibration may move a feeder, a nozzle or a camera, in millimetres on each axis (the limit of the plugin).
pub const MAX_OFFSET_MM: f64 = 5.0;

/// Which way the program works.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// Everything is done here (the default).
    #[default]
    Local,
    /// The commands of the server may send jobs to the server.
    Server,
}

impl Mode {
    pub fn parse(text: &str) -> Option<Mode> {
        match text.trim().to_ascii_lowercase().as_str() {
            "local" => Some(Mode::Local),
            "server" => Some(Mode::Server),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Mode::Local => "local",
            Mode::Server => "server",
        }
    }
}

/// One request and its answer: the status and the JSON body (`null` when the body is not JSON).
pub trait Transport {
    fn call(
        &mut self,
        method: &str,
        path: &str,
        body: Option<&Value>,
    ) -> Result<(u16, Value), String>;
}

impl Transport for Box<dyn Transport> {
    fn call(
        &mut self,
        method: &str,
        path: &str,
        body: Option<&Value>,
    ) -> Result<(u16, Value), String> {
        (**self).call(method, path, body)
    }
}

/// Where the server is and who this program is on it.
#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    pub address: String,
    pub token: String,
    pub machine: String,
    pub timeout_s: f64,
    pub poll_ms: u64,
    pub attempts: u32,
    /// How many more times the common client tries a request that failed on the network (0 to 5).
    pub retries: u32,
    /// Whether the common client may look for the server on the local network (off unless asked).
    pub mdns: bool,
}

/// Whether an id can go into a path as it is: letters, digits, `_` and `-`, up to 64.
pub fn safe_segment(s: &str) -> bool {
    (1..=64).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

impl Target {
    /// The target for these settings, or why there is none: the address and the machine have to be there, and the machine is a name that can go in a path.
    pub fn new(
        address: &str,
        token: &str,
        machine: &str,
        timeout_s: f64,
        poll_ms: u64,
        attempts: u32,
    ) -> Result<Target, String> {
        let address = address.trim().trim_end_matches('/');
        let address = address.strip_prefix("http://").unwrap_or(address);
        if address.is_empty() {
            return Err("server_address is empty: say where the server is (host:port)".into());
        }
        if address.starts_with("https://") {
            return Err("server_address is an https address: this program speaks plain HTTP (use the address of the server on the local network)".into());
        }
        if address.contains(['/', ' ', '\r', '\n']) {
            return Err("server_address is a host and a port, such as 192.168.0.180:18200".into());
        }
        let machine = machine.trim();
        if machine.is_empty() {
            return Err("server_machine is empty: say the id of the machine on the server".into());
        }
        if !safe_segment(machine) {
            return Err("server_machine has characters that cannot go into a request".into());
        }
        let token = token.trim();
        if token.contains(['\r', '\n']) {
            return Err("server_token has a line break".into());
        }
        Ok(Target {
            address: address.to_string(),
            token: token.to_string(),
            machine: machine.to_string(),
            timeout_s,
            poll_ms,
            attempts,
            retries: sc::settings::DEFAULT_RETRIES,
            mdns: false,
        })
    }

    /// The same target with the retries of the link (0 to 5) and the search on the local network.
    pub fn with_link(mut self, retries: u32, mdns: bool) -> Target {
        self.retries = retries.min(5);
        self.mdns = mdns;
        self
    }
}

/// The way to the server of the settings, or why there is none: the program works locally (`mode = local`), or the address or the machine is missing. With `sim` the target is the
/// server in memory (the machine of the settings, or `sim-1`).
pub fn target_for(cfg: &MachineConfig, sim: bool) -> Result<Target, String> {
    if sim {
        let machine = if cfg.server_machine.is_empty() {
            "sim-1"
        } else {
            cfg.server_machine.as_str()
        };
        return Target::new(
            "sim",
            "",
            machine,
            cfg.server_timeout_s,
            cfg.server_poll_ms.into(),
            cfg.server_attempts,
        )
        .map(|t| t.with_link(cfg.server_retries, false));
    }
    if cfg.mode != Mode::Server {
        return Err("mode is local in the settings: set mode = server (with server_address and server_machine) to talk to the server".into());
    }
    Target::new(
        &cfg.server_address,
        &cfg.server_token,
        &cfg.server_machine,
        cfg.server_timeout_s,
        cfg.server_poll_ms.into(),
        cfg.server_attempts,
    )
    .map(|t| t.with_link(cfg.server_retries, cfg.server_mdns))
}

/// The machine of the target, over the server in memory (`sim`) or over the link of the common client, which tries a request again by itself (so the request is not tried again here).
/// The refusals of the server are told in the language `lang` (`es`, `pt`, ...).
pub fn connect(
    target: &Target,
    sim: bool,
    lang: &str,
) -> Result<Remote<Box<dyn Transport>>, String> {
    let transport: Box<dyn Transport> = if sim {
        let mut s = SimServer::new(&[target.machine.as_str()]);
        s.steps_per_read = 50;
        Box::new(s)
    } else {
        Box::new(ScLink::new(target)?)
    };
    let mut r = Remote::new(transport, target);
    if !sim {
        r.attempts = 1;
    }
    r.speak(lang);
    Ok(r)
}

/// The settings of the common client for a target: the keys keep their ranges (the time of a try is rounded up to whole seconds, 1 to 120) and an answer is read up to 1 MiB.
pub fn link_settings(target: &Target) -> Result<sc::Settings, String> {
    fn set(s: &mut sc::Settings, key: &str, value: &str) -> Result<(), String> {
        s.set(key, value).map(|_| ()).map_err(|e| e.to_string())
    }
    let mut s = sc::Settings::default();
    set(&mut s, "server_address", &target.address)?;
    set(&mut s, "server_token", &target.token)?;
    set(&mut s, "server_machine", &target.machine)?;
    let seconds = target.timeout_s.ceil().clamp(1.0, 120.0) as u64;
    set(&mut s, "server_timeout_s", &seconds.to_string())?;
    set(&mut s, "server_retries", &target.retries.min(5).to_string())?;
    set(
        &mut s,
        "server_mdns",
        if target.mdns { "on" } else { "off" },
    )?;
    set(&mut s, "server_max_answer_mib", "1")?;
    Ok(s)
}

/// The link to the server: the common client of the suite behind the [`Transport`] of this program. The token of a program in `server_token` goes with every request; a refusal of the
/// server comes back as an answer (its status and its code), so the commands say it as they always did, and a failure of the network, after the tries of the client, is an error.
pub struct ScLink {
    client: sc::Client,
}

impl ScLink {
    /// The link to the server of the target, over plain HTTP/1.1.
    pub fn new(target: &Target) -> Result<ScLink, String> {
        Ok(ScLink {
            client: sc::Client::connect(link_settings(target)?),
        })
    }

    /// The link over a transport of the common client (a tunnel, or a double that answers for the tests).
    pub fn over(target: &Target, transport: Box<dyn sc::Transport>) -> Result<ScLink, String> {
        Ok(ScLink {
            client: sc::Client::new(link_settings(target)?, transport),
        })
    }

    /// The link over a client that was made apart (its settings, its transport and its pauses).
    pub fn with_client(client: sc::Client) -> ScLink {
        ScLink { client }
    }

    /// The state of the link: not tried, connected, disconnected or logged out.
    pub fn state(&self) -> sc::Link {
        self.client.link()
    }

    /// The counters of the link (requests, tries, retries, answers, losses and the time of the answers) as `key = value` lines; there is never a token in them.
    pub fn metrics_text(&self) -> String {
        self.client.metrics().to_text()
    }
}

/// The body of a refusal that the server gave, as a program of this suite reads it.
fn refusal_body(code: Option<&str>, detail: &str) -> Value {
    match (code, detail.is_empty()) {
        (Some(c), false) => json!({"error": c, "detail": detail}),
        (Some(c), true) => json!({"error": c}),
        (None, false) => json!({"error": detail}),
        (None, true) => Value::Null,
    }
}

impl Transport for ScLink {
    fn call(
        &mut self,
        method: &str,
        path: &str,
        body: Option<&Value>,
    ) -> Result<(u16, Value), String> {
        if !self.client.has_token() {
            return Err(
                "there is no token to send (server_token is empty, or the server refused it)"
                    .into(),
            );
        }
        let text = body.map(Value::to_string);
        match self.client.request(method, path, text.as_deref()) {
            Ok(reply) => Ok((
                reply.status,
                serde_json::from_str(&reply.body).unwrap_or(Value::Null),
            )),
            Err(sc::ClientError::Server {
                status,
                code,
                detail,
            }) => Ok((status, refusal_body(code.as_deref(), &detail))),
            Err(sc::ClientError::Unauthorized { code }) => {
                Ok((401, refusal_body(code.as_deref(), "")))
            }
            Err(sc::ClientError::Forbidden { code }) => {
                Ok((403, refusal_body(code.as_deref(), "")))
            }
            Err(e) => Err(e.text(sc::Lang::En)),
        }
    }
}

/// The status and the JSON body of an HTTP response (a body sent in chunks is put together), as the common client reads it.
pub fn parse_response(raw: &[u8]) -> Result<(u16, Value), String> {
    let reply = sc::transport::parse_response(raw).map_err(|e| e.to_string())?;
    Ok((
        reply.status,
        serde_json::from_str(&reply.body).unwrap_or(Value::Null),
    ))
}

/// What the server answers when it refuses, as one line.
pub fn error_text(status: u16, body: &Value) -> String {
    match (body["error"].as_str(), body["detail"].as_str()) {
        (Some(code), Some(detail)) if !detail.is_empty() => format!("{code}: {detail}"),
        (Some(code), _) => code.to_string(),
        _ => format!("the server answered {status}"),
    }
}

/// A name for the server: the characters it allows (letters, digits and `extra`), the others turned into `_`, at most `max` long.
pub fn label(text: &str, max: usize, extra: &str) -> String {
    let mut out: String = text
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || extra.contains(c) {
                c
            } else {
                '_'
            }
        })
        .take(max)
        .collect();
    if out.is_empty() {
        out.push('_');
    }
    out
}

/// The name that a part goes by on the server: its package and its value.
pub fn part_name(value: &str, package: &str) -> String {
    label(
        &format!("{}_{}", package.trim(), value.trim()),
        64,
        "_-./+:",
    )
}

/// A feeder as the server holds it.
#[derive(Debug, Clone, PartialEq)]
pub struct FeederRow {
    /// The id of the feeder in the table of this program.
    pub id: String,
    pub slot: u32,
    pub part: String,
    pub count: u64,
    /// What the feeder holds, as the parts find it (`value|package` in lower case).
    pub key: String,
}

/// The feeders that are switched on, each in a slot of the server (the first in slot 1, in the order of the table).
pub fn feeder_rows(feeders: &[FeederSpec]) -> Result<Vec<FeederRow>, String> {
    let mut rows = Vec::new();
    for f in feeders.iter().filter(|f| f.enabled) {
        let slot = rows.len() as u32 + 1;
        if slot > MAX_SLOTS {
            return Err(format!(
                "more than {MAX_SLOTS} feeders are switched on: a machine of the server has no more slots"
            ));
        }
        let count = u64::from(f.base.count);
        if count > MAX_COUNT {
            return Err(format!(
                "feeder {} holds {count} parts: the server takes up to {MAX_COUNT}",
                f.base.id
            ));
        }
        rows.push(FeederRow {
            id: f.base.id.clone(),
            slot,
            part: part_name(&f.base.value, &f.base.package),
            count,
            key: f.base.key(),
        });
    }
    Ok(rows)
}

/// The request that puts the table of feeders on the machine.
pub fn feeders_request(rows: &[FeederRow], replace: bool) -> Value {
    json!({
        "kind": "load_feeders",
        "replace": replace,
        "feeders": rows.iter().map(|r| json!({"slot": r.slot, "part": r.part, "count": r.count})).collect::<Vec<_>>(),
    })
}

/// What `placements` makes of a table of parts.
#[derive(Debug, Clone, PartialEq)]
pub struct Placements {
    /// The request of the job (`place`).
    pub request: Value,
    /// The references of the parts that no feeder holds: they are not in the request.
    pub unmatched: Vec<String>,
    /// How many parts are in the request.
    pub count: usize,
}

/// The request that places the parts: each part goes by the name of the feeder that holds its value and package. `offset` is where the origin of the board is on the bed.
pub fn placements(
    parts: &[Part],
    rows: &[FeederRow],
    offset: (f64, f64),
) -> Result<Placements, String> {
    let mut by_key: BTreeMap<&str, &str> = BTreeMap::new();
    for r in rows {
        by_key.entry(r.key.as_str()).or_insert(r.part.as_str());
    }
    let mut list = Vec::new();
    let mut unmatched = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for p in parts {
        let Some(name) = by_key.get(p.key().as_str()) else {
            unmatched.push(p.reference.clone());
            continue;
        };
        let reference = label(&p.reference, 32, "_-.");
        if !seen.insert(reference.clone()) {
            return Err(format!(
                "the reference {} is also {reference} for another part: the server wants every reference once",
                p.reference
            ));
        }
        list.push(json!({
            "ref": reference,
            "part": name,
            "x": p.x,
            "y": p.y,
            "rot": p.rotation,
            "side": if p.side == Side::Top { "top" } else { "bottom" },
        }));
    }
    if list.len() > MAX_PLACEMENTS {
        return Err(format!(
            "{} placements: a job of the server has up to {MAX_PLACEMENTS}",
            list.len()
        ));
    }
    let count = list.len();
    let mut request = json!({"kind": "place", "placements": list});
    if offset != (0.0, 0.0) {
        request["offset"] = json!({"x": offset.0, "y": offset.1});
    }
    Ok(Placements {
        request,
        unmatched,
        count,
    })
}

/// What the server says of a job.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct JobInfo {
    pub id: String,
    /// `queued`, `running`, `paused`, `done`, `failed` or `cancelled`.
    pub state: String,
    pub done: u64,
    pub total: u64,
    pub message: String,
}

impl JobInfo {
    pub fn from_json(v: &Value) -> JobInfo {
        JobInfo {
            id: v["id"].as_str().unwrap_or("").to_string(),
            state: v["state"].as_str().unwrap_or("").to_string(),
            done: v["done"].as_u64().unwrap_or(0),
            total: v["total"].as_u64().unwrap_or(0),
            message: v["message"].as_str().unwrap_or("").to_string(),
        }
    }

    pub fn finished(&self) -> bool {
        matches!(self.state.as_str(), "done" | "failed" | "cancelled")
    }

    pub fn percent(&self) -> u64 {
        (self.done * 100)
            .checked_div(self.total)
            .map_or(0, |p| p.min(100))
    }

    /// One line for a person.
    pub fn line(&self) -> String {
        let mut out = format!(
            "{}: {}, {} of {} ({}%)",
            self.id,
            self.state,
            self.done,
            self.total,
            self.percent()
        );
        if !self.message.is_empty() {
            out.push_str(&format!(" - {}", self.message));
        }
        out
    }
}

/// What the operator can tell a job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Order {
    Start,
    Pause,
    Resume,
    Cancel,
}

impl Order {
    fn word(self) -> &'static str {
        match self {
            Order::Start => "start",
            Order::Pause => "pause",
            Order::Resume => "resume",
            Order::Cancel => "cancel",
        }
    }
}

/// A machine of the server as `server-status` shows it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MachineInfo {
    pub id: String,
    pub plugin: String,
    pub state: String,
    /// The feeders the server holds: `(slot, part, count)`.
    pub feeders: Vec<FeederLine>,
}

impl MachineInfo {
    pub fn from_json(v: &Value) -> MachineInfo {
        let feeders = v["detail"]["feeders"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|f| {
                        (
                            f["slot"].as_u64().unwrap_or(0) as u32,
                            f["part"].as_str().unwrap_or("").to_string(),
                            f["count"].as_u64().unwrap_or(0),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        MachineInfo {
            id: v["id"].as_str().unwrap_or("").to_string(),
            plugin: v["plugin"].as_str().unwrap_or("").to_string(),
            state: v["state"].as_str().unwrap_or("").to_string(),
            feeders,
        }
    }
}

/// One feeder as the plugin lists it with `GET .../feeders`.
#[derive(Debug, Clone, PartialEq)]
pub struct FeederState {
    pub slot: u32,
    pub part: String,
    pub count: u64,
    pub empty: bool,
    /// Where the head picks from, with the calibration of the feeder counted in.
    pub x_mm: f64,
    pub y_mm: f64,
    /// The calibration of the feeder.
    pub offset: (f64, f64),
}

/// The feeders of the machine on the server: the slots that hold a part and the free ones.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FeederTable {
    pub slots: u32,
    pub feeders: Vec<FeederState>,
    pub free: Vec<u32>,
}

impl FeederTable {
    pub fn from_json(v: &Value) -> FeederTable {
        let number = |f: &Value, k: &str| f[k].as_f64().unwrap_or(0.0);
        FeederTable {
            slots: v["slots"].as_u64().unwrap_or(0) as u32,
            feeders: v["feeders"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|f| FeederState {
                            slot: f["slot"].as_u64().unwrap_or(0) as u32,
                            part: f["part"].as_str().unwrap_or("").to_string(),
                            count: f["count"].as_u64().unwrap_or(0),
                            empty: f["empty"].as_bool().unwrap_or(false),
                            x_mm: number(f, "x_mm"),
                            y_mm: number(f, "y_mm"),
                            offset: (number(&f["offset_mm"], "x"), number(&f["offset_mm"], "y")),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            free: v["free"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_u64)
                        .map(|n| n as u32)
                        .collect()
                })
                .unwrap_or_default(),
        }
    }

    /// How many feeders hold no parts.
    pub fn empty_count(&self) -> usize {
        self.feeders
            .iter()
            .filter(|f| f.empty || f.count == 0)
            .count()
    }

    /// The table for a person: a line that sums it up and a line for each feeder.
    pub fn text(&self) -> String {
        let mut out = format!(
            "{} of {} slots hold a part, {} are free, {} are empty\n",
            self.feeders.len(),
            self.slots,
            self.free.len(),
            self.empty_count()
        );
        for f in &self.feeders {
            out.push_str(&format!(
                "slot {}: {}, {} left{}, picked at {:.2},{:.2} mm",
                f.slot,
                f.part,
                f.count,
                if f.count == 0 { " (empty)" } else { "" },
                f.x_mm,
                f.y_mm
            ));
            if f.offset != (0.0, 0.0) {
                out.push_str(&format!(
                    ", calibrated by {:.3},{:.3} mm",
                    f.offset.0, f.offset.1
                ));
            }
            out.push('\n');
        }
        out
    }

    /// The table as `slot,part,count,x_mm,y_mm,offset_x_mm,offset_y_mm`.
    pub fn csv(&self) -> String {
        let mut out = String::from("slot,part,count,x_mm,y_mm,offset_x_mm,offset_y_mm\n");
        for f in &self.feeders {
            out.push_str(&format!(
                "{},{},{},{:.3},{:.3},{:.3},{:.3}\n",
                f.slot,
                crate::parts::csv_field(&f.part),
                f.count,
                f.x_mm,
                f.y_mm,
                f.offset.0,
                f.offset.1
            ));
        }
        out
    }
}

/// What a calibration of the plugin is of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Calibrate {
    Feeder,
    Nozzle,
    Camera,
}

impl Calibrate {
    pub fn parse(text: &str) -> Option<Calibrate> {
        match text.trim().to_ascii_lowercase().as_str() {
            "feeder" => Some(Calibrate::Feeder),
            "nozzle" => Some(Calibrate::Nozzle),
            "camera" => Some(Calibrate::Camera),
            _ => None,
        }
    }

    pub fn word(self) -> &'static str {
        match self {
            Calibrate::Feeder => "feeder",
            Calibrate::Nozzle => "nozzle",
            Calibrate::Camera => "camera",
        }
    }
}

/// The request of a calibration job: the target of a feeder is its slot (1 to 200) and the one of a nozzle or a camera is a name; the offset (up to 5 mm on each axis) is the one the request
/// brings, because the simulated machine of the server measures nothing. A request that the plugin would refuse is refused here, with the reason.
pub fn calibrate_request(
    what: Calibrate,
    target: &str,
    offset: (f64, f64),
) -> Result<Value, String> {
    let target = target.trim();
    if what == Calibrate::Feeder {
        let slot = target
            .parse::<u32>()
            .ok()
            .filter(|s| (1..=MAX_SLOTS).contains(s));
        if slot.is_none() {
            return Err(format!(
                "the target of a feeder is its slot, 1 to {MAX_SLOTS}"
            ));
        }
    } else if !label_ok(target, 64, "_-./+:") {
        return Err("the target is a name of up to 64 letters, digits and _-./+:".into());
    }
    for (axis, v) in [("x", offset.0), ("y", offset.1)] {
        if !v.is_finite() || v.abs() > MAX_OFFSET_MM {
            return Err(format!(
                "the offset on {axis} is a number from -{MAX_OFFSET_MM} to {MAX_OFFSET_MM} mm"
            ));
        }
    }
    Ok(json!({
        "kind": "calibrate", "what": what.word(), "target": target,
        "offset_mm": {"x": offset.0, "y": offset.1},
    }))
}

/// The `place` request as a dry run: it is planned and checked like any other, and the head of the simulated machine goes through it without taking a part off a feeder.
pub fn dry_run(mut request: Value) -> Value {
    if request.is_object() {
        request["dry_run"] = Value::Bool(true);
    }
    request
}

/// The body of a request that declares or refills one slot at once (`PUT .../feeders`).
pub fn feeder_body(slot: u32, part: &str, count: u64) -> Result<Value, String> {
    if !(1..=MAX_SLOTS).contains(&slot) {
        return Err(format!("the slot is a whole number from 1 to {MAX_SLOTS}"));
    }
    if !label_ok(part, 64, "_-./+:") {
        return Err("the part is a name of up to 64 letters, digits and _-./+:".into());
    }
    if count > MAX_COUNT {
        return Err(format!("the count is a whole number from 0 to {MAX_COUNT}"));
    }
    Ok(json!({"slot": slot, "part": part, "count": count}))
}

/// The body of a request that empties one slot (`PUT .../feeders`).
pub fn clear_body(slot: u32) -> Result<Value, String> {
    if !(1..=MAX_SLOTS).contains(&slot) {
        return Err(format!("the slot is a whole number from 1 to {MAX_SLOTS}"));
    }
    Ok(json!({"slot": slot, "clear": true}))
}

/// The machine of the server and the way to it.
pub struct Remote<T: Transport> {
    pub transport: T,
    pub machine: String,
    /// How many times a request that did not get an answer is tried (1 to 10). The link of the common client tries again by itself, so the commands set 1 for it.
    pub attempts: u32,
    /// The language in which the refusals of the server are told when the table of the common client has their code.
    pub lang: sc::Lang,
}

impl<T: Transport> Remote<T> {
    pub fn new(transport: T, target: &Target) -> Remote<T> {
        Remote {
            transport,
            machine: target.machine.clone(),
            attempts: target.attempts.clamp(1, 10),
            lang: sc::Lang::En,
        }
    }

    /// Tells the refusals in the language of a code (`es`, `pt`, ...); a language the suite does not have leaves English.
    pub fn speak(&mut self, code: &str) {
        self.lang = sc::Lang::from_tag(code).unwrap_or(sc::Lang::En);
    }

    /// What a refusal says: the code and the detail of the server and, when the language is not English and the code is one the server has, the text of the code in that language first.
    pub fn refusal(&self, status: u16, body: &Value) -> String {
        let line = error_text(status, body);
        match body["error"].as_str() {
            Some(code) if self.lang != sc::Lang::En && sc::lookup(code).is_some() => {
                format!("{} [{line}]", sc::text_for(code, self.lang))
            }
            _ => line,
        }
    }

    /// One request. A request that gets no answer is tried again (up to `attempts` times in all); an answer, whatever it says, is final. Only an answer of 200 or 201 is a success.
    fn ok(&mut self, method: &str, path: &str, body: Option<&Value>) -> Result<Value, String> {
        let mut last = String::new();
        for _ in 0..self.attempts.max(1) {
            match self.transport.call(method, path, body) {
                Ok((200 | 201, v)) => return Ok(v),
                Ok((status, v)) => return Err(self.refusal(status, &v)),
                Err(e) => last = e,
            }
        }
        Err(last)
    }

    /// The machines of the server.
    pub fn machines(&mut self) -> Result<Vec<MachineInfo>, String> {
        let v = self.ok("GET", "/api/v1/work/machines", None)?;
        Ok(v["machines"]
            .as_array()
            .map(|a| a.iter().map(MachineInfo::from_json).collect())
            .unwrap_or_default())
    }

    /// The machine of the settings, as the server lists it.
    pub fn machine(&mut self) -> Result<MachineInfo, String> {
        let id = self.machine.clone();
        self.machines()?
            .into_iter()
            .find(|m| m.id == id)
            .ok_or_else(|| format!("the server has no machine {id}"))
    }

    /// Makes a job, starts it and reads it until it is over or `max_polls` readings were got; `wait` is called between two readings. Gives the last reading.
    pub fn run_job(
        &mut self,
        name: &str,
        request: &Value,
        max_polls: u32,
        wait: &mut dyn FnMut(),
    ) -> Result<JobInfo, String> {
        let job = self.create(name, request)?;
        self.order(&job.id, Order::Start)?;
        self.follow(&job.id, max_polls, &mut |_| {}, wait)
    }

    fn feeders_path(&self) -> String {
        format!("/api/v1/work/machines/{}/feeders", self.machine)
    }

    /// The feeders the machine holds, with where each is picked from and its calibration (`GET .../feeders`).
    pub fn feeders_table(&mut self) -> Result<FeederTable, String> {
        let path = self.feeders_path();
        Ok(FeederTable::from_json(&self.ok("GET", &path, None)?))
    }

    /// Declares or refills one slot at once, with no job (`PUT .../feeders`); the machine must not be running a job. The answer is the table after the change.
    pub fn set_feeder(&mut self, slot: u32, part: &str, count: u64) -> Result<FeederTable, String> {
        let body = feeder_body(slot, part, count)?;
        let path = self.feeders_path();
        Ok(FeederTable::from_json(&self.ok(
            "PUT",
            &path,
            Some(&body),
        )?))
    }

    /// Empties one slot at once.
    pub fn clear_feeder(&mut self, slot: u32) -> Result<FeederTable, String> {
        let body = clear_body(slot)?;
        let path = self.feeders_path();
        Ok(FeederTable::from_json(&self.ok(
            "PUT",
            &path,
            Some(&body),
        )?))
    }

    /// Prepares a job on the machine (it waits, queued, until it is started).
    pub fn create(&mut self, name: &str, request: &Value) -> Result<JobInfo, String> {
        let body = json!({"machine": self.machine, "request": request, "name": name, "app": APP});
        Ok(JobInfo::from_json(&self.ok(
            "POST",
            "/api/v1/work/jobs",
            Some(&body),
        )?))
    }

    pub fn job(&mut self, id: &str) -> Result<JobInfo, String> {
        let id = checked_id(id)?;
        Ok(JobInfo::from_json(&self.ok(
            "GET",
            &format!("/api/v1/work/jobs/{id}"),
            None,
        )?))
    }

    pub fn order(&mut self, id: &str, what: Order) -> Result<JobInfo, String> {
        let id = checked_id(id)?;
        let path = format!("/api/v1/work/jobs/{id}/{}", what.word());
        Ok(JobInfo::from_json(&self.ok("POST", &path, None)?))
    }

    /// Asks for the job until it is over or `max_polls` answers were got; `seen` gets every answer and `wait` is called between two. Returns the last answer.
    pub fn follow(
        &mut self,
        id: &str,
        max_polls: u32,
        seen: &mut dyn FnMut(&JobInfo),
        wait: &mut dyn FnMut(),
    ) -> Result<JobInfo, String> {
        let mut last = self.job(id)?;
        seen(&last);
        let mut polls = 1;
        while !last.finished() && polls < max_polls {
            wait();
            last = self.job(id)?;
            seen(&last);
            polls += 1;
        }
        Ok(last)
    }
}

fn checked_id(id: &str) -> Result<&str, String> {
    if safe_segment(id) {
        Ok(id)
    } else {
        Err("the id of the job has characters that cannot go into a request".into())
    }
}

/// A feeder of a `load_feeders` request: the slot, the part and how many.
type FeederLine = (u32, String, u64);
/// The feeders of a request and whether they replace the table.
type Loading = (Vec<FeederLine>, bool);
/// A placement of a request: the reference and the part.
type Placed = (String, String);
/// A refusal: the status, the code and what is wrong.
type Refusal = (u16, &'static str, String);

/// A calibration of a request: what, of which target, the slot when it is a feeder, and the offset.
type Calibration = (String, String, Option<u32>, (f64, f64));

#[derive(Debug, Clone)]
enum Work {
    Load {
        feeders: Vec<FeederLine>,
        replace: bool,
    },
    Place {
        placements: Vec<Placed>,
        dry: bool,
    },
    Calibrate(Calibration),
}

#[derive(Debug, Clone)]
struct SimJob {
    id: String,
    machine: String,
    name: String,
    state: &'static str,
    done: u64,
    total: u64,
    message: String,
    work: Work,
}

impl SimJob {
    fn view(&self) -> Value {
        json!({
            "id": self.id, "machine": self.machine, "plugin": "pnp", "name": self.name,
            "state": self.state, "done": self.done, "total": self.total,
            "percent": JobInfo::from_json(&json!({"done": self.done, "total": self.total})).percent(),
            "message": self.message,
            "dry_run": matches!(self.work, Work::Place { dry: true, .. }),
        })
    }
}

/// A server in memory with `pnp` machines: it checks the requests with the rules of the plugin (slots from 1 to 200, names of up to 64 letters, digits and `_-./+:`, counts up to 100000,
/// a reference once, the stock of the parts) and carries a placement job out, one part for each time the job is read while it runs.
pub struct SimServer {
    machines: BTreeMap<String, BTreeMap<u32, (String, u64)>>,
    /// The calibration of each feeder of each machine, and the calibrations that were made (`what:target`).
    offsets: BTreeMap<String, BTreeMap<u32, (f64, f64)>>,
    calibrations: BTreeMap<String, Vec<String>>,
    jobs: Vec<SimJob>,
    next: u64,
    /// How many placements a running job makes each time it is read (1 by default).
    pub steps_per_read: u64,
    /// The requests that came in, `METHOD path`, in order.
    pub log: Vec<String>,
    refuse: Option<(u16, Value)>,
    drop_calls: u32,
}

impl SimServer {
    pub fn new(machine_ids: &[&str]) -> SimServer {
        SimServer {
            machines: machine_ids
                .iter()
                .map(|m| ((*m).to_string(), BTreeMap::new()))
                .collect(),
            offsets: BTreeMap::new(),
            calibrations: BTreeMap::new(),
            jobs: Vec::new(),
            next: 1,
            steps_per_read: 1,
            log: Vec::new(),
            refuse: None,
            drop_calls: 0,
        }
    }

    /// The next request is answered with this status and body (a refusal such as 401).
    pub fn refuse_next(&mut self, status: u16, body: Value) {
        self.refuse = Some((status, body));
    }

    /// The next `n` requests get no answer (the network failed).
    pub fn drop_next(&mut self, n: u32) {
        self.drop_calls = n;
    }

    /// What a feeder of a machine holds: the part and how many are left.
    pub fn feeder(&self, machine: &str, slot: u32) -> Option<(String, u64)> {
        self.machines.get(machine)?.get(&slot).cloned()
    }

    /// The calibration of a feeder of a machine, when it has one.
    pub fn feeder_offset(&self, machine: &str, slot: u32) -> Option<(f64, f64)> {
        self.offsets.get(machine)?.get(&slot).copied()
    }

    /// The calibrations a machine has been through, as `what:target`, in order.
    pub fn calibrations(&self, machine: &str) -> Vec<String> {
        self.calibrations.get(machine).cloned().unwrap_or_default()
    }

    fn feeders_view(&self, machine: &str) -> Value {
        let held = &self.machines[machine];
        let offsets = self.offsets.get(machine);
        let mut feeders = Vec::new();
        let mut free = Vec::new();
        for slot in 1..=MAX_SLOTS {
            let (dx, dy) = offsets
                .and_then(|o| o.get(&slot))
                .copied()
                .unwrap_or((0.0, 0.0));
            match held.get(&slot) {
                Some((part, count)) => feeders.push(json!({
                    "slot": slot, "part": part, "count": count, "empty": *count == 0,
                    "x_mm": 20.0 + 10.0 * f64::from(slot - 1) + dx, "y_mm": -30.0 + dy,
                    "offset_mm": {"x": dx, "y": dy},
                })),
                None => free.push(slot),
            }
        }
        json!({"slots": MAX_SLOTS, "feeders": feeders, "free": free})
    }

    fn set_feeder(&mut self, machine: &str, body: &Value) -> Result<(u16, Value), String> {
        if self.busy(machine) {
            return Self::error(409, "machine_busy", machine);
        }
        let Some(slot) = body["slot"]
            .as_u64()
            .filter(|s| (1..=u64::from(MAX_SLOTS)).contains(s))
        else {
            return Self::error(
                422,
                "bad_plugin_request",
                &format!("slot is a whole number from 1 to {MAX_SLOTS}"),
            );
        };
        let slot = slot as u32;
        match body.get("clear") {
            None => match parse_load(&json!({"feeders": [body]})) {
                Ok((list, _)) => {
                    let (_, part, count) = list.into_iter().next().expect("one feeder");
                    if let Some(held) = self.machines.get_mut(machine) {
                        held.insert(slot, (part, count));
                    }
                }
                Err(why) => return Self::error(422, "bad_plugin_request", &why),
            },
            Some(Value::Bool(true)) => {
                if let Some(held) = self.machines.get_mut(machine) {
                    held.remove(&slot);
                }
            }
            Some(_) => return Self::error(422, "bad_plugin_request", "clear is true"),
        }
        Ok((200, self.feeders_view(machine)))
    }

    fn parse_calibrate(request: &Value) -> Result<Calibration, String> {
        let what = match request["what"].as_str() {
            Some(w @ ("feeder" | "nozzle" | "camera")) => w,
            _ => return Err("what is `feeder`, `nozzle` or `camera`".into()),
        };
        let target = match &request["target"] {
            Value::String(t) => t.clone(),
            Value::Number(n) => n.to_string(),
            _ => {
                return Err(
                    "target is the slot of the feeder or the name of the nozzle or the camera"
                        .into(),
                )
            }
        };
        let slot = if what == "feeder" {
            let slot = target
                .parse::<u32>()
                .ok()
                .filter(|s| (1..=MAX_SLOTS).contains(s));
            if slot.is_none() {
                return Err(format!(
                    "the target of a feeder is its slot, 1 to {MAX_SLOTS}"
                ));
            }
            slot
        } else if label_ok(&target, 64, "_-./+:") {
            None
        } else {
            return Err("target is a name of up to 64 letters, digits and _-./+:".into());
        };
        if request.get("offset_mm").is_some_and(|o| !o.is_object()) {
            return Err("offset_mm is {\"x\": .., \"y\": ..}".into());
        }
        let axis = |key: &str| -> Result<f64, String> {
            match request["offset_mm"].get(key) {
                None => Ok(0.0),
                Some(v) => match v.as_f64() {
                    Some(n) if n.is_finite() && n.abs() <= MAX_OFFSET_MM => Ok(n),
                    _ => Err(format!(
                        "offset_mm.{key} is a number from -{MAX_OFFSET_MM} to {MAX_OFFSET_MM}"
                    )),
                },
            }
        };
        Ok((what.to_string(), target, slot, (axis("x")?, axis("y")?)))
    }

    fn error(status: u16, code: &str, detail: &str) -> Result<(u16, Value), String> {
        Ok((status, json!({"error": code, "detail": detail})))
    }

    fn busy(&self, machine: &str) -> bool {
        self.jobs
            .iter()
            .any(|j| j.machine == machine && matches!(j.state, "running" | "paused"))
    }

    fn machine_view(&self, id: &str) -> Value {
        let feeders: Vec<Value> = self.machines[id]
            .iter()
            .map(|(slot, (part, count))| json!({"slot": slot, "part": part, "count": count}))
            .collect();
        json!({
            "id": id, "name": id, "plugin": "pnp",
            "state": if self.busy(id) { "busy" } else { "idle" },
            "detail": {"feeders": feeders, "calibrations": self.calibrations.get(id).cloned().unwrap_or_default()},
        })
    }

    fn create(&mut self, body: &Value) -> Result<(u16, Value), String> {
        let machine = body["machine"].as_str().unwrap_or("");
        if !self.machines.contains_key(machine) {
            return Self::error(404, "unknown_machine", machine);
        }
        let request = &body["request"];
        let work = match request["kind"].as_str() {
            Some("load_feeders") => match parse_load(request) {
                Ok((feeders, replace)) => Work::Load { feeders, replace },
                Err(why) => return Self::error(400, "bad_request", &why),
            },
            Some("calibrate") => match Self::parse_calibrate(request) {
                Ok(c) => Work::Calibrate(c),
                Err(why) => return Self::error(400, "bad_request", &why),
            },
            Some("place") => {
                let dry = match (
                    request.get("dry_run"),
                    request.get("options").and_then(|o| o.get("dry_run")),
                ) {
                    (None, None) => false,
                    (Some(Value::Bool(b)), None) | (None, Some(Value::Bool(b))) => *b,
                    (Some(Value::Bool(a)), Some(Value::Bool(b))) => *a || *b,
                    _ => return Self::error(400, "bad_request", "dry_run is true or false"),
                };
                match self.parse_place(machine, request) {
                    Ok(p) => Work::Place {
                        placements: p,
                        dry,
                    },
                    Err((status, code, why)) => return Self::error(status, code, &why),
                }
            }
            _ => {
                return Self::error(
                    400,
                    "bad_request",
                    "the request has to be {\"kind\": \"place\" | \"load_feeders\" | \"calibrate\", ...}",
                )
            }
        };
        let total = match &work {
            Work::Load { feeders, .. } => feeders.len() as u64,
            Work::Place { placements, .. } => placements.len() as u64,
            Work::Calibrate(_) => 1,
        };
        let id = format!("w-{:04}", self.next);
        self.next += 1;
        let job = SimJob {
            id,
            machine: machine.to_string(),
            name: body["name"].as_str().unwrap_or("").to_string(),
            state: "queued",
            done: 0,
            total,
            message: String::new(),
            work,
        };
        let view = job.view();
        self.jobs.push(job);
        Ok((201, view))
    }

    fn parse_place(&self, machine: &str, request: &Value) -> Result<Vec<Placed>, Refusal> {
        let bad = |m: String| (400, "bad_request", m);
        let list = request["placements"]
            .as_array()
            .ok_or_else(|| bad("a place request has the list `placements`".into()))?;
        if list.is_empty() {
            return Err(bad("the job has no placements".into()));
        }
        if list.len() > MAX_PLACEMENTS {
            return Err((400, "too_big", "too many placements".into()));
        }
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        let mut need: BTreeMap<String, u64> = BTreeMap::new();
        for (n, item) in list.iter().enumerate() {
            let row = n + 1;
            let reference = item["ref"]
                .as_str()
                .filter(|r| label_ok(r, 32, "_-."))
                .ok_or_else(|| {
                    bad(format!(
                        "placement {row}: ref is a name of up to 32 letters, digits and _-."
                    ))
                })?;
            if !seen.insert(reference.to_string()) {
                return Err(bad(format!(
                    "placement {row}: the reference {reference} is in the list twice"
                )));
            }
            let part = item["part"]
                .as_str()
                .filter(|p| label_ok(p, 64, "_-./+:"))
                .ok_or_else(|| {
                    bad(format!(
                        "placement {row}: part is a name of up to 64 letters, digits and _-./+:"
                    ))
                })?;
            let finite = |k: &str| item[k].as_f64().filter(|v| v.is_finite());
            if finite("x").is_none() || finite("y").is_none() {
                return Err(bad(format!("placement {row}: x and y are numbers")));
            }
            if let Some(r) = item.get("rot") {
                if !r.as_f64().is_some_and(|r| (-360.0..=360.0).contains(&r)) {
                    return Err(bad(format!(
                        "placement {row}: rot is from -360 to 360 degrees"
                    )));
                }
            }
            if let Some(s) = item.get("side") {
                if !matches!(s.as_str(), Some("top" | "bottom")) {
                    return Err(bad(format!("placement {row}: side is top or bottom")));
                }
            }
            *need.entry(part.to_string()).or_insert(0) += 1;
            out.push((reference.to_string(), part.to_string()));
        }
        let held = &self.machines[machine];
        for (part, n) in &need {
            let have: u64 = held
                .values()
                .filter(|(p, _)| p == part)
                .map(|(_, c)| *c)
                .sum();
            if have < *n {
                return Err((
                    409,
                    "pnp_feeder_short",
                    format!("{part}: the job needs {n} and the feeders hold {have}"),
                ));
            }
        }
        Ok(out)
    }

    fn find(&mut self, id: &str) -> Option<usize> {
        self.jobs.iter().position(|j| j.id == id)
    }

    fn start(&mut self, at: usize) -> (u16, Value) {
        let machine = self.jobs[at].machine.clone();
        if self.busy(&machine) {
            return (409, json!({"error": "machine_busy"}));
        }
        if self.jobs[at].state != "queued" {
            return (409, json!({"error": "not_in_state"}));
        }
        let held = self.machines.get_mut(&machine).expect("machine of a job");
        match self.jobs[at].work.clone() {
            Work::Load { feeders, replace } => {
                if replace {
                    held.clear();
                }
                for (slot, part, count) in feeders {
                    held.insert(slot, (part, count));
                }
                self.jobs[at].done = self.jobs[at].total;
                self.jobs[at].state = "done";
            }
            Work::Calibrate((what, target, slot, offset)) => {
                if let Some(slot) = slot {
                    self.offsets
                        .entry(machine.clone())
                        .or_default()
                        .insert(slot, offset);
                }
                self.calibrations
                    .entry(machine.clone())
                    .or_default()
                    .push(format!("{what}:{target}"));
                self.jobs[at].done = self.jobs[at].total;
                self.jobs[at].state = "done";
            }
            Work::Place { .. } => self.jobs[at].state = "running",
        }
        (200, self.jobs[at].view())
    }

    /// A running job places `steps_per_read` parts.
    fn advance(&mut self, at: usize) {
        if self.jobs[at].state != "running" {
            return;
        }
        let machine = self.jobs[at].machine.clone();
        let Work::Place { placements, dry } = self.jobs[at].work.clone() else {
            return;
        };
        let mut steps = self.steps_per_read.max(1);
        while steps > 0 && self.jobs[at].done < self.jobs[at].total {
            let (reference, part) = &placements[self.jobs[at].done as usize];
            if dry {
                self.jobs[at].done += 1;
                steps -= 1;
                continue;
            }
            let held = self.machines.get_mut(&machine).expect("machine of a job");
            let slot = held
                .iter()
                .find(|(_, (p, c))| p == part && *c > 0)
                .map(|(s, _)| *s);
            match slot {
                Some(s) => {
                    if let Some(f) = held.get_mut(&s) {
                        f.1 -= 1;
                    }
                    self.jobs[at].done += 1;
                }
                None => {
                    self.jobs[at].state = "failed";
                    self.jobs[at].message = format!("{reference}: the feeder of {part} is empty");
                    return;
                }
            }
            steps -= 1;
        }
        if self.jobs[at].done >= self.jobs[at].total {
            self.jobs[at].state = "done";
        }
    }
}

impl Transport for SimServer {
    fn call(
        &mut self,
        method: &str,
        path: &str,
        body: Option<&Value>,
    ) -> Result<(u16, Value), String> {
        self.log.push(format!("{method} {path}"));
        if self.drop_calls > 0 {
            self.drop_calls -= 1;
            return Err("the connection was lost".into());
        }
        if let Some(r) = self.refuse.take() {
            return Ok(r);
        }
        let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
        match (method, parts.as_slice()) {
            ("GET", ["api", "v1", "work", "machines"]) => {
                let list: Vec<Value> = self.machines.keys().map(|m| self.machine_view(m)).collect();
                Ok((200, json!({"machines": list})))
            }
            ("GET", ["api", "v1", "work", "machines", m, "feeders"]) => {
                if self.machines.contains_key(*m) {
                    Ok((200, self.feeders_view(m)))
                } else {
                    Self::error(404, "unknown_machine", m)
                }
            }
            ("PUT", ["api", "v1", "work", "machines", m, "feeders"]) => {
                if !self.machines.contains_key(*m) {
                    return Self::error(404, "unknown_machine", m);
                }
                match body {
                    Some(b) => self.set_feeder(m, b),
                    None => Self::error(422, "bad_plugin_request", "a body is needed"),
                }
            }
            ("POST", ["api", "v1", "work", "jobs"]) => match body {
                Some(b) => self.create(b),
                None => Self::error(400, "bad_request", "a body is needed"),
            },
            ("GET", ["api", "v1", "work", "jobs", id]) => match self.find(id) {
                Some(at) => {
                    self.advance(at);
                    Ok((200, self.jobs[at].view()))
                }
                None => Self::error(404, "unknown_job", id),
            },
            ("POST", ["api", "v1", "work", "jobs", id, what]) => {
                let Some(at) = self.find(id) else {
                    return Self::error(404, "unknown_job", id);
                };
                match *what {
                    "start" => Ok(self.start(at)),
                    "pause" if self.jobs[at].state == "running" => {
                        self.jobs[at].state = "paused";
                        Ok((200, self.jobs[at].view()))
                    }
                    "resume" if self.jobs[at].state == "paused" => {
                        self.jobs[at].state = "running";
                        Ok((200, self.jobs[at].view()))
                    }
                    "cancel" if matches!(self.jobs[at].state, "queued" | "running" | "paused") => {
                        self.jobs[at].state = "cancelled";
                        Ok((200, self.jobs[at].view()))
                    }
                    "pause" | "resume" | "cancel" => Self::error(409, "not_in_state", what),
                    _ => Self::error(404, "not_found", path),
                }
            }
            _ => Self::error(404, "not_found", path),
        }
    }
}

fn label_ok(text: &str, max: usize, extra: &str) -> bool {
    !text.is_empty()
        && text.chars().count() <= max
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || extra.contains(c))
}

fn parse_load(request: &Value) -> Result<Loading, String> {
    let replace = match request.get("replace") {
        None => false,
        Some(Value::Bool(b)) => *b,
        Some(_) => return Err("replace is true or false".into()),
    };
    let list = request["feeders"]
        .as_array()
        .ok_or("a load_feeders request has the list `feeders`")?;
    if list.is_empty() && !replace {
        return Err("the list of feeders is empty: say replace: true to empty the table".into());
    }
    let mut out: Vec<FeederLine> = Vec::new();
    for (n, item) in list.iter().enumerate() {
        let slot = item["slot"]
            .as_u64()
            .filter(|s| (1..=u64::from(MAX_SLOTS)).contains(s))
            .ok_or_else(|| {
                format!(
                    "feeder {}: slot is a whole number from 1 to {MAX_SLOTS}",
                    n + 1
                )
            })? as u32;
        if out.iter().any(|(s, _, _)| *s == slot) {
            return Err(format!("slot {slot} is in the list twice"));
        }
        let part = item["part"]
            .as_str()
            .filter(|p| label_ok(p, 64, "_-./+:"))
            .ok_or_else(|| {
                format!(
                    "feeder {}: part is a name of up to 64 letters, digits and _-./+:",
                    n + 1
                )
            })?;
        let count = item["count"]
            .as_u64()
            .filter(|c| *c <= MAX_COUNT)
            .ok_or_else(|| {
                format!(
                    "feeder {}: count is a whole number from 0 to {MAX_COUNT}",
                    n + 1
                )
            })?;
        out.push((slot, part.to_string(), count));
    }
    Ok((out, replace))
}
