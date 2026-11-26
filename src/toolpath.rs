// EH3D-PHOENIX-PNP - the lines of a job and their fingerprint.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! The lines that a driver profile writes for a plan, from connecting to switching the motors off, can be looked at before they are run, and the fingerprint of those lines
//! (SHA-256 of the lines, each one ended by a line break) says whether what is run is what was seen. The same plan and the same profile always give the same lines and the same
//! fingerprint; a line that changes, is added or goes, or a place that moves by a thousandth of a millimetre, gives another one. This is a local check of the lines of this program and nothing
//! authorises with it: the server makes its own hash of the request it holds, and that one (see `server`) is what authorises a job there. Nothing is sent anywhere; the lines are text.

use crate::gcodedriver::DriverProfile;
use crate::plan::Placement;
use crate::sha256::sha256_hex;

/// The lines of the whole job in order: connect, absolute mode, homing, each placement, switch off. A step the profile cannot write is an error and stops it here.
pub fn script(profile: &DriverProfile, placements: &[Placement]) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    out.extend(profile.connect.clone());
    out.push(profile.absolute_mode.clone());
    out.extend(profile.home_lines());
    for p in placements {
        for op in profile.placement_ops(p)? {
            out.extend(profile.lines(&op)?);
        }
    }
    out.extend(profile.disable.clone());
    Ok(out)
}

/// The lines as one text, each ended by a line break.
pub fn text(lines: &[String]) -> String {
    let mut t = lines.join("\n");
    t.push('\n');
    t
}

/// The fingerprint of the lines: 64 letters of lower case hexadecimal.
pub fn fingerprint(lines: &[String]) -> String {
    sha256_hex(text(lines).as_bytes())
}

/// Whether a text the person gives is a fingerprint: 64 hexadecimal digits, in either case.
pub fn is_fingerprint(text: &str) -> bool {
    text.len() == 64 && text.chars().all(|c| c.is_ascii_hexdigit())
}

/// Whether the fingerprint that was given (in either case) is the one of the lines.
pub fn matches(lines: &[String], given: &str) -> bool {
    is_fingerprint(given.trim())
        && crate::sha256::same(&fingerprint(lines), &given.trim().to_ascii_lowercase())
}
