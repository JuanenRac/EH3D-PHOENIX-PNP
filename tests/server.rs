//! The way to the EH3D server: the settings of `mode = server`, the requests that the feeders and the placements make, the server in memory and its rules, the plain HTTP transport against a
//! small server that is made here, and the commands `server-send`, `server-status` and `server-job`.

use eh3d_phoenix_pnp::config::{MachineConfig, KEYS};
use eh3d_phoenix_pnp::feeding::from_csv as feeders_from_csv;
use eh3d_phoenix_pnp::parts::{parse, Part};
use eh3d_phoenix_pnp::server::{
    error_text, feeder_rows, feeders_request, label, parse_response, part_name, placements,
    safe_segment, HttpTransport, JobInfo, Mode, Order, Remote, SimServer, Target, Transport,
};
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
    let t = Target::new(&address, "the-token", "m1", 5.0, 50, 1).unwrap();
    let mut http = HttpTransport::new(&t);
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
    let t = Target::new(&address, "", "m1", 1.0, 50, 1).unwrap();
    let why = HttpTransport::new(&t).call("GET", "/x", None).unwrap_err();
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
