// EH3D-PHOENIX-PNP - a bounded reader of XML for the configuration files of other programs.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! The files that other pick and place programs keep their machine in are XML. This reads the shape of such a file (elements, their attributes, what is inside them) and nothing else, and it
//! is made for files that somebody else wrote: it is limited in size, in depth, in the number of elements and attributes and in the length of a name and of a value; a document type
//! with entities is refused (so there is no expansion to blow up); only the five entities of XML and the numeric ones are read; it never panics on any bytes. The text between elements is
//! not kept: the files read here hold their data in attributes.

use crate::error::Lang;
use crate::msg8::say;

/// The largest document (bytes).
pub const MAX_BYTES: usize = 8 * 1024 * 1024;
/// The deepest nesting of elements.
pub const MAX_DEPTH: usize = 32;
/// The most elements.
pub const MAX_ELEMENTS: usize = 200_000;
/// The most attributes of one element.
pub const MAX_ATTRIBUTES: usize = 64;
/// The longest name of an element or an attribute (letters).
pub const MAX_NAME: usize = 128;
/// The longest value of an attribute (letters).
pub const MAX_VALUE: usize = 4096;

/// An element with its attributes and the elements inside it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Element {
    pub name: String,
    pub attributes: Vec<(String, String)>,
    pub children: Vec<Element>,
    /// The line where the element opens.
    pub line: usize,
}

impl Element {
    /// The value of an attribute.
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    /// The first child with the name.
    pub fn child(&self, name: &str) -> Option<&Element> {
        self.children.iter().find(|c| c.name == name)
    }

    /// The children with the name.
    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> {
        self.children.iter().filter(move |c| c.name == name)
    }

    /// The number of elements in this one and under it.
    pub fn count(&self) -> usize {
        1 + self.children.iter().map(Element::count).sum::<usize>()
    }
}

/// What is wrong in a document.
#[derive(Debug, Clone, PartialEq)]
pub enum Problem {
    TooBig,
    NotText,
    Empty,
    Doctype,
    Unclosed(String),
    Mismatch(String, String),
    BadMarkup,
    TooDeep,
    TooMany,
    TooManyAttributes,
    NameTooLong,
    ValueTooLong,
    BadEntity,
    SeveralRoots,
}

/// An error with its line.
#[derive(Debug, Clone, PartialEq)]
pub struct XmlError {
    pub line: usize,
    pub problem: Problem,
}

