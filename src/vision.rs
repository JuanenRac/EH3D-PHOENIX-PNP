//! The vision pipeline, on pictures made in memory: it measures where a part is under the nozzle and how much it is turned, from a grey picture, with the steps of a file of settings
//! (`vision.cfg`). The steps are a mask, a filter for the noise, a threshold (a fixed one or the one of Otsu), the spots of the picture, a range of area for the spot that is the part,
//! and the smallest rectangle, turned, that holds the spot. There is no real camera here: `render` draws the picture of a part from its true place, its turn and a noise that comes from
//! a seed, so that the same part is always seen the same way, and the simulated camera of the job measures that picture instead of reading a number.
//!
//! Coordinates. A picture has its first row on top. The middle of the picture is the axis of the nozzle: a part found to the right of it has a positive `dx_mm`, a part found above it
//! (towards the first row) has a positive `dy_mm`, as on the machine, and a turn is positive counter-clockwise. A pixel is a square of side `mm_per_px`.
//!
//! The turn of a rectangle is known up to a half turn (a rectangle turned 180 degrees looks the same), so the pipeline gives it from -90 (not included) to 90 degrees, measured from the
//! axis along which the long side of the part lies when it is not turned (`nominal_long_axis`); a part that is nearly a square (the long side shorter than `square_ratio` times the
//! short one) is known up to a quarter turn and its turn is given from -45 (not included) to 45 degrees.

use crate::config::{bad, int, num};
use crate::error::{Code, PnpError};
use crate::model::Offset;
use crate::sim::Rng;
use std::collections::HashSet;

/// The largest vision file that is read.
pub const MAX_VISION_BYTES: usize = 65_536;
/// The version of the format of the file that this program writes and reads.
pub const VISION_SCHEMA_VERSION: u32 = 1;
const MAX_LINE_CHARS: usize = 1024;

/// Every key that the file knows, in the order of `examples/vision.cfg`.
pub const KEYS: [&str; 19] = [
    "schema_version",
    "mm_per_px",
    "mask",
    "filter",
    "filter_passes",
    "threshold_mode",
    "threshold",
    "part_is_dark",
    "min_area_px",
    "max_area_px",
    "nominal_long_axis",
    "square_ratio",
    "image_width",
    "image_height",
    "sim_part_long_mm",
    "sim_part_short_mm",
    "sim_background",
    "sim_foreground",
    "sim_noise_levels",
];

/// The text of the file with every key at its default and a line for what it means: `examples/vision.cfg` is this text, and a test keeps them equal.
pub fn default_file() -> &'static str {
    DEFAULT_FILE
}

const DEFAULT_FILE: &str = r#"# EH3D-PHOENIX-PNP vision settings: the steps that measure a part in a picture. Every line is optional; a missing line keeps the default shown here.
# The format is key = value, one per line. A line that starts with # is a comment. A key that is not known, a key written twice or a value out of its range is an error.

# The version of this file format. A file with a higher number than the program knows is refused.
schema_version = 1

# The size of a pixel on the part, in millimetres (0.001 to 10): what turns the pixels of the picture into millimetres.
mm_per_px = 0.05

# The mask: only what is inside it is looked at. none; rect x0 y0 x1 y1 (pixels from x0 up to, not including, x1; the same in y); or circle cx cy r (the middle and the radius, in pixels).
mask = none

# The filter that takes the noise out of the grey picture before the threshold: none, or median3 (the median of the 3 x 3 pixels around each one), repeated filter_passes times (1 to 5).
filter = median3
filter_passes = 1

# The threshold that tells the part from the background: fixed (the level of threshold, 0 to 255) or otsu (the level that best separates two groups of grey, found in each picture).
# A pixel is part of the spot when it is brighter than the level, or, when part_is_dark is yes, when it is at most as bright as the level.
threshold_mode = fixed
threshold = 128
part_is_dark = no

# The area, in pixels, that the spot of the part can have. A spot smaller than min_area_px is noise and is left out; a single spot larger than max_area_px is not a part.
min_area_px = 50
max_area_px = 4000

# The axis of the picture, x or y, along which the long side of the part lies when the part is not turned, and how much longer than the short side the long side must be to tell
# which one it is (1 to 3): a part less elongated than this is taken for a square and its turn is known up to a quarter turn only.
nominal_long_axis = x
square_ratio = 1.1

