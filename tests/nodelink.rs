//! The link to the head node and to the feeder node, tried against nodes on a computer that speak their contract: the states read from the answers, the orders and their refusals, a refused
//! token, silence, the hold that keeps a machine still, faults drawn from a fixed seed, and the interlock of the axes that reads whether the head is clear.

use eh3d_phoenix_pnp::driver::{Kind, Session};
use eh3d_phoenix_pnp::gcodedriver::{DriverProfile, Target};
use eh3d_phoenix_pnp::gcodesim::{Faults, GcodeSim, SimLink};
use eh3d_phoenix_pnp::nodelink::{
    xy_clear_signal, ChainStatus, Feeders, Head, HeadStatus, Hold, LinkError, NodeLink, ScNodeLink,
    XY_CLEAR,
};
use eh3d_phoenix_pnp::nodesim::{Inject, Injection, SimFeeders, SimHead};
use eh3d_phoenix_pnp::slotbank::Seen;
use std::path::PathBuf;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_phoenix-pnp");
const MARLIN: &str = include_str!("../examples/driver_marlin.cfg");

fn over_http(sim: &SimHead, token: &str) -> ScNodeLink {
    ScNodeLink::new("sim:1", token, 2, 0, Box::new(sim.clone())).without_waiting()
}

fn refused(e: LinkError) -> (u16, Option<String>) {
    match e {
        LinkError::Refused { status, why } => (status, why),
        other => panic!("not a refusal: {other:?}"),
    }
}

#[test]
fn the_state_of_the_head_is_read_from_its_answer() {
    let sim = SimHead::new(2, "t");
    let mut head = Head::new(sim.clone(), 3);
    let s = head.refresh().unwrap().clone();
    assert!(s.xy_clear && !s.safe && !s.xy_moving);
    assert_eq!(s.nozzles.len(), 2);
    assert_eq!(s.nozzles[0].n, 1);
    assert!(s.nozzles[0].raised && !s.nozzles[0].part);
    assert_eq!(s.selfcheck, "ok");
    assert!(s.blocking.is_empty());
    sim.set_z(2, 0.0);
    let s = head.refresh().unwrap().clone();
    assert!(!s.xy_clear);
    assert_eq!(s.blocking, [2]);
    assert!(!s.nozzles[1].raised);
}

#[test]
fn the_machine_may_move_only_while_every_nozzle_is_raised_and_the_head_keeps_it_so() {
    let sim = SimHead::new(2, "t");
    let mut head = Head::new(sim.clone(), 3);
    sim.set_z(1, 1.0);
    let (status, why) = refused(head.set_xy_moving(true).unwrap_err());
    assert_eq!(
        (status, why.as_deref()),
        (409, Some("interlock_xy_nozzle_low"))
    );
    assert!(!sim.xy_moving());
    sim.set_z(1, 5.0);
    head.set_xy_moving(true).unwrap();
    assert!(sim.xy_moving());
    // while the machine holds the permission a nozzle is not lowered
    let (status, why) = refused(head.move_z(1, 0.0).unwrap_err());
    assert_eq!((status, why.as_deref()), (409, Some("interlock_xy_moving")));
    head.move_z(1, 6.0).unwrap();
    head.set_xy_moving(false).unwrap();
    head.move_z(1, 0.0).unwrap();
    assert!(!head.xy_clear());
}

#[test]
fn a_pick_a_place_and_the_refusals_of_the_vacuum() {
    let sim = SimHead::new(2, "t");
    let mut head = Head::new(sim.clone(), 3);
    head.pick(1).unwrap();
    assert_eq!(sim.picks(1), 1);
    assert!(head.refresh().unwrap().nozzles[0].part);
    assert_eq!(
        refused(head.pick(1).unwrap_err()).1.as_deref(),
        Some("busy")
    );
    head.place(1).unwrap();
    assert_eq!(
        refused(head.place(1).unwrap_err()).1.as_deref(),
        Some("not_holding")
    );
    sim.set_no_part(2, true);
    assert_eq!(
        refused(head.pick(2).unwrap_err()).1.as_deref(),
        Some("vacuum_not_reached")
    );
    // a nozzle that does not exist is not asked
    let asked = sim.requests();
    for n in [0u8, 7, 200] {
        assert!(matches!(head.pick(n), Err(LinkError::Bad(_))));
    }
    assert_eq!(sim.requests(), asked);
    // the node itself refuses a number that is not one of its nozzles
    let mut raw = sim.clone();
    let e = raw.call("POST", "/api/nozzle/pick?n=3", true).unwrap_err();
    assert_eq!(refused(e).0, 400);
}

