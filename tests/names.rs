//! The names of the files of the repository: no two differ only in case, and none is a name that Windows keeps for a device or cannot hold. A system that does not tell capitals from
//! small letters (Windows, macOS) takes two names that differ only in case for one file, and a name like `aux` or `a:b` cannot be written there at all.

use std::collections::BTreeMap;
use std::path::Path;

/// The files under `dir`, as paths relative to `root` with `/`, leaving out the folders that are not part of the repository.
fn walk(dir: &Path, root: &Path, out: &mut Vec<String>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            if matches!(name.as_str(), "target" | ".git" | "dist") {
                continue;
            }
            walk(&path, root, out);
        } else if let Ok(relative) = path.strip_prefix(root) {
            out.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
}

fn files() -> Vec<String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

#[test]
fn no_two_files_have_names_that_differ_only_in_case() {
    let all = files();
    assert!(all.len() > 50, "only {} files were found", all.len());
    let mut by_lower: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    for f in &all {
        by_lower.entry(f.to_lowercase()).or_default().push(f);
    }
    let clashes: Vec<&Vec<&str>> = by_lower.values().filter(|v| v.len() > 1).collect();
    assert!(clashes.is_empty(), "{clashes:?}");
}

#[test]
fn no_file_has_a_name_that_windows_keeps_for_a_device_or_cannot_hold() {
    const DEVICES: [&str; 22] = [
        "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
        "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
    ];
    for f in files() {
        for part in f.split('/') {
            let stem = part.split('.').next().unwrap_or("").to_lowercase();
            assert!(
                !DEVICES.contains(&stem.as_str()),
                "{f}: '{part}' is a name of a device"
            );
            assert!(
                !part
                    .chars()
                    .any(|c| "<>:\"|?*".contains(c) || c.is_control()),
                "{f}: '{part}' has a character that Windows cannot hold in a name"
            );
            assert!(
                !part.ends_with('.') && !part.ends_with(' '),
                "{f}: '{part}' ends with a dot or a space"
            );
        }
    }
}