# The simulated picture: its size in pixels (16 to 1024), the size of the part in millimetres, the grey of the background and of the part (0 to 255) and how much grey noise is added
# to every pixel (from minus to plus this many levels, 0 to 127, drawn from sim_seed).
image_width = 128
image_height = 128
sim_part_long_mm = 1.6
sim_part_short_mm = 0.8
sim_background = 30
sim_foreground = 200
sim_noise_levels = 0
"#;

/// How the grey level that tells the part from the background is found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThresholdMode {
    /// The level of the `threshold` setting.
    #[default]
    Fixed,
    /// The level that separates the greys of the picture into two groups with the most distance between them (the method of Otsu).
    Otsu,
}

/// The filter that takes the noise out of the picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Filter {
    None,
    /// The median of the 3 x 3 pixels around each pixel (the edge of the picture repeats itself).
    #[default]
    Median3,
}

/// An axis of the picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Axis {
    #[default]
    X,
    Y,
}

/// The part of the picture that is looked at; what is outside is background.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Mask {
    /// All of the picture.
    #[default]
    None,
    /// The pixels from `x0` up to, not including, `x1`, and from `y0` up to, not including, `y1`.
    Rect {
        x0: usize,
        y0: usize,
        x1: usize,
        y1: usize,
    },
    /// The pixels whose middle is at most `r` pixels from `(cx, cy)`, measured from the corner of the picture (the middle of the pixel of the first column and row is at 0.5, 0.5).
    Circle { cx: f64, cy: f64, r: f64 },
}

impl Mask {
    fn contains(&self, x: usize, y: usize) -> bool {
        match *self {
            Mask::None => true,
            Mask::Rect { x0, y0, x1, y1 } => x >= x0 && x < x1 && y >= y0 && y < y1,
            Mask::Circle { cx, cy, r } => {
                let dx: f64 = x as f64 + 0.5 - cx;
                let dy: f64 = y as f64 + 0.5 - cy;
                dx * dx + dy * dy <= r * r
            }
        }
    }
}

/// The settings of the pipeline and of the simulated picture.
#[derive(Debug, Clone, PartialEq)]
pub struct VisionConfig {
    pub schema_version: u32,
    pub mm_per_px: f64,
    pub mask: Mask,
    pub filter: Filter,
    pub filter_passes: u32,
    pub threshold_mode: ThresholdMode,
    pub threshold: u8,
    pub part_is_dark: bool,
    pub min_area_px: usize,
    pub max_area_px: usize,
    pub nominal_long_axis: Axis,
    pub square_ratio: f64,
    pub image_width: usize,
    pub image_height: usize,
    pub sim_part_long_mm: f64,
    pub sim_part_short_mm: f64,
    pub sim_background: u8,
    pub sim_foreground: u8,
    pub sim_noise_levels: u32,
}

impl Default for VisionConfig {
    fn default() -> VisionConfig {
        VisionConfig {
            schema_version: VISION_SCHEMA_VERSION,
            mm_per_px: 0.05,
            mask: Mask::None,
            filter: Filter::Median3,
            filter_passes: 1,
            threshold_mode: ThresholdMode::Fixed,
            threshold: 128,
            part_is_dark: false,
            min_area_px: 50,
            max_area_px: 4000,
            nominal_long_axis: Axis::X,
            square_ratio: 1.1,
            image_width: 128,
            image_height: 128,
            sim_part_long_mm: 1.6,
            sim_part_short_mm: 0.8,
            sim_background: 30,
            sim_foreground: 200,
            sim_noise_levels: 0,
        }
    }
}

fn mask_numbers(line: usize, key: &str, v: &str, words: &[&str]) -> Result<Vec<f64>, PnpError> {
    words
        .iter()
        .map(|w| {
            w.parse::<f64>()
                .ok()
                .filter(|x: &f64| x.is_finite())
                .ok_or_else(|| bad(line, key, v, "none, rect x0 y0 x1 y1 or circle cx cy r"))
        })
        .collect()
}

