// EH3D-PHOENIX-PNP - a small pattern matcher for the lines that a controller answers.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
//! The profile of a driver tells what an answer looks like with a pattern (`^ok`, `X:(-?\d+\.?\d*) Y:(-?\d+\.?\d*)`). A pattern is text from a file, so the matcher has to be safe with
//! text that somebody else wrote: it has no back-references and no look-around, it runs every thread of the pattern one letter at a time (so the time is the length of the text times the
//! size of the pattern, never more), and a pattern or a text over its limit is refused or cut. It knows: letters, `.`, `\d \D \s \S \w \W`, classes (`[a-z0-9_]`, `[^ ]`), `* + ?`, groups
//! (`( )` that capture, `(?: )` that do not), `|`, `^`, `$` and a `(?i)` at the start for case.

/// The longest pattern (letters).
pub const MAX_PATTERN: usize = 256;
/// The longest text that is looked at (letters); the rest is left out.
pub const MAX_TEXT: usize = 4096;
/// The most groups that capture.
pub const MAX_GROUPS: usize = 9;
/// The most steps of the compiled pattern.
const MAX_PROGRAM: usize = 2000;
/// The deepest nesting of groups.
const MAX_DEPTH: usize = 16;

#[derive(Debug, Clone, PartialEq)]
enum Node {
    Empty,
    Char(char),
    Any,
    Class(Vec<(char, char)>, bool),
    Cat(Vec<Node>),
    Alt(Vec<Node>),
    Star(Box<Node>),
    Plus(Box<Node>),
    Quest(Box<Node>),
    Group(Option<usize>, Box<Node>),
    Bol,
    Eol,
}

#[derive(Debug, Clone, PartialEq)]
enum Inst {
    Char(char),
    Any,
    Class(Vec<(char, char)>, bool),
    Split(usize, usize),
    Jmp(usize),
    Save(usize),
    Bol,
    Eol,
    Match,
}

/// A pattern that was read and checked.
#[derive(Debug, Clone, PartialEq)]
pub struct Pattern {
    source: String,
    prog: Vec<Inst>,
    groups: usize,
    ci: bool,
}

/// What a match found: the span of the whole match and of each group (in letters of the text), and the text.
#[derive(Debug, Clone, PartialEq)]
pub struct Captures {
    chars: Vec<char>,
    spans: Vec<Option<(usize, usize)>>,
}

impl Captures {
    /// The text of group `i` (0 is the whole match); `None` when the group did not take part.
    pub fn get(&self, i: usize) -> Option<String> {
        let (a, b) = (*self.spans.get(i)?)?;
        Some(self.chars[a..b].iter().collect())
    }

    /// The number of the group `i` read as a number, when it is one.
    pub fn number(&self, i: usize) -> Option<f64> {
        self.get(i)
            .and_then(|t| t.trim().parse::<f64>().ok())
            .filter(|v| v.is_finite())
    }
}

struct Parser {
    chars: Vec<char>,
    at: usize,
    groups: usize,
    depth: usize,
}

fn shorthand(c: char) -> Option<(Vec<(char, char)>, bool)> {
    let digits = vec![('0', '9')];
    let space = vec![(' ', ' '), ('\t', '\t'), ('\n', '\n'), ('\r', '\r')];
    let word = vec![('a', 'z'), ('A', 'Z'), ('0', '9'), ('_', '_')];
    match c {
        'd' => Some((digits, false)),
        'D' => Some((digits, true)),
        's' => Some((space, false)),
        'S' => Some((space, true)),
        'w' => Some((word, false)),
        'W' => Some((word, true)),
        _ => None,
    }
}

