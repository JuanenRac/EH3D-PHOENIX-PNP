// EH3D-PHOENIX-PNP - a read-only import of the configuration files of OpenPnP.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! A person who comes from another program brings its files: the packages, the parts and the machine with its feeders. This reads them (and never writes to them) into the tables of this
//! program: a table of packages (`package,height_mm,body_long_mm,body_short_mm`), a table of parts (`value,package,height_mm,rank`) and a table of feeders (`id,value,package,x,y,count`),
//! with the units brought to millimetres. Whatever it cannot map is listed with its file and line and a name, never dropped without a word; a row it cannot use is an error and is left out;
//! the readers are the bounded ones of `xmlmini`, so a file made to hurt them is refused. The attribute names are those of the public description of the format, tried on files written for
//! the tests; a real file may hold more, and the unmapped parts are told.

use crate::error::Lang;
use crate::msg8::{num, say};
use crate::parts::csv_field;
use crate::xmlmini::{self, Element, XmlError};

/// Whether a finding stops the row or only tells something.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Warning,
    Error,
}

/// What a finding is about.
#[derive(Debug, Clone, PartialEq)]
pub enum Issue {
    Xml(XmlError),
    WrongRoot(String, String),
    UnknownUnit(String),
    NotANumber(String, String),
    Missing(String, String),
    DuplicateId(String),
    UnknownPackage(String, String),
    UnknownPart(String, String),
    NoLocation(String),
    NoCount(String),
    NoPackageKnown(String),
    Disabled(String),
    Unmapped(String, usize),
}

