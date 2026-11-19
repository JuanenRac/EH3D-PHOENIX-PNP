// EH3D-PHOENIX-PNP - the way to the EH3D server.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! The server of the suite runs machines for the programs that talk to it, and the `pnp` plugin of the server is the pick-and-place machine behind it. With `mode = server` in the settings
//! this program sends its table of feeders and its placements there as jobs (the same `/api/v1/work` routes that the other programs of the suite use), starts them, follows them and
//! holds or stops them; the machine, its feeders and the head are the server's. Where the server is, the token of this program and the id of the machine are keys of the settings
//! (`server_address`, `server_token`, `server_machine`), never in the code. The local mode is not touched: nothing here runs unless a command of the server is given.
//!
//! The requests go through [`Transport`]: [`HttpTransport`] is plain HTTP/1.1, one request for each connection, and [`SimServer`] is a server in memory that keeps the same rules for the
//! requests (the slots, the names, the stock, the bed) so that everything can be tried without a network. What the real plugin does with a job on a real machine has not been tried: the
//! requests follow its documented contract and the simulated server follows the requests.

use crate::feeding::FeederSpec;
use crate::parts::{Part, Side};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

/// The most slots of feeders that a machine of the server has.
pub const MAX_SLOTS: u32 = 200;
/// The most parts a feeder may be said to hold.
pub const MAX_COUNT: u64 = 100_000;
/// The most placements of one job.
pub const MAX_PLACEMENTS: usize = 200_000;
/// The name this program gives itself on the server.
pub const APP: &str = "phoenix-pnp";
/// The most that the answer of a server may have: a reply above it is an error, not a reason to grow in memory.
const MAX_ANSWER_BYTES: u64 = 1 << 20;

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
        })
    }
}

/// Plain HTTP/1.1, one request for each connection.
pub struct HttpTransport {
    address: String,
    token: String,
    timeout: Duration,
}

impl HttpTransport {
    pub fn new(target: &Target) -> HttpTransport {
        HttpTransport {
            address: target.address.clone(),
            token: target.token.clone(),
            timeout: Duration::from_secs_f64(target.timeout_s.clamp(0.5, 600.0)),
        }
    }
}

impl Transport for HttpTransport {
    fn call(
        &mut self,
        method: &str,
        path: &str,
        body: Option<&Value>,
    ) -> Result<(u16, Value), String> {
        let found = self
            .address
            .to_socket_addrs()
            .map_err(|e| format!("{}: {e}", self.address))?
            .next()
            .ok_or_else(|| format!("{}: no address", self.address))?;
        let mut stream = TcpStream::connect_timeout(&found, self.timeout)
            .map_err(|e| format!("{}: {e}", self.address))?;
        stream
            .set_read_timeout(Some(self.timeout))
            .and_then(|()| stream.set_write_timeout(Some(self.timeout)))
            .map_err(|e| e.to_string())?;
        let payload = body.map(Value::to_string).unwrap_or_default();
        let mut request = format!(
            "{method} {path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nAccept: application/json\r\n",
            self.address
        );
        if !self.token.is_empty() {
            request.push_str(&format!("Authorization: Bearer {}\r\n", self.token));
        }
        if body.is_some() {
            request.push_str(&format!(
                "Content-Type: application/json\r\nContent-Length: {}\r\n",
                payload.len()
            ));
        } else if method == "POST" || method == "PUT" {
            request.push_str("Content-Length: 0\r\n");
        }
        request.push_str("\r\n");
        request.push_str(&payload);
        stream
            .write_all(request.as_bytes())
            .map_err(|e| e.to_string())?;
        let mut raw = Vec::new();
        (&stream)
            .take(MAX_ANSWER_BYTES + 1)
            .read_to_end(&mut raw)
            .map_err(|e| e.to_string())?;
        if raw.len() as u64 > MAX_ANSWER_BYTES {
            return Err("the answer of the server is too big".into());
        }
        parse_response(&raw)
    }
}

