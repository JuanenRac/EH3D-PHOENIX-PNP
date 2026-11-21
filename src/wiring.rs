// EH3D-PHOENIX-PNP - the wiring sheet: what is to be connected to which pin of the controller, and the pneumatic diagram, from a driver profile.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! A person who builds the machine needs to know what goes where: which axis is which letter, which pin switches the pump and which the blow-off valve, and how the air runs from the pump to
//! the nozzles. All of that is already in the driver profile that the program drives the controller with; this reads it and writes it down as a table (component, role, pin, the line that
//! switches it on and the one that switches it off, and whether the line that makes the machine safe lets it go) and as a drawing of the pneumatics. Nothing is connected or switched on
//! here, and the pins are the ones the profile names: whether they are the pins of a given board is for the person who wires it to check.
//!
//! The pin of an actuator is read from its lines: `P5` in `M42 P5 S255`, the name in `PIN=pump` of `SET_PIN PIN=pump VALUE=1`, or, for a line that has neither, its code (`M7`). An actuator
//! whose lines give none is a problem of the sheet, as is a pump or a blow-off valve the profile does not name.

use crate::gcodedriver::DriverProfile;
use crate::parts::csv_field;

/// One thing that is connected.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub component: String,
    /// `axis`, `pump`, `release` (the blow-off valve) or `valve`.
    pub role: &'static str,
    /// The pin, the name of the pin, or for an axis the letter of its driver.
    pub pin: String,
    pub on: String,
    pub off: String,
    /// Whether the lines that make the machine safe switch it off or let it go (`yes`, `no`; an axis has none).
    pub at_stop: &'static str,
}

/// The sheet of a profile.
#[derive(Debug, Clone, PartialEq)]
pub struct Sheet {
    pub dialect: &'static str,
    pub rows: Vec<Row>,
    pub nozzles: usize,
    /// What is wrong with the sheet, one line each.
    pub problems: Vec<String>,
}