/// A finding with the file it is in (`packages`, `parts` or `machine`) and the line.
#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    pub file: &'static str,
    pub line: usize,
    pub severity: Severity,
    pub issue: Issue,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PackageRow {
    pub id: String,
    pub body_long_mm: Option<f64>,
    pub body_short_mm: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PartRow {
    pub id: String,
    pub package: String,
    pub height_mm: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FeederRow {
    pub id: String,
    pub part: String,
    pub package: String,
    pub x: f64,
    pub y: f64,
    pub count: u32,
}

/// What was read.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Imported {
    pub packages: Vec<PackageRow>,
    pub parts: Vec<PartRow>,
    pub feeders: Vec<FeederRow>,
    pub findings: Vec<Finding>,
}

/// The millimetres in one unit of OpenPnP's lengths; `None` for a unit that is not known.
pub fn unit_factor(name: &str) -> Option<f64> {
    match name.trim().to_ascii_lowercase().as_str() {
        "millimeters" | "millimetres" | "millimeter" | "millimetre" | "mm" => Some(1.0),
        "centimeters" | "centimetres" | "centimeter" | "centimetre" | "cm" => Some(10.0),
        "meters" | "metres" | "meter" | "metre" | "m" => Some(1000.0),
        "inches" | "inch" | "in" => Some(25.4),
        "feet" | "foot" | "ft" => Some(304.8),
        "mils" | "mil" | "thou" => Some(0.0254),
        "microns" | "micrometers" | "micrometres" | "um" => Some(0.001),
        _ => None,
    }
}

impl Imported {
    fn note(&mut self, file: &'static str, line: usize, severity: Severity, issue: Issue) {
        self.findings.push(Finding {
            file,
            line,
            severity,
            issue,
        });
    }

    /// How many findings stop a row.
    pub fn errors(&self) -> usize {
        self.findings
            .iter()
            .filter(|f| f.severity == Severity::Error)
            .count()
    }

    /// A length from an attribute, in millimetres. `Ok(None)` when the attribute is not there.
    fn length(
        &mut self,
        file: &'static str,
        e: &Element,
        attr: &str,
        unit: &str,
    ) -> Result<Option<f64>, ()> {
        let Some(raw) = e.attr(attr) else {
            return Ok(None);
        };
        let Some(f) = unit_factor(unit) else {
            self.note(
                file,
                e.line,
                Severity::Error,
                Issue::UnknownUnit(unit.to_string()),
            );
            return Err(());
        };
        match raw.trim().parse::<f64>() {
            Ok(v) if v.is_finite() && v.abs() < 1.0e7 => Ok(Some(v * f)),
            _ => {
                self.note(
                    file,
                    e.line,
                    Severity::Error,
                    Issue::NotANumber(attr.to_string(), raw.to_string()),
                );
                Err(())
            }
        }
    }

    fn open(&mut self, file: &'static str, bytes: &[u8], root_name: &str) -> Option<Element> {
        match xmlmini::parse(bytes) {
            Err(e) => {
                self.note(file, e.line, Severity::Error, Issue::Xml(e));
                None
            }
            Ok(root) if root.name != root_name => {
                self.note(
                    file,
                    root.line,
                    Severity::Error,
                    Issue::WrongRoot(root_name.to_string(), root.name),
                );
                None
            }
            Ok(root) => Some(root),
        }
    }

    /// Reads the file of packages: each `package` with the body of its footprint.
    pub fn read_packages(&mut self, bytes: &[u8]) {
        let Some(root) = self.open("packages", bytes, "openpnp-packages") else {
            return;
        };
        for p in root.children_named("package") {
            let Some(id) = p.attr("id").map(str::to_string).filter(|s| !s.is_empty()) else {
                self.note(
                    "packages",
                    p.line,
                    Severity::Error,
                    Issue::Missing("package".into(), "id".into()),
                );
                continue;
            };
            if self.packages.iter().any(|x| x.id == id) {
                self.note(
                    "packages",
                    p.line,
                    Severity::Warning,
                    Issue::DuplicateId(id),
                );
                continue;
            }
            let (mut long, mut short) = (None, None);
            if let Some(fp) = p.child("footprint") {
                let unit = fp.attr("units").unwrap_or("Millimeters").to_string();
                let w = self.length("packages", fp, "body-width", &unit);
                let h = self.length("packages", fp, "body-height", &unit);
                if let (Ok(Some(w)), Ok(Some(h))) = (w, h) {
                    if w > 0.0 && h > 0.0 {
                        long = Some(w.max(h));
                        short = Some(w.min(h));
                    }
                }
            }
            self.packages.push(PackageRow {
                id,
                body_long_mm: long,
                body_short_mm: short,
            });
        }
    }

    /// Reads the file of parts: each `part` with its package and height.
    pub fn read_parts(&mut self, bytes: &[u8]) {
        let Some(root) = self.open("parts", bytes, "openpnp-parts") else {
            return;
        };
        for p in root.children_named("part") {
            let Some(id) = p.attr("id").map(str::to_string).filter(|s| !s.is_empty()) else {
                self.note(
                    "parts",
                    p.line,
                    Severity::Error,
                    Issue::Missing("part".into(), "id".into()),
                );
                continue;
            };
            if self.parts.iter().any(|x| x.id == id) {
                self.note("parts", p.line, Severity::Warning, Issue::DuplicateId(id));
                continue;
            }
            let Some(package) = p
                .attr("package-id")
                .map(str::to_string)
                .filter(|s| !s.is_empty())
            else {
                self.note(
                    "parts",
                    p.line,
                    Severity::Error,
                    Issue::Missing(id, "package-id".into()),
                );
                continue;
            };
            let unit = p.attr("height-units").unwrap_or("Millimeters").to_string();
            let Ok(height) = self.length("parts", p, "height", &unit) else {
                continue;
            };
            self.parts.push(PartRow {
                id,
                package,
                height_mm: height.filter(|h| *h > 0.0),
            });
        }
    }

    fn collect_feeders<'a>(
        e: &'a Element,
        out: &mut Vec<&'a Element>,
        sections: &mut Vec<(String, usize, usize)>,
    ) {
        for c in &e.children {
            match c.name.as_str() {
                "feeders" => out.extend(c.children_named("feeder")),
                "heads" | "cameras" | "actuators" | "nozzle-tips" | "nozzles" | "axes"
                | "drivers" => {
                    sections.push((c.name.clone(), c.children.len(), c.line));
                }
                _ => Self::collect_feeders(c, out, sections),
            }
        }
    }

    /// Reads the file of the machine: the feeders with the place each is picked from; the heads, cameras, actuators and the rest are counted and told as not mapped.
    pub fn read_machine(&mut self, bytes: &[u8]) {
        let Some(root) = self.open("machine", bytes, "openpnp-machine") else {
            return;
        };
        let mut feeders: Vec<&Element> = Vec::new();
        let mut sections: Vec<(String, usize, usize)> = Vec::new();
        Self::collect_feeders(&root, &mut feeders, &mut sections);
        for (name, n, line) in sections {
            if n > 0 {
                self.note("machine", line, Severity::Warning, Issue::Unmapped(name, n));
            }
        }
        for (i, f) in feeders.iter().enumerate() {
            let id = f
                .attr("name")
                .or_else(|| f.attr("id"))
                .filter(|s| !s.is_empty())
                .map_or_else(|| format!("F{}", i + 1), str::to_string);
            if f.attr("enabled")
                .is_some_and(|v| v.eq_ignore_ascii_case("false"))
            {
                self.note("machine", f.line, Severity::Warning, Issue::Disabled(id));
                continue;
            }
            if self.feeders.iter().any(|x| x.id == id) {
                self.note("machine", f.line, Severity::Warning, Issue::DuplicateId(id));
                continue;
            }
            let Some(part) = f
                .attr("part-id")
                .map(str::to_string)
                .filter(|s| !s.is_empty())
            else {
                self.note(
                    "machine",
                    f.line,
                    Severity::Error,
                    Issue::Missing(id, "part-id".into()),
                );
                continue;
            };
            let Some(loc) = f.child("location").or_else(|| f.child("pick-location")) else {
                self.note("machine", f.line, Severity::Error, Issue::NoLocation(id));
                continue;
            };
            let unit = loc.attr("units").unwrap_or("Millimeters").to_string();
            let (Ok(x), Ok(y)) = (
                self.length("machine", loc, "x", &unit),
                self.length("machine", loc, "y", &unit),
            ) else {
                continue;
            };
            let (Some(x), Some(y)) = (x, y) else {
                self.note("machine", loc.line, Severity::Error, Issue::NoLocation(id));
                continue;
            };
            let count_attr = ["part-count", "count", "tray-count"]
                .iter()
                .find_map(|a| f.attr(a).map(|v| (*a, v)));
            let count = match count_attr {
                Some((a, v)) => match v.trim().parse::<u32>() {
                    Ok(n) if n <= 1_000_000 => n,
                    _ => {
                        self.note(
                            "machine",
                            f.line,
                            Severity::Error,
                            Issue::NotANumber(a.to_string(), v.to_string()),
                        );
                        continue;
                    }
                },
                None => {
                    self.note(
                        "machine",
                        f.line,
                        Severity::Warning,
                        Issue::NoCount(id.clone()),
                    );
                    0
                }
            };
            self.feeders.push(FeederRow {
                id,
                part,
                package: String::new(),
                x,
                y,
                count,
            });
        }
    }

    /// Joins the three files: a part of a package that is not in the file of packages, and a feeder of a part that is not in the file of parts, are told; a feeder takes its package from its part.
    pub fn link(&mut self) {
        let parts = std::mem::take(&mut self.parts);
        let packages_known = !self.packages.is_empty();
        for p in &parts {
            if packages_known && !self.packages.iter().any(|k| k.id == p.package) {
                self.findings.push(Finding {
                    file: "parts",
                    line: 0,
                    severity: Severity::Warning,
                    issue: Issue::UnknownPackage(p.id.clone(), p.package.clone()),
                });
            }
        }
        let feeders = std::mem::take(&mut self.feeders);
        let mut kept = Vec::new();
        for mut f in feeders {
            match parts.iter().find(|p| p.id == f.part) {
                Some(p) => {
                    f.package = p.package.clone();
                    kept.push(f);
                }
                None if parts.is_empty() => self.findings.push(Finding {
                    file: "machine",
                    line: 0,
                    severity: Severity::Warning,
                    issue: Issue::NoPackageKnown(f.id.clone()),
                }),
                None => self.findings.push(Finding {
                    file: "machine",
                    line: 0,
                    severity: Severity::Warning,
                    issue: Issue::UnknownPart(f.id.clone(), f.part.clone()),
                }),
            }
        }
        self.parts = parts;
        self.feeders = kept;
    }

    /// The table of packages.
    pub fn packages_csv(&self) -> String {
        let mut out = String::from("package,height_mm,body_long_mm,body_short_mm\n");
        for p in &self.packages {
            let cell = |v: Option<f64>| v.map_or(String::new(), |v| num(round4(v)));
            out.push_str(&format!(
                "{},,{},{}\n",
                csv_field(&p.id),
                cell(p.body_long_mm),
                cell(p.body_short_mm)
            ));
        }
        out
    }

    /// The table of parts.
    pub fn parts_csv(&self) -> String {
        let mut out = String::from("value,package,height_mm,rank\n");
        for p in &self.parts {
            out.push_str(&format!(
                "{},{},{},\n",
                csv_field(&p.id),
                csv_field(&p.package),
                p.height_mm.map_or(String::new(), |v| num(round4(v)))
            ));
        }
        out
    }

    /// The table of feeders.
    pub fn feeders_csv(&self) -> String {
        let mut out = String::from("id,value,package,x,y,count\n");
        for f in &self.feeders {
            out.push_str(&format!(
                "{},{},{},{},{},{}\n",
                csv_field(&f.id),
                csv_field(&f.part),
                csv_field(&f.package),
                num(round4(f.x)),
                num(round4(f.y)),
                f.count
            ));
        }
        out
    }
}

