//! Every key of the settings: it is in the list, in the documented example with a value that is valid, checked when it is read (only the keys that take a name or a list take any text), refused
//! when it is written twice, and a key that is not in the list is refused with its line.

use eh3d_phoenix_pnp::config::{MachineConfig, KEYS};
use eh3d_phoenix_pnp::error::Code;
use std::collections::BTreeMap;

const EXAMPLE: &str = include_str!("../examples/machine.cfg");

/// The keys that take a name, a list of words or an address, and so take any text without a control character. Every other key takes a number, a word of a short list or a yes or a no.
const TEXT_KEYS: [&str; 20] = [
    "default_tip",
    "header_reference",
    "header_value",
    "header_package",
    "header_x",
    "header_y",
    "header_rotation",
    "header_side",
    "header_dnp",
    "header_fitted",
    "dnp_yes_words",
    "dnp_no_words",
    "fiducial_words",
    "fiducial_refs",
    "polarity_refs",
    "polarity_packages",
    "server_address",
    "server_token",
    "fab_dnp_column",
    "fab_tht_words",
];

/// The keys that may be written more than once, one line for each rule.
const LISTS: [&str; 6] = [
    "rotation",
    "sim_part_fail",
    "sim_feeder_fail",
    "actuator",
    "head",
    "tip_compat",
];

/// The first value that the example gives to each key.
fn example_values() -> BTreeMap<&'static str, &'static str> {
    let mut map = BTreeMap::new();
    for line in EXAMPLE
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#') && l.contains('='))
    {
        let (k, v) = line.split_once('=').unwrap();
        map.entry(k.trim()).or_insert_with(|| v.trim());
    }
    map
}

fn code_of(text: &str) -> Option<Code> {
    MachineConfig::parse(text).err().map(|e| e.code)
}

#[test]
fn the_keys_are_different_names_in_lower_case() {
    let mut seen = std::collections::BTreeSet::new();
    for k in KEYS {
        assert!(seen.insert(k), "{k} is in the list twice");
        assert!(
            !k.is_empty()
                && k.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
            "{k}"
        );
        assert!(
            !k.starts_with('_') && !k.ends_with('_') && !k.contains("__"),
            "{k}"
        );
    }
}

#[test]
fn the_example_gives_every_key_and_only_the_keys_of_the_list() {
    let given = example_values();
    for k in KEYS {
        assert!(given.contains_key(k), "the example does not give {k}");
    }
    for k in given.keys() {
        assert!(
            KEYS.contains(k),
            "the example gives {k}, which is not in the list"
        );
    }
    assert_eq!(given.len(), KEYS.len());
}

#[test]
fn every_key_of_the_list_is_known_to_the_reader_with_the_value_of_the_example() {
    for (k, v) in example_values() {
        let one = format!("{k} = {v}");
        if let Err(e) = MachineConfig::parse(&one) {
            assert_ne!(
                e.code,
                Code::ConfigUnknownKey,
                "{k} is in the list and not known to the reader"
            );
            // a key that is only valid together with others (the safety limits) is read in the whole file
            assert!(MachineConfig::parse(EXAMPLE).is_ok(), "{k}: {e}");
        }
    }
    assert!(MachineConfig::parse(EXAMPLE).is_ok());
}

#[test]
fn a_key_that_is_not_in_the_list_is_refused_with_its_line_and_the_code_of_an_unknown_key() {
    for bad in [
        "nozzle = 2",
        "server = x",
        "mode_ = local",
        "journal = 1",
        "x = 1",
    ] {
        let e = MachineConfig::parse(&format!("\n\n{bad}\n")).unwrap_err();
        assert_eq!(e.code, Code::ConfigUnknownKey, "{bad}");
        assert!(e.to_string().contains("line 3"), "{e}");
        assert!(e.to_string().contains("PHOENIX-PNP-004"), "{e}");
    }
}

#[test]
fn only_the_keys_that_take_a_name_or_a_list_take_any_text() {
    let mut got: Vec<&str> = KEYS
        .iter()
        .copied()
        .filter(|k| MachineConfig::parse(&format!("{k} = @@@")).is_ok())
        .collect();
    let mut want = TEXT_KEYS.to_vec();
    got.sort_unstable();
    want.sort_unstable();
    // server_machine is an id of letters, digits, _ and -, so "@@@" is not one; a number that is too big is refused by every key that takes a number
    assert_eq!(got, want);
    for k in KEYS {
        if !TEXT_KEYS.contains(&k) && k != "server_machine" {
            assert!(
                MachineConfig::parse(&format!("{k} = 1e999")).is_err(),
                "{k} takes a number that is too big"
            );
        }
    }
}

#[test]
fn a_control_character_is_refused_by_every_key_that_takes_an_address_a_token_or_an_id() {
    for k in ["server_address", "server_token", "server_machine"] {
        assert!(
            MachineConfig::parse(&format!("{k} = a\u{7}b")).is_err(),
            "{k}"
        );
        assert!(
            MachineConfig::parse(&format!("{k} = a\u{0}b")).is_err(),
            "{k}"
        );
    }
}

#[test]
fn a_key_written_twice_is_refused_unless_it_is_a_list_of_rules() {
    for (k, v) in example_values() {
        let twice = format!("{k} = {v}\n{k} = {v}\n");
        let got = code_of(&twice);
        if LISTS.contains(&k) {
            assert!(got != Some(Code::ConfigDuplicateKey), "{k}");
        } else {
            assert_eq!(got, Some(Code::ConfigDuplicateKey), "{k} twice");
        }
    }
}

#[test]
fn the_name_of_a_key_may_be_in_capitals_and_the_value_has_its_spaces_trimmed() {
    let cfg = MachineConfig::parse("NOZZLES   =   3  \nServer_Poll_Ms=250\n").unwrap();
    assert_eq!((cfg.nozzles, cfg.server_poll_ms), (3, 250));
}

#[test]
fn a_comment_or_an_empty_line_is_no_key_and_a_line_without_equals_is_a_syntax_error() {
    assert!(MachineConfig::parse("# nozzles = 9\n\n   \n# server_token = x\n").is_ok());
    let e = MachineConfig::parse("nozzles 2\n").unwrap_err();
    assert_eq!(e.code, Code::ConfigSyntax);
    assert!(e.to_string().contains("line 1"));
}
