//! The picture mode of the window: its arguments are read and checked without a screen, and a build without the window says so.

use eh3d_phoenix_pnp::error::Lang;
use eh3d_phoenix_pnp::gui::shot::{
    parse_size, parse_window_args, wants_picture, ShotRequest, WindowArgs, DEFAULT_SIZE,
    DEFAULT_WAIT,
};
use eh3d_phoenix_pnp::gui::theme::ThemeId;
use std::path::PathBuf;

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_owned()).collect()
}

#[test]
fn a_size_is_two_sides_inside_the_limits() {
    assert_eq!(parse_size("1280x720"), Some((1280, 720)));
    assert_eq!(parse_size(" 800 X 600 "), Some((800, 600)));
    assert_eq!(parse_size("320x8192"), Some((320, 8192)));
    for bad in [
        "", "1280", "x720", "1280x", "319x600", "800x8193", "-1x600", "1e3x600", "10x20x30",
        "800*600",
    ] {
        assert_eq!(parse_size(bad), None, "{bad:?}");
    }
}

#[test]
fn a_theme_is_read_in_any_case_and_the_default_is_the_phoenix() {
    let parsed =
        parse_window_args(&args(&["--shot", "a.png", "--theme", " Contrast "]), 1).unwrap();
    assert_eq!(parsed.shot.unwrap().theme, ThemeId::Contrast);
    for theme in ThemeId::ALL {
        let list = ["--shot", "a.png", "--theme", theme.key()];
        let shot = parse_window_args(&args(&list), 1).unwrap().shot.unwrap();
        assert_eq!(shot.theme, theme);
    }
    let default = parse_window_args(&args(&["--shot", "a.png"]), 1).unwrap();
    assert_eq!(default.shot.unwrap().theme, ThemeId::Phoenix);
}

#[test]
fn only_the_shot_option_asks_for_the_picture() {
    assert!(wants_picture(&args(&["--shot", "a.png"])));
    assert!(wants_picture(&args(&["gui", "x.pos", "--shot", "a.png"])));
    assert!(!wants_picture(&args(&["gui", "x.pos"])));
    assert!(!wants_picture(&[]));
}

#[test]
fn no_arguments_open_the_window_alone() {
    assert_eq!(parse_window_args(&[], 1), Ok(WindowArgs::default()));
    assert_eq!(
        parse_window_args(&args(&["gui"]), 1),
        Ok(WindowArgs::default())
    );
}

#[test]
fn files_and_a_language_are_read_in_any_order() {
    let parsed = parse_window_args(
        &args(&["gui", "--lang", "pt", "a.pos", "--open", "b.csv"]),
        3,
    )
    .unwrap();
    assert_eq!(
        parsed.files,
        [PathBuf::from("a.pos"), PathBuf::from("b.csv")]
    );
    assert_eq!(parsed.lang, Some(Lang::Pt));
    assert_eq!(parsed.shot, None);
}

#[test]
fn the_picture_mode_has_its_defaults() {
    let parsed = parse_window_args(&args(&["--shot", "start.png"]), 1).unwrap();
    assert_eq!(
        parsed.shot,
        Some(ShotRequest {
            output: PathBuf::from("start.png"),
            replace: false,
            size: DEFAULT_SIZE,
            theme: ThemeId::Phoenix,
            wait: DEFAULT_WAIT,
        })
    );
    let upper = parse_window_args(&args(&["--shot", "B.PNG", "x.pos"]), 1).unwrap();
    assert_eq!(upper.shot.unwrap().output, PathBuf::from("B.PNG"));
}

#[test]
fn every_option_of_the_picture_is_read() {
    let parsed = parse_window_args(
        &args(&[
            "--shot",
            "a.png",
            "--size",
            "1280x720",
            "--theme",
            "Light",
            "--wait",
            "12",
            "--replace",
            "--open",
            "x.pos",
            "--lang",
            "de",
        ]),
        1,
    )
    .unwrap();
    assert_eq!(parsed.lang, Some(Lang::De));
    assert_eq!(parsed.files, [PathBuf::from("x.pos")]);
    assert_eq!(
        parsed.shot,
        Some(ShotRequest {
            output: PathBuf::from("a.png"),
            replace: true,
            size: (1280, 720),
            theme: ThemeId::Light,
            wait: 12,
        })
    );
}

#[test]
fn a_wait_of_zero_and_of_the_limit_are_allowed() {
    for (text, wait) in [("0", 0), ("3600", 3600)] {
        let parsed = parse_window_args(&args(&["--shot", "a.png", "--wait", text]), 1);
        assert_eq!(parsed.unwrap().shot.unwrap().wait, wait);
    }
}

#[test]
fn a_mistake_gives_its_reason() {
    for (list, reason) in [
        (vec!["--shot"], "needs a file to write"),
        (vec!["--shot", "a.jpg"], "must end in .png"),
        (vec!["--shot", "a.png", "--size"], "needs WIDTHxHEIGHT"),
        (
            vec!["--shot", "a.png", "--size", "10x10"],
            "from 320 to 8192",
        ),
        (
            vec!["--shot", "a.png", "--theme", "blue"],
            "phoenix, light or contrast",
        ),
        (vec!["--shot", "a.png", "--wait", "-1"], "from 0 to 3600"),
        (vec!["--shot", "a.png", "--wait", "3601"], "from 0 to 3600"),
        (vec!["--size", "800x600"], "need --shot"),
        (vec!["--replace"], "need --shot"),
        (vec!["--lang"], "needs a language code"),
        (vec!["--lang", "xx"], "unknown language code"),
        (vec!["--open"], "needs a file"),
        (vec!["--nonsense"], "unknown option --nonsense"),
        (vec!["a.pos", "b.pos"], "only one file"),
    ] {
        let error = parse_window_args(&args(&list), 1).unwrap_err();
        assert!(error.contains(reason), "{list:?}: {error}");
    }
    let error = parse_window_args(&args(&["a", "b", "c"]), 2).unwrap_err();
    assert!(error.contains("at most 2 files"), "{error}");
}

#[cfg(not(feature = "gui"))]
#[test]
fn the_shot_option_says_that_this_build_has_no_window() {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_phoenix-pnp"))
        .args(["--shot", "picture.png"])
        .output()
        .expect("the program runs");
    assert!(!out.status.success());
    let text = String::from_utf8_lossy(&out.stderr);
    assert!(text.contains("--features gui"), "{text}");
    assert!(!std::path::Path::new("picture.png").exists());
}
