// EH3D-PHOENIX-PNP - the picture mode of the window.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! The picture mode of the window (`--shot FILE.png`): what is asked for, read and checked without a screen. The window opens at the size asked, loads the files given with `--open`,
//! waits a number of frames so that the drawing is complete, saves what it drew as a PNG file and closes by itself. In this mode the window keeps no settings, so a run leaves
//! nothing of the person's changed.

use super::theme::ThemeId;
use crate::error::Lang;
use std::path::PathBuf;

/// The smallest side of the window of a picture, in pixels.
pub const SIZE_MIN: u32 = 320;
/// The largest side of the window of a picture, in pixels.
pub const SIZE_MAX: u32 = 8192;
/// The size of the window of a picture when `--size` is not given.
pub const DEFAULT_SIZE: (u32, u32) = (1180, 760);
/// The frames drawn before the picture is taken when `--wait` is not given.
pub const DEFAULT_WAIT: u32 = 6;
/// The most frames `--wait` may ask for.
pub const WAIT_MAX: u32 = 3600;
/// The frames after the one asked for in which the picture may still come back; after them the run fails.
pub const PATIENCE_FRAMES: u32 = 600;

/// What the picture mode was asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShotRequest {
    /// The PNG file to write.
    pub output: PathBuf,
    /// True when a file that exists may be replaced; otherwise it is an error.
    pub replace: bool,
    /// The size of the window in pixels, width and height.
    pub size: (u32, u32),
    /// The look of the window: one of the themes of the program.
    pub theme: ThemeId,
    /// The frames drawn before the picture is taken.
    pub wait: u32,
}

/// What the window was asked for on the command line.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WindowArgs {
    /// The files to open at once (bare arguments and `--open FILE`).
    pub files: Vec<PathBuf>,
    /// The language of the window; none takes the one of the system.
    pub lang: Option<Lang>,
    /// The picture mode: the window opens, saves a PNG file and closes by itself.
    pub shot: Option<ShotRequest>,
}

/// Reads a size written `WIDTHxHEIGHT` (`1280x720`, also with a capital X), each side between [`SIZE_MIN`] and [`SIZE_MAX`].
pub fn parse_size(text: &str) -> Option<(u32, u32)> {
    let (width, height) = text.trim().split_once(['x', 'X'])?;
    let width: u32 = width.trim().parse().ok()?;
    let height: u32 = height.trim().parse().ok()?;
    let ok = |side: u32| (SIZE_MIN..=SIZE_MAX).contains(&side);
    (ok(width) && ok(height)).then_some((width, height))
}

/// Whether the arguments ask for the picture mode.
pub fn wants_picture(args: &[String]) -> bool {
    args.iter().any(|a| a == "--shot")
}

/// The options of the picture as they are read, before they are checked together.
#[derive(Default)]
struct PictureOptions {
    size: Option<(u32, u32)>,
    theme: Option<ThemeId>,
    wait: Option<u32>,
    replace: bool,
}

/// Reads the arguments of the window (after an optional leading `gui`): `--shot FILE.png`, `--size WxH`, `--theme`, `--wait N`, `--replace`, `--lang CODE`, `--open FILE` and bare
/// file names, of which at most `max_files` are accepted. The answer for a mistake is the reason.
pub fn parse_window_args(args: &[String], max_files: usize) -> Result<WindowArgs, String> {
    let args = if args.first().is_some_and(|a| a == "gui") {
        &args[1..]
    } else {
        args
    };
    let mut parsed = WindowArgs::default();
    let mut output: Option<PathBuf> = None;
    let mut picture = PictureOptions::default();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--shot" => {
                let path = iter.next().ok_or("--shot needs a file to write")?;
                if !path.to_ascii_lowercase().ends_with(".png") {
                    return Err("--shot writes a PNG file: the name must end in .png".to_owned());
                }
                output = Some(PathBuf::from(path));
            }
            "--size" => {
                let text = iter.next().ok_or("--size needs WIDTHxHEIGHT")?;
                picture.size = Some(parse_size(text).ok_or(
                    "--size is WIDTHxHEIGHT in pixels, each side from 320 to 8192, as 1280x720",
                )?);
            }
            "--theme" => {
                let text = iter
                    .next()
                    .ok_or("--theme needs phoenix, light or contrast")?;
                picture.theme =
                    Some(ThemeId::from_key(text).ok_or("--theme is phoenix, light or contrast")?);
            }
            "--wait" => {
                let text = iter.next().ok_or("--wait needs a number of frames")?;
                picture.wait = Some(
                    text.parse::<u32>()
                        .ok()
                        .filter(|n| *n <= WAIT_MAX)
                        .ok_or("--wait is a number of frames from 0 to 3600")?,
                );
            }
            "--lang" => {
                let tag = iter.next().ok_or("--lang needs a language code")?;
                parsed.lang = Some(Lang::from_code(tag).ok_or("unknown language code")?);
            }
            "--replace" => picture.replace = true,
            "--open" => {
                let path = iter.next().ok_or("--open needs a file")?;
                add_file(&mut parsed, path, max_files)?;
            }
            other if other.starts_with("--") => return Err(format!("unknown option {other}")),
            other => add_file(&mut parsed, other, max_files)?,
        }
    }
    parsed.shot = match output {
        Some(output) => Some(ShotRequest {
            output,
            replace: picture.replace,
            size: picture.size.unwrap_or(DEFAULT_SIZE),
            theme: picture.theme.unwrap_or(ThemeId::Phoenix),
            wait: picture.wait.unwrap_or(DEFAULT_WAIT),
        }),
        None => {
            if picture.size.is_some()
                || picture.theme.is_some()
                || picture.wait.is_some()
                || picture.replace
            {
                return Err("--size, --theme, --wait and --replace need --shot".to_owned());
            }
            None
        }
    };
    Ok(parsed)
}

fn add_file(parsed: &mut WindowArgs, path: &str, max_files: usize) -> Result<(), String> {
    if parsed.files.len() >= max_files {
        return Err(if max_files == 1 {
            "only one file can be opened".to_owned()
        } else {
            format!("at most {max_files} files can be opened")
        });
    }
    parsed.files.push(PathBuf::from(path));
    Ok(())
}
