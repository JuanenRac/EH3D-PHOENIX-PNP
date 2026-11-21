# EH3D-PHOENIX-PNP: requests to the common client

The link to the head node and to the feeder node goes through the common client of the suite (`eh3d-serverclient`, in `EH3D-COMMON`). This program does not change that crate; two things it does not give are written here with the change that would give them, so that whoever maintains the client can take them as they are.

## 1. The reason (`why`) of a refusal

**What happens.** A node that refuses an order with `409` says why in the field `why` of its answer (`busy`, `not_holding`, `vacuum_not_reached`). `Client::check` turns every answer of 400 or more into a `ClientError`, and `Client::code_and_detail` keeps only the fields `error` and `detail`. The `why` is lost before the caller sees it, so over the common client a refusal reaches this program as a status with no reason, while against the nodes on a computer it is read whole.

**What this program does meanwhile.** `ScNodeLink` gives the `error` of the answer as the reason and, when there is none, its `detail`; with neither, the status alone. The tests of `nodelink` show the three cases.

**The change that would fix it.**

* `errors.rs`: `ClientError::Server { status, code, detail }` gets a fourth field, `why: Option<String>`, and `ClientError` gets `pub fn why(&self) -> Option<&str>` that gives it for a `Server` error.
* `client.rs`: `code_and_detail` also reads `j.get("why").and_then(Json::as_str)` and returns it as a third value; `check` puts it in `ClientError::Server`. The value is cut to 256 characters, as `detail` is.
* The existing code that matches `ClientError::Server { status, code, detail }` adds `..` or the new field; the text of the error does not change.

A test in the crate sends a `409` with `{"ok":false,"why":"busy"}` through a transport and expects `why() == Some("busy")`.

## 2. A fixed token is not forgotten after a `401`

**What happens.** After an answer of `401`, `Client::check(reply, true)` calls `forget()`, which is right for a login that ran out. The token of a node is not a login: it is written in the settings and does not run out, and a node that refuses it once (a node that has just started, a token being rotated) is not refusing it for ever. The client then answers every following question with `Unauthorized` without asking the node.

**What this program does meanwhile.** After every `Unauthorized` answer `ScNodeLink` gives the token back with `set_token`, so the next question asks again. The test `a_node_that_refuses_the_token_is_asked_again...` asks three times in a row.

**The change that would fix it.** `Client::with_fixed_token(self, fixed: bool) -> Client` (and the setting `server_token_fixed`, off by default): when it is on, `check` does not call `forget()` for a `401`, and `request` and `request_full` do not turn a missing token into `Unauthorized` without asking. The link state still goes to `LoggedOut` for a refusal, so that the panel shows it.

## When this page can go

When both changes are in the crate, `ScNodeLink` reads `why()` and turns `with_fixed_token` on, and the two paragraphs "What this program does meanwhile" and the lines about them in `STATUS.md` and `NOT_YET.md` are removed.
