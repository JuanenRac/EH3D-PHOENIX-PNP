//! The way to the EH3D server: the settings of `mode = server`, the requests that the feeders and the placements make, the server in memory and its rules, the plain HTTP transport against a
//! small server that is made here, and the commands `server-send`, `server-status` and `server-job`.

use eh3d_phoenix_pnp::config::{MachineConfig, KEYS};
use eh3d_phoenix_pnp::feeding::from_csv as feeders_from_csv;
use eh3d_phoenix_pnp::parts::{parse, Part};
use eh3d_phoenix_pnp::server::{
    calibrate_request, dry_run, error_text, feeder_rows, feeders_request, label, link_settings,
    parse_response, part_name, placements, safe_segment, Calibrate, JobInfo, Mode, Order, Remote,
    ScLink, SimServer, Target, Transport,
};
use eh3d_serverclient as sc;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Arc, Mutex};
use std::thread;

const BIN: &str = env!("CARGO_BIN_EXE_phoenix-pnp");
const MACHINE: &str = include_str!("../examples/machine.cfg");

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn run(args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .current_dir(root())
        .output()
        .unwrap()
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn temp(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("phoenix_pnp_server_{name}_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn target() -> Target {
    Target::new("127.0.0.1:18200", "tok", "m1", 5.0, 50, 3).unwrap()
}

fn board() -> Vec<Part> {
    parse(&std::fs::read_to_string(root().join("examples/board.pos")).unwrap()).unwrap()
}

fn feeders() -> Vec<eh3d_phoenix_pnp::feeding::FeederSpec> {
    feeders_from_csv(&std::fs::read_to_string(root().join("examples/feeders.csv")).unwrap())
        .unwrap()
}

fn sim() -> Remote<SimServer> {
    Remote::new(SimServer::new(&["m1"]), &target())
}

/// Loads the feeders of the example on the machine and returns the request of the placements.
fn loaded(r: &mut Remote<SimServer>) -> Value {
    let rows = feeder_rows(&feeders()).unwrap();
    let load = r.create("feeders", &feeders_request(&rows, true)).unwrap();
    assert_eq!(load.state, "queued");
    assert_eq!(r.order(&load.id, Order::Start).unwrap().state, "done");
    placements(&board(), &rows, (0.0, 0.0)).unwrap().request
}

// ---- the settings

#[test]
fn the_server_is_in_the_settings_and_the_local_mode_is_the_default() {
    let d = MachineConfig::default();
    assert_eq!(d.mode, Mode::Local);
    assert_eq!(
        (
            d.server_address.as_str(),
            d.server_token.as_str(),
            d.server_machine.as_str()
        ),
        ("", "", "")
    );
    assert_eq!(
        (d.server_timeout_s, d.server_poll_ms, d.server_attempts),
        (5.0, 500, 3)
    );
    for key in [
        "mode",
        "server_address",
        "server_token",
        "server_machine",
        "server_timeout_s",
        "server_poll_ms",
        "server_attempts",
    ] {
        assert!(KEYS.contains(&key), "{key}");
        assert!(
            MACHINE.contains(&format!("\n{key} =")),
            "the example lacks {key}"
        );
    }
    let e = MachineConfig::parse(MACHINE).unwrap();
    assert_eq!(
        (
            e.mode,
            e.server_address.as_str(),
            e.server_machine.as_str(),
            e.server_timeout_s,
            e.server_poll_ms,
            e.server_attempts
        ),
        (
            d.mode,
            "",
            "",
            d.server_timeout_s,
            d.server_poll_ms,
            d.server_attempts
        )
    );
    let c = MachineConfig::parse(
        "mode = Server\nserver_address = 192.168.0.180:18200\nserver_token = abc\nserver_machine = pnp-1\nserver_timeout_s = 2.5\nserver_poll_ms = 100\nserver_attempts = 5\n",
    )
    .unwrap();
    assert_eq!(c.mode, Mode::Server);
    assert_eq!(c.server_address, "192.168.0.180:18200");
    assert_eq!(
        (c.server_timeout_s, c.server_poll_ms, c.server_attempts),
        (2.5, 100, 5)
    );
}

#[test]
fn the_values_of_the_server_have_their_ranges() {
    let cases: [(&str, [&str; 2], [&str; 3]); 3] = [
        ("server_timeout_s", ["0.5", "120"], ["0.4", "120.1", "x"]),
        ("server_poll_ms", ["50", "60000"], ["49", "60001", "1.5"]),
        ("server_attempts", ["1", "10"], ["0", "11", "-1"]),
    ];
    for (key, good, bad) in cases {
        for v in good {
            assert!(
                MachineConfig::parse(&format!("{key} = {v}")).is_ok(),
                "{key} {v}"
            );
        }
        for v in bad {
            assert!(
                MachineConfig::parse(&format!("{key} = {v}")).is_err(),
                "{key} {v}"
            );
        }
    }
    for bad in [
        "mode = remote",
        "mode =",
        "server_machine = a b",
        "server_machine = ../x",
    ] {
        assert!(MachineConfig::parse(bad).is_err(), "{bad}");
    }
    assert!(MachineConfig::parse(&format!("server_address = {}", "a".repeat(256))).is_err());
    assert!(MachineConfig::parse(&format!("server_machine = {}", "a".repeat(65))).is_err());
    assert!(MachineConfig::parse("mode = server\nmode = local").is_err());
}

#[test]
fn a_token_is_never_shown_by_the_diff_of_two_settings() {
    let d = eh3d_phoenix_pnp::configdiff::diff(
        "server_token = secret-one\n",
        "server_token = secret-two\n",
    )
    .unwrap();
    assert_eq!(d.changes.len(), 1);
    let text = d.text();
    assert!(text.contains("server_token: (set) -> (set)"), "{text}");
    assert!(!text.contains("secret"));
}

#[test]
fn the_address_the_machine_and_the_token_are_checked_before_anything_is_sent() {
    let t = Target::new(
        " http://192.168.0.180:18200/ ",
        " tok ",
        " m1 ",
        5.0,
        500,
        3,
    )
    .unwrap();
    assert_eq!(
        (t.address.as_str(), t.token.as_str(), t.machine.as_str()),
        ("192.168.0.180:18200", "tok", "m1")
    );
    for (a, tok, m) in [
        ("", "", "m1"),
        ("host:1", "", ""),
        ("host:1", "", "a/b"),
        ("host:1", "", "a b"),
        ("https://host:1", "", "m1"),
        ("host:1/api", "", "m1"),
        ("host:1", "a\nb", "m1"),
    ] {
        assert!(Target::new(a, tok, m, 5.0, 500, 3).is_err(), "{a:?} {m:?}");
    }
    assert!(safe_segment("a-b_9") && !safe_segment("") && !safe_segment(&"a".repeat(65)));
}

// ---- the requests

#[test]
fn the_names_that_the_server_takes_are_made_from_what_the_tables_say() {
    assert_eq!(part_name("10k", "R_0603"), "R_0603_10k");
    assert_eq!(part_name(" 4.7 uF ", "C 0805"), "C_0805_4.7_uF");
    assert_eq!(label("A/R1", 32, "_-."), "A_R1");
    assert_eq!(label("", 32, "_-."), "_");
    assert_eq!(label(&"x".repeat(100), 32, "_-.").len(), 32);
    assert!(part_name("é", "ü").is_ascii());
}

#[test]
fn the_feeders_that_are_on_take_the_slots_in_order_and_the_others_are_left_out() {
    let mut f = feeders();
    f[1].enabled = false;
    let rows = feeder_rows(&f).unwrap();
    let got: Vec<(&str, u32, &str, u64)> = rows
        .iter()
        .map(|r| (r.id.as_str(), r.slot, r.part.as_str(), r.count))
        .collect();
    assert_eq!(
        got,
        [
            ("F1", 1, "R_0603_10k", 100),
            ("F3", 2, "QFN-32_MCU", 10),
            ("F4", 3, "LED_0603_LED", 50)
        ]
    );
    let request = feeders_request(&rows, true);
    assert_eq!(request["kind"], "load_feeders");
    assert_eq!(request["replace"], true);
    assert_eq!(
        request["feeders"][1],
        json!({"slot": 2, "part": "QFN-32_MCU", "count": 10})
    );
}

#[test]
fn too_many_feeders_or_too_many_parts_in_one_feeder_are_refused_here() {
    let one = &feeders()[0];
    let many = vec![one.clone(); 201];
    assert!(feeder_rows(&many).unwrap_err().contains("200"));
    assert_eq!(feeder_rows(&many[..200]).unwrap().len(), 200);
    let mut big = one.clone();
    big.base.count = 100_001;
    assert!(feeder_rows(&[big]).unwrap_err().contains("100000"));
}

#[test]
fn a_part_goes_by_the_name_of_the_feeder_that_holds_its_value_and_package() {
    let mut f = feeders();
    f[0].base.value = "10K".into(); // another case of the same value
    let rows = feeder_rows(&f).unwrap();
    let made = placements(&board(), &rows, (0.0, 0.0)).unwrap();
    assert_eq!(made.count, 7);
    assert!(made.unmatched.is_empty());
    let list = made.request["placements"].as_array().unwrap();
    assert_eq!(list[0]["ref"], "R1");
    assert_eq!(list[0]["part"], "R_0603_10K", "the name is the feeder's");
    assert_eq!(list[1]["rot"], 90.0);
    assert_eq!(list[6]["side"], "bottom");
    assert!(made.request.get("offset").is_none());
}

#[test]
fn the_parts_that_no_feeder_holds_are_listed_and_the_offset_is_sent_when_there_is_one() {
    let mut f = feeders();
    f.remove(2); // the MCU
    let rows = feeder_rows(&f).unwrap();
    let made = placements(&board(), &rows, (10.0, -5.5)).unwrap();
    assert_eq!(made.unmatched, ["U1"]);
    assert_eq!(made.count, 6);
    assert_eq!(made.request["offset"], json!({"x": 10.0, "y": -5.5}));
}

#[test]
fn two_references_that_are_one_for_the_server_stop_the_request() {
    let mut parts = board();
    parts[0].reference = "A/R1".into();
    parts[1].reference = "A_R1".into();
    let rows = feeder_rows(&feeders()).unwrap();
    let why = placements(&parts, &rows, (0.0, 0.0)).unwrap_err();
    assert!(why.contains("A_R1"), "{why}");
}

// ---- the server in memory

#[test]
fn a_job_is_prepared_started_and_followed_until_the_parts_are_placed() {
    let mut r = sim();
    r.transport.steps_per_read = 3;
    let request = loaded(&mut r);
    let job = r.create("board", &request).unwrap();
    assert_eq!((job.state.as_str(), job.total), ("queued", 7));
    assert_eq!(r.order(&job.id, Order::Start).unwrap().state, "running");
    let mut seen = Vec::new();
    let end = r
        .follow(
            &job.id,
            100,
            &mut |i| seen.push((i.state.clone(), i.done)),
            &mut || {},
        )
        .unwrap();
    assert_eq!(end.state, "done");
    assert_eq!(
        seen,
        [
            ("running".into(), 3),
            ("running".into(), 6),
            ("done".into(), 7)
        ]
    );
    // every pick took a part off its feeder
    assert_eq!(r.transport.feeder("m1", 1), Some(("R_0603_10k".into(), 97)));
    assert_eq!(r.transport.feeder("m1", 3), Some(("QFN-32_MCU".into(), 9)));
    let machine = r.machine().unwrap();
    assert_eq!((machine.state.as_str(), machine.feeders.len()), ("idle", 4));
    assert_eq!(machine.feeders[0], (1, "R_0603_10k".to_string(), 97));
}

#[test]
fn a_job_can_be_held_let_go_and_stopped_and_only_when_it_is_in_the_state_for_it() {
    let mut r = sim();
    r.transport.steps_per_read = 1;
    let request = loaded(&mut r);
    let job = r.create("board", &request).unwrap();
    assert!(r
        .order(&job.id, Order::Pause)
        .unwrap_err()
        .contains("not_in_state"));
    r.order(&job.id, Order::Start).unwrap();
    assert_eq!(r.machine().unwrap().state, "busy");
    assert_eq!(r.order(&job.id, Order::Pause).unwrap().state, "paused");
    let held = r.job(&job.id).unwrap();
    assert_eq!(
        (held.state.as_str(), held.done),
        ("paused", 0),
        "a held job places nothing"
    );
    assert!(r
        .order(&job.id, Order::Start)
        .unwrap_err()
        .contains("machine_busy"));
    assert_eq!(r.order(&job.id, Order::Resume).unwrap().state, "running");
    assert_eq!(r.job(&job.id).unwrap().done, 1);
    assert_eq!(r.order(&job.id, Order::Cancel).unwrap().state, "cancelled");
    assert!(r.order(&job.id, Order::Cancel).is_err());
    assert_eq!(r.machine().unwrap().state, "idle");
}

#[test]
fn a_job_that_needs_more_parts_than_the_feeders_hold_is_refused_when_it_is_prepared() {
    let mut r = sim();
    let rows = feeder_rows(&feeders()).unwrap();
    let mut small = feeders();
    small[2].base.count = 0; // the MCU feeder is empty
    let load = r
        .create(
            "feeders",
            &feeders_request(&feeder_rows(&small).unwrap(), true),
        )
        .unwrap();
    r.order(&load.id, Order::Start).unwrap();
    let request = placements(&board(), &rows, (0.0, 0.0)).unwrap().request;
    let why = r.create("board", &request).unwrap_err();
    assert!(why.starts_with("pnp_feeder_short: QFN-32_MCU"), "{why}");
}

#[test]
fn the_server_in_memory_refuses_what_the_plugin_refuses() {
    let mut r = sim();
    let ok = json!({"slot": 1, "part": "a", "count": 1});
    for request in [
        json!({"kind": "load_feeders", "feeders": []}),
        json!({"kind": "load_feeders", "feeders": [{"slot": 0, "part": "a", "count": 1}]}),
        json!({"kind": "load_feeders", "feeders": [{"slot": 201, "part": "a", "count": 1}]}),
        json!({"kind": "load_feeders", "feeders": [{"slot": 1, "part": "a b", "count": 1}]}),
        json!({"kind": "load_feeders", "feeders": [{"slot": 1, "part": "a", "count": 100_001}]}),
        json!({"kind": "load_feeders", "feeders": [ok.clone(), ok.clone()]}),
        json!({"kind": "load_feeders", "feeders": [ok.clone()], "replace": 1}),
        json!({"kind": "place", "placements": []}),
        json!({"kind": "place", "placements": [{"ref": "R 1", "part": "a", "x": 1, "y": 1}]}),
        json!({"kind": "place", "placements": [{"ref": "R1", "part": "a", "x": "1", "y": 1}]}),
        json!({"kind": "place", "placements": [{"ref": "R1", "part": "a", "x": 1, "y": 1, "rot": 361}]}),
        json!({"kind": "place", "placements": [{"ref": "R1", "part": "a", "x": 1, "y": 1, "side": "left"}]}),
        json!({"kind": "place", "placements": [{"ref": "R1", "part": "a", "x": 1, "y": 1}, {"ref": "R1", "part": "a", "x": 2, "y": 2}]}),
        json!({"kind": "drill"}),
        json!({}),
    ] {
        assert!(r.create("x", &request).is_err(), "{request}");
    }
    let unknown =
        Remote::new(SimServer::new(&["other"]), &target()).create("x", &json!({"kind": "place"}));
    assert!(unknown.unwrap_err().starts_with("unknown_machine"));
}

#[test]
fn a_feeder_that_runs_dry_fails_the_job_and_says_which_part() {
    let mut r = sim();
    r.transport.steps_per_read = 100;
    let request = loaded(&mut r);
    let job = r.create("board", &request).unwrap();
    // the table is replaced by one whose MCU feeder is empty before the job runs
    let mut small = feeders();
    small[2].base.count = 0;
    let swap = r
        .create(
            "empty",
            &feeders_request(&feeder_rows(&small).unwrap(), true),
        )
        .unwrap();
    r.order(&swap.id, Order::Start).unwrap();
    r.order(&job.id, Order::Start).unwrap();
    let end = r.job(&job.id).unwrap();
    assert_eq!(end.state, "failed");
    assert!(end.message.contains("QFN-32_MCU"), "{}", end.message);
    assert!(end.done < end.total);
}

#[test]
fn a_request_that_gets_no_answer_is_tried_again_and_an_answer_is_final() {
    let mut r = sim();
    r.transport.drop_next(2);
    assert_eq!(
        r.machines().unwrap().len(),
        1,
        "the third try of three gets through"
    );
    r.transport.drop_next(3);
    assert!(r.machines().unwrap_err().contains("lost"));
    r.attempts = 1;
    r.transport.drop_next(1);
    assert!(r.machines().is_err());
    // a refusal is not tried again
    r.attempts = 5;
    r.transport.log.clear();
    r.transport.refuse_next(
        401,
        json!({"error": "unauthorized", "detail": "the token is not valid"}),
    );
    assert_eq!(
        r.machines().unwrap_err(),
        "unauthorized: the token is not valid"
    );
    assert_eq!(r.transport.log.len(), 1);
}

#[test]
fn following_a_job_stops_after_the_polls_it_is_given() {
    let mut r = sim();
    r.transport.steps_per_read = 1;
    let request = loaded(&mut r);
    let job = r.create("board", &request).unwrap();
    r.order(&job.id, Order::Start).unwrap();
    r.order(&job.id, Order::Pause).unwrap();
    let mut waits = 0;
    let last = r
        .follow(&job.id, 4, &mut |_| {}, &mut || waits += 1)
        .unwrap();
    assert_eq!((last.state.as_str(), waits), ("paused", 3));
    assert!(r.follow("../x", 4, &mut |_| {}, &mut || {}).is_err());
}

#[test]
fn what_the_server_says_of_a_job_is_read_with_care() {
    let i = JobInfo::from_json(
        &json!({"id": "w-1", "state": "running", "done": 3, "total": 12, "message": "x"}),
    );
    assert_eq!((i.percent(), i.finished()), (25, false));
    assert_eq!(i.line(), "w-1: running, 3 of 12 (25%) - x");
    let empty = JobInfo::from_json(&json!(null));
    assert_eq!(
        (empty.percent(), empty.finished(), empty.state.as_str()),
        (0, false, "")
    );
    assert!(JobInfo::from_json(&json!({"state": "failed"})).finished());
    assert_eq!(
        JobInfo::from_json(&json!({"done": 9, "total": 3})).percent(),
        100
    );
    assert_eq!(error_text(500, &json!(null)), "the server answered 500");
    assert_eq!(
        error_text(409, &json!({"error": "machine_busy"})),
        "machine_busy"
    );
}

// ---- the plain HTTP

#[test]
fn an_answer_in_chunks_is_put_together_and_a_cut_one_is_an_error() {
    let raw = b"HTTP/1.1 201 Created\r\nTransfer-Encoding: chunked\r\n\r\n6\r\n{\"a\":1\r\n1\r\n}\r\n0\r\n\r\n";
    assert_eq!(parse_response(raw).unwrap(), (201, json!({"a": 1})));
    assert_eq!(
        parse_response(b"HTTP/1.1 204 No Content\r\n\r\n").unwrap(),
        (204, Value::Null)
    );
    for cut in [
        &b"HTTP/1.1 200 OK"[..],
        b"",
        b"garbage\r\n\r\n",
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nzz\r\n",
    ] {
        assert!(
            parse_response(cut).is_err(),
            "{:?}",
            String::from_utf8_lossy(cut)
        );
    }
}

/// A small server on the loopback address that gives the requests to a server in memory. It answers `n` requests and keeps the `Authorization` header of each.
fn serve(n: usize) -> (String, Arc<Mutex<Vec<String>>>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let keep = seen.clone();
    let handle = thread::spawn(move || {
        let mut server = SimServer::new(&["m1"]);
        server.steps_per_read = 100;
        for _ in 0..n {
            let (mut s, _) = listener.accept().unwrap();
            let mut buf = Vec::new();
            let mut byte = [0u8; 1];
            while !buf.ends_with(b"\r\n\r\n") {
                if s.read(&mut byte).unwrap() == 0 {
                    break;
                }
                buf.push(byte[0]);
            }
            let head = String::from_utf8_lossy(&buf).to_string();
            let mut lines = head.lines();
            let first = lines.next().unwrap_or("").to_string();
            let mut length = 0usize;
            let mut auth = String::new();
            for l in lines {
                let lower = l.to_ascii_lowercase();
                if let Some(v) = lower.strip_prefix("content-length:") {
                    length = v.trim().parse().unwrap_or(0);
                }
                if lower.starts_with("authorization:") {
                    auth = l.to_string();
                }
            }
            let mut body = vec![0u8; length];
            s.read_exact(&mut body).unwrap();
            keep.lock().unwrap().push(auth);
            let mut words = first.split_whitespace();
            let (method, path) = (words.next().unwrap_or(""), words.next().unwrap_or(""));
            let json_body: Option<Value> = serde_json::from_slice(&body).ok();
            let (status, answer) = server.call(method, path, json_body.as_ref()).unwrap();
            let text = answer.to_string();
            let reply = format!(
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
                text.len()
            );
            s.write_all(reply.as_bytes()).unwrap();
        }
    });
    (address, seen, handle)
}

#[test]
fn the_http_transport_sends_the_token_and_reads_the_answer() {
    let (address, seen, handle) = serve(2);
    let t = Target::new(&address, "the-token", "m1", 5.0, 50, 1)
        .unwrap()
        .with_link(0, false);
    let mut http = ScLink::new(&t).unwrap();
    let (status, v) = http.call("GET", "/api/v1/work/machines", None).unwrap();
    assert_eq!((status, v["machines"][0]["id"].as_str()), (200, Some("m1")));
    let (status, v) = http
        .call(
            "POST",
            "/api/v1/work/jobs",
            Some(&json!({"machine": "m1", "request": {"kind": "drill"}})),
        )
        .unwrap();
    assert_eq!((status, v["error"].as_str()), (400, Some("bad_request")));
    handle.join().unwrap();
    assert_eq!(
        seen.lock().unwrap().as_slice(),
        ["Authorization: Bearer the-token"; 2]
    );
}

#[test]
fn a_server_that_is_not_there_is_an_error_that_names_it() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    drop(listener);
    let t = Target::new(&address, "tok", "m1", 1.0, 50, 1)
        .unwrap()
        .with_link(0, false);
    let why = ScLink::new(&t)
        .unwrap()
        .call("GET", "/api/v1/x", None)
        .unwrap_err();
    assert!(why.contains(&address), "{why}");
}

// ---- the commands

fn config(dir: &Path, address: &str, mode: &str) -> String {
    let path = dir.join("machine.cfg");
    std::fs::write(
        &path,
        format!("mode = {mode}\nserver_address = {address}\nserver_token = never-print-this\nserver_machine = m1\nserver_poll_ms = 50\nserver_attempts = 1\n"),
    )
    .unwrap();
    path.to_string_lossy().into_owned()
}

#[test]
fn the_commands_do_nothing_in_the_local_mode() {
    let dir = temp("local");
    let cfg = config(&dir, "127.0.0.1:1", "local");
    for args in [
        vec![
            "server-send",
            "examples/board.pos",
            "examples/feeders.csv",
            "--config",
            &cfg,
        ],
        vec!["server-status", "--config", &cfg],
        vec!["server-job", "w-1", "--cancel", "--config", &cfg],
    ] {
        let o = run(&args);
        assert_eq!(o.status.code(), Some(1), "{args:?}");
        assert!(err(&o).contains("mode is local"), "{}", err(&o));
    }
    let o = run(&["server-status"]);
    assert!(err(&o).contains("mode is local"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn server_send_against_the_server_in_memory_loads_the_feeders_and_places_every_part() {
    let o = run(&[
        "server-send",
        "examples/board.pos",
        "examples/feeders.csv",
        "--sim",
        "--follow",
    ]);
    assert!(o.status.success(), "{}", err(&o));
    let text = out(&o);
    assert!(text.contains("feeders: 4 slots loaded on sim-1"), "{text}");
    assert!(text.contains("prepared: 7 placements on sim-1"), "{text}");
    assert!(text.contains("done, 7 of 7 (100%)"), "{text}");
    // without --start the job waits
    let o = run(&[
        "server-send",
        "examples/board.pos",
        "examples/feeders.csv",
        "--sim",
    ]);
    assert!(out(&o).contains("waits, queued"), "{}", out(&o));
    let o = run(&[
        "server-send",
        "examples/board.pos",
        "examples/feeders.csv",
        "--sim",
        "--start",
    ]);
    assert!(out(&o).contains("started"), "{}", out(&o));
}

#[test]
fn the_parts_that_no_feeder_holds_stop_server_send_unless_partial_is_said() {
    let dir = temp("partial");
    let feeders = dir.join("feeders.csv");
    std::fs::write(
        &feeders,
        "id,value,package,x,y,count\nF1,10k,R_0603,-20,10,100\n",
    )
    .unwrap();
    let f = feeders.to_string_lossy().into_owned();
    let o = run(&["server-send", "examples/board.pos", &f, "--sim"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(err(&o).contains("4 parts have no feeder"), "{}", err(&o));
    let o = run(&[
        "server-send",
        "examples/board.pos",
        &f,
        "--sim",
        "--partial",
        "--follow",
    ]);
    assert!(o.status.success(), "{}", err(&o));
    assert!(
        out(&o).contains("left out, no feeder holds them: C1, C2, U1, D1"),
        "{}",
        out(&o)
    );
    assert!(out(&o).contains("3 of 3"), "{}", out(&o));
    let none = dir.join("none.csv");
    std::fs::write(&none, "id,value,package,x,y,count\nF9,1,X,0,0,1\n").unwrap();
    let o = run(&[
        "server-send",
        "examples/board.pos",
        &none.to_string_lossy(),
        "--sim",
        "--partial",
    ]);
    assert!(err(&o).contains("no part is left"), "{}", err(&o));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_options_of_server_send_are_checked() {
    for args in [
        vec!["--offset", "1"],
        vec!["--offset", "a,b"],
        vec!["--timeout", "0"],
        vec!["--timeout", "86401"],
        vec!["--nope"],
    ] {
        let mut all = vec![
            "server-send",
            "examples/board.pos",
            "examples/feeders.csv",
            "--sim",
        ];
        all.extend(args.iter());
        let o = run(&all);
        assert_eq!(o.status.code(), Some(1), "{args:?}");
    }
    let o = run(&["server-send", "examples/board.pos", "--sim"]);
    assert!(
        err(&o).contains("needs a table of parts and a table of feeders"),
        "{}",
        err(&o)
    );
    let o = run(&[
        "server-send",
        "examples/board.pos",
        "examples/feeders.csv",
        "--sim",
        "--offset",
        "300,0",
    ]);
    assert!(
        o.status.success(),
        "the simulated server does not know the bed: {}",
        err(&o)
    );
}

#[test]
fn the_commands_talk_to_a_server_with_the_token_and_never_print_it() {
    let dir = temp("http");
    // server-send (feeders: create, start, follow; place: create, start, follow) then status and a job
    let (address, seen, _server) = serve(100);
    let cfg = config(&dir, &address, "server");
    let o = run(&[
        "server-send",
        "examples/board.pos",
        "examples/feeders.csv",
        "--follow",
        "--config",
        &cfg,
    ]);
    assert!(o.status.success(), "{} {}", out(&o), err(&o));
    assert!(out(&o).contains("done, 7 of 7 (100%)"), "{}", out(&o));
    let o2 = run(&["server-status", "--config", &cfg]);
    assert!(o2.status.success(), "{}", err(&o2));
    assert!(
        out(&o2).contains("m1: idle (pnp), 4 feeders"),
        "{}",
        out(&o2)
    );
    assert!(
        out(&o2).contains("slot 1: R_0603_10k, 97 left"),
        "{}",
        out(&o2)
    );
    let o3 = run(&["server-status", "w-0002", "--config", &cfg]);
    assert!(out(&o3).contains("w-0002: done"), "{}", out(&o3));
    let o4 = run(&["server-job", "w-0002", "--cancel", "--config", &cfg]);
    assert_eq!(
        o4.status.code(),
        Some(1),
        "a finished job cannot be stopped"
    );
    assert!(err(&o4).contains("not_in_state"), "{}", err(&o4));
    for o in [&o, &o2, &o3, &o4] {
        assert!(!out(o).contains("never-print-this") && !err(o).contains("never-print-this"));
    }
    assert!(seen
        .lock()
        .unwrap()
        .iter()
        .all(|a| a == "Authorization: Bearer never-print-this"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn server_job_wants_one_order_and_one_id() {
    let dir = temp("job");
    let cfg = config(&dir, "127.0.0.1:1", "server");
    for args in [
        vec!["server-job", "w-1", "--config", &cfg],
        vec!["server-job", "w-1", "--pause", "--cancel", "--config", &cfg],
        vec!["server-job", "--pause", "--config", &cfg],
        vec!["server-job", "a", "b", "--pause", "--config", &cfg],
    ] {
        let o = run(&args);
        assert_eq!(o.status.code(), Some(1), "{args:?}");
    }
    // a server that is not there: the error names the address and not the token
    let o = run(&["server-status", "--config", &cfg]);
    assert!(err(&o).contains("127.0.0.1:1"), "{}", err(&o));
    assert!(!err(&o).contains("never-print-this"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn server_request_writes_the_two_requests_that_server_send_would_make() {
    let dir = temp("request");
    let out_dir = dir.join("requests");
    let d = out_dir.to_string_lossy().into_owned();
    let o = run(&[
        "server-request",
        "examples/board.pos",
        "examples/feeders.csv",
        "--out",
        &d,
        "--offset",
        "10,-5.5",
    ]);
    assert!(o.status.success(), "{}", err(&o));
    assert!(
        out(&o).contains("4 feeders and 7 placements written"),
        "{}",
        out(&o)
    );
    let feeders: Value =
        serde_json::from_str(&std::fs::read_to_string(out_dir.join("feeders.json")).unwrap())
            .unwrap();
    let place: Value =
        serde_json::from_str(&std::fs::read_to_string(out_dir.join("place.json")).unwrap())
            .unwrap();
    let rows = feeder_rows(
        &feeders_from_csv(&std::fs::read_to_string(root().join("examples/feeders.csv")).unwrap())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(feeders, feeders_request(&rows, true));
    assert_eq!(
        place,
        placements(&board(), &rows, (10.0, -5.5)).unwrap().request
    );
    // what is written is what a server in memory takes
    let mut r = sim();
    r.create("feeders", &feeders).unwrap();
    assert!(
        r.create("board", &place).is_err(),
        "the feeders were only prepared, not loaded: the stock is not there"
    );
    // a folder that has the files is not written over
    let again = run(&[
        "server-request",
        "examples/board.pos",
        "examples/feeders.csv",
        "--out",
        &d,
    ]);
    assert_eq!(again.status.code(), Some(1));
    assert!(err(&again).contains("PHOENIX-PNP-040"), "{}", err(&again));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn server_request_wants_a_folder_and_leaves_out_parts_without_a_feeder_only_when_told() {
    let o = run(&[
        "server-request",
        "examples/board.pos",
        "examples/feeders.csv",
    ]);
    assert!(err(&o).contains("needs --out DIR"), "{}", err(&o));
    let dir = temp("request_partial");
    let feeders = dir.join("feeders.csv");
    std::fs::write(
        &feeders,
        "id,value,package,x,y,count\nF1,10k,R_0603,-20,10,100\n",
    )
    .unwrap();
    let f = feeders.to_string_lossy().into_owned();
    let d = dir.join("o").to_string_lossy().into_owned();
    let o = run(&["server-request", "examples/board.pos", &f, "--out", &d]);
    assert_eq!(o.status.code(), Some(1));
    assert!(err(&o).contains("4 parts have no feeder"), "{}", err(&o));
    assert!(
        !Path::new(&d).exists(),
        "nothing is made when the request is refused"
    );
    let o = run(&[
        "server-request",
        "examples/board.pos",
        &f,
        "--out",
        &d,
        "--partial",
    ]);
    assert!(o.status.success(), "{}", err(&o));
    assert!(
        out(&o).contains("1 feeders and 3 placements"),
        "{}",
        out(&o)
    );
    for args in [
        vec!["server-request", "examples/board.pos", "--out", d.as_str()],
        vec![
            "server-request",
            "examples/board.pos",
            f.as_str(),
            "--out",
            d.as_str(),
            "--offset",
            "1",
        ],
    ] {
        assert_eq!(run(&args).status.code(), Some(1), "{args:?}");
    }
    let _ = std::fs::remove_dir_all(dir);
}

// ---- the common client, the routes of the plugin and the commands that use them

/// A transport of the common client that answers the first requests with a failure of the network and then gives every request to a server in memory.
struct Flaky {
    fail: Mutex<u32>,
    inner: Mutex<SimServer>,
    seen: Arc<Mutex<Vec<String>>>,
}

impl sc::Transport for Flaky {
    fn send(&self, request: &sc::Request) -> Result<sc::Reply, sc::TransportError> {
        self.seen
            .lock()
            .unwrap()
            .push(format!("{} {}", request.method, request.path));
        {
            let mut left = self.fail.lock().unwrap();
            if *left > 0 {
                *left -= 1;
                return Err(sc::TransportError::Connect("down".into()));
            }
        }
        let body: Option<Value> = request
            .body
            .as_deref()
            .and_then(|b| serde_json::from_str(b).ok());
        let (status, answer) = self
            .inner
            .lock()
            .unwrap()
            .call(&request.method, &request.path, body.as_ref())
            .unwrap();
        Ok(sc::Reply {
            status,
            body: answer.to_string(),
        })
    }
}

struct NoSleep;

impl sc::client::Sleeper for NoSleep {
    fn sleep(&self, _how_long: std::time::Duration) {}
}

/// A link of the common client over a server in memory that fails `fail` times first, with `retries` more tries allowed and no pauses; the second value lists the requests that were sent.
fn flaky(fail: u32, retries: u32) -> (Remote<ScLink>, Arc<Mutex<Vec<String>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let double = Flaky {
        fail: Mutex::new(fail),
        inner: Mutex::new(SimServer::new(&["m1"])),
        seen: seen.clone(),
    };
    let t = Target::new("127.0.0.1:18200", "tok", "m1", 5.0, 50, 1)
        .unwrap()
        .with_link(retries, false);
    let client = sc::Client::new(link_settings(&t).unwrap(), Box::new(double))
        .with_sleeper(Box::new(NoSleep));
    (Remote::new(ScLink::with_client(client), &t), seen)
}

#[test]
fn the_link_to_the_server_has_the_keys_of_the_common_client_with_their_ranges() {
    let d = MachineConfig::default();
    assert_eq!((d.server_retries, d.server_mdns), (2, false));
    for key in ["server_retries", "server_mdns"] {
        assert!(KEYS.contains(&key), "{key}");
        assert!(
            MACHINE.contains(&format!("\n{key} =")),
            "the example lacks {key}"
        );
    }
    for (v, ok) in [
        ("0", true),
        ("5", true),
        ("6", false),
        ("-1", false),
        ("1.5", false),
        ("x", false),
    ] {
        assert_eq!(
            MachineConfig::parse(&format!("server_retries = {v}")).is_ok(),
            ok,
            "server_retries {v}"
        );
    }
    for (v, want) in [
        ("on", Some(true)),
        ("off", Some(false)),
        ("yes", Some(true)),
        ("maybe", None),
    ] {
        let got = MachineConfig::parse(&format!("server_mdns = {v}"))
            .ok()
            .map(|c| c.server_mdns);
        assert_eq!(got, want, "server_mdns {v}");
    }
    let c = MachineConfig::parse("server_retries = 4\nserver_mdns = on\n").unwrap();
    assert_eq!((c.server_retries, c.server_mdns), (4, true));
}

#[test]
fn the_settings_of_the_link_follow_the_target_and_hide_the_token() {
    let t = Target::new("127.0.0.1:18200", "hidden-token-text", "m1", 0.5, 50, 3)
        .unwrap()
        .with_link(9, true);
    assert_eq!((t.retries, t.mdns), (5, true));
    let s = link_settings(&t).unwrap();
    assert_eq!(s.server_address, "127.0.0.1:18200");
    assert_eq!(s.server_machine, "m1");
    // half a second is one whole second for the common client
    assert_eq!(s.server_timeout_s, 1);
    assert_eq!(
        (s.server_retries, s.server_mdns, s.server_max_answer_mib),
        (5, true, 1)
    );
    assert!(!format!("{s:?}").contains("hidden-token-text"));
    let t = Target::new("h:1", "", "m1", 500.0, 50, 1).unwrap();
    assert_eq!(link_settings(&t).unwrap().server_timeout_s, 120);
    assert_eq!(link_settings(&t).unwrap().server_retries, 2);
    let t = Target::new("h:1", "a b", "m1", 5.0, 50, 1).unwrap();
    assert!(link_settings(&t).unwrap_err().contains("server_token"));
}

#[test]
fn a_request_that_got_no_answer_is_tried_again_as_often_as_server_retries_says() {
    let (mut r, seen) = flaky(2, 2);
    assert_eq!(r.machines().unwrap().len(), 1);
    assert_eq!(seen.lock().unwrap().len(), 3);
    let (mut r, seen) = flaky(3, 2);
    let why = r.machines().unwrap_err();
    assert!(why.contains("cannot be reached"), "{why}");
    assert_eq!(seen.lock().unwrap().len(), 3);
    let (mut r, seen) = flaky(1, 0);
    assert!(r.machines().is_err());
    assert_eq!(seen.lock().unwrap().len(), 1);
}

#[test]
fn a_job_is_made_with_a_single_post_over_the_link() {
    let (mut r, seen) = flaky(0, 2);
    let request =
        json!({"kind": "load_feeders", "feeders": [{"slot": 1, "part": "R_0603_10k", "count": 5}]});
    let made = r.create("feeders", &request).unwrap();
    assert_eq!(made.state, "queued");
    assert_eq!(seen.lock().unwrap().as_slice(), ["POST /api/v1/work/jobs"]);
}

#[test]
fn a_refusal_comes_back_as_an_answer_and_is_told_in_the_language_asked() {
    let (mut r, _) = flaky(0, 0);
    r.set_feeder(1, "R_0603_10k", 3).unwrap();
    let load = r
        .create(
            "place",
            &json!({"kind": "place", "placements": [{"ref": "R1", "part": "R_0603_10k", "x": 1, "y": 2}]}),
        )
        .unwrap();
    r.order(&load.id, Order::Start).unwrap();
    // the machine is running a job: changing a feeder is refused
    let english = r.set_feeder(2, "C_0603_100n", 3).unwrap_err();
    assert_eq!(english, "machine_busy: m1");
    r.speak("es");
    let spanish = r.set_feeder(2, "C_0603_100n", 3).unwrap_err();
    assert!(
        spanish.starts_with("Hay un trabajo en marcha en esta máquina.")
            && spanish.ends_with("[machine_busy: m1]"),
        "{spanish}"
    );
    r.speak("pt");
    assert!(r
        .set_feeder(2, "C_0603_100n", 3)
        .unwrap_err()
        .starts_with("Há um trabalho em curso"));
    // a language the suite does not have leaves English
    r.speak("xx");
    assert_eq!(r.set_feeder(2, "C_0603_100n", 3).unwrap_err(), english);
}

#[test]
fn a_link_without_a_token_says_there_is_no_token_to_send() {
    let t = Target::new("127.0.0.1:18200", "", "m1", 5.0, 50, 1).unwrap();
    let mut empty = Remote::new(ScLink::new(&t).unwrap(), &t);
    let why = empty.machines().unwrap_err();
    assert!(why.contains("no token to send"), "{why}");
    assert!(!why.contains("never-print"), "{why}");
    let (mut r, seen) = flaky(0, 0);
    let _ = r.machines().unwrap();
    assert_eq!(seen.lock().unwrap().len(), 1);
}

#[test]
fn the_feeders_of_the_machine_are_read_declared_refilled_and_emptied_with_no_job() {
    let mut r = sim();
    let t = r.feeders_table().unwrap();
    assert_eq!((t.slots, t.feeders.len(), t.free.len()), (200, 0, 200));
    let t = r.set_feeder(3, "R_0603_10k", 150).unwrap();
    assert_eq!(t.feeders.len(), 1);
    let f = &t.feeders[0];
    assert_eq!(
        (f.slot, f.part.as_str(), f.count, f.empty),
        (3, "R_0603_10k", 150, false)
    );
    assert!(!t.free.contains(&3) && t.free.len() == 199);
    // a refill replaces the count
    let t = r.set_feeder(3, "R_0603_10k", 20).unwrap();
    assert_eq!(t.feeders[0].count, 20);
    // an empty feeder is one that holds a part and no count
    let t = r.set_feeder(4, "C_0603_100n", 0).unwrap();
    assert_eq!(t.empty_count(), 1);
    assert!(
        t.text().contains("slot 4: C_0603_100n, 0 left (empty)"),
        "{}",
        t.text()
    );
    let t = r.clear_feeder(4).unwrap();
    assert_eq!((t.feeders.len(), t.empty_count()), (1, 0));
    assert_eq!(
        t.csv(),
        "slot,part,count,x_mm,y_mm,offset_x_mm,offset_y_mm\n3,R_0603_10k,20,40.000,-30.000,0.000,0.000\n"
    );
    // what the plugin would refuse is refused before the request is made
    for (slot, part, count) in [
        (0, "a", 1),
        (201, "a", 1),
        (1, "has space", 1),
        (1, "", 1),
        (1, "a", 100_001),
    ] {
        assert!(
            r.set_feeder(slot, part, count).is_err(),
            "{slot} {part} {count}"
        );
    }
    assert!(r.clear_feeder(0).is_err() && r.clear_feeder(201).is_err());
    // and a machine that is running a job refuses the change
    let rows = feeder_rows(&feeders()).unwrap();
    let load = r.create("feeders", &feeders_request(&rows, true)).unwrap();
    r.order(&load.id, Order::Start).unwrap();
    let place = r
        .create(
            "place",
            &placements(&board(), &rows, (0.0, 0.0)).unwrap().request,
        )
        .unwrap();
    r.order(&place.id, Order::Start).unwrap();
    assert_eq!(
        r.set_feeder(9, "R_0603_10k", 1).unwrap_err(),
        "machine_busy: m1"
    );
}

#[test]
fn the_server_in_memory_answers_the_routes_of_the_feeders_as_the_plugin_does() {
    let mut s = SimServer::new(&["m1"]);
    assert_eq!(
        s.call("GET", "/api/v1/work/machines/zz/feeders", None)
            .unwrap()
            .0,
        404
    );
    assert_eq!(
        s.call(
            "PUT",
            "/api/v1/work/machines/zz/feeders",
            Some(&json!({"slot": 1, "clear": true}))
        )
        .unwrap()
        .0,
        404
    );
    for bad in [
        json!({"slot": 0, "part": "a", "count": 1}),
        json!({"slot": 201, "part": "a", "count": 1}),
        json!({"slot": "1", "part": "a", "count": 1}),
        json!({"slot": 1, "part": "a"}),
        json!({"slot": 1, "part": "a", "count": -1}),
        json!({"slot": 1, "clear": "yes"}),
        json!({}),
    ] {
        let (status, v) = s
            .call("PUT", "/api/v1/work/machines/m1/feeders", Some(&bad))
            .unwrap();
        assert_eq!(
            (status, v["error"].as_str()),
            (422, Some("bad_plugin_request")),
            "{bad}"
        );
    }
    assert_eq!(
        s.call("PUT", "/api/v1/work/machines/m1/feeders", None)
            .unwrap()
            .0,
        422
    );
    let (status, v) = s
        .call(
            "PUT",
            "/api/v1/work/machines/m1/feeders",
            Some(&json!({"slot": 2, "part": "R", "count": 7})),
        )
        .unwrap();
    assert_eq!(
        (
            status,
            v["feeders"][0]["slot"].as_u64(),
            v["feeders"][0]["count"].as_u64()
        ),
        (200, Some(2), Some(7))
    );
    assert_eq!(s.feeder("m1", 2), Some(("R".to_string(), 7)));
    let (_, v) = s
        .call(
            "PUT",
            "/api/v1/work/machines/m1/feeders",
            Some(&json!({"slot": 2, "clear": true})),
        )
        .unwrap();
    assert_eq!(v["feeders"].as_array().unwrap().len(), 0);
    assert_eq!(s.feeder("m1", 2), None);
}

#[test]
fn a_dry_run_goes_through_the_job_and_takes_no_part_off_a_feeder() {
    let mut r = sim();
    let place = loaded(&mut r);
    let before: Vec<_> = (1..=4).map(|s| r.transport.feeder("m1", s)).collect();
    assert!(before.iter().all(Option::is_some));
    let dry = r.create("dry", &dry_run(place.clone())).unwrap();
    assert_eq!(dry.total, 7);
    r.order(&dry.id, Order::Start).unwrap();
    let end = r.follow(&dry.id, 20, &mut |_| {}, &mut || {}).unwrap();
    assert_eq!((end.state.as_str(), end.done), ("done", 7));
    let after: Vec<_> = (1..=4).map(|s| r.transport.feeder("m1", s)).collect();
    assert_eq!(before, after);
    // the same request without the dry run takes the parts
    let real = r.create("real", &place).unwrap();
    r.order(&real.id, Order::Start).unwrap();
    r.follow(&real.id, 20, &mut |_| {}, &mut || {}).unwrap();
    let taken: Vec<_> = (1..=4).map(|s| r.transport.feeder("m1", s)).collect();
    assert_ne!(before, taken);
    // a dry run still needs the parts to be there, and a value that is not true or false is refused
    let mut r = sim();
    let lonely = json!({"kind": "place", "placements": [{"ref": "R1", "part": "R_0603_10k", "x": 1, "y": 2}], "dry_run": true});
    assert!(r
        .create("x", &lonely)
        .unwrap_err()
        .starts_with("pnp_feeder_short"));
    let mut s = SimServer::new(&["m1"]);
    let body = json!({"machine": "m1", "request": {"kind": "place", "placements": [{"ref": "R1", "part": "a", "x": 1, "y": 2}], "dry_run": "yes"}});
    assert_eq!(
        s.call("POST", "/api/v1/work/jobs", Some(&body)).unwrap().0,
        400
    );
    assert_eq!(dry_run(json!(5)), json!(5));
}

#[test]
fn a_calibration_job_keeps_the_offset_of_a_feeder_and_the_server_lists_it() {
    let mut r = sim();
    r.set_feeder(3, "R_0603_10k", 10).unwrap();
    let request = calibrate_request(Calibrate::Feeder, "3", (0.5, -0.25)).unwrap();
    assert_eq!(request["what"], "feeder");
    let job = r.create("cal", &request).unwrap();
    assert_eq!((job.state.as_str(), job.total), ("queued", 1));
    r.order(&job.id, Order::Start).unwrap();
    assert_eq!(r.job(&job.id).unwrap().state, "done");
    assert_eq!(r.transport.feeder_offset("m1", 3), Some((0.5, -0.25)));
    assert_eq!(r.transport.calibrations("m1"), ["feeder:3"]);
    let f = &r.feeders_table().unwrap().feeders[0];
    assert_eq!(f.offset, (0.5, -0.25));
    assert!(
        (f.x_mm - 40.5).abs() < 1e-9 && (f.y_mm + 30.25).abs() < 1e-9,
        "{f:?}"
    );
    // 0 and 0 puts it back; a nozzle and a camera are kept by name
    for (what, target, offset) in [
        (Calibrate::Feeder, "3", (0.0, 0.0)),
        (Calibrate::Nozzle, "N1", (0.1, 0.0)),
        (Calibrate::Camera, "up", (0.0, 0.0)),
    ] {
        let job = r
            .create("cal", &calibrate_request(what, target, offset).unwrap())
            .unwrap();
        r.order(&job.id, Order::Start).unwrap();
    }
    assert_eq!(r.feeders_table().unwrap().feeders[0].offset, (0.0, 0.0));
    assert_eq!(
        r.transport.calibrations("m1"),
        ["feeder:3", "feeder:3", "nozzle:N1", "camera:up"]
    );
}

#[test]
fn a_calibration_the_plugin_would_refuse_is_refused_here_with_the_reason() {
    for (what, target, offset, why) in [
        (Calibrate::Feeder, "0", (0.0, 0.0), "slot, 1 to 200"),
        (Calibrate::Feeder, "201", (0.0, 0.0), "slot, 1 to 200"),
        (Calibrate::Feeder, "left", (0.0, 0.0), "slot, 1 to 200"),
        (Calibrate::Nozzle, "", (0.0, 0.0), "a name"),
        (Calibrate::Camera, "has space", (0.0, 0.0), "a name"),
        (Calibrate::Nozzle, "N1", (5.01, 0.0), "offset on x"),
        (Calibrate::Nozzle, "N1", (0.0, -5.01), "offset on y"),
        (Calibrate::Nozzle, "N1", (f64::NAN, 0.0), "offset on x"),
    ] {
        let e = calibrate_request(what, target, offset).unwrap_err();
        assert!(e.contains(why), "{e}");
    }
    assert!(calibrate_request(Calibrate::Nozzle, "N1", (5.0, -5.0)).is_ok());
    assert_eq!(Calibrate::parse(" Camera "), Some(Calibrate::Camera));
    assert_eq!(Calibrate::parse("laser"), None);
    // the server in memory refuses the same requests with a 400
    let mut s = SimServer::new(&["m1"]);
    for request in [
        json!({"kind": "calibrate"}),
        json!({"kind": "calibrate", "what": "laser", "target": "x"}),
        json!({"kind": "calibrate", "what": "feeder", "target": 0}),
        json!({"kind": "calibrate", "what": "nozzle"}),
        json!({"kind": "calibrate", "what": "nozzle", "target": "N1", "offset_mm": {"x": 6}}),
        json!({"kind": "calibrate", "what": "nozzle", "target": "N1", "offset_mm": 1}),
    ] {
        let body = json!({"machine": "m1", "request": request});
        assert_eq!(
            s.call("POST", "/api/v1/work/jobs", Some(&body)).unwrap().0,
            400,
            "{request}"
        );
    }
}

#[test]
fn the_commands_of_the_feeders_and_of_the_calibration_work_against_the_server_in_memory() {
    let o = run(&["server-feeders", "--sim"]);
    assert!(o.status.success(), "{}", err(&o));
    assert!(
        out(&o).starts_with("0 of 200 slots hold a part, 200 are free, 0 are empty"),
        "{}",
        out(&o)
    );
    let o = run(&["server-feeders", "set", "3", "R_0603_10k", "150", "--sim"]);
    assert!(o.status.success(), "{}", err(&o));
    assert!(
        out(&o).contains("slot 3: R_0603_10k, 150 left, picked at 40.00,-30.00 mm"),
        "{}",
        out(&o)
    );
    let o = run(&["server-feeders", "set", "3", "R_0603_10k", "0", "--sim"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(err(&o).contains("1 feeders hold no parts"), "{}", err(&o));
    assert!(out(&o).contains("(empty)"));
    let o = run(&["server-feeders", "clear", "3", "--sim"]);
    assert!(o.status.success(), "{}", err(&o));
    for bad in [
        vec!["server-feeders", "set", "3", "--sim"],
        vec!["server-feeders", "set", "x", "R", "1", "--sim"],
        vec!["server-feeders", "set", "3", "R", "-1", "--sim"],
        vec!["server-feeders", "clear", "--sim"],
        vec!["server-feeders", "list", "now", "--sim"],
        vec!["server-feeders", "empty", "--sim"],
        vec!["server-feeders", "set", "0", "R", "1", "--sim"],
    ] {
        let o = run(&bad);
        assert_eq!(o.status.code(), Some(1), "{bad:?}");
        assert!(err(&o).starts_with("error: "), "{bad:?} {}", err(&o));
    }
    let o = run(&[
        "server-calibrate",
        "feeder",
        "3",
        "--offset",
        "0.5,-0.25",
        "--sim",
    ]);
    assert!(o.status.success(), "{}", err(&o));
    assert!(out(&o).contains("done, 1 of 1 (100%)"), "{}", out(&o));
    let o = run(&["server-calibrate", "nozzle", "N1", "--sim"]);
    assert!(o.status.success(), "{}", err(&o));
    for bad in [
        vec!["server-calibrate", "--sim"],
        vec!["server-calibrate", "feeder", "--sim"],
        vec!["server-calibrate", "laser", "1", "--sim"],
        vec!["server-calibrate", "feeder", "999", "--sim"],
        vec![
            "server-calibrate",
            "nozzle",
            "N1",
            "--offset",
            "9,0",
            "--sim",
        ],
        vec!["server-calibrate", "nozzle", "N1", "--offset", "1", "--sim"],
        vec![
            "server-calibrate",
            "nozzle",
            "N1",
            "--timeout",
            "0",
            "--sim",
        ],
    ] {
        let o = run(&bad);
        assert_eq!(o.status.code(), Some(1), "{bad:?}");
    }
    // a dry run says so, and the job still places every part
    let o = run(&[
        "server-send",
        "examples/board.pos",
        "examples/feeders.csv",
        "--sim",
        "--follow",
        "--dry-run",
    ]);
    assert!(o.status.success(), "{}", err(&o));
    assert!(
        out(&o).contains("(dry run: no part is taken off a feeder)"),
        "{}",
        out(&o)
    );
    assert!(out(&o).contains("done, 7 of 7 (100%)"), "{}", out(&o));
}

#[test]
fn the_commands_of_the_feeders_write_a_table_and_never_over_a_file() {
    let dir = temp("feedersout");
    let file = dir
        .join("feeders_server.csv")
        .to_string_lossy()
        .into_owned();
    let o = run(&[
        "server-feeders",
        "set",
        "2",
        "C_0603_100n",
        "40",
        "--sim",
        "--out",
        &file,
    ]);
    assert!(o.status.success(), "{}", err(&o));
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "slot,part,count,x_mm,y_mm,offset_x_mm,offset_y_mm\n2,C_0603_100n,40,30.000,-30.000,0.000,0.000\n"
    );
    let o = run(&["server-feeders", "list", "--sim", "--out", &file]);
    assert_eq!(o.status.code(), Some(1));
    assert!(err(&o).contains("PHOENIX-PNP-"), "{}", err(&o));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_commands_of_the_feeders_talk_to_a_server_with_the_token_and_do_not_print_it() {
    let (address, seen, handle) = serve(3);
    let dir = temp("feedersnet");
    let cfg = config(&dir, &address, "server");
    let o = run(&[
        "server-feeders",
        "set",
        "5",
        "R_0603_10k",
        "12",
        "--config",
        &cfg,
    ]);
    assert!(o.status.success(), "{}", err(&o));
    assert!(
        out(&o).contains("slot 5: R_0603_10k, 12 left"),
        "{}",
        out(&o)
    );
    let o = run(&["server-feeders", "--config", &cfg]);
    assert!(o.status.success(), "{}", err(&o));
    let o = run(&["server-feeders", "clear", "5", "--config", &cfg]);
    assert!(o.status.success(), "{}", err(&o));
    handle.join().unwrap();
    // each command is a new process with a new server in memory behind the socket... but the socket server is one: the table it holds is the same
    assert_eq!(seen.lock().unwrap().len(), 3);
    for text in [out(&o), err(&o)] {
        assert!(!text.contains("never-print-this"), "{text}");
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_new_commands_do_nothing_in_the_local_mode_either() {
    let dir = temp("local2");
    let cfg = config(&dir, "127.0.0.1:1", "local");
    for args in [
        vec!["server-feeders", "--config", &cfg],
        vec!["server-feeders", "set", "1", "R", "1", "--config", &cfg],
        vec!["server-calibrate", "feeder", "1", "--config", &cfg],
    ] {
        let o = run(&args);
        assert_eq!(o.status.code(), Some(1), "{args:?}");
        assert!(err(&o).contains("mode is local"), "{}", err(&o));
    }
    let _ = std::fs::remove_dir_all(dir);
}

// Bug hunt (round 4): what the server answers with numbers that are too large, what is written for it when a number is not one, and the names and ids that go into its requests.
mod hunt_round4 {
    use eh3d_phoenix_pnp::parts::{Part, Side};
    use eh3d_phoenix_pnp::server::{
        calibrate_request, clear_body, dry_run, error_text, feeder_body, feeders_request, label,
        parse_response, part_name, placements, safe_segment, Calibrate, FeederRow, FeederState,
        FeederTable, JobInfo, MachineInfo, Mode, MAX_COUNT, MAX_OFFSET_MM, MAX_PLACEMENTS,
        MAX_SLOTS,
    };
    use serde_json::{json, Value};

    fn part(reference: &str, x: f64, y: f64, rotation: f64) -> Part {
        Part {
            reference: reference.to_string(),
            value: "100n".to_string(),
            package: "0402".to_string(),
            x,
            y,
            rotation,
            side: Side::Top,
        }
    }

    fn row() -> FeederRow {
        FeederRow {
            id: "f1".to_string(),
            slot: 1,
            part: "0402_100n".to_string(),
            count: 10,
            key: "100n|0402".to_string(),
        }
    }

    fn feeder(slot: u32, part: &str, count: u64) -> FeederState {
        FeederState {
            slot,
            part: part.to_string(),
            count,
            empty: false,
            x_mm: 1.0,
            y_mm: 2.0,
            offset: (0.0, 0.0),
        }
    }

    // ---- what the server answers ----

    #[test]
    fn a_job_with_a_huge_count_of_lines_does_not_overflow_the_percent() {
        let at = |done, total| {
            JobInfo {
                done,
                total,
                ..JobInfo::default()
            }
            .percent()
        };
        assert_eq!(at(u64::MAX, u64::MAX), 100);
        assert_eq!(at(1 << 62, 1 << 63), 50);
        assert_eq!(at(u64::MAX, 1), 100);
        assert_eq!(at(0, u64::MAX), 0);
        assert_eq!(at(5, 0), 0);
        assert_eq!(at(0, 0), 0);
        assert_eq!(at(1, 3), 33);
        assert_eq!(at(9, 3), 100);
    }

    #[test]
    fn an_answer_with_the_wrong_types_gives_an_empty_job_and_an_empty_machine() {
        for v in [
            json!(null),
            json!([]),
            json!("x"),
            json!(5),
            json!({"id": 5, "state": null, "done": "7", "total": -1, "message": [1]}),
            json!({"done": 1.5, "total": 2.5}),
        ] {
            let j = JobInfo::from_json(&v);
            assert_eq!(j, JobInfo::default(), "{v}");
            assert_eq!(j.percent(), 0);
            assert!(!j.finished());
            let m = MachineInfo::from_json(&v);
            assert_eq!(m, MachineInfo::default(), "{v}");
        }
    }

    #[test]
    fn the_state_of_a_job_decides_whether_it_is_over_and_the_line_says_it() {
        for (state, over) in [
            ("done", true),
            ("failed", true),
            ("cancelled", true),
            ("running", false),
            ("queued", false),
            ("DONE", false),
            ("", false),
        ] {
            let j = JobInfo {
                state: state.to_string(),
                ..JobInfo::default()
            };
            assert_eq!(j.finished(), over, "{state}");
        }
        let j = JobInfo {
            id: "j1".to_string(),
            state: "running".to_string(),
            done: 1,
            total: 2,
            message: "half".to_string(),
        };
        assert_eq!(j.line(), "j1: running, 1 of 2 (50%) - half");
        let quiet = JobInfo {
            message: String::new(),
            ..j
        };
        assert_eq!(quiet.line(), "j1: running, 1 of 2 (50%)");
    }

    #[test]
    fn a_slot_that_does_not_fit_32_bits_is_zero_and_never_wraps_to_another_slot() {
        let table = FeederTable::from_json(&json!({
            "slots": 4_294_967_296u64 + 5,
            "feeders": [
                {"slot": 4_294_967_297u64, "part": "a", "count": 1},
                {"slot": 7, "part": "b", "count": 2},
                {"slot": u64::MAX, "part": "c", "count": 3},
                {"slot": -1, "part": "d", "count": 4},
                {"slot": "9", "part": "e", "count": 5},
                {"slot": 4_294_967_295u64, "part": "f", "count": 6}
            ],
            "free": [1, 4_294_967_296u64, 2, u64::MAX, -3, 4_294_967_295u64]
        }));
        assert_eq!(table.slots, 0);
        let slots: Vec<u32> = table.feeders.iter().map(|f| f.slot).collect();
        assert_eq!(slots, vec![0, 7, 0, 0, 0, 4_294_967_295]);
        assert_eq!(table.free, vec![1, 2, 4_294_967_295]);
    }

    #[test]
    fn a_machine_with_a_slot_that_does_not_fit_32_bits_shows_slot_zero() {
        let m = MachineInfo::from_json(&json!({
            "id": "m1",
            "detail": {"feeders": [{"slot": 4_294_967_297u64, "part": "a", "count": 2}, {"slot": 3, "part": "b", "count": 1}]}
        }));
        assert_eq!(m.id, "m1");
        assert_eq!(
            m.feeders,
            vec![(0, "a".to_string(), 2), (3, "b".to_string(), 1)]
        );
    }

    #[test]
    fn a_table_that_says_nothing_is_empty_and_never_panics() {
        for v in [
            json!(null),
            json!({}),
            json!([]),
            json!({"feeders": "x", "free": 5, "slots": "many"}),
        ] {
            let t = FeederTable::from_json(&v);
            assert_eq!(t, FeederTable::default(), "{v}");
            assert_eq!(t.empty_count(), 0);
            assert!(t
                .text()
                .starts_with("0 of 0 slots hold a part, 0 are free, 0 are empty"));
            assert_eq!(
                t.csv(),
                "slot,part,count,x_mm,y_mm,offset_x_mm,offset_y_mm\n"
            );
        }
    }

    #[test]
    fn the_table_sums_up_the_slots_and_counts_the_empty_ones() {
        let mut empty = feeder(2, "b", 0);
        empty.offset = (0.5, -0.25);
        let table = FeederTable {
            slots: 5,
            feeders: vec![
                feeder(1, "a", 3),
                empty,
                FeederState {
                    empty: true,
                    ..feeder(3, "c", 9)
                },
            ],
            free: vec![4, 5],
        };
        assert_eq!(table.empty_count(), 2);
        let text = table.text();
        assert!(
            text.starts_with("3 of 5 slots hold a part, 2 are free, 2 are empty\n"),
            "{text}"
        );
        assert!(
            text.contains(
                "slot 2: b, 0 left (empty), picked at 1.00,2.00 mm, calibrated by 0.500,-0.250 mm"
            ),
            "{text}"
        );
        assert!(!text.contains("slot 1: a, 3 left (empty)"));
    }

    #[test]
    fn a_part_of_the_server_that_starts_like_a_formula_is_written_as_text_in_the_csv() {
        for (part, cell) in [
            ("=1+1", "'=1+1"),
            ("+3V3", "'+3V3"),
            ("-x", "'-x"),
            ("@SUM(A1)", "'@SUM(A1)"),
            ("0402_100n", "0402_100n"),
            ("a=b", "a=b"),
        ] {
            let table = FeederTable {
                slots: 1,
                feeders: vec![feeder(1, part, 1)],
                free: vec![],
            };
            let csv = table.csv();
            let line = csv.lines().nth(1).unwrap();
            assert_eq!(line.split(',').nth(1).unwrap(), cell, "{part}");
        }
        let table = FeederTable {
            slots: 1,
            feeders: vec![feeder(1, "-a,b", 1), feeder(2, "say \"hi\"", 1)],
            free: vec![],
        };
        let csv = table.csv();
        assert!(csv.contains(",\"'-a,b\","), "{csv}");
        assert!(csv.contains(",\"say \"\"hi\"\"\","), "{csv}");
    }

    #[test]
    fn what_the_server_refuses_is_told_in_one_line_whatever_the_types() {
        assert_eq!(
            error_text(400, &json!({"error": "bad", "detail": "why"})),
            "bad: why"
        );
        assert_eq!(
            error_text(400, &json!({"error": "bad", "detail": ""})),
            "bad"
        );
        assert_eq!(error_text(400, &json!({"error": "bad"})), "bad");
        assert_eq!(error_text(500, &json!({})), "the server answered 500");
        assert_eq!(error_text(502, &json!(null)), "the server answered 502");
        assert_eq!(
            error_text(404, &json!({"error": 5, "detail": []})),
            "the server answered 404"
        );
        assert_eq!(error_text(0, &json!("x")), "the server answered 0");
    }

    #[test]
    fn a_response_that_is_not_http_is_refused_and_one_with_a_body_that_is_not_json_has_a_null_body()
    {
        assert!(parse_response(b"").is_err());
        assert!(parse_response(b"not http at all").is_err());
        let (status, body) =
            parse_response(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 4\r\n\r\noops").unwrap();
        assert_eq!(status, 502);
        assert_eq!(body, Value::Null);
        let (status, body) =
            parse_response(b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\n\r\n{\"ok\":true}").unwrap();
        assert_eq!(status, 200);
        assert_eq!(body, json!({"ok": true}));
    }

    // ---- what is written for the server ----

    #[test]
    fn a_part_with_a_position_or_a_rotation_that_is_not_a_number_is_not_put_in_the_request() {
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            for parts in [
                vec![part("R1", bad, 1.0, 0.0)],
                vec![part("R1", 1.0, bad, 0.0)],
                vec![part("R1", 1.0, 1.0, bad)],
            ] {
                let e = placements(&parts, &[row()], (0.0, 0.0)).unwrap_err();
                assert!(e.contains("R1") && e.contains("not a number"), "{e}");
            }
        }
    }

    #[test]
    fn an_offset_of_the_board_that_is_not_a_number_is_refused() {
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(placements(&[part("R1", 1.0, 1.0, 0.0)], &[row()], (bad, 0.0)).is_err());
            assert!(placements(&[part("R1", 1.0, 1.0, 0.0)], &[row()], (0.0, bad)).is_err());
        }
    }

    #[test]
    fn a_part_that_no_feeder_holds_is_left_out_with_its_reference_and_the_numbers_are_kept() {
        let mut other = part("R2", 3.0, 4.0, 90.0);
        other.value = "1u".to_string();
        let p = placements(&[part("R1", 1.5, -2.5, 270.0), other], &[row()], (0.0, 0.0)).unwrap();
        assert_eq!(p.count, 1);
        assert_eq!(p.unmatched, vec!["R2".to_string()]);
        let first = &p.request["placements"][0];
        assert_eq!(first["x"], json!(1.5));
        assert_eq!(first["y"], json!(-2.5));
        assert_eq!(first["rot"], json!(270.0));
        assert_eq!(first["side"], json!("top"));
        assert_eq!(first["part"], json!("0402_100n"));
        assert!(p.request.get("offset").is_none());
    }

    #[test]
    fn the_offset_is_written_only_when_it_is_not_zero_and_a_negative_zero_is_zero() {
        let with = |o: (f64, f64)| {
            placements(&[part("R1", 1.0, 1.0, 0.0)], &[row()], o)
                .unwrap()
                .request
        };
        assert!(with((0.0, 0.0)).get("offset").is_none());
        assert!(with((-0.0, 0.0)).get("offset").is_none());
        assert_eq!(with((1.5, -2.0))["offset"], json!({"x": 1.5, "y": -2.0}));
    }

    #[test]
    fn two_references_that_become_the_same_name_for_the_server_are_refused() {
        let e = placements(
            &[part("R 1", 1.0, 1.0, 0.0), part("R_1", 2.0, 2.0, 0.0)],
            &[row()],
            (0.0, 0.0),
        )
        .unwrap_err();
        assert!(e.contains("also"), "{e}");
        assert!(placements(
            &[part("R1", 1.0, 1.0, 0.0), part("R1", 2.0, 2.0, 0.0)],
            &[row()],
            (0.0, 0.0)
        )
        .is_err());
    }

    #[test]
    fn a_job_of_more_placements_than_the_server_takes_is_refused() {
        let parts: Vec<Part> = (0..=MAX_PLACEMENTS)
            .map(|i| part(&format!("R{i}"), 1.0, 1.0, 0.0))
            .collect();
        let e = placements(&parts, &[row()], (0.0, 0.0)).unwrap_err();
        assert!(e.contains("placements"), "{e}");
    }

    #[test]
    fn the_request_that_loads_the_feeders_keeps_the_slots_the_parts_and_the_counts() {
        let v = feeders_request(&[row()], true);
        assert_eq!(v["kind"], json!("load_feeders"));
        assert_eq!(v["replace"], json!(true));
        assert_eq!(
            v["feeders"],
            json!([{"slot": 1, "part": "0402_100n", "count": 10}])
        );
        assert_eq!(feeders_request(&[], false)["feeders"], json!([]));
    }

    #[test]
    fn a_dry_run_marks_an_object_and_leaves_anything_else_alone() {
        assert_eq!(dry_run(json!({"kind": "place"}))["dry_run"], json!(true));
        assert_eq!(dry_run(json!([1, 2])), json!([1, 2]));
        assert_eq!(dry_run(json!(null)), json!(null));
        assert_eq!(dry_run(json!("x")), json!("x"));
    }

    // ---- names and ids ----

    #[test]
    fn an_id_that_could_change_the_path_of_a_request_is_refused() {
        for bad in [
            "",
            "..",
            ".",
            "a/b",
            "a\\b",
            "../x",
            "a b",
            "a%2e",
            "a\n",
            "a\0",
            "\u{e9}",
            "a;b",
            "a?b",
            "a#b",
            "a:b",
            "@",
            "a@b",
            "user:pw@host",
            "C:\\x",
            "/abs",
        ] {
            assert!(!safe_segment(bad), "{bad:?}");
        }
        assert!(safe_segment("a"));
        assert!(safe_segment("Machine_1-b"));
        assert!(safe_segment(&"x".repeat(64)));
        assert!(!safe_segment(&"x".repeat(65)));
    }

    #[test]
    fn a_name_for_the_server_keeps_its_characters_cuts_by_characters_and_is_never_empty() {
        assert_eq!(label("", 10, ""), "_");
        assert_eq!(label("   ", 10, ""), "_");
        assert_eq!(label(" a b ", 10, ""), "a_b");
        assert_eq!(label("a.b", 10, "."), "a.b");
        assert_eq!(label("a.b", 10, ""), "a_b");
        assert_eq!(label("../x", 10, "_-."), ".._x");
        assert_eq!(label("\u{1f600}\u{1f600}\u{1f600}", 2, ""), "__");
        assert_eq!(label("abcdef", 3, ""), "abc");
        assert_eq!(label("ab\ncd\u{0}", 10, ""), "ab_cd_");
        assert_eq!(label("\u{e9}\u{65e5}", 10, ""), "__");
    }

    #[test]
    fn a_name_is_never_longer_than_asked_and_only_has_allowed_characters() {
        for text in [
            "",
            "a",
            "A very long text with many words and symbols !\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~",
            "\u{1f600}\u{65e5}\u{e9}x",
        ] {
            for max in [1usize, 2, 5, 64] {
                let l = label(text, max, "_-");
                assert!(!l.is_empty() && l.chars().count() <= max, "{text:?} {max}");
                assert!(
                    l.chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
                    "{l}"
                );
            }
        }
    }

    #[test]
    fn the_name_of_a_part_joins_the_package_and_the_value_and_stays_inside_sixty_four_characters() {
        assert_eq!(part_name("100n", "0402"), "0402_100n");
        assert_eq!(part_name(" 100n ", " 0402 "), "0402_100n");
        assert_eq!(part_name("1\u{b5}F", "0603"), "0603_1_F");
        assert_eq!(part_name("a b", "c d"), "c_d_a_b");
        assert_eq!(part_name(&"v".repeat(100), "p").chars().count(), 64);
        assert_eq!(part_name("", ""), "_");
    }

    #[test]
    fn a_slot_and_a_count_have_exact_edges() {
        assert!(feeder_body(1, "p", 0).is_ok());
        assert!(feeder_body(MAX_SLOTS, "p", MAX_COUNT).is_ok());
        assert!(feeder_body(0, "p", 0).is_err());
        assert!(feeder_body(MAX_SLOTS + 1, "p", 0).is_err());
        assert!(feeder_body(u32::MAX, "p", 0).is_err());
        assert!(feeder_body(1, "p", MAX_COUNT + 1).is_err());
        assert!(feeder_body(1, "p", u64::MAX).is_err());
        assert!(clear_body(1).is_ok());
        assert!(clear_body(MAX_SLOTS).is_ok());
        assert!(clear_body(0).is_err());
        assert!(clear_body(MAX_SLOTS + 1).is_err());
        assert_eq!(clear_body(7).unwrap(), json!({"slot": 7, "clear": true}));
    }

    #[test]
    fn the_name_of_a_part_in_a_request_has_one_to_sixty_four_allowed_characters() {
        assert!(feeder_body(1, &"x".repeat(64), 1).is_ok());
        assert!(feeder_body(1, &"x".repeat(65), 1).is_err());
        assert!(feeder_body(1, "a/b.c-d_e+f:g", 1).is_ok());
        for bad in [
            "", "a b", "a\n", "\u{e9}", "a;b", "a\"b", "a\\b", "a%b", "a*b",
        ] {
            assert!(feeder_body(1, bad, 1).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_calibration_of_a_feeder_names_a_slot_and_the_offset_has_exact_edges() {
        for ok in ["1", "200", " 7 "] {
            assert!(
                calibrate_request(Calibrate::Feeder, ok, (0.0, 0.0)).is_ok(),
                "{ok:?}"
            );
        }
        for bad in [
            "0",
            "201",
            "",
            "-1",
            "abc",
            "1.5",
            "4294967297",
            "1e1",
            "0x10",
            "1 2",
        ] {
            assert!(
                calibrate_request(Calibrate::Feeder, bad, (0.0, 0.0)).is_err(),
                "{bad:?}"
            );
        }
        assert!(
            calibrate_request(Calibrate::Nozzle, "n1", (MAX_OFFSET_MM, -MAX_OFFSET_MM)).is_ok()
        );
        for bad in [
            MAX_OFFSET_MM + 0.001,
            -MAX_OFFSET_MM - 0.001,
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ] {
            assert!(
                calibrate_request(Calibrate::Camera, "cam", (bad, 0.0)).is_err(),
                "{bad}"
            );
            assert!(
                calibrate_request(Calibrate::Camera, "cam", (0.0, bad)).is_err(),
                "{bad}"
            );
        }
    }

    #[test]
    fn a_calibration_of_a_nozzle_or_a_camera_names_it_with_up_to_sixty_four_characters() {
        assert!(calibrate_request(Calibrate::Nozzle, &"n".repeat(64), (0.0, 0.0)).is_ok());
        assert!(calibrate_request(Calibrate::Nozzle, &"n".repeat(65), (0.0, 0.0)).is_err());
        for bad in ["", "   ", "a b", "a;b", "\u{e9}", "a\nb"] {
            assert!(
                calibrate_request(Calibrate::Nozzle, bad, (0.0, 0.0)).is_err(),
                "{bad:?}"
            );
        }
        let v = calibrate_request(Calibrate::Camera, " cam-1 ", (1.0, 2.0)).unwrap();
        assert_eq!(v["target"], json!("cam-1"));
        assert_eq!(v["what"], json!("camera"));
        assert_eq!(v["offset_mm"], json!({"x": 1.0, "y": 2.0}));
    }

    #[test]
    fn the_words_of_the_modes_and_of_the_calibrations_read_back_their_own_names() {
        for m in [Mode::Local, Mode::Server] {
            assert_eq!(Mode::parse(m.name()), Some(m));
        }
        assert_eq!(Mode::parse(" SERVER "), Some(Mode::Server));
        for c in [Calibrate::Feeder, Calibrate::Nozzle, Calibrate::Camera] {
            assert_eq!(Calibrate::parse(c.word()), Some(c));
        }
        assert_eq!(Calibrate::parse(" Camera "), Some(Calibrate::Camera));
        for bad in ["", "remote", "loca l", "server\n x", "feeders", "\u{e9}"] {
            assert_eq!(Mode::parse(bad), None, "{bad:?}");
            assert_eq!(Calibrate::parse(bad), None, "{bad:?}");
        }
    }
}