#[test]
fn a_refused_token_is_told_and_the_state_is_still_read_without_it() {
    let sim = SimHead::new(2, "t");
    let mut head = Head::new(over_http(&sim, "wrong"), 3);
    // the state needs no token
    assert!(head.refresh().unwrap().xy_clear);
    assert_eq!(head.pick(1), Err(LinkError::Unauthorized));
    assert_eq!(sim.picks(1), 0);
    // a node with no token refuses every order, even with the right word
    let none = SimHead::new(2, "");
    let mut head = Head::new(over_http(&none, "t"), 3);
    assert_eq!(head.set_xy_moving(true), Err(LinkError::Unauthorized));
    assert_eq!(head.refresh().unwrap().selfcheck, "warn");
    // a refusal is not silence: the guard does not count it
    assert_eq!(head.guard().silent_run(), 0);
}

#[test]
fn a_head_that_cannot_be_heard_is_never_taken_to_be_clear_and_is_held_until_it_answers_and_a_person_acknowledges(
) {
    let sim = SimHead::new(2, "t");
    sim.set_injection(Injection::scripted(vec![Some(Inject::Silence); 3]));
    let mut head = Head::new(over_http(&sim, "t"), 3);
    for i in 1..=3 {
        assert!(!head.xy_clear(), "{i}");
    }
    assert_eq!(head.guard().held(), Some(Hold::Silent));
    // held: an order does not even reach the node
    let asked = sim.requests();
    assert_eq!(head.pick(1), Err(LinkError::Held(Hold::Silent)));
    assert_eq!(head.set_xy_moving(true), Err(LinkError::Held(Hold::Silent)));
    assert_eq!(sim.requests(), asked);
    // the head answers again, but is not taken for clear until a person acknowledges
    assert!(!head.xy_clear());
    assert_eq!(head.guard().silent_run(), 0);
    assert!(head.acknowledge());
    assert!(head.xy_clear());
    head.pick(1).unwrap();
    // an acknowledgement while the node is silent does not take
    sim.set_injection(Injection::scripted(vec![Some(Inject::Silence); 3]));
    for _ in 0..3 {
        head.refresh().unwrap_err();
    }
    assert_eq!(head.guard().held(), Some(Hold::Silent));
    sim.set_injection(Injection::scripted(vec![Some(Inject::Silence)]));
    head.refresh().unwrap_err();
    assert!(!head.acknowledge());
    // letting the permission go is always tried, even when held
    assert!(head.set_xy_moving(false).is_ok());
}

#[test]
fn a_head_in_its_safe_state_is_held_and_its_own_acknowledgement_lets_it_go() {
    let sim = SimHead::new(2, "t");
    let mut head = Head::new(sim.clone(), 3);
    sim.set_safe(true);
    assert!(!head.xy_clear());
    assert_eq!(head.guard().held(), Some(Hold::Safe));
    assert_eq!(head.last().map(|s| s.safe), Some(true));
    assert_eq!(head.pick(1), Err(LinkError::Held(Hold::Safe)));
    head.acknowledge_node().unwrap();
    assert_eq!(head.guard().held(), None);
    assert!(head.xy_clear());
    head.pick(1).unwrap();
}

#[test]
fn the_faults_drawn_from_a_seed_come_the_same_every_time_and_a_run_ends_in_a_sound_state() {
    fn run(seed: u64) -> (Vec<String>, u64) {
        let sim = SimHead::new(2, "t");
        sim.set_injection(Injection::seeded(seed, 80, 60, 100));
        // the client does not ask again, so every fault is seen
        let mut head = Head::new(over_http(&sim, "t"), 1000);
        let mut log = Vec::new();
        for step in 0..300u32 {
            let line = match step % 3 {
                0 => format!("{:?}", head.set_xy_moving(true).map_err(|e| e.to_string())),
                1 => format!("{:?}", head.pick(1).map_err(|e| e.to_string())),
                _ => format!("{:?}", head.set_xy_moving(false).map_err(|e| e.to_string())),
            };
            log.push(line);
        }
        (log, sim.requests())
    }
    let (a, n) = run(7);
    let (b, m) = run(7);
    assert_eq!(a, b);
    assert_eq!(n, m);
    assert_ne!(a, run(8).0);
    let silent = a.iter().filter(|l| l.contains("does not answer")).count();
    let token = a.iter().filter(|l| l.contains("refuses the token")).count();
    let interlock = a.iter().filter(|l| l.contains("refuses (409)")).count();
    assert!(
        silent > 5 && token > 5 && interlock > 5,
        "{silent} {token} {interlock}"
    );
    // about the rates asked for, with a seed that is fixed
    assert!(silent < 60 && token < 50, "{silent} {token}");
}