fn escaped(c: char) -> char {
    match c {
        'n' => '\n',
        't' => '\t',
        'r' => '\r',
        other => other,
    }
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }

    fn alternation(&mut self) -> Result<Node, String> {
        let mut branches = vec![self.sequence()?];
        while self.peek() == Some('|') {
            self.at += 1;
            branches.push(self.sequence()?);
        }
        Ok(if branches.len() == 1 {
            branches.remove(0)
        } else {
            Node::Alt(branches)
        })
    }

    fn sequence(&mut self) -> Result<Node, String> {
        let mut items = Vec::new();
        while let Some(c) = self.peek() {
            if c == '|' || c == ')' {
                break;
            }
            let atom = self.atom()?;
            items.push(self.quantified(atom)?);
        }
        Ok(match items.len() {
            0 => Node::Empty,
            1 => items.remove(0),
            _ => Node::Cat(items),
        })
    }

    fn quantified(&mut self, atom: Node) -> Result<Node, String> {
        let mut node = atom;
        while let Some(q) = self.peek() {
            node = match q {
                '*' => Node::Star(Box::new(node)),
                '+' => Node::Plus(Box::new(node)),
                '?' => Node::Quest(Box::new(node)),
                _ => break,
            };
            self.at += 1;
        }
        Ok(node)
    }

    fn atom(&mut self) -> Result<Node, String> {
        let c = self
            .peek()
            .ok_or("the pattern ends where a letter is needed")?;
        self.at += 1;
        match c {
            '.' => Ok(Node::Any),
            '^' => Ok(Node::Bol),
            '$' => Ok(Node::Eol),
            '*' | '+' | '?' => Err(format!("'{c}' has nothing before it to repeat")),
            '(' => self.group(),
            '[' => self.class(),
            '\\' => {
                let e = self.peek().ok_or("the pattern ends after a backslash")?;
                self.at += 1;
                Ok(match shorthand(e) {
                    Some((ranges, neg)) => Node::Class(ranges, neg),
                    None => Node::Char(escaped(e)),
                })
            }
            other => Ok(Node::Char(other)),
        }
    }

    fn group(&mut self) -> Result<Node, String> {
        if self.depth >= MAX_DEPTH {
            return Err(format!("groups are nested over {MAX_DEPTH} deep"));
        }
        let mut capture = true;
        if self.peek() == Some('?') {
            if self.chars.get(self.at + 1) == Some(&':') {
                self.at += 2;
                capture = false;
            } else {
                return Err("only (?: ) and a (?i) at the start are known after '('".into());
            }
        }
        let number = if capture {
            if self.groups >= MAX_GROUPS {
                return Err(format!("there are more than {MAX_GROUPS} groups"));
            }
            self.groups += 1;
            Some(self.groups)
        } else {
            None
        };
        self.depth += 1;
        let inner = self.alternation()?;
        self.depth -= 1;
        if self.peek() != Some(')') {
            return Err("a '(' is not closed".into());
        }
        self.at += 1;
        Ok(Node::Group(number, Box::new(inner)))
    }

    fn class(&mut self) -> Result<Node, String> {
        let mut negate = false;
        if self.peek() == Some('^') {
            negate = true;
            self.at += 1;
        }
        let mut ranges: Vec<(char, char)> = Vec::new();
        let mut first = true;
        loop {
            let c = self.peek().ok_or("a '[' is not closed")?;
            self.at += 1;
            if c == ']' && !first {
                break;
            }
            first = false;
            let lo = if c == '\\' {
                let e = self.peek().ok_or("the pattern ends after a backslash")?;
                self.at += 1;
                if let Some((more, neg)) = shorthand(e) {
                    if neg {
                        return Err("\\D, \\S and \\W cannot be used inside a class".into());
                    }
                    ranges.extend(more);
                    continue;
                }
                escaped(e)
            } else {
                c
            };
            if self.peek() == Some('-') && self.chars.get(self.at + 1).is_some_and(|n| *n != ']') {
                self.at += 1;
                let mut hi = self.peek().ok_or("a '[' is not closed")?;
                self.at += 1;
                if hi == '\\' {
                    hi = escaped(self.peek().ok_or("the pattern ends after a backslash")?);
                    self.at += 1;
                }
                if hi < lo {
                    return Err(format!("the range {lo}-{hi} runs backwards"));
                }
                ranges.push((lo, hi));
            } else {
                ranges.push((lo, lo));
            }
            if ranges.len() > 64 {
                return Err("a class holds over 64 ranges".into());
            }
        }
        Ok(Node::Class(ranges, negate))
    }
}

