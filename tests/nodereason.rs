//! The link to a node over the common client with the feature `common-refusal`: the `why` of a refusal as the node gave it, a token that stays after a `401` and a piece of the body of a
//! refusal with the secrets masked. The feature is off until the build is pinned to a release of the common client that has these; without it this file is empty.
#![cfg(feature = "common-refusal")]

use eh3d_phoenix_pnp::nodelink::{LinkError, NodeLink, ScNodeLink};
use eh3d_serverclient::{Reply, Request, Transport, TransportError};
use std::sync::{Arc, Mutex};

/// A node that answers every question with one status and one body and keeps the token each question carried.
#[derive(Clone)]
struct Node {
    status: u16,
    body: String,
    seen: Arc<Mutex<Vec<Option<String>>>>,
}

impl Node {
    fn new(status: u16, body: &str) -> Node {
        Node {
            status,
            body: body.to_string(),
            seen: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn tokens(&self) -> Vec<Option<String>> {
        self.seen.lock().unwrap().clone()
    }
}

impl Transport for Node {
    fn send(&self, request: &Request) -> Result<Reply, TransportError> {
        self.seen
            .lock()
            .unwrap()
            .push(request.bearer().map(str::to_string));
        Ok(Reply::new(self.status, self.body.clone()))
    }
}

fn link(node: &Node) -> ScNodeLink {
    ScNodeLink::new("node:1", "tk-1234", 2, 0, Box::new(node.clone())).without_waiting()
}

fn refusal(e: LinkError) -> (u16, Option<String>) {
    match e {
        LinkError::Refused { status, why } => (status, why),
        other => panic!("not a refusal: {other:?}"),
    }
}

#[test]
fn a_409_gives_the_why_the_node_wrote_and_nothing_else() {
    for why in ["busy", "not_holding", "vacuum_not_reached"] {
        let node = Node::new(409, &format!("{{\"ok\":false,\"why\":\"{why}\"}}"));
        let mut l = link(&node);
        let e = l.call("POST", "/api/nozzle/place?n=1", true).unwrap_err();
        assert_eq!(refusal(e), (409, Some(why.to_string())));
    }
    // the `error` and the `detail` of an answer are not taken for the reason
    let node = Node::new(
        409,
        "{\"error\":\"busy\",\"detail\":\"a pick is under way\"}",
    );
    let e = link(&node)
        .call("POST", "/api/nozzle/pick?n=1", true)
        .unwrap_err();
    assert_eq!(refusal(e), (409, None));
    // a refusal that is not JSON, and one of another status
    let e = link(&Node::new(500, "boom"))
        .call("GET", "/api/state", false)
        .unwrap_err();
    assert_eq!(refusal(e), (500, None));
    let e = link(&Node::new(503, "{\"why\":\"offline\"}"))
        .call("POST", "/api/feeder/advance?slot=2&count=1", true)
        .unwrap_err();
    assert_eq!(refusal(e), (503, Some("offline".to_string())));
}

#[test]
fn a_reason_is_cut_and_the_next_refusal_does_not_inherit_it() {
    let long = "w".repeat(1000);
    let node = Node::new(409, &format!("{{\"why\":\"{long}\"}}"));
    let (_, why) = refusal(link(&node).call("POST", "/api/stop", true).unwrap_err());
    assert_eq!(why.unwrap().chars().count(), 256);
    // the same link refused twice with different words says each in turn
    let mut l = link(&Node::new(409, "{\"why\":\"busy\"}"));
    let (_, a) = refusal(l.call("POST", "/api/stop", true).unwrap_err());
    let (_, b) = refusal(l.call("POST", "/api/stop", true).unwrap_err());
    assert_eq!((a.as_deref(), b.as_deref()), (Some("busy"), Some("busy")));
}

#[test]
fn three_refusals_of_the_token_in_a_row_still_carry_it_every_time() {
    let node = Node::new(401, "{\"error\":\"unauthorized\"}");
    let mut l = link(&node);
    for _ in 0..3 {
        assert_eq!(
            l.call("POST", "/api/stop", true).unwrap_err(),
            LinkError::Unauthorized
        );
    }
    let tokens = node.tokens();
    assert_eq!(tokens.len(), 3, "every question reached the node");
    assert!(
        tokens.iter().all(|t| t.as_deref() == Some("tk-1234")),
        "{tokens:?}"
    );
    // a question without the token (the state) carries none
    let _ = l.call("GET", "/api/status", false);
    assert_eq!(node.tokens().last(), Some(&None));
}

#[test]
fn the_body_of_a_refusal_is_kept_only_when_asked_for_and_with_its_secrets_masked() {
    let body = "{\"why\":\"busy\",\"token\":\"tk-1234\",\"detail\":\"bad token tk-1234 for the head\",\"password\":\"hunter2\"}";
    // by default nothing of the body is kept
    let node = Node::new(409, body);
    let mut l = link(&node);
    l.call("POST", "/api/stop", true).unwrap_err();
    assert_eq!(l.refused_body(), None);
    // asked for: a piece of it, without the token wherever it appears and without the values under a secret name
    let mut l =
        ScNodeLink::new_with_excerpt("node:1", "tk-1234", 2, 0, 200, Box::new(node.clone()))
            .without_waiting();
    l.call("POST", "/api/stop", true).unwrap_err();
    let piece = l.refused_body().expect("a piece was kept").to_string();
    assert!(piece.contains("busy"), "{piece}");
    assert!(
        !piece.contains("tk-1234") && !piece.contains("hunter2"),
        "{piece}"
    );
    assert!(piece.contains("***"), "{piece}");
    // the piece is cut to the bytes asked for
    let big = Node::new(409, &format!("{{\"detail\":\"{}\"}}", "x".repeat(5000)));
    let mut l =
        ScNodeLink::new_with_excerpt("node:1", "t", 2, 0, 64, Box::new(big)).without_waiting();
    l.call("POST", "/api/stop", true).unwrap_err();
    assert!(l.refused_body().unwrap().len() <= 64);
    // a good answer clears it
    let mut ok =
        ScNodeLink::new_with_excerpt("node:1", "t", 2, 0, 64, Box::new(Node::new(200, "{}")))
            .without_waiting();
    ok.call("GET", "/api/status", false).unwrap();
    assert_eq!(ok.refused_body(), None);
}