#[test]
fn the_answers_of_a_node_that_cannot_be_read_are_refused_not_guessed() {
    let ok = r#"{"safe":false,"xy_clear":true,"xy_moving":false,"xy_blocking":"","z_safe_mm":5.0,"selfcheck":"ok","nozzles":[]}"#;
    assert!(HeadStatus::from_body(ok).is_ok());
    for bad in [
        "",
        "not json",
        "{}",
        r#"{"safe":"yes","xy_clear":true,"xy_moving":false,"xy_blocking":"","z_safe_mm":5.0,"selfcheck":"ok"}"#,
        r#"{"safe":false,"xy_clear":true,"xy_moving":false,"xy_blocking":"a,b","z_safe_mm":5.0,"selfcheck":"ok"}"#,
        r#"{"safe":false,"xy_clear":true,"xy_moving":false,"xy_blocking":"","selfcheck":"ok"}"#,
    ] {
        assert!(
            matches!(HeadStatus::from_body(bad), Err(LinkError::Bad(_))),
            "{bad:?}"
        );
    }
    let many: Vec<String> = (1..=7)
        .map(|n| format!(r#"{{"n":{n},"phase":"idle","part":false,"vacuum_ok":false,"raised":true,"z_mm":5.0}}"#))
        .collect();
    let seven = ok.replace(
        r#""nozzles":[]"#,
        &format!(r#""nozzles":[{}]"#, many.join(",")),
    );
    assert!(matches!(
        HeadStatus::from_body(&seven),
        Err(LinkError::Bad(_))
    ));
    assert!(matches!(
        ChainStatus::from_body("{}"),
        Err(LinkError::Bad(_))
    ));
    // a key the program does not know is ignored
    let extra = ok.replace(
        r#""selfcheck":"ok""#,
        r#""selfcheck":"ok","something_new":1"#,
    );
    assert!(HeadStatus::from_body(&extra).is_ok());
}

#[test]
fn the_feeders_advance_run_out_jam_and_are_cleared_through_the_link() {
    let sim = SimFeeders::new(4, Some(3), "t");
    let mut node = Feeders::new(sim.clone(), 3);
    let s = node.refresh().unwrap().clone();
    assert_eq!((s.slots, s.online, s.safe), (4, 4, false));
    assert_eq!(s.feeders[0].remaining, Some(3));
    node.advance(1, 2).unwrap();
    assert_eq!(sim.advances(1), 2);
    assert_eq!(node.refresh().unwrap().feeders[0].remaining, Some(1));
    assert!(node.refresh().unwrap().feeders[0].low);
    node.advance(1, 1).unwrap();
    assert_eq!(
        refused(node.advance(1, 1).unwrap_err()).1.as_deref(),
        Some("empty")
    );
    assert_eq!(node.refresh().unwrap().feeders[0].seen(), Seen::Empty);
    // a feeder that does not answer is a 503, and is not taken for a fault
    sim.set_online(2, false);
    let (status, why) = refused(node.advance(2, 1).unwrap_err());
    assert_eq!((status, why.as_deref()), (503, Some("offline")));
    assert_eq!(node.refresh().unwrap().feeders[1].seen(), Seen::Unknown);
    // a jam stays until its cause is gone
    sim.jam(3);
    assert_eq!(
        refused(node.advance(3, 1).unwrap_err()).1.as_deref(),
        Some("fault")
    );
    assert_eq!(node.refresh().unwrap().feeders[2].seen(), Seen::Fault);
    assert_eq!(
        refused(node.ack(3).unwrap_err()).1.as_deref(),
        Some("still_active")
    );
    sim.clear_cause(3);
    node.ack(3).unwrap();
    node.advance(3, 1).unwrap();
    // arguments that make no sense are not asked
    let asked = sim.requests();
    for (slot, count) in [(0u8, 1u8), (33, 1), (1, 0), (1, 9)] {
        assert!(
            matches!(node.advance(slot, count), Err(LinkError::Bad(_))),
            "{slot} {count}"
        );
    }
    assert!(matches!(node.ack(0), Err(LinkError::Bad(_))));
    assert_eq!(sim.requests(), asked);
}

#[test]
fn a_feeder_node_that_goes_silent_or_safe_is_held_and_no_tape_moves() {
    let sim = SimFeeders::new(4, None, "t");
    let link = ScNodeLink::new("sim:2", "t", 2, 0, Box::new(sim.clone())).without_waiting();
    let mut node = Feeders::new(link, 2);
    sim.set_injection(Injection::scripted(vec![Some(Inject::Silence); 2]));
    node.refresh().unwrap_err();
    node.refresh().unwrap_err();
    assert_eq!(node.guard().held(), Some(Hold::Silent));
    let asked = sim.requests();
    assert_eq!(node.advance(1, 1), Err(LinkError::Held(Hold::Silent)));
    assert_eq!(sim.requests(), asked);
    node.refresh().unwrap();
    assert!(node.acknowledge());
    node.advance(1, 1).unwrap();
    sim.set_safe(true);
    node.refresh().unwrap();
    assert_eq!(node.guard().held(), Some(Hold::Safe));
    assert_eq!(node.advance(1, 1), Err(LinkError::Held(Hold::Safe)));
    assert_eq!(sim.advances(1), 1);
}

fn profile() -> DriverProfile {
    DriverProfile::parse(&format!("{MARLIN}\ninterlock = {XY_CLEAR} | on | xy\n")).unwrap()
}

fn ready() -> Session<SimLink> {
    let p = profile();
    let mut s = Session::new(&p, SimLink::new(GcodeSim::new(&p, Faults::default(), 1)));
    s.connect().unwrap();
    s.home().unwrap();
    s
}

fn xy(x: f64, y: f64) -> Target {
    Target {
        x: Some(x),
        y: Some(y),
        z: None,
        rot: None,
    }
}

#[test]
fn the_axes_of_the_machine_move_only_while_the_head_says_it_is_clear() {
    let sim = SimHead::new(2, "t");
    sim.set_injection(Injection::none());
    let mut head = Head::new(over_http(&sim, "t"), 3);
    // clear: the move is made
    let mut s = ready();
    assert!(s.set_signal(XY_CLEAR, xy_clear_signal(&mut head)));
    s.move_to(&xy(5.0, 5.0), 6000.0).unwrap();
    // a nozzle goes down: the next reading is no, and the move is refused and stops the session
    sim.set_z(2, 1.0);
    assert!(s.set_signal(XY_CLEAR, xy_clear_signal(&mut head)));
    let f = s.move_to(&xy(9.0, 9.0), 6000.0).unwrap_err();
    assert_eq!(f.kind, Kind::Interlock);
    assert!(f.detail.contains(XY_CLEAR));
    assert!(s.is_halted());
    // a head that does not answer is no either
    let mut s = ready();
    sim.set_z(2, 5.0);
    sim.set_injection(Injection::scripted(vec![Some(Inject::Silence)]));
    assert!(s.set_signal(XY_CLEAR, xy_clear_signal(&mut head)));
    assert_eq!(s.signals.get(XY_CLEAR), Some(&false));
    assert_eq!(
        s.move_to(&xy(9.0, 9.0), 6000.0).unwrap_err().kind,
        Kind::Interlock
    );
    // and it is yes again once the head is heard and clear
    let mut s = ready();
    assert!(s.set_signal(XY_CLEAR, xy_clear_signal(&mut head)));
    s.move_to(&xy(9.0, 9.0), 6000.0).unwrap();
    // before anyone asked, the signal is unknown and does not satisfy the rule
    let mut s = ready();
    assert_eq!(
        s.move_to(&xy(1.0, 1.0), 6000.0).unwrap_err().kind,
        Kind::Interlock
    );
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn node_sim(args: &[&str]) -> (i32, String) {
    let out = Command::new(BIN)
        .arg("node-sim")
        .args(args)
        .current_dir(root())
        .output()
        .unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

#[test]
fn the_command_runs_a_job_on_made_up_nodes_with_the_same_text_for_the_same_seed() {
    let (code, a) = node_sim(&["--seed", "7", "--steps", "10"]);
    assert_eq!(code, 0, "{a}");
    assert!(
        a.contains("10 steps: 10 moved, 0 refused, 0 not clear"),
        "{a}"
    );
    assert_eq!(node_sim(&["--seed", "7", "--steps", "10"]).1, a);
    // faults drawn from the seed: the same every time, the machine stays still on the steps it could not be sure
    let (_, b) = node_sim(&[
        "--seed",
        "3",
        "--steps",
        "60",
        "--silence",
        "120",
        "--interlock",
        "150",
    ]);
    assert_eq!(
        node_sim(&[
            "--seed",
            "3",
            "--steps",
            "60",
            "--silence",
            "120",
            "--interlock",
            "150"
        ])
        .1,
        b
    );
    assert!(b.contains("not clear") && b.contains("refused"), "{b}");
    // a node that never answers is held, and the command says the check was not passed
    let (code, c) = node_sim(&["--steps", "10", "--silence", "1000"]);
    assert_eq!(code, 2, "{c}");
    assert!(c.contains("head silent"), "{c}");
    // an option that is wrong is an error of the arguments
    assert_eq!(node_sim(&["--steps", "0"]).0, 1);
    assert_eq!(node_sim(&["--oops"]).0, 1);
}

/// A node that answers every question with one status and one body, to see what the common client lets through of a refusal.
struct Fixed(u16, &'static str);

impl eh3d_serverclient::Transport for Fixed {
    fn send(
        &self,
        _request: &eh3d_serverclient::Request,
    ) -> Result<eh3d_serverclient::Reply, eh3d_serverclient::TransportError> {
        Ok(eh3d_serverclient::Reply::new(self.0, self.1))
    }
}

fn link_to(status: u16, body: &'static str) -> ScNodeLink {
    ScNodeLink::new("node:1", "t", 2, 0, Box::new(Fixed(status, body))).without_waiting()
}

// with the feature `common-refusal` the reason is the `why` of the node (tests/nodereason.rs)
#[cfg(not(feature = "common-refusal"))]
#[test]
fn the_reason_of_a_refusal_over_the_common_client_is_its_code_or_its_detail() {
    // the `error` of the answer is the reason the common client keeps
    let mut l = link_to(
        409,
        "{\"error\":\"busy\",\"detail\":\"a pick is under way\"}",
    );
    assert_eq!(
        refused(l.call("POST", "/api/nozzle/pick?n=1", true).unwrap_err()),
        (409, Some("busy".to_string()))
    );
    // with no code, the detail
    let mut l = link_to(409, "{\"detail\":\"a pick is under way\"}");
    assert_eq!(
        refused(l.call("POST", "/api/nozzle/pick?n=1", true).unwrap_err()),
        (409, Some("a pick is under way".to_string()))
    );
    // with neither, the status alone: the `why` of the node is not kept by the client (see docs/COMMON_REQUESTS.md)
    let mut l = link_to(409, "{\"ok\":false,\"why\":\"not_holding\"}");
    assert_eq!(
        refused(l.call("POST", "/api/nozzle/place?n=1", true).unwrap_err()),
        (409, None)
    );
    // a refusal that is not JSON is still a refusal
    let mut l = link_to(500, "boom");
    assert_eq!(
        refused(l.call("GET", "/api/state", false).unwrap_err()),
        (500, None)
    );
}

#[test]
fn a_node_that_refuses_the_token_is_asked_again_with_it_and_a_link_that_is_fixed_is_never_logged_out(
) {
    let mut l = link_to(401, "{\"error\":\"unauthorized\"}");
    // refused, and refused again: the token of a node is fixed in the settings, so the client forgetting it after the first refusal must not turn the second into silence or a login error
    for _ in 0..3 {
        assert_eq!(
            l.call("POST", "/api/stop", true).unwrap_err(),
            LinkError::Unauthorized
        );
    }
    // the answer of a node that accepts it comes back whole
    let mut ok = link_to(200, "{\"ok\":true}");
    assert_eq!(ok.call("POST", "/api/stop", true).unwrap(), "{\"ok\":true}");
}