fn parse_mask(line: usize, key: &str, v: &str) -> Result<Mask, PnpError> {
    let shape = "none, rect x0 y0 x1 y1 or circle cx cy r";
    let words: Vec<&str> = v.split_whitespace().collect();
    match words.first().map(|w| w.to_ascii_lowercase()).as_deref() {
        Some("none") if words.len() == 1 => Ok(Mask::None),
        Some("rect") if words.len() == 5 => {
            let n = mask_numbers(line, key, v, &words[1..])?;
            let whole = n
                .iter()
                .all(|x| x.fract() == 0.0 && (0.0..=100_000.0).contains(x));
            if !whole || n[0] >= n[2] || n[1] >= n[3] {
                return Err(bad(
                    line,
                    key,
                    v,
                    "a rectangle of whole pixels with x0 below x1 and y0 below y1",
                ));
            }
            Ok(Mask::Rect {
                x0: n[0] as usize,
                y0: n[1] as usize,
                x1: n[2] as usize,
                y1: n[3] as usize,
            })
        }
        Some("circle") if words.len() == 4 => {
            let n = mask_numbers(line, key, v, &words[1..])?;
            if !n.iter().all(|x| x.abs() <= 100_000.0) || n[2] < 0.5 {
                return Err(bad(
                    line,
                    key,
                    v,
                    "a circle with a radius of at least half a pixel",
                ));
            }
            Ok(Mask::Circle {
                cx: n[0],
                cy: n[1],
                r: n[2],
            })
        }
        _ => Err(bad(line, key, v, shape)),
    }
}

impl VisionConfig {
    /// Reads a vision file. A missing key keeps its default; the result is checked as a whole.
    pub fn parse(text: &str) -> Result<VisionConfig, PnpError> {
        if text.len() > MAX_VISION_BYTES {
            return Err(PnpError::new(
                Code::FileTooBig,
                format!("the vision settings are larger than {MAX_VISION_BYTES} bytes"),
            ));
        }
        let mut cfg = VisionConfig::default();
        let mut seen: HashSet<String> = HashSet::new();
        for (i, raw) in text.trim_start_matches('\u{feff}').lines().enumerate() {
            let line = i + 1;
            let t = raw.trim();
            if t.is_empty() || t.starts_with('#') {
                continue;
            }
            if t.chars().count() > MAX_LINE_CHARS {
                return Err(PnpError::new(
                    Code::ConfigSyntax,
                    format!("line {line}: longer than {MAX_LINE_CHARS} characters"),
                ));
            }
            let Some((k, v)) = t.split_once('=') else {
                return Err(PnpError::new(
                    Code::ConfigSyntax,
                    format!("line {line}: '{t}' is not key = value"),
                ));
            };
            let (key, value) = (k.trim().to_ascii_lowercase(), v.trim());
            if !seen.insert(key.clone()) {
                return Err(PnpError::new(
                    Code::ConfigDuplicateKey,
                    format!("line {line}: '{key}' is written twice"),
                ));
            }
            cfg.set(&key, value, line)?;
        }
        cfg.validate()?;
        Ok(cfg)
    }

