# EH3D-PHOENIX-PNP: requests to the common client

The link to the head node and to the feeder node goes through the common client of the suite (`eh3d-serverclient`, in `EH3D-COMMON`). This program does not change that crate. Two things that the earlier release did not give were asked for; both are in the release 0.3.0, which the build and the CI are pinned to (`SIBLING_REFS` of the workflows), and the program uses them as they are.

## 1. The reason (`why`) of a refusal

**What was asked.** A node that refuses an order with `409` says why in the field `why` of its answer (`busy`, `not_holding`, `vacuum_not_reached`). The earlier client kept only the fields `error` and `detail`, so the `why` was lost before the caller saw it.

**What the release 0.3.0 gives.** `Client::last_refusal()` keeps the `why` of the last refusal (cut to 256 characters) and, when `server_error_body_bytes` asks for it, a piece of the body with the token and the values under names such as `token` or `password` replaced by `***`. `Client::why_of(&error)` gives the `why` only when the error is that refusal, so an old refusal is never given to a new error.

**What this program does.** Right after the error, `ScNodeLink` asks `why_of` and gives the `why` as the node wrote it (and none when the answer has none); `ScNodeLink::new_with_excerpt` keeps a masked piece of the body, which `refused_body` gives. The reading of the `error` or the `detail` of the answer as the reason, which stood in for it, is gone.

## 2. A fixed token is not forgotten after a `401`

**What was asked.** After an answer of `401` the client forgot the token, which is right for a login that ran out. The token of a node is written in the settings and does not run out: a node that has just started, or a token being rotated, may refuse it once.

**What the release 0.3.0 gives.** `Client::with_fixed_token(true)` (and the setting `server_token_fixed`, off by default) keeps the token after a `401`; the answer is still `Unauthorized` and the link still goes to `LoggedOut`, so that the panel shows it, and the next question asks the node again.

**What this program does.** `ScNodeLink` turns `with_fixed_token(true)` on. The `set_token` that gave the token back after every `Unauthorized` is gone.

## State

Both are used and tried against nodes on a computer (`tests/nodereason.rs`, `tests/nodelink.rs`); no real node has answered. Nothing more is asked of the common client at the moment.