impl XmlError {
    /// The error in a language.
    pub fn localized(&self, lang: Lang) -> String {
        let (a, b) = match &self.problem {
            Problem::Unclosed(n) => (n.clone(), String::new()),
            Problem::Mismatch(open, close) => (open.clone(), close.clone()),
            _ => (String::new(), String::new()),
        };
        let row: [&str; 8] = match &self.problem {
            Problem::TooBig => [
                "the document is over 8 MiB",
                "el documento pasa de 8 MiB",
                "das Dokument ist größer als 8 MiB",
                "le document dépasse 8 Mio",
                "il documento supera 8 MiB",
                "ドキュメントが 8 MiB を超えています",
                "文档超过 8 MiB",
                "o documento passa de 8 MiB",
            ],
            Problem::NotText => [
                "line {line}: the document is not text",
                "línea {line}: el documento no es texto",
                "Zeile {line}: das Dokument ist kein Text",
                "ligne {line} : le document n'est pas du texte",
                "riga {line}: il documento non è testo",
                "{line} 行目：ドキュメントがテキストではありません",
                "第 {line} 行：文档不是文本",
                "linha {line}: o documento não é texto",
            ],
            Problem::Empty => [
                "the document has no element",
                "el documento no tiene ningún elemento",
                "das Dokument hat kein Element",
                "le document n'a aucun élément",
                "il documento non ha elementi",
                "ドキュメントに要素がありません",
                "文档没有任何元素",
                "o documento não tem nenhum elemento",
            ],
            Problem::Doctype => [
                "line {line}: a document type with entities is not read",
                "línea {line}: no se lee un tipo de documento con entidades",
                "Zeile {line}: ein Dokumenttyp mit Entitäten wird nicht gelesen",
                "ligne {line} : un type de document avec des entités n'est pas lu",
                "riga {line}: un tipo di documento con entità non viene letto",
                "{line} 行目：エンティティを含む文書型は読み込みません",
                "第 {line} 行：不读取带实体的文档类型",
                "linha {line}: não se lê um tipo de documento com entidades",
            ],
            Problem::Unclosed(_) => [
                "line {line}: the element '{a}' is not closed",
                "línea {line}: el elemento '{a}' no está cerrado",
                "Zeile {line}: das Element '{a}' ist nicht geschlossen",
                "ligne {line} : l'élément '{a}' n'est pas fermé",
                "riga {line}: l'elemento '{a}' non è chiuso",
                "{line} 行目：要素「{a}」が閉じられていません",
                "第 {line} 行：元素“{a}”没有闭合",
                "linha {line}: o elemento '{a}' não está fechado",
            ],
            Problem::Mismatch(..) => [
                "line {line}: '{a}' is closed by '{b}'",
                "línea {line}: '{a}' se cierra con '{b}'",
                "Zeile {line}: '{a}' wird mit '{b}' geschlossen",
                "ligne {line} : '{a}' est fermé par '{b}'",
                "riga {line}: '{a}' è chiuso da '{b}'",
                "{line} 行目：「{a}」が「{b}」で閉じられています",
                "第 {line} 行：“{a}”被“{b}”闭合",
                "linha {line}: '{a}' é fechado por '{b}'",
            ],
            Problem::BadMarkup => [
                "line {line}: the markup is not valid",
                "línea {line}: el marcado no es válido",
                "Zeile {line}: das Markup ist ungültig",
                "ligne {line} : le balisage n'est pas valide",
                "riga {line}: il markup non è valido",
                "{line} 行目：マークアップが正しくありません",
                "第 {line} 行：标记无效",
                "linha {line}: a marcação não é válida",
            ],
            Problem::TooDeep => [
                "line {line}: elements are nested over 32 deep",
                "línea {line}: los elementos se anidan a más de 32 niveles",
                "Zeile {line}: Elemente sind tiefer als 32 Ebenen verschachtelt",
                "ligne {line} : les éléments sont imbriqués sur plus de 32 niveaux",
                "riga {line}: gli elementi sono annidati oltre 32 livelli",
                "{line} 行目：要素の入れ子が 32 段を超えています",
                "第 {line} 行：元素嵌套超过 32 层",
                "linha {line}: os elementos estão aninhados em mais de 32 níveis",
            ],
            Problem::TooMany => [
                "line {line}: the document has over 200000 elements",
                "línea {line}: el documento tiene más de 200000 elementos",
                "Zeile {line}: das Dokument hat über 200000 Elemente",
                "ligne {line} : le document a plus de 200000 éléments",
                "riga {line}: il documento ha più di 200000 elementi",
                "{line} 行目：要素が 200000 個を超えています",
                "第 {line} 行：文档元素超过 200000 个",
                "linha {line}: o documento tem mais de 200000 elementos",
            ],
            Problem::TooManyAttributes => [
                "line {line}: an element has over 64 attributes",
                "línea {line}: un elemento tiene más de 64 atributos",
                "Zeile {line}: ein Element hat über 64 Attribute",
                "ligne {line} : un élément a plus de 64 attributs",
                "riga {line}: un elemento ha più di 64 attributi",
                "{line} 行目：属性が 64 個を超える要素があります",
                "第 {line} 行：有元素的属性超过 64 个",
                "linha {line}: um elemento tem mais de 64 atributos",
            ],
            Problem::NameTooLong => [
                "line {line}: a name is over 128 letters",
                "línea {line}: un nombre pasa de 128 letras",
                "Zeile {line}: ein Name hat über 128 Zeichen",
                "ligne {line} : un nom dépasse 128 caractères",
                "riga {line}: un nome supera 128 caratteri",
                "{line} 行目：名前が 128 文字を超えています",
                "第 {line} 行：名称超过 128 个字符",
                "linha {line}: um nome passa de 128 letras",
            ],
            Problem::ValueTooLong => [
                "line {line}: a value is over 4096 letters",
                "línea {line}: un valor pasa de 4096 letras",
                "Zeile {line}: ein Wert hat über 4096 Zeichen",
                "ligne {line} : une valeur dépasse 4096 caractères",
                "riga {line}: un valore supera 4096 caratteri",
                "{line} 行目：値が 4096 文字を超えています",
                "第 {line} 行：值超过 4096 个字符",
                "linha {line}: um valor passa de 4096 letras",
            ],
            Problem::BadEntity => [
                "line {line}: an entity is not one of the five of XML or a number",
                "línea {line}: una entidad no es una de las cinco de XML ni un número",
                "Zeile {line}: eine Entität ist weder eine der fünf von XML noch eine Zahl",
                "ligne {line} : une entité n'est ni l'une des cinq de XML ni un nombre",
                "riga {line}: un'entità non è una delle cinque di XML né un numero",
                "{line} 行目：エンティティが XML の 5 つでも数値でもありません",
                "第 {line} 行：实体不是 XML 的五个之一，也不是数字",
                "linha {line}: uma entidade não é uma das cinco de XML nem um número",
            ],
            Problem::SeveralRoots => [
                "line {line}: the document has more than one root element",
                "línea {line}: el documento tiene más de un elemento raíz",
                "Zeile {line}: das Dokument hat mehr als ein Wurzelelement",
                "ligne {line} : le document a plus d'un élément racine",
                "riga {line}: il documento ha più di un elemento radice",
                "{line} 行目：ルート要素が複数あります",
                "第 {line} 行：文档有多个根元素",
                "linha {line}: o documento tem mais de um elemento raiz",
            ],
        };
        say(
            lang,
            &row,
            &[("line", self.line.to_string()), ("a", a), ("b", b)],
        )
    }
}