    fn set(&mut self, key: &str, v: &str, line: usize) -> Result<(), PnpError> {
        let n = |lo: f64, hi: f64| num(line, key, v, lo, hi);
        match key {
            "schema_version" => {
                let s = int(line, key, v, 1, 1_000_000)? as u32;
                if s > VISION_SCHEMA_VERSION {
                    return Err(PnpError::new(
                        Code::ConfigSchemaNewer,
                        format!("line {line}: schema_version {s}, this program reads up to {VISION_SCHEMA_VERSION}"),
                    ));
                }
                self.schema_version = s;
            }
            "mm_per_px" => self.mm_per_px = n(0.001, 10.0)?,
            "mask" => self.mask = parse_mask(line, key, v)?,
            "filter" => {
                self.filter = match v.to_ascii_lowercase().as_str() {
                    "none" => Filter::None,
                    "median3" => Filter::Median3,
                    _ => return Err(bad(line, key, v, "none or median3")),
                }
            }
            "filter_passes" => self.filter_passes = int(line, key, v, 1, 5)? as u32,
            "threshold_mode" => {
                self.threshold_mode = match v.to_ascii_lowercase().as_str() {
                    "fixed" => ThresholdMode::Fixed,
                    "otsu" => ThresholdMode::Otsu,
                    _ => return Err(bad(line, key, v, "fixed or otsu")),
                }
            }
            "threshold" => self.threshold = int(line, key, v, 0, 255)? as u8,
            "part_is_dark" => {
                self.part_is_dark = match v.to_ascii_lowercase().as_str() {
                    "yes" | "true" | "on" | "1" => true,
                    "no" | "false" | "off" | "0" => false,
                    _ => return Err(bad(line, key, v, "yes or no")),
                }
            }
            "min_area_px" => self.min_area_px = int(line, key, v, 1, 10_000_000)? as usize,
            "max_area_px" => self.max_area_px = int(line, key, v, 1, 10_000_000)? as usize,
            "nominal_long_axis" => {
                self.nominal_long_axis = match v.to_ascii_lowercase().as_str() {
                    "x" => Axis::X,
                    "y" => Axis::Y,
                    _ => return Err(bad(line, key, v, "x or y")),
                }
            }
            "square_ratio" => self.square_ratio = n(1.0, 3.0)?,
            "image_width" => self.image_width = int(line, key, v, 16, 1024)? as usize,
            "image_height" => self.image_height = int(line, key, v, 16, 1024)? as usize,
            "sim_part_long_mm" => self.sim_part_long_mm = n(0.05, 50.0)?,
            "sim_part_short_mm" => self.sim_part_short_mm = n(0.05, 50.0)?,
            "sim_background" => self.sim_background = int(line, key, v, 0, 255)? as u8,
            "sim_foreground" => self.sim_foreground = int(line, key, v, 0, 255)? as u8,
            "sim_noise_levels" => self.sim_noise_levels = int(line, key, v, 0, 127)? as u32,
            _ => {
                return Err(PnpError::new(
                    Code::ConfigUnknownKey,
                    format!("line {line}: '{key}' is not a setting of the vision"),
                ))
            }
        }
        Ok(())
    }

    /// The checks that look at more than one setting.
    pub fn validate(&self) -> Result<(), PnpError> {
        let unsafe_setting = |m: String| Err(PnpError::new(Code::UnsafeSetting, m));
        if self.min_area_px > self.max_area_px {
            return unsafe_setting(format!(
                "min_area_px {} is above max_area_px {}",
                self.min_area_px, self.max_area_px
            ));
        }
        if self.sim_part_short_mm > self.sim_part_long_mm {
            return unsafe_setting(format!(
                "sim_part_short_mm {} is above sim_part_long_mm {}",
                self.sim_part_short_mm, self.sim_part_long_mm
            ));
        }
        if self.sim_foreground == self.sim_background {
            return unsafe_setting(
                "sim_foreground and sim_background are the same grey: the part could not be seen"
                    .to_string(),
            );
        }
        Ok(())
    }
}

/// A picture in grey levels, row by row, the first row on top.
#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
}

impl Image {
    /// A picture of one grey.
    pub fn new(width: usize, height: usize, grey: u8) -> Image {
        Image {
            width,
            height,
            pixels: vec![grey; width * height],
        }
    }

    pub fn get(&self, x: usize, y: usize) -> u8 {
        self.pixels[y * self.width + x]
    }

    pub fn set(&mut self, x: usize, y: usize, grey: u8) {
        self.pixels[y * self.width + x] = grey;
    }

    /// Paints the pixels from `x0` up to, not including, `x1`, and from `y0` up to, not including, `y1`, that are inside the picture.
    pub fn fill_rect(&mut self, x0: usize, y0: usize, x1: usize, y1: usize, grey: u8) {
        for y in y0..y1.min(self.height) {
            for x in x0..x1.min(self.width) {
                self.set(x, y, grey);
            }
        }
    }
}