fn emit(node: &Node, prog: &mut Vec<Inst>) -> Result<(), String> {
    if prog.len() > MAX_PROGRAM {
        return Err(format!("the pattern is over {MAX_PROGRAM} steps"));
    }
    match node {
        Node::Empty => {}
        Node::Char(c) => prog.push(Inst::Char(*c)),
        Node::Any => prog.push(Inst::Any),
        Node::Class(r, n) => prog.push(Inst::Class(r.clone(), *n)),
        Node::Bol => prog.push(Inst::Bol),
        Node::Eol => prog.push(Inst::Eol),
        Node::Cat(items) => {
            for i in items {
                emit(i, prog)?;
            }
        }
        Node::Alt(branches) => {
            let mut jumps = Vec::new();
            for (k, b) in branches.iter().enumerate() {
                if k + 1 < branches.len() {
                    let split = prog.len();
                    prog.push(Inst::Split(split + 1, 0));
                    emit(b, prog)?;
                    jumps.push(prog.len());
                    prog.push(Inst::Jmp(0));
                    let next = prog.len();
                    prog[split] = Inst::Split(split + 1, next);
                } else {
                    emit(b, prog)?;
                }
            }
            let end = prog.len();
            for j in jumps {
                prog[j] = Inst::Jmp(end);
            }
        }
        Node::Star(inner) => {
            let split = prog.len();
            prog.push(Inst::Split(split + 1, 0));
            emit(inner, prog)?;
            prog.push(Inst::Jmp(split));
            let end = prog.len();
            prog[split] = Inst::Split(split + 1, end);
        }
        Node::Plus(inner) => {
            let start = prog.len();
            emit(inner, prog)?;
            let split = prog.len();
            prog.push(Inst::Split(start, split + 1));
        }
        Node::Quest(inner) => {
            let split = prog.len();
            prog.push(Inst::Split(split + 1, 0));
            emit(inner, prog)?;
            let end = prog.len();
            prog[split] = Inst::Split(split + 1, end);
        }
        Node::Group(number, inner) => match number {
            Some(n) => {
                prog.push(Inst::Save(2 * n));
                emit(inner, prog)?;
                prog.push(Inst::Save(2 * n + 1));
            }
            None => emit(inner, prog)?,
        },
    }
    Ok(())
}

type Caps = Vec<Option<usize>>;

fn class_has(ranges: &[(char, char)], negate: bool, c: char, ci: bool) -> bool {
    let hit = |ch: char| ranges.iter().any(|(lo, hi)| (*lo..=*hi).contains(&ch));
    let found = hit(c) || (ci && (c.to_lowercase().any(&hit) || c.to_uppercase().any(&hit)));
    found != negate
}

impl Pattern {
    /// Reads a pattern. A pattern over `MAX_PATTERN` letters, with more than `MAX_GROUPS` groups, nested over 16 deep or compiled to over 2000 steps is refused, with what is wrong.
    pub fn new(source: &str) -> Result<Pattern, String> {
        if source.chars().count() > MAX_PATTERN {
            return Err(format!("the pattern is over {MAX_PATTERN} letters"));
        }
        let (body, ci) = match source.strip_prefix("(?i)") {
            Some(rest) => (rest, true),
            None => (source, false),
        };
        let mut p = Parser {
            chars: body.chars().collect(),
            at: 0,
            groups: 0,
            depth: 0,
        };
        let tree = p.alternation()?;
        if p.at < p.chars.len() {
            return Err("a ')' has no '(' before it".into());
        }
        let mut prog = vec![Inst::Save(0)];
        emit(&tree, &mut prog)?;
        prog.push(Inst::Save(1));
        prog.push(Inst::Match);
        if prog.len() > MAX_PROGRAM {
            return Err(format!("the pattern is over {MAX_PROGRAM} steps"));
        }
        Ok(Pattern {
            source: source.to_string(),
            prog,
            groups: p.groups,
            ci,
        })
    }

    /// The text the pattern was read from.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// How many groups capture.
    pub fn groups(&self) -> usize {
        self.groups
    }

    /// Whether the pattern is found anywhere in the text.
    pub fn is_match(&self, text: &str) -> bool {
        self.find(text).is_some()
    }

    /// The first match in the text (the one that starts first; of those, the one the pattern prefers), with its groups.
    pub fn find(&self, text: &str) -> Option<Captures> {
        let chars: Vec<char> = text.chars().take(MAX_TEXT).collect();
        let caps = self.run(&chars)?;
        let spans = (0..=self.groups)
            .map(|g| match (caps[2 * g], caps[2 * g + 1]) {
                (Some(a), Some(b)) if a <= b => Some((a, b)),
                _ => None,
            })
            .collect();
        Some(Captures { chars, spans })
    }

