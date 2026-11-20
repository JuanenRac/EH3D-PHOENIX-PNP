//! The pattern matcher of the driver profiles: what it reads, what it refuses, that the time it takes is bounded whatever the pattern, and that no pattern or text, made by chance on a fixed
//! seed, makes it panic.

use eh3d_phoenix_pnp::textmatch::{Pattern, MAX_GROUPS, MAX_PATTERN, MAX_TEXT};

fn m(p: &str, t: &str) -> bool {
    Pattern::new(p).unwrap().is_match(t)
}

#[test]
fn letters_dots_classes_and_the_shorthands() {
    assert!(m("a.c", "abc"));
    assert!(!m("a.c", "ac"));
    assert!(m("[a-c]x", "bx"));
    assert!(!m("[a-c]x", "dx"));
    assert!(m("[^a-c]x", "dx"));
    assert!(m(r"\d\d", "x42"));
    assert!(!m(r"\d", "abc"));
    assert!(m(r"\D", "abc"));
    assert!(m(r"a\sb", "a b"));
    assert!(m(r"\w+", "snake_case1"));
    assert!(m(r"[\d.]+", "3.14"));
    assert!(m(r"\.", "a.b"));
    assert!(!m(r"\.", "ab"));
    assert!(m("[]]", "]"));
    assert!(m("[a-]", "-"));
}

#[test]
fn repeats_alternatives_and_the_ends() {
    assert!(m("^ab*c$", "ac"));
    assert!(m("^ab*c$", "abbbc"));
    assert!(!m("^ab+c$", "ac"));
    assert!(m("^ab?c$", "abc"));
    assert!(!m("^ab?c$", "abbc"));
    assert!(m("^(ok|fine)$", "fine"));
    assert!(!m("^(ok|fine)$", "okfine"));
    assert!(m("(?:ab)+$", "xabab"));
    assert!(m("^$", ""));
    assert!(m("x*", ""));
}

#[test]
fn groups_are_numbered_by_their_opening_and_a_missing_one_is_none() {
    let p = Pattern::new(r"(a)(?:b)(c)?(d)").unwrap();
    assert_eq!(p.groups(), 3);
    let c = p.find("abd").unwrap();
    assert_eq!(c.get(1).as_deref(), Some("a"));
    assert_eq!(c.get(2), None);
    assert_eq!(c.get(3).as_deref(), Some("d"));
    assert_eq!(c.get(9), None);
}

#[test]
fn the_leftmost_match_is_the_one_that_is_given_and_a_greedy_repeat_takes_most() {
    let p = Pattern::new(r"(\d+)\.?(\d*)").unwrap();
    let c = p.find("x 12.50 and 7").unwrap();
    assert_eq!(c.get(1).as_deref(), Some("12"));
    assert_eq!(c.get(2).as_deref(), Some("50"));
    assert_eq!(c.number(1), Some(12.0));
}

#[test]
fn the_case_flag_works_for_letters_and_classes() {
    assert!(m("(?i)hello", "HeLLo"));
    assert!(m("(?i)[a-c]+", "ABC"));
    assert!(!m("hello", "HELLO"));
}

#[test]
fn a_pattern_that_is_wrong_is_refused_with_what_is_wrong() {
    for bad in [
        "(", ")", "[a", "*a", "+", "(?=a)", "[z-a]", r"a\", r"[\D]", "a(b",
    ] {
        let e = Pattern::new(bad).unwrap_err();
        assert!(!e.is_empty(), "{bad}");
    }
}

#[test]
fn the_limits_of_a_pattern_are_kept() {
    assert!(Pattern::new(&"a".repeat(MAX_PATTERN)).is_ok());
    assert!(Pattern::new(&"a".repeat(MAX_PATTERN + 1)).is_err());
    let nine = "(a)".repeat(MAX_GROUPS);
    assert!(Pattern::new(&nine).is_ok());
    assert!(Pattern::new(&format!("{nine}(a)")).is_err());
    let deep = format!("{}a{}", "(?:".repeat(17), ")".repeat(17));
    assert!(Pattern::new(&deep).is_err());
    let fine = format!("{}a{}", "(?:".repeat(10), ")".repeat(10));
    assert!(Pattern::new(&fine).is_ok());
}

#[test]
fn a_pattern_made_to_take_forever_in_a_backtracking_matcher_is_fast_here() {
    let text = format!("{}!", "a".repeat(MAX_TEXT));
    for p in ["(a+)+$", "(a*)*b", "(a|a)*c", "(a|aa)+$", "^(a?){40}a{40}$"] {
        // the result is not the point; that it comes back, with every pattern, is
        let _ = Pattern::new(p).map(|p| p.is_match(&text));
    }
    let q = Pattern::new("(x+x+)+y").unwrap();
    assert!(!q.is_match(&"x".repeat(MAX_TEXT)));
}

#[test]
fn a_text_over_the_limit_is_cut_and_a_match_after_the_cut_is_not_seen() {
    let p = Pattern::new("needle").unwrap();
    let far = format!("{}needle", "a".repeat(MAX_TEXT));
    assert!(!p.is_match(&far));
    let near = format!("{}needle", "a".repeat(MAX_TEXT - 6));
    assert!(p.is_match(&near));
}

#[test]
fn multibyte_text_is_read_by_letters_and_never_cut_in_the_middle() {
    assert!(m("é+", "caféé"));
    assert!(m("^日本語$", "日本語"));
    assert!(m("(?i)ÉCOLE", "école"));
    let c = Pattern::new("(.)$").unwrap().find("ab日").unwrap();
    assert_eq!(c.get(1).as_deref(), Some("日"));
}

fn next(state: &mut u64) -> u64 {
    *state = state
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    *state >> 33
}

#[test]
fn patterns_and_texts_made_by_chance_never_panic() {
    let alphabet: Vec<char> = "ab()[]|*+?^$\\.-:dsw é".chars().collect();
    let mut s = 7u64;
    let mut compiled = 0;
    for _ in 0..4000 {
        let len = (next(&mut s) % 24) as usize;
        let pat: String = (0..len)
            .map(|_| alphabet[(next(&mut s) as usize) % alphabet.len()])
            .collect();
        if let Ok(p) = Pattern::new(&pat) {
            compiled += 1;
            for _ in 0..3 {
                let tl = (next(&mut s) % 30) as usize;
                let text: String = (0..tl)
                    .map(|_| alphabet[(next(&mut s) as usize) % alphabet.len()])
                    .collect();
                let _ = p.find(&text);
            }
        }
    }
    assert!(compiled > 200, "{compiled}");
}