/// The status and the JSON body of an HTTP response (a body sent in chunks is put together).
pub fn parse_response(raw: &[u8]) -> Result<(u16, Value), String> {
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or("the answer of the server is incomplete")?;
    let head = String::from_utf8_lossy(&raw[..split]).to_string();
    let status: u16 = head
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .ok_or("the answer of the server has no status")?;
    let body = &raw[split + 4..];
    let chunked = head.lines().any(|l| {
        let l = l.to_ascii_lowercase();
        l.starts_with("transfer-encoding:") && l.contains("chunked")
    });
    let body = if chunked {
        unchunk(body)?
    } else {
        body.to_vec()
    };
    Ok((status, serde_json::from_slice(&body).unwrap_or(Value::Null)))
}

fn unchunk(mut rest: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    loop {
        let eol = rest
            .windows(2)
            .position(|w| w == b"\r\n")
            .ok_or("a chunk of the answer is cut")?;
        let size = usize::from_str_radix(String::from_utf8_lossy(&rest[..eol]).trim(), 16)
            .map_err(|_| "a chunk of the answer has a bad size")?;
        rest = &rest[eol + 2..];
        if size == 0 {
            return Ok(out);
        }
        if rest.len() < size + 2 {
            return Err("a chunk of the answer is cut".into());
        }
        out.extend_from_slice(&rest[..size]);
        rest = &rest[size + 2..];
    }
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

/// The machine of the server and the way to it.
pub struct Remote<T: Transport> {
    pub transport: T,
    pub machine: String,
    /// How many times a request that did not get an answer is tried (1 to 10).
    pub attempts: u32,
}

impl<T: Transport> Remote<T> {
    pub fn new(transport: T, target: &Target) -> Remote<T> {
        Remote {
            transport,
            machine: target.machine.clone(),
            attempts: target.attempts.clamp(1, 10),
        }
    }

    /// One request. A request that gets no answer is tried again (up to `attempts` times in all); an answer, whatever it says, is final. Only an answer of 200 or 201 is a success.
    fn ok(&mut self, method: &str, path: &str, body: Option<&Value>) -> Result<Value, String> {
        let mut last = String::new();
        for _ in 0..self.attempts.max(1) {
            match self.transport.call(method, path, body) {
                Ok((200 | 201, v)) => return Ok(v),
                Ok((status, v)) => return Err(error_text(status, &v)),
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

#[derive(Debug, Clone)]
enum Work {
    Load {
        feeders: Vec<FeederLine>,
        replace: bool,
    },
    Place {
        placements: Vec<Placed>,
    },
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
        })
    }
}

/// A server in memory with `pnp` machines: it checks the requests with the rules of the plugin (slots from 1 to 200, names of up to 64 letters, digits and `_-./+:`, counts up to 100000,
/// a reference once, the stock of the parts) and carries a placement job out, one part for each time the job is read while it runs.
pub struct SimServer {
    machines: BTreeMap<String, BTreeMap<u32, (String, u64)>>,
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
            "detail": {"feeders": feeders},
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
            Some("place") => match self.parse_place(machine, request) {
                Ok(p) => Work::Place { placements: p },
                Err((status, code, why)) => return Self::error(status, code, &why),
            },
            _ => {
                return Self::error(
                    400,
                    "bad_request",
                    "the request has to be {\"kind\": \"place\" | \"load_feeders\", ...}",
                )
            }
        };
        let total = match &work {
            Work::Load { feeders, .. } => feeders.len() as u64,
            Work::Place { placements } => placements.len() as u64,
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
        let Work::Place { placements } = self.jobs[at].work.clone() else {
            return;
        };
        let mut steps = self.steps_per_read.max(1);
        while steps > 0 && self.jobs[at].done < self.jobs[at].total {
            let (reference, part) = &placements[self.jobs[at].done as usize];
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
