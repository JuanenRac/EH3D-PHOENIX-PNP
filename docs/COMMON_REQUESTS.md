# EH3D-PHOENIX-PNP: requests to the common client

The link to the head node and to the feeder node goes through the common client of the suite (`eh3d-serverclient`, in `EH3D-COMMON`). This program does not change that crate. Two things the release it is pinned to does not give were asked for, and the client now has both in its main branch; they have no release yet.

## 1. The reason (`why`) of a refusal

**What happens with the pinned release.** A node that refuses an order with `409` says why in the field `why` of its answer (`busy`, `not_holding`, `vacuum_not_reached`). The client keeps only the fields `error` and `detail`, so the `why` is lost before the caller sees it.

**What the next release gives.** `Client::last_refusal()` keeps the `why` of the last refusal (cut to 256 characters) and, when `server_error_body_bytes` asks for it, a piece of the body with the token and the values under names such as `token` or `password` replaced by `***`. `Client::why_of(&error)` gives the `why` only when the error is that refusal, so an old refusal is never given to a new error.

**What this program does.** Without the feature `common-refusal`, `ScNodeLink` gives the `error` of the answer as the reason and, when there is none, its `detail`; with neither, the status alone. With the feature, right after the error it asks `why_of` and gives the `why` as the node wrote it (and none when the answer has none); `ScNodeLink::new_with_excerpt` keeps a masked piece of the body, which `refused_body` gives.

## 2. A fixed token is not forgotten after a `401`

**What happens with the pinned release.** After an answer of `401` the client forgets the token, which is right for a login that ran out. The token of a node is written in the settings and does not run out: a node that has just started, or a token being rotated, may refuse it once.

**What the next release gives.** `Client::with_fixed_token(true)` (and the setting `server_token_fixed`, off by default) keeps the token after a `401`; the answer is still `Unauthorized` and the link still goes to `LoggedOut`, so that the panel shows it, and the next question asks the node again.

**What this program does.** Without the feature, `ScNodeLink` gives the token back with `set_token` after every `Unauthorized`. With the feature it turns `with_fixed_token(true)` on and the workaround is not compiled.

## When this page can go

When the build is pinned to a release of `EH3D-COMMON` that has `Client::why_of` and `Client::with_fixed_token` (the `SIBLING_REFS` of the workflows and the version in the documents), the feature `common-refusal` is made a default of `Cargo.toml`, the code of the two workarounds and the tests that name them are removed, and this page, the lines about it in `STATUS.md` and `NOT_YET.md` and the row of the register go with them.