/// The picture that a camera under the nozzle would give of a part whose middle is `truth.dx_mm` to the right of and `truth.dy_mm` above the axis of the nozzle, turned `truth.rotation_deg`
/// counter-clockwise: the part is `sim_foreground` and the rest `sim_background`, plus the noise (every pixel moves by up to `sim_noise_levels` levels either way). A pixel is the part
/// when its middle is inside the rectangle. Two numbers are drawn from `rng` for every pixel whatever the settings, so that a setting does not move the noise of the others.
pub fn render(cfg: &VisionConfig, truth: Offset, rng: &mut Rng) -> Image {
    let (w, h) = (cfg.image_width, cfg.image_height);
    let mut img = Image::new(w, h, cfg.sim_background);
    let theta: f64 = truth.rotation_deg.to_radians();
    let (sn, cs) = theta.sin_cos();
    let (half_long, half_short): (f64, f64) =
        (cfg.sim_part_long_mm / 2.0, cfg.sim_part_short_mm / 2.0);
    let (half_u, half_v) = match cfg.nominal_long_axis {
        Axis::X => (half_long, half_short),
        Axis::Y => (half_short, half_long),
    };
    for iy in 0..h {
        for ix in 0..w {
            let x_mm: f64 = (ix as f64 + 0.5 - w as f64 / 2.0) * cfg.mm_per_px - truth.dx_mm;
            let y_mm: f64 = (h as f64 / 2.0 - iy as f64 - 0.5) * cfg.mm_per_px - truth.dy_mm;
            let u: f64 = x_mm * cs + y_mm * sn;
            let v: f64 = -x_mm * sn + y_mm * cs;
            let inside = u.abs() <= half_u && v.abs() <= half_v;
            let base = if inside {
                cfg.sim_foreground
            } else {
                cfg.sim_background
            };
            let shift: f64 = (2.0 * rng.next_f64() - 1.0) * f64::from(cfg.sim_noise_levels);
            let grey = (f64::from(base) + shift.round()).clamp(0.0, 255.0);
            img.set(ix, iy, grey as u8);
        }
    }
    img
}

/// The median of the 3 x 3 pixels around each pixel; the edge of the picture repeats itself.
pub fn median3(img: &Image) -> Image {
    let (w, h) = (img.width, img.height);
    if w == 0 || h == 0 {
        return img.clone();
    }
    let mut out = Image::new(w, h, 0);
    for y in 0..h {
        for x in 0..w {
            let mut around = [0u8; 9];
            for (k, slot) in around.iter_mut().enumerate() {
                let sx = (x + k % 3).saturating_sub(1).min(w - 1);
                let sy = (y + k / 3).saturating_sub(1).min(h - 1);
                *slot = img.get(sx, sy);
            }
            around.sort_unstable();
            out.set(x, y, around[4]);
        }
    }
    out
}

/// The level of Otsu for the pixels inside the mask: the level at which the pixels up to it and the pixels above it are the most apart (the largest variance between the two groups);
/// the lowest level wins a tie. `None` when no level separates two groups (the picture has one grey only).
pub fn otsu_level(img: &Image, mask: &Mask) -> Option<u8> {
    let mut hist = [0u64; 256];
    let mut total: u64 = 0;
    for y in 0..img.height {
        for x in 0..img.width {
            if mask.contains(x, y) {
                hist[usize::from(img.get(x, y))] += 1;
                total += 1;
            }
        }
    }
    let sum_all: f64 = hist
        .iter()
        .enumerate()
        .map(|(i, &c)| i as f64 * c as f64)
        .sum();
    let (mut below, mut sum_below) = (0u64, 0.0f64);
    let (mut best, mut level): (f64, Option<u8>) = (0.0, None);
    for (t, &count) in hist.iter().enumerate().take(255) {
        below += count;
        sum_below += t as f64 * count as f64;
        if below == 0 {
            continue;
        }
        let above = total - below;
        if above == 0 {
            break;
        }
        let mean_below: f64 = sum_below / below as f64;
        let mean_above: f64 = (sum_all - sum_below) / above as f64;
        let diff: f64 = mean_below - mean_above;
        let between: f64 = below as f64 * above as f64 * diff * diff;
        if between > best {
            best = between;
            level = Some(t as u8);
        }
    }
    level
}

/// What the pipeline found.
#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    /// Where the part is from the axis of the nozzle, and how much it is turned.
    pub offset: Offset,
    /// The middle of the rectangle, in pixels from the corner of the picture.
    pub centre_px: (f64, f64),
    /// The sides of the rectangle in pixels, the long one first.
    pub long_px: f64,
    pub short_px: f64,
    /// The pixels of the spot.
    pub area_px: usize,
    /// The grey level used as the threshold.
    pub threshold: u8,
}

