// EH3D-PHOENIX-PNP - starting points for the settings of a machine.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! A new machine needs a file of settings, and the file with every key (`examples/machine.cfg`) is long. These are short files for the kinds of machine people start from: a small
//! desktop machine with one nozzle or with two, a machine with two heads, a faster one with four nozzles, one that fails now and then to try how a job deals with it, and one that works
//! through the EH3D server. Each is a few lines that say what differs from the defaults, with a comment that says why; the rest of the keys keep their defaults. `full` is the whole
//! documented example. The numbers are starting points to be tuned on the machine (they are not measurements of any machine), and every template is read by the same reader as any
//! file of settings, so a template that is not valid cannot be shipped (a test reads them all).

/// The documented file with every key.
const FULL: &str = include_str!("../examples/machine.cfg");

/// A template: its name, what it is for and its lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Template {
    pub name: &'static str,
    pub about: &'static str,
    lines: &'static str,
}

const TEMPLATES: [Template; 7] = [
    Template {
        name: "desktop-1",
        about: "a small desktop machine with one nozzle, slow and gentle",
        lines: "nozzles = 1\nspeed_mm_s = 100\nx_accel_mm_s2 = 1000\ny_accel_mm_s2 = 1000\npick_s = 0.5\nplace_s = 0.5\nrotation_deg_s = 360\nfeed_retries = 2\npick_retries = 3\nlimit_x_min_mm = 0\nlimit_x_max_mm = 400\nlimit_y_min_mm = 0\nlimit_y_max_mm = 400\n",
    },
    Template {
        name: "desktop-2",
        about: "a desktop machine with two nozzles on one head",
        lines: "nozzles = 2\nspeed_mm_s = 150\nx_accel_mm_s2 = 2000\ny_accel_mm_s2 = 2000\ntip_changer_x_mm = -50\ntip_changer_y_mm = 0\nlimit_x_min_mm = -100\nlimit_x_max_mm = 500\nlimit_y_min_mm = -100\nlimit_y_max_mm = 500\n",
    },
    Template {
        name: "dual-head",
        about: "two heads of two nozzles, 60 mm apart, that share the feeders",
        lines: "nozzles = 2\nspeed_mm_s = 200\nx_accel_mm_s2 = 3000\ny_accel_mm_s2 = 3000\nhead = left : 2 : 0 : 0\nhead = right : 2 : 60 : 0\nlimit_x_min_mm = -100\nlimit_x_max_mm = 600\nlimit_y_min_mm = -100\nlimit_y_max_mm = 500\n",
    },
    Template {
        name: "production-4",
        about: "a faster machine with four nozzles, for boards with many parts",
        lines: "nozzles = 4\nspeed_mm_s = 300\nx_accel_mm_s2 = 4000\ny_accel_mm_s2 = 4000\npick_s = 0.25\nplace_s = 0.25\nrotation_deg_s = 1080\ntwo_opt = on\ntip_strategy = minimize\nlimit_x_min_mm = -200\nlimit_x_max_mm = 800\nlimit_y_min_mm = -200\nlimit_y_max_mm = 600\n",
    },
    Template {
        name: "simulation",
        about: "a machine that fails now and then (feeds, picks and places), to try how a job deals with it",
        lines: "nozzles = 2\nsim_seed = 1\nsim_feed_fail_rate = 0.01\nsim_pick_fail_rate = 0.03\nsim_place_fail_rate = 0.005\nsimulate_runs_count = 50\nerror_policy = alert\n",
    },
    Template {
        name: "server",
        about: "a machine that works through the EH3D server (give the token of this program there)",
        lines: "mode = server\nserver_address = 192.168.0.180:18200\nserver_machine = pnp-1\n# server_token = the token of this program on the server (keep it out of files that are shared)\nserver_timeout_s = 5\nserver_poll_ms = 500\nserver_attempts = 3\n",
    },
    Template {
        name: "full",
        about: "every key with its default and its range, as in examples/machine.cfg",
        lines: "",
    },
];

/// Every template.
pub fn all() -> &'static [Template] {
    &TEMPLATES
}

/// The template with this name (the case does not matter).
pub fn find(name: &str) -> Option<&'static Template> {
    TEMPLATES
        .iter()
        .find(|t| t.name.eq_ignore_ascii_case(name.trim()))
}

impl Template {
    /// The file: a comment that says what it is and then the lines. The lines of `full` are the example as it is.
    pub fn text(&self) -> String {
        if self.name == "full" {
            return FULL.to_string();
        }
        format!(
            "# {}: {}.\n# The numbers are starting points to tune on the machine; every key that is not here keeps its default (examples/machine.cfg lists them all).\n{}",
            self.name, self.about, self.lines
        )
    }
}

/// The list of templates for a person: a line for each.
pub fn list_text() -> String {
    let mut out = String::new();
    for t in all() {
        out.push_str(&format!("{}: {}\n", t.name, t.about));
    }
    out
}
