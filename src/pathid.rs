// EH3D-PHOENIX-PNP - portable paths: how a path is kept in a saved file and how two paths are told to be the same file.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! A queue, a panel or a table of jobs is written on one computer and read on another, and a person moves the folder, copies it to a stick or opens it on Windows and on Linux. A path written
//! with the separators of one system, or with the prefix `\\?\` that Windows puts on a path it has resolved, or with a `..` in the middle, is then a path that does not lead anywhere, or two
//! spellings of one file that are taken for two. This module has the two things that are needed and nothing else (no file is opened here):
//!
//! * `portable`: the path as it is **kept** in a saved file: `/` between the names, no `\\?\` prefix (a `\\?\UNC\server\share` is `//server/share`), no doubled separator, no `.`, the `..` that
//!   can be taken away taken away, and no separator at the end. A relative path stays relative.
//! * `identity`: the path as it is **compared**: the portable path, in capitals when the system does not tell capitals from small letters (Windows and macOS do not, Linux does). Two paths
//!   with the same identity are the same file, whether or not it exists.

/// The path as it is kept in a saved file (see the module).
pub fn portable(path: &str) -> String {
    let mut text = path.trim().replace('\\', "/");
    // the prefix of a path that Windows resolved: \\?\C:\x and \\?\UNC\server\share\x
    let unc = text
        .strip_prefix("//?/UNC/")
        .or_else(|| text.strip_prefix("//./UNC/"))
        .map(|rest| format!("//{rest}"));
    if let Some(rest) = unc {
        text = rest;
    } else if let Some(rest) = text
        .strip_prefix("//?/")
        .or_else(|| text.strip_prefix("//./"))
    {
        text = rest.to_string();
    }
    let network = text.starts_with("//") && !text.starts_with("///");
    let rooted = text.starts_with('/') && !network;
    let mut names: Vec<&str> = Vec::new();
    let mut drive = "";
    let body = text.trim_start_matches('/');
    let mut parts = body.split('/').peekable();
    // a drive letter is kept at the front: `C:`
    if let Some(first) = parts.peek() {
        if first.len() == 2
            && first.as_bytes()[1] == b':'
            && first.as_bytes()[0].is_ascii_alphabetic()
        {
            drive = first;
            parts.next();
        }
    }
    let absolute = rooted || network || !drive.is_empty() && body.len() > drive.len();
    for name in parts {
        match name {
            "" | "." => {}
            ".." => match names.last() {
                Some(last) if *last != ".." => {
                    names.pop();
                }
                // nothing above the root of an absolute path
                _ if absolute => {}
                _ => names.push(".."),
            },
            other => names.push(other),
        }
    }
    let mut out = String::new();
    if network {
        out.push_str("//");
    } else if rooted {
        out.push('/');
    }
    if !drive.is_empty() {
        out.push_str(drive);
        if absolute {
            out.push('/');
        }
    }
    out.push_str(&names.join("/"));
    if out.is_empty() {
        ".".to_string()
    } else {
        out
    }
}

/// The path as it is compared, with capitals folded when `fold` (a system that does not tell them apart).
pub fn identity_with(path: &str, fold: bool) -> String {
    let p = portable(path);
    if fold {
        p.to_uppercase()
    } else {
        p
    }
}

/// The path as it is compared on this system (capitals folded on Windows and macOS).
pub fn identity(path: &str) -> String {
    identity_with(path, cfg!(any(windows, target_os = "macos")))
}

/// Whether two paths are the same file by their identity (the files need not exist).
pub fn same(a: &str, b: &str) -> bool {
    identity(a) == identity(b)
}