    #[allow(clippy::too_many_arguments)]
    fn add(
        &self,
        list: &mut Vec<(usize, Caps)>,
        mark: &mut [usize],
        stamp: usize,
        pc: usize,
        pos: usize,
        mut caps: Caps,
        len: usize,
    ) {
        if mark[pc] == stamp {
            return;
        }
        mark[pc] = stamp;
        match &self.prog[pc] {
            Inst::Jmp(t) => self.add(list, mark, stamp, *t, pos, caps, len),
            Inst::Split(a, b) => {
                self.add(list, mark, stamp, *a, pos, caps.clone(), len);
                self.add(list, mark, stamp, *b, pos, caps, len);
            }
            Inst::Save(k) => {
                if let Some(slot) = caps.get_mut(*k) {
                    *slot = Some(pos);
                }
                self.add(list, mark, stamp, pc + 1, pos, caps, len);
            }
            Inst::Bol => {
                if pos == 0 {
                    self.add(list, mark, stamp, pc + 1, pos, caps, len);
                }
            }
            Inst::Eol => {
                if pos == len {
                    self.add(list, mark, stamp, pc + 1, pos, caps, len);
                }
            }
            _ => list.push((pc, caps)),
        }
    }

    fn run(&self, chars: &[char]) -> Option<Caps> {
        let ncap = 2 * (self.groups + 1);
        let mut mark = vec![0usize; self.prog.len()];
        let mut clist: Vec<(usize, Caps)> = Vec::new();
        let mut matched: Option<Caps> = None;
        for pos in 0..=chars.len() {
            if matched.is_none() {
                self.add(
                    &mut clist,
                    &mut mark,
                    pos + 1,
                    0,
                    pos,
                    vec![None; ncap],
                    chars.len(),
                );
            }
            if clist.is_empty() && matched.is_some() {
                break;
            }
            let mut nlist: Vec<(usize, Caps)> = Vec::new();
            for (pc, caps) in std::mem::take(&mut clist) {
                match &self.prog[pc] {
                    Inst::Match => {
                        matched = Some(caps);
                        break;
                    }
                    inst => {
                        let Some(&c) = chars.get(pos) else { continue };
                        let ok = match inst {
                            Inst::Char(w) => {
                                *w == c || (self.ci && w.to_lowercase().eq(c.to_lowercase()))
                            }
                            Inst::Any => true,
                            Inst::Class(r, n) => class_has(r, *n, c, self.ci),
                            _ => false,
                        };
                        if ok {
                            self.add(
                                &mut nlist,
                                &mut mark,
                                pos + 2,
                                pc + 1,
                                pos + 1,
                                caps,
                                chars.len(),
                            );
                        }
                    }
                }
            }
            clist = nlist;
        }
        matched
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_letter_pattern_is_found_anywhere() {
        let p = Pattern::new("ok").unwrap();
        assert!(p.is_match("ok"));
        assert!(p.is_match("it is ok now"));
        assert!(!p.is_match("o k"));
    }

    #[test]
    fn the_anchors_hold_the_ends() {
        let p = Pattern::new("^ok$").unwrap();
        assert!(p.is_match("ok"));
        assert!(!p.is_match("ok "));
        assert!(!p.is_match("not ok"));
    }

    #[test]
    fn a_group_takes_the_number() {
        let p = Pattern::new(r"X:(-?\d+\.?\d*) Y:(-?\d+\.?\d*)").unwrap();
        let c = p.find("X:10.50 Y:-3 Z:0").unwrap();
        assert_eq!(c.number(1), Some(10.5));
        assert_eq!(c.number(2), Some(-3.0));
        assert_eq!(c.get(0).as_deref(), Some("X:10.50 Y:-3"));
    }

    #[test]
    fn a_case_flag_ignores_case() {
        let p = Pattern::new("(?i)^error").unwrap();
        assert!(p.is_match("Error: halted"));
        assert!(p.is_match("ERROR"));
        assert!(!Pattern::new("^error").unwrap().is_match("Error"));
    }

    #[test]
    fn a_pattern_made_to_blow_up_a_matcher_is_fast() {
        let p = Pattern::new("(a*)*b").unwrap();
        let text = "a".repeat(MAX_TEXT);
        assert!(!p.is_match(&text));
        let q = Pattern::new("(a|aa)+$").unwrap();
        assert!(q.is_match(&text));
    }
}