fn spots(part: &[bool], w: usize, h: usize) -> (Vec<u32>, Vec<usize>) {
    let mut label = vec![0u32; w * h];
    let mut areas: Vec<usize> = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    for start in 0..w * h {
        if !part[start] || label[start] != 0 {
            continue;
        }
        areas.push(0);
        let id = areas.len() as u32;
        label[start] = id;
        stack.push(start);
        while let Some(p) = stack.pop() {
            areas[id as usize - 1] += 1;
            let (x, y) = ((p % w) as i64, (p / w) as i64);
            for dy in -1i64..=1 {
                for dx in -1i64..=1 {
                    let (nx, ny) = (x + dx, y + dy);
                    if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                        continue;
                    }
                    let q = ny as usize * w + nx as usize;
                    if part[q] && label[q] == 0 {
                        label[q] = id;
                        stack.push(q);
                    }
                }
            }
        }
    }
    (label, areas)
}

fn cross(o: (i64, i64), a: (i64, i64), b: (i64, i64)) -> i64 {
    (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0)
}

/// The convex hull of points with whole coordinates (the monotone chain), without the points on a side.
fn hull(mut points: Vec<(i64, i64)>) -> Vec<(i64, i64)> {
    points.sort_unstable();
    points.dedup();
    if points.len() < 3 {
        return points;
    }
    let mut lower: Vec<(i64, i64)> = Vec::new();
    for &p in &points {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], p) <= 0 {
            lower.pop();
        }
        lower.push(p);
    }
    let mut upper: Vec<(i64, i64)> = Vec::new();
    for &p in points.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], p) <= 0 {
            upper.pop();
        }
        upper.push(p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// The rectangle of the smallest area that holds the points of a hull, turned: one of its sides lies along a side of the hull.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub centre: (f64, f64),
    pub long: f64,
    pub short: f64,
    /// The direction of the long side, in degrees from the x axis of the picture towards its y axis (down), from -90 (not included) to 90.
    pub long_deg: f64,
    /// The direction of a side of the rectangle in degrees (any of the four), found for the search, from -180 to 180.
    side_deg: f64,
}

/// A number brought into `period / 2` either way of 0: from minus half the period (not included) to half the period.
pub fn wrap(angle: f64, period: f64) -> f64 {
    let r: f64 = angle.rem_euclid(period);
    if r > period / 2.0 {
        r - period
    } else {
        r
    }
}