struct Reader {
    chars: Vec<char>,
    at: usize,
    line: usize,
}

impl Reader {
    fn err(&self, p: Problem) -> XmlError {
        XmlError {
            line: self.line,
            problem: p,
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }

    fn starts(&self, s: &str) -> bool {
        s.chars()
            .enumerate()
            .all(|(i, c)| self.chars.get(self.at + i) == Some(&c))
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.at += 1;
        if c == '\n' {
            self.line += 1;
        }
        Some(c)
    }

    fn skip_until(&mut self, end: &str) -> Result<(), XmlError> {
        while self.at < self.chars.len() {
            if self.starts(end) {
                for _ in 0..end.chars().count() {
                    self.bump();
                }
                return Ok(());
            }
            self.bump();
        }
        Err(self.err(Problem::BadMarkup))
    }

    fn skip_space(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.bump();
        }
    }

    fn name(&mut self) -> Result<String, XmlError> {
        let mut s = String::new();
        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || matches!(c, '_' | '-' | ':' | '.') {
                s.push(c);
                self.bump();
                if s.chars().count() > MAX_NAME {
                    return Err(self.err(Problem::NameTooLong));
                }
            } else {
                break;
            }
        }
        if s.is_empty() {
            return Err(self.err(Problem::BadMarkup));
        }
        Ok(s)
    }

    fn entity(&mut self) -> Result<char, XmlError> {
        // the '&' is already taken
        let mut body = String::new();
        loop {
            match self.bump() {
                Some(';') => break,
                Some(c) if body.chars().count() < 10 && !c.is_whitespace() => body.push(c),
                _ => return Err(self.err(Problem::BadEntity)),
            }
        }
        match body.as_str() {
            "lt" => Ok('<'),
            "gt" => Ok('>'),
            "amp" => Ok('&'),
            "quot" => Ok('"'),
            "apos" => Ok('\''),
            b => {
                let code = if let Some(h) = b.strip_prefix("#x").or_else(|| b.strip_prefix("#X")) {
                    u32::from_str_radix(h, 16).ok()
                } else {
                    b.strip_prefix('#').and_then(|d| d.parse::<u32>().ok())
                };
                code.and_then(char::from_u32)
                    .filter(|c| !c.is_control() || matches!(c, '\t' | '\n' | '\r'))
                    .ok_or_else(|| self.err(Problem::BadEntity))
            }
        }
    }

    fn value(&mut self) -> Result<String, XmlError> {
        let quote = self
            .bump()
            .filter(|c| *c == '"' || *c == '\'')
            .ok_or_else(|| self.err(Problem::BadMarkup))?;
        let mut s = String::new();
        loop {
            let c = self.bump().ok_or_else(|| self.err(Problem::BadMarkup))?;
            if c == quote {
                return Ok(s);
            }
            if c == '<' {
                return Err(self.err(Problem::BadMarkup));
            }
            s.push(if c == '&' { self.entity()? } else { c });
            if s.chars().count() > MAX_VALUE {
                return Err(self.err(Problem::ValueTooLong));
            }
        }
    }
}

