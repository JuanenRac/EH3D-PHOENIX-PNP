// EH3D-PHOENIX-PNP - the specification sheet of a machine, as its owner wrote it in the settings.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! A report of a job, or a comparison of two machines, says little without the machine it was made on: the table, the machine, the work area, the smallest part that was ever put down on
//! it and the greatest speed. The settings carry five lines for that (`spec_table`, `spec_machine`, `spec_work_area`, `spec_smallest_part` and `spec_max_speed_mm_s`). A line that is not
//! given is shown as "not measured" in the language of the reader: a number is never made up, and none is taken from the limits or the speeds that the plan uses, because those are
//! settings and not measurements. The sheet reads the settings and changes nothing.

use crate::config::MachineConfig;
use crate::error::Lang;
use crate::msg8::{num, pick, say};

/// The fields of the sheet, in the order they are shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Table,
    Machine,
    WorkArea,
    SmallestPart,
    MaxSpeed,
}

impl Field {
    pub const ALL: [Field; 5] = [
        Field::Table,
        Field::Machine,
        Field::WorkArea,
        Field::SmallestPart,
        Field::MaxSpeed,
    ];

    /// The name that the sheet uses in its JSON and its CSV, the same as the key of the settings without `spec_`.
    pub fn name(self) -> &'static str {
        match self {
            Field::Table => "table",
            Field::Machine => "machine",
            Field::WorkArea => "work_area",
            Field::SmallestPart => "smallest_part",
            Field::MaxSpeed => "max_speed_mm_s",
        }
    }

    fn label(self, lang: Lang) -> &'static str {
        let row: [&str; 8] = match self {
            Field::Table => [
                "Table",
                "Mesa",
                "Tisch",
                "Table",
                "Tavolo",
                "テーブル",
                "工作台",
                "Mesa",
            ],
            Field::Machine => [
                "Machine",
                "Máquina",
                "Maschine",
                "Machine",
                "Macchina",
                "マシン",
                "机器",
                "Máquina",
            ],
            Field::WorkArea => [
                "Work area",
                "Área de trabajo",
                "Arbeitsbereich",
                "Zone de travail",
                "Area di lavoro",
                "作業範囲",
                "工作区域",
                "Área de trabalho",
            ],
            Field::SmallestPart => [
                "Smallest part tried",
                "Pieza más pequeña probada",
                "Kleinstes erprobtes Teil",
                "Plus petite pièce essayée",
                "Parte più piccola provata",
                "試した最小の部品",
                "已试过的最小零件",
                "Peça mais pequena testada",
            ],
            Field::MaxSpeed => [
                "Greatest speed",
                "Velocidad máxima",
                "Höchstgeschwindigkeit",
                "Vitesse maximale",
                "Velocità massima",
                "最高速度",
                "最高速度",
                "Velocidade máxima",
            ],
        };
        pick(lang, &row)
    }
}

/// The word for a field that nobody measured.
pub fn not_measured(lang: Lang) -> &'static str {
    pick(
        lang,
        &[
            "not measured",
            "sin medir",
            "nicht gemessen",
            "non mesuré",
            "non misurato",
            "未測定",
            "未测量",
            "por medir",
        ],
    )
}

/// The title of the sheet.
pub fn title(lang: Lang) -> &'static str {
    pick(
        lang,
        &[
            "Machine",
            "Máquina",
            "Maschine",
            "Machine",
            "Macchina",
            "マシン",
            "机器",
            "Máquina",
        ],
    )
}

/// The value of each field as the settings give it; `None` where the settings leave it out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineSheet {
    pub values: Vec<(Field, Option<String>)>,
}

/// The sheet of a configuration. An empty text, and a speed of 0, are "not given".
pub fn from_config(cfg: &MachineConfig) -> MachineSheet {
    let text = |s: &str| (!s.trim().is_empty()).then(|| s.trim().to_string());
    let speed = (cfg.spec_max_speed_mm_s > 0.0).then(|| num(cfg.spec_max_speed_mm_s));
    MachineSheet {
        values: vec![
            (Field::Table, text(&cfg.spec_table)),
            (Field::Machine, text(&cfg.spec_machine)),
            (Field::WorkArea, text(&cfg.spec_work_area)),
            (Field::SmallestPart, text(&cfg.spec_smallest_part)),
            (Field::MaxSpeed, speed),
        ],
    }
}

impl MachineSheet {
    /// How many fields have a value.
    pub fn given(&self) -> usize {
        self.values.iter().filter(|(_, v)| v.is_some()).count()
    }

    /// The label and the value of each field in a language; a field that is not given has the words "not measured".
    pub fn rows(&self, lang: Lang) -> Vec<(String, String)> {
        self.values
            .iter()
            .map(|(f, v)| {
                let value = match (f, v) {
                    (_, None) => not_measured(lang).to_string(),
                    (Field::MaxSpeed, Some(v)) => format!("{v} mm/s"),
                    (_, Some(v)) => v.clone(),
                };
                (f.label(lang).to_string(), value)
            })
            .collect()
    }

    /// The sheet for a person: a line for each field and one that sums up how many were given.
    pub fn text(&self, lang: Lang) -> String {
        let mut out = String::new();
        for (label, value) in self.rows(lang) {
            out.push_str(&format!("{label}: {value}\n"));
        }
        let sum: [&str; 8] = [
            "{n} of {total} fields are given; the others are not measured and no figure is made up for them",
            "{n} de {total} campos están dados; los demás están sin medir y no se inventa ninguna cifra",
            "{n} von {total} Feldern sind angegeben; die übrigen sind nicht gemessen, und es wird keine Zahl erfunden",
            "{n} champs sur {total} sont donnés ; les autres ne sont pas mesurés et aucun chiffre n'est inventé",
            "{n} campi su {total} sono dati; gli altri non sono misurati e nessuna cifra è inventata",
            "{total} 項目のうち {n} 項目が設定されています。残りは未測定で、数値は作りません",
            "{total} 项中已给出 {n} 项；其余未测量，不会编造数值",
            "{n} de {total} campos estão dados; os restantes estão por medir e não se inventa nenhum valor",
        ];
        out.push_str(&say(
            lang,
            &sum,
            &[
                ("n", self.given().to_string()),
                ("total", self.values.len().to_string()),
            ],
        ));
        out.push('\n');
        out
    }

    /// The sheet as JSON: a field that is not given is `null`, and nothing else is written for it.
    pub fn json(&self) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        for (f, v) in &self.values {
            let value = match (f, v) {
                (Field::MaxSpeed, Some(v)) => v
                    .parse::<f64>()
                    .ok()
                    .and_then(serde_json::Number::from_f64)
                    .map_or(serde_json::Value::Null, serde_json::Value::Number),
                (_, Some(v)) => serde_json::Value::String(v.clone()),
                (_, None) => serde_json::Value::Null,
            };
            map.insert(f.name().to_string(), value);
        }
        serde_json::Value::Object(map)
    }
}