fn round4(v: f64) -> f64 {
    (v * 10000.0).round() / 10000.0
}

/// A finding in a language.
pub fn message(f: &Finding, lang: Lang) -> String {
    let (a, b) = match &f.issue {
        Issue::WrongRoot(x, y)
        | Issue::NotANumber(x, y)
        | Issue::Missing(x, y)
        | Issue::UnknownPackage(x, y)
        | Issue::UnknownPart(x, y) => (x.clone(), y.clone()),
        Issue::UnknownUnit(x)
        | Issue::DuplicateId(x)
        | Issue::NoLocation(x)
        | Issue::NoCount(x)
        | Issue::NoPackageKnown(x)
        | Issue::Disabled(x) => (x.clone(), String::new()),
        Issue::Unmapped(x, n) => (x.clone(), n.to_string()),
        Issue::Xml(e) => return format!("{}: {}", f.file, e.localized(lang)),
    };
    let row: [&str; 8] = match &f.issue {
        Issue::Xml(_) => [
            "{file}", "{file}", "{file}", "{file}", "{file}", "{file}", "{file}", "{file}",
        ],
        Issue::WrongRoot(..) => [
            "{file} line {line}: the file should start with '{a}' and starts with '{b}'",
            "{file} línea {line}: el archivo debería empezar con '{a}' y empieza con '{b}'",
            "{file} Zeile {line}: die Datei sollte mit '{a}' beginnen und beginnt mit '{b}'",
            "{file} ligne {line} : le fichier devrait commencer par '{a}' et commence par '{b}'",
            "{file} riga {line}: il file dovrebbe iniziare con '{a}' e inizia con '{b}'",
            "{file} {line} 行目：ファイルは「{a}」で始まるはずですが「{b}」で始まっています",
            "{file} 第 {line} 行：文件应以“{a}”开头，实际以“{b}”开头",
            "{file} linha {line}: o ficheiro devia começar por '{a}' e começa por '{b}'",
        ],
        Issue::UnknownUnit(_) => [
            "{file} line {line}: the unit '{a}' is not known; the row is left out",
            "{file} línea {line}: la unidad '{a}' no se conoce; se deja fuera la fila",
            "{file} Zeile {line}: die Einheit '{a}' ist unbekannt; die Zeile bleibt weg",
            "{file} ligne {line} : l'unité '{a}' est inconnue ; la ligne est laissée de côté",
            "{file} riga {line}: l'unità '{a}' non è nota; la riga viene esclusa",
            "{file} {line} 行目：単位「{a}」は不明です。この行は除外します",
            "{file} 第 {line} 行：单位“{a}”未知，该行被略过",
            "{file} linha {line}: a unidade '{a}' não é conhecida; a linha fica de fora",
        ],
        Issue::NotANumber(..) => [
            "{file} line {line}: '{a}' is '{b}', which is not a number; the row is left out",
            "{file} línea {line}: '{a}' vale '{b}', que no es un número; se deja fuera la fila",
            "{file} Zeile {line}: '{a}' ist '{b}', keine Zahl; die Zeile bleibt weg",
            "{file} ligne {line} : '{a}' vaut '{b}', ce qui n'est pas un nombre ; la ligne est laissée de côté",
            "{file} riga {line}: '{a}' vale '{b}', che non è un numero; la riga viene esclusa",
            "{file} {line} 行目：「{a}」は「{b}」で数値ではありません。この行は除外します",
            "{file} 第 {line} 行：“{a}”为“{b}”，不是数字，该行被略过",
            "{file} linha {line}: '{a}' vale '{b}', que não é um número; a linha fica de fora",
        ],
        Issue::Missing(..) => [
            "{file} line {line}: '{a}' has no '{b}'; the row is left out",
            "{file} línea {line}: '{a}' no tiene '{b}'; se deja fuera la fila",
            "{file} Zeile {line}: '{a}' hat kein '{b}'; die Zeile bleibt weg",
            "{file} ligne {line} : '{a}' n'a pas de '{b}' ; la ligne est laissée de côté",
            "{file} riga {line}: '{a}' non ha '{b}'; la riga viene esclusa",
            "{file} {line} 行目：「{a}」に「{b}」がありません。この行は除外します",
            "{file} 第 {line} 行：“{a}”没有“{b}”，该行被略过",
            "{file} linha {line}: '{a}' não tem '{b}'; a linha fica de fora",
        ],
        Issue::DuplicateId(_) => [
            "{file} line {line}: '{a}' is there twice; the second is left out",
            "{file} línea {line}: '{a}' está dos veces; se deja fuera la segunda",
            "{file} Zeile {line}: '{a}' kommt zweimal vor; das zweite bleibt weg",
            "{file} ligne {line} : '{a}' est là deux fois ; la seconde est laissée de côté",
            "{file} riga {line}: '{a}' compare due volte; la seconda viene esclusa",
            "{file} {line} 行目：「{a}」が二重にあります。2 つ目は除外します",
            "{file} 第 {line} 行：“{a}”出现两次，第二个被略过",
            "{file} linha {line}: '{a}' está duas vezes; a segunda fica de fora",
        ],
        Issue::UnknownPackage(..) => [
            "{file}: the part '{a}' names the package '{b}', which the file of packages does not have",
            "{file}: la pieza '{a}' nombra el paquete '{b}', que el archivo de paquetes no tiene",
            "{file}: das Teil '{a}' nennt das Gehäuse '{b}', das die Gehäusedatei nicht hat",
            "{file} : la pièce '{a}' nomme le boîtier '{b}', que le fichier des boîtiers n'a pas",
            "{file}: il componente '{a}' nomina il package '{b}', che il file dei package non ha",
            "{file}：部品「{a}」のパッケージ「{b}」はパッケージのファイルにありません",
            "{file}：元件“{a}”所用封装“{b}”不在封装文件中",
            "{file}: a peça '{a}' nomeia o encapsulamento '{b}', que o ficheiro de encapsulamentos não tem",
        ],
        Issue::UnknownPart(..) => [
            "{file}: the feeder '{a}' holds the part '{b}', which the file of parts does not have; the feeder is left out",
            "{file}: el alimentador '{a}' lleva la pieza '{b}', que el archivo de piezas no tiene; se deja fuera",
            "{file}: der Zuführer '{a}' hält das Teil '{b}', das die Teiledatei nicht hat; er bleibt weg",
            "{file} : l'alimentateur '{a}' porte la pièce '{b}', que le fichier des pièces n'a pas ; il est laissé de côté",
            "{file}: l'alimentatore '{a}' porta il componente '{b}', che il file dei componenti non ha; viene escluso",
            "{file}：フィーダ「{a}」の部品「{b}」は部品ファイルにありません。除外します",
            "{file}：供料器“{a}”的元件“{b}”不在元件文件中，已略过",
            "{file}: o alimentador '{a}' leva a peça '{b}', que o ficheiro de peças não tem; fica de fora",
        ],
        Issue::NoLocation(_) => [
            "{file} line {line}: the feeder '{a}' has no place to pick from; it is left out",
            "{file} línea {line}: el alimentador '{a}' no tiene lugar de recogida; se deja fuera",
            "{file} Zeile {line}: der Zuführer '{a}' hat keinen Aufnahmeort; er bleibt weg",
            "{file} ligne {line} : l'alimentateur '{a}' n'a pas d'endroit de prise ; il est laissé de côté",
            "{file} riga {line}: l'alimentatore '{a}' non ha un punto di prelievo; viene escluso",
            "{file} {line} 行目：フィーダ「{a}」にピック位置がありません。除外します",
            "{file} 第 {line} 行：供料器“{a}”没有取料位置，已略过",
            "{file} linha {line}: o alimentador '{a}' não tem local de recolha; fica de fora",
        ],
        Issue::NoCount(_) => [
            "{file} line {line}: the feeder '{a}' has no count of parts; it is taken as 0",
            "{file} línea {line}: el alimentador '{a}' no tiene cuenta de piezas; se toma 0",
            "{file} Zeile {line}: der Zuführer '{a}' hat keine Stückzahl; es gilt 0",
            "{file} ligne {line} : l'alimentateur '{a}' n'a pas de compte de pièces ; on prend 0",
            "{file} riga {line}: l'alimentatore '{a}' non ha un conteggio dei pezzi; si prende 0",
            "{file} {line} 行目：フィーダ「{a}」に部品数がありません。0 とみなします",
            "{file} 第 {line} 行：供料器“{a}”没有元件数量，按 0 处理",
            "{file} linha {line}: o alimentador '{a}' não tem contagem de peças; toma-se 0",
        ],
        Issue::NoPackageKnown(_) => [
            "{file}: the package of the feeder '{a}' is not known without the file of parts; the feeder is left out",
            "{file}: sin el archivo de piezas no se conoce el paquete del alimentador '{a}'; se deja fuera",
            "{file}: ohne die Teiledatei ist das Gehäuse des Zuführers '{a}' unbekannt; er bleibt weg",
            "{file} : sans le fichier des pièces, le boîtier de l'alimentateur '{a}' est inconnu ; il est laissé de côté",
            "{file}: senza il file dei componenti il package dell'alimentatore '{a}' non è noto; viene escluso",
            "{file}：部品ファイルがないためフィーダ「{a}」のパッケージが分かりません。除外します",
            "{file}：没有元件文件，无法确定供料器“{a}”的封装，已略过",
            "{file}: sem o ficheiro de peças não se conhece o encapsulamento do alimentador '{a}'; fica de fora",
        ],
        Issue::Disabled(_) => [
            "{file} line {line}: the feeder '{a}' is switched off in the file; it is left out",
            "{file} línea {line}: el alimentador '{a}' está apagado en el archivo; se deja fuera",
            "{file} Zeile {line}: der Zuführer '{a}' ist in der Datei abgeschaltet; er bleibt weg",
            "{file} ligne {line} : l'alimentateur '{a}' est désactivé dans le fichier ; il est laissé de côté",
            "{file} riga {line}: l'alimentatore '{a}' è disattivato nel file; viene escluso",
            "{file} {line} 行目：フィーダ「{a}」はファイルで無効になっています。除外します",
            "{file} 第 {line} 行：供料器“{a}”在文件中已关闭，已略过",
            "{file} linha {line}: o alimentador '{a}' está desligado no ficheiro; fica de fora",
        ],
        Issue::Unmapped(..) => [
            "{file} line {line}: {b} entries of '{a}' are not mapped (this program has no table for them)",
            "{file} línea {line}: {b} entradas de '{a}' no se traducen (este programa no tiene tabla para ellas)",
            "{file} Zeile {line}: {b} Einträge von '{a}' werden nicht übernommen (dafür gibt es keine Tabelle)",
            "{file} ligne {line} : {b} entrées de '{a}' ne sont pas reprises (ce programme n'a pas de table pour elles)",
            "{file} riga {line}: {b} voci di '{a}' non vengono riportate (questo programma non ha una tabella per esse)",
            "{file} {line} 行目：「{a}」の {b} 件は取り込まれません（対応する表がありません）",
            "{file} 第 {line} 行：“{a}”的 {b} 项未映射（本程序没有对应的表）",
            "{file} linha {line}: {b} entradas de '{a}' não são convertidas (este programa não tem tabela para elas)",
        ],
    };
    say(
        lang,
        &row,
        &[
            ("file", f.file.to_string()),
            ("line", f.line.to_string()),
            ("a", a),
            ("b", b),
        ],
    )
}