/// Reads a document: its one root element with everything under it.
pub fn parse(bytes: &[u8]) -> Result<Element, XmlError> {
    if bytes.len() > MAX_BYTES {
        return Err(XmlError {
            line: 0,
            problem: Problem::TooBig,
        });
    }
    let text = match std::str::from_utf8(bytes) {
        Ok(t) => t.trim_start_matches('\u{feff}'),
        Err(e) => {
            let line = bytes[..e.valid_up_to()]
                .iter()
                .filter(|b| **b == b'\n')
                .count()
                + 1;
            return Err(XmlError {
                line,
                problem: Problem::NotText,
            });
        }
    };
    let mut r = Reader {
        chars: text.chars().collect(),
        at: 0,
        line: 1,
    };
    let mut stack: Vec<Element> = Vec::new();
    let mut root: Option<Element> = None;
    let mut elements = 0usize;
    while r.at < r.chars.len() {
        if r.peek() != Some('<') {
            // text between elements is not kept
            r.bump();
            continue;
        }
        if r.starts("<!--") {
            r.skip_until("-->")?;
        } else if r.starts("<![CDATA[") {
            r.skip_until("]]>")?;
        } else if r.starts("<?") {
            r.skip_until("?>")?;
        } else if r.starts("<!DOCTYPE") || r.starts("<!ENTITY") || r.starts("<!") {
            return Err(r.err(Problem::Doctype));
        } else if r.starts("</") {
            r.bump();
            r.bump();
            let close = r.name()?;
            r.skip_space();
            if r.bump() != Some('>') {
                return Err(r.err(Problem::BadMarkup));
            }
            let Some(done) = stack.pop() else {
                return Err(r.err(Problem::BadMarkup));
            };
            if done.name != close {
                return Err(r.err(Problem::Mismatch(done.name, close)));
            }
            match stack.last_mut() {
                Some(parent) => parent.children.push(done),
                None => {
                    if root.is_some() {
                        return Err(r.err(Problem::SeveralRoots));
                    }
                    root = Some(done);
                }
            }
        } else {
            r.bump();
            let line = r.line;
            let name = r.name()?;
            elements += 1;
            if elements > MAX_ELEMENTS {
                return Err(r.err(Problem::TooMany));
            }
            let mut el = Element {
                name,
                attributes: Vec::new(),
                children: Vec::new(),
                line,
            };
            let mut self_closing = false;
            loop {
                r.skip_space();
                match r.peek() {
                    Some('/') => {
                        r.bump();
                        if r.bump() != Some('>') {
                            return Err(r.err(Problem::BadMarkup));
                        }
                        self_closing = true;
                        break;
                    }
                    Some('>') => {
                        r.bump();
                        break;
                    }
                    Some(_) => {
                        let key = r.name()?;
                        r.skip_space();
                        if r.bump() != Some('=') {
                            return Err(r.err(Problem::BadMarkup));
                        }
                        r.skip_space();
                        let v = r.value()?;
                        if el.attributes.len() >= MAX_ATTRIBUTES {
                            return Err(r.err(Problem::TooManyAttributes));
                        }
                        if !el.attributes.iter().any(|(k, _)| *k == key) {
                            el.attributes.push((key, v));
                        }
                    }
                    None => return Err(r.err(Problem::Unclosed(el.name))),
                }
            }
            if self_closing {
                match stack.last_mut() {
                    Some(parent) => parent.children.push(el),
                    None => {
                        if root.is_some() {
                            return Err(r.err(Problem::SeveralRoots));
                        }
                        root = Some(el);
                    }
                }
            } else {
                if stack.len() >= MAX_DEPTH {
                    return Err(r.err(Problem::TooDeep));
                }
                stack.push(el);
            }
        }
    }
    if let Some(open) = stack.pop() {
        return Err(XmlError {
            line: open.line,
            problem: Problem::Unclosed(open.name),
        });
    }
    root.ok_or(XmlError {
        line: r.line,
        problem: Problem::Empty,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_small_document_is_read_with_its_attributes_and_children() {
        let e = parse(b"<?xml version=\"1.0\"?><a x=\"1\" y='two'><b/><c z=\"&lt;&#65;\"></c></a>")
            .unwrap();
        assert_eq!(e.name, "a");
        assert_eq!(e.attr("y"), Some("two"));
        assert_eq!(e.children.len(), 2);
        assert_eq!(e.child("c").unwrap().attr("z"), Some("<A"));
        assert_eq!(e.count(), 3);
    }

    #[test]
    fn a_document_type_is_refused_so_no_entity_can_blow_up() {
        let doc = b"<!DOCTYPE a [<!ENTITY x \"yyyy\">]><a v=\"&x;\"/>";
        assert_eq!(parse(doc).unwrap_err().problem, Problem::Doctype);
    }
}