/// The pin that a line switches, if it names one.
pub fn pin_of(line: &str) -> Option<String> {
    let words: Vec<&str> = line.split_whitespace().collect();
    for w in &words {
        if let Some(name) = w.strip_prefix("PIN=") {
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
    }
    for w in words.iter().skip(1) {
        let b = w.as_bytes();
        if b.len() >= 2 && b[0] == b'P' && b[1..].iter().all(u8::is_ascii_digit) {
            return Some((*w).to_string());
        }
    }
    let code = words.first()?;
    let b = code.as_bytes();
    (b.len() >= 2 && matches!(b[0], b'M' | b'G') && b[1..].iter().all(u8::is_ascii_digit))
        .then(|| (*code).to_string())
}

/// Builds the sheet from a profile.
pub fn build(profile: &DriverProfile) -> Sheet {
    let mut rows = Vec::new();
    let mut problems = Vec::new();
    let axes = [
        ("X", &profile.axis_x),
        ("Y", &profile.axis_y),
        ("Z", &profile.axis_z),
        ("rotation", &profile.axis_rot),
    ];
    for (name, letter) in axes {
        if letter.is_empty() {
            continue;
        }
        rows.push(Row {
            component: format!("axis {name}"),
            role: "axis",
            pin: format!("driver {letter}"),
            on: String::new(),
            off: String::new(),
            at_stop: "",
        });
    }
    for a in &profile.actuators {
        let role = if a.name == profile.pump {
            "pump"
        } else if a.name == profile.release {
            "release"
        } else {
            "valve"
        };
        let pin = pin_of(&a.on).or_else(|| pin_of(&a.off));
        if pin.is_none() {
            problems.push(format!("{}: no pin can be read from its lines", a.name));
        }
        let at_stop = if profile.safe_stop.iter().any(|l| l == &a.off) {
            "yes"
        } else {
            "no"
        };
        rows.push(Row {
            component: a.name.clone(),
            role,
            pin: pin.unwrap_or_default(),
            on: a.on.clone(),
            off: a.off.clone(),
            at_stop,
        });
    }
    if !rows.iter().any(|r| r.role == "pump") {
        problems.push("the profile names no pump among its actuators".to_string());
    }
    if !rows.iter().any(|r| r.role == "release") {
        problems.push("the profile names no blow-off valve among its actuators".to_string());
    }
    // two actuators on one pin are one thing wired twice
    for (i, a) in rows.iter().enumerate() {
        if a.role != "axis"
            && !a.pin.is_empty()
            && rows[..i].iter().any(|b| b.role != "axis" && b.pin == a.pin)
        {
            problems.push(format!(
                "{}: the pin {} is used by another actuator",
                a.component, a.pin
            ));
        }
    }
    Sheet {
        dialect: profile.dialect.name(),
        rows,
        nozzles: profile.nozzles,
        problems,
    }
}

impl Sheet {
    /// The table as a CSV.
    pub fn csv(&self) -> String {
        let mut out = String::from("component,role,pin,on,off,at_stop\n");
        for r in &self.rows {
            out.push_str(&format!(
                "{},{},{},{},{},{}\n",
                csv_field(&r.component),
                r.role,
                csv_field(&r.pin),
                csv_field(&r.on),
                csv_field(&r.off),
                r.at_stop
            ));
        }
        out
    }

    /// The table for a person.
    pub fn text(&self) -> String {
        let mut out = format!(
            "wiring sheet: {} controller, {} nozzle(s)\n",
            self.dialect, self.nozzles
        );
        out.push_str(&format!(
            "{:<14} {:<8} {:<14} {:<26} {:<26} {}\n",
            "component", "role", "pin", "on", "off", "off at stop"
        ));
        for r in &self.rows {
            out.push_str(&format!(
                "{:<14} {:<8} {:<14} {:<26} {:<26} {}\n",
                r.component, r.role, r.pin, r.on, r.off, r.at_stop
            ));
        }
        for p in &self.problems {
            out.push_str(&format!("problem: {p}\n"));
        }
        out.push_str(
            "evidence=simulator: read from the profile; nothing was connected or switched on\n",
        );
        out
    }

    /// The drawing of the pneumatics: the pump, the line, a nozzle for each nozzle of the profile and the blow-off valve, with the pin of each.
    pub fn svg(&self) -> String {
        let pin_of_role = |role: &str| {
            self.rows
                .iter()
                .find(|r| r.role == role)
                .map(|r| (r.component.clone(), r.pin.clone()))
        };
        let (pump, pump_pin) = pin_of_role("pump").unwrap_or(("pump".into(), String::new()));
        let (blow, blow_pin) = pin_of_role("release").unwrap_or(("blow-off".into(), String::new()));
        let n = self.nozzles.clamp(1, 6);
        let height = 150 + 70 * n;
        let mut s = String::new();
        s.push_str(&format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 640 {height}\" width=\"640\" height=\"{height}\" font-family=\"sans-serif\" font-size=\"13\">\n"
        ));
        s.push_str("<style>.b{fill:#f4f6f8;stroke:#34495e;stroke-width:1.5}.l{stroke:#2e86c1;stroke-width:3;fill:none}.k{stroke:#c0392b;stroke-width:3;fill:none;stroke-dasharray:6 4}.t{fill:#1b2631}.d{fill:#566573;font-size:11px}</style>\n");
        s.push_str(&format!(
            "<text class=\"t\" x=\"20\" y=\"26\" font-size=\"16\" font-weight=\"bold\">Pneumatics: {} controller</text>\n",
            xml(self.dialect)
        ));
        // the pump, the blow-off valve and the line between them and the nozzles
        s.push_str(&format!(
            "<rect class=\"b\" x=\"20\" y=\"60\" width=\"150\" height=\"54\" rx=\"6\"/><text class=\"t\" x=\"32\" y=\"84\">{}</text><text class=\"d\" x=\"32\" y=\"102\">pin {}</text>\n",
            xml(&pump),
            xml(&pump_pin)
        ));
        s.push_str(&format!(
            "<rect class=\"b\" x=\"20\" y=\"{}\" width=\"150\" height=\"54\" rx=\"6\"/><text class=\"t\" x=\"32\" y=\"{}\">{}</text><text class=\"d\" x=\"32\" y=\"{}\">pin {}</text>\n",
            100 + 70 * n,
            124 + 70 * n,
            xml(&blow),
            142 + 70 * n,
            xml(&blow_pin)
        ));
        let line_x = 260;
        s.push_str(&format!(
            "<path class=\"l\" d=\"M170 87 H{line_x} V{}\"/>\n",
            85 + 70 * (n - 1)
        ));
        s.push_str(&format!(
            "<path class=\"k\" d=\"M170 {} H{line_x} V{}\"/>\n",
            127 + 70 * n,
            85 + 70 * (n - 1)
        ));
        s.push_str(&format!(
            "<text class=\"d\" x=\"180\" y=\"80\">vacuum</text><text class=\"d\" x=\"180\" y=\"{}\">blow-off</text>\n",
            120 + 70 * n
        ));
        for i in 0..n {
            let y = 62 + 70 * i;
            s.push_str(&format!(
                "<path class=\"l\" d=\"M{line_x} {} H400\"/><rect class=\"b\" x=\"400\" y=\"{y}\" width=\"150\" height=\"46\" rx=\"6\"/><text class=\"t\" x=\"412\" y=\"{}\">nozzle {}</text>\n",
                y + 23,
                y + 28,
                i + 1
            ));
        }
        s.push_str("</svg>\n");
        s
    }
}

/// A text for an XML document.
fn xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