/// The smallest rectangle, turned, that holds all the points; `None` for fewer than three points that do not lie on a line.
pub fn min_area_rect(points: &[(i64, i64)]) -> Option<Rect> {
    let h = hull(points.to_vec());
    if h.len() < 3 {
        return None;
    }
    let mut best: Option<(f64, Rect)> = None;
    for i in 0..h.len() {
        let (p, q) = (h[i], h[(i + 1) % h.len()]);
        let (ex, ey): (f64, f64) = ((q.0 - p.0) as f64, (q.1 - p.1) as f64);
        let len: f64 = ex.hypot(ey);
        if len == 0.0 {
            continue;
        }
        let (d, n) = ((ex / len, ey / len), (-ey / len, ex / len));
        let (mut u0, mut u1, mut v0, mut v1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
        for &(px, py) in &h {
            let (x, y): (f64, f64) = (px as f64, py as f64);
            let u: f64 = x * d.0 + y * d.1;
            let v: f64 = x * n.0 + y * n.1;
            u0 = u0.min(u);
            u1 = u1.max(u);
            v0 = v0.min(v);
            v1 = v1.max(v);
        }
        let (wu, wv): (f64, f64) = (u1 - u0, v1 - v0);
        let area: f64 = wu * wv;
        if best.as_ref().is_none_or(|(a, _)| area < *a - 1e-9) {
            let side_deg: f64 = ey.atan2(ex).to_degrees();
            let (uc, vc): (f64, f64) = ((u0 + u1) / 2.0, (v0 + v1) / 2.0);
            let long_deg: f64 = wrap(if wu >= wv { side_deg } else { side_deg + 90.0 }, 180.0);
            best = Some((
                area,
                Rect {
                    centre: (uc * d.0 + vc * n.0, uc * d.1 + vc * n.1),
                    long: wu.max(wv),
                    short: wu.min(wv),
                    long_deg,
                    side_deg,
                },
            ));
        }
    }
    best.map(|(_, r)| r)
}

/// The pixels of the spot `id` as the corners of its pixels (the picture is a grid of squares whose corners have whole coordinates), reduced to the ends of every row, which have the same hull.
fn spot_corners(label: &[u32], id: u32, w: usize, h: usize) -> Vec<(i64, i64)> {
    let mut corners = Vec::new();
    for y in 0..h {
        let row = &label[y * w..(y + 1) * w];
        let first = row.iter().position(|&l| l == id);
        let last = row.iter().rposition(|&l| l == id);
        if let (Some(a), Some(b)) = (first, last) {
            let (x0, x1, y0, y1) = (a as i64, b as i64 + 1, y as i64, y as i64 + 1);
            corners.extend([(x0, y0), (x0, y1), (x1, y0), (x1, y1)]);
        }
    }
    corners
}

/// Measures the part in a picture with the steps of the settings. The mistakes are `PHOENIX-PNP-037` (no spot of the size of the part), `038` (the one spot has an area out of the range) and
/// `039` (more than one spot could be the part).
pub fn measure_image(cfg: &VisionConfig, img: &Image) -> Result<Reading, PnpError> {
    let (w, h) = (img.width, img.height);
    if w == 0 || h == 0 || img.pixels.len() != w * h {
        return Err(PnpError::new(
            Code::VisionNotFound,
            "the picture has no pixels",
        ));
    }
    let mut grey = img.clone();
    if cfg.filter == Filter::Median3 {
        for _ in 0..cfg.filter_passes {
            grey = median3(&grey);
        }
    }
    let level = match cfg.threshold_mode {
        ThresholdMode::Fixed => cfg.threshold,
        ThresholdMode::Otsu => otsu_level(&grey, &cfg.mask).ok_or_else(|| {
            PnpError::new(
                Code::VisionNotFound,
                "the picture has one grey only: nothing tells the part from the background",
            )
        })?,
    };
    let mut part = vec![false; w * h];
    for y in 0..h {
        for x in 0..w {
            if cfg.mask.contains(x, y) {
                let g = grey.get(x, y);
                part[y * w + x] = if cfg.part_is_dark {
                    g <= level
                } else {
                    g > level
                };
            }
        }
    }
    let (label, areas) = spots(&part, w, h);
    let mut candidates: Vec<(usize, u32)> = areas
        .iter()
        .enumerate()
        .filter(|(_, &a)| a >= cfg.min_area_px)
        .map(|(i, &a)| (a, i as u32 + 1))
        .collect();
    if candidates.is_empty() {
        let largest = areas.iter().max().copied().unwrap_or(0);
        return Err(PnpError::new(
            Code::VisionNotFound,
            format!(
                "no spot of at least {} pixels (the largest has {largest})",
                cfg.min_area_px
            ),
        ));
    }
    if candidates.len() > 1 {
        candidates.sort_unstable_by(|a, b| b.cmp(a));
        let list: Vec<String> = candidates.iter().take(5).map(|c| c.0.to_string()).collect();
        return Err(PnpError::new(
            Code::VisionSeveralSpots,
            format!(
                "{} spots of at least {} pixels, the largest have {}",
                candidates.len(),
                cfg.min_area_px,
                list.join(", ")
            ),
        ));
    }
    let (area, id) = candidates[0];
    if area > cfg.max_area_px {
        return Err(PnpError::new(
            Code::VisionAreaOutOfRange,
            format!(
                "the spot has {area} pixels, the part can have {} to {}",
                cfg.min_area_px, cfg.max_area_px
            ),
        ));
    }
    let rect = min_area_rect(&spot_corners(&label, id, w, h)).ok_or_else(|| {
        PnpError::new(
            Code::VisionNotFound,
            "the spot is too thin to be a rectangle",
        )
    })?;
    let ratio: f64 = rect.long / rect.short;
    // the turn is read in the frame of the machine (the second axis points up), so its sign changes
    let rotation_deg = if ratio < cfg.square_ratio {
        wrap(-rect.side_deg, 90.0)
    } else {
        let machine: f64 = -rect.long_deg;
        match cfg.nominal_long_axis {
            Axis::X => wrap(machine, 180.0),
            Axis::Y => wrap(machine - 90.0, 180.0),
        }
    };
    let offset = Offset {
        dx_mm: (rect.centre.0 - w as f64 / 2.0) * cfg.mm_per_px,
        dy_mm: (h as f64 / 2.0 - rect.centre.1) * cfg.mm_per_px,
        rotation_deg,
    };
    Ok(Reading {
        offset,
        centre_px: rect.centre,
        long_px: rect.long,
        short_px: rect.short,
        area_px: area,
        threshold: level,
    })
}
