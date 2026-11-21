// EH3D-PHOENIX-PNP - the revision of a shared table: a write is refused when the file changed since it was read.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! The tables of feeders, of parts and of queues can be edited by this program, by the window and by a person with a text editor. Two writers that read the same table and write one after the
//! other lose the work of the first. A revision is a short token of the content of the file, taken when it is read; a write that carries the token replaces the file only if the file still
//! has that revision, and says so when it does not. The token ignores the kind of line end, so a program that saves with another one does not make a change out of nothing. The write is made to a
//! file beside the target and then moved over it, so a reader never sees half of a table. A file that is not there has no revision, and a write that carries a token to one is refused.

use crate::error::{Code, PnpError};
use crate::sha256::sha256_hex;
use std::path::Path;

/// How many characters of the hash the token has: enough to tell a change, short enough to type.
pub const TOKEN_CHARS: usize = 16;

/// The revision of a text: the start of the SHA-256 of its content with every line end as `\n`.
pub fn token_of(text: &str) -> String {
    let normal = text.trim_start_matches('\u{feff}').replace("\r\n", "\n");
    sha256_hex(normal.as_bytes())[..TOKEN_CHARS].to_string()
}

/// Whether a text is a token: the right number of lower-case hexadecimal digits.
pub fn is_token(text: &str) -> bool {
    text.len() == TOKEN_CHARS
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn failed(path: &str, why: impl std::fmt::Display) -> PnpError {
    PnpError::new(Code::DriverError, format!("{path}: {why}"))
}

/// Reads a table with a limit on its size and gives its text and its revision.
pub fn read(path: &str, max_bytes: u64) -> Result<(String, String), PnpError> {
    let bytes = std::fs::read(path).map_err(|e| failed(path, e))?;
    if bytes.len() as u64 > max_bytes {
        return Err(PnpError::new(Code::FileTooBig, path));
    }
    let text = String::from_utf8(bytes).map_err(|_| failed(path, "is not text in UTF-8"))?;
    let token = token_of(&text);
    Ok((text, token))
}

/// Replaces the file `path` with `text` if its revision is still `expected`, and gives the revision of the new content. A file that changed, that is gone or that was never there is
/// refused, and nothing is written.
pub fn replace(path: &str, expected: &str, text: &str, max_bytes: u64) -> Result<String, PnpError> {
    if !is_token(expected) {
        return Err(failed(
            path,
            format!("'{expected}' is not a revision ({TOKEN_CHARS} hexadecimal digits)"),
        ));
    }
    if !Path::new(path).is_file() {
        return Err(failed(
            path,
            "there is no file to replace, so no revision to hold it to",
        ));
    }
    let (_, now) = read(path, max_bytes)?;
    if now != expected {
        return Err(PnpError::new(
            Code::ReportExists,
            format!(
                "{path} changed since it was read (revision {now}, not {expected}): read it again"
            ),
        ));
    }
    let target = Path::new(path);
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let temp = target.with_file_name(format!(".{name}.{}.tmp", std::process::id()));
    let result =
        std::fs::write(&temp, text.as_bytes()).and_then(|()| std::fs::rename(&temp, target));
    if let Err(e) = result {
        let _ = std::fs::remove_file(&temp);
        return Err(failed(path, e));
    }
    Ok(token_of(text))
}
