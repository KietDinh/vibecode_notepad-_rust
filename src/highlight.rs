//! Incremental syntax highlighting and fold computation.
//!
//! Each line has a cached lexer state + fold level at its start. Edits invalidate from the
//! edited line; re-lexing stops as soon as the state converges with the old cache.

use crate::lang::{keywords, spec, Fold, KwSets, Lang, LexKind, Spec};
use ropey::Rope;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum St {
    Default,
    Comment,
    CommentLine,
    CommentDoc,
    Number,
    Keyword,
    Type,
    Builtin,
    String,
    Char,
    Triple,
    Operator,
    Preproc,
    FuncName,
    ClassName,
    Decorator,
    Lifetime,
    Macro,
    Variable,
    Property,
    Tag,
    Attr,
    AttrValue,
    Entity,
    CData,
    XmlDecl,
    Sgml,
    Text,
    Label,
    Section,
    Key,
    Assign,
    Value,
    Heading,
    Emph,
    Strong,
    Code,
    Link,
    Quote,
    ListMark,
    DiffAdd,
    DiffDel,
    DiffHeader,
    DiffPos,
    DiffCmd,
    CssClass,
    CssId,
    Pseudo,
    Important,
    Directive,
    SrHeader,
    SrFile,
    SrLine,
}

pub type Runs = Vec<(usize, St)>;

// lexer modes (low byte of state)
const M_NORMAL: u32 = 0;
const M_BLOCK: u32 = 1;
const M_DOCBLOCK: u32 = 2;
const M_STR: u32 = 3;
const M_TRIPLE: u32 = 4;
const M_RAW: u32 = 5;
const M_TAG: u32 = 6;
const M_HCOMMENT: u32 = 7;
const M_CDATA: u32 = 8;
const M_PI: u32 = 9;
const M_SGML: u32 = 10;
const M_ATTR_DQ: u32 = 11;
const M_ATTR_SQ: u32 = 12;
const M_FENCE: u32 = 13;

fn mode(s: u32) -> u32 {
    s & 0xFF
}

#[derive(Clone, Copy, Default, Debug)]
pub struct FoldInfo {
    /// If this line is a fold header: last line of the fold body; 0 otherwise.
    pub end: u32,
    /// Number of folds that strictly contain this line (header excluded).
    pub depth: u16,
    /// Number of folds whose last line is this line.
    pub ends: u16,
}

#[derive(Default)]
pub struct Highlighter {
    starts: Vec<u32>,
    levels: Vec<i32>,
    valid_upto: usize,
    known_upto: usize,
    must_relex_to: usize,
    fold_version: u64,
    pub folds: Vec<FoldInfo>,
}

const FOLD_LINE_LIMIT: usize = 400_000;

impl Highlighter {
    pub fn reset(&mut self, lines: usize) {
        self.starts = vec![0; lines + 1];
        self.levels = vec![0; lines + 1];
        self.valid_upto = 0;
        self.known_upto = 0;
        self.must_relex_to = 0;
        self.fold_version = 0;
        self.folds.clear();
    }

    pub fn on_edit(&mut self, line: usize, removed: usize, added: usize) {
        if self.starts.is_empty() {
            return;
        }
        let lo = (line + 1).min(self.starts.len());
        let hi = (line + 1 + removed).min(self.starts.len());
        let fill_s = self.starts.get(line).copied().unwrap_or(0);
        let fill_l = self.levels.get(line).copied().unwrap_or(0);
        self.starts.splice(lo..hi, std::iter::repeat(fill_s).take(added));
        self.levels.splice(lo..hi, std::iter::repeat(fill_l).take(added));
        self.known_upto = if self.known_upto > line + removed { self.known_upto + added - removed } else { self.known_upto.min(line) };
        self.valid_upto = self.valid_upto.min(line);
        self.must_relex_to = if self.must_relex_to > line + removed {
            (self.must_relex_to + added - removed).max(line + added)
        } else {
            self.must_relex_to.max(line + added)
        };
        self.fold_version = 0;
    }

    /// Make sure start states are valid for lines 0..=upto.
    pub fn ensure(&mut self, text: &Rope, lang: Lang, upto: usize) {
        let n = text.len_lines();
        if self.starts.len() != n + 1 {
            self.reset(n);
        }
        let upto = upto.min(n);
        if self.valid_upto >= upto {
            return;
        }
        let sp = spec(lang);
        let kw = keywords(lang);
        let mut i = self.valid_upto;
        let mut buf = Vec::new();
        while i < upto && i < n {
            line_chars(text, i, &mut buf);
            let mut lv = self.levels[i];
            let st = lex_line(sp, kw, &buf, self.starts[i], &mut lv, None);
            let next = i + 1;
            if next > self.must_relex_to && next <= self.known_upto && self.starts[next] == st && self.levels[next] == lv {
                self.valid_upto = self.known_upto.max(next);
                self.must_relex_to = 0;
                if self.valid_upto >= upto {
                    return;
                }
                i = self.valid_upto;
                continue;
            }
            self.starts[next] = st;
            self.levels[next] = lv;
            i = next;
            self.valid_upto = i;
            if self.known_upto < i {
                self.known_upto = i;
            }
        }
    }

    /// Style runs for one line (start state must be valid).
    pub fn line_runs(&mut self, text: &Rope, lang: Lang, line: usize, chars: &[char]) -> Runs {
        self.ensure(text, lang, line);
        let mut out = Vec::new();
        let sp = spec(lang);
        let kw = keywords(lang);
        let st = self.starts.get(line).copied().unwrap_or(0);
        let mut lv = self.levels.get(line).copied().unwrap_or(0);
        lex_line(sp, kw, chars, st, &mut lv, Some(&mut out));
        out
    }

    /// Recompute fold ranges if the document changed.
    pub fn update_folds(&mut self, text: &Rope, lang: Lang, version: u64, tab: usize) {
        if self.fold_version == version {
            return;
        }
        self.fold_version = version;
        let n = text.len_lines();
        self.folds = vec![FoldInfo::default(); n];
        let sp = spec(lang);
        if sp.fold == Fold::None || n > FOLD_LINE_LIMIT {
            return;
        }
        let mut ranges: Vec<(usize, usize)> = Vec::new();
        match sp.fold {
            Fold::Brace | Fold::Tag => {
                self.ensure(text, lang, n);
                let mut stack: Vec<(usize, i32)> = Vec::new();
                for i in 0..n {
                    let ls = self.levels[i];
                    let le = self.levels[i + 1];
                    while let Some(&(h, base)) = stack.last() {
                        if le <= base && h < i {
                            ranges.push((h, i));
                            stack.pop();
                        } else {
                            break;
                        }
                    }
                    if le > ls {
                        stack.push((i, ls));
                    }
                }
                for (h, _) in stack {
                    if n - 1 > h {
                        ranges.push((h, n - 1));
                    }
                }
            }
            Fold::Indent => {
                let mut stack: Vec<(usize, usize)> = Vec::new();
                let mut last_nonblank = 0usize;
                for i in 0..n {
                    let Some(ind) = line_indent(text, i, tab) else { continue };
                    while let Some(&(h, hi)) = stack.last() {
                        if hi >= ind {
                            if last_nonblank > h {
                                ranges.push((h, last_nonblank));
                            }
                            stack.pop();
                        } else {
                            break;
                        }
                    }
                    stack.push((i, ind));
                    last_nonblank = i;
                }
                for (h, _) in stack {
                    if last_nonblank > h {
                        ranges.push((h, last_nonblank));
                    }
                }
            }
            Fold::Section => {
                let mut prev: Option<usize> = None;
                for i in 0..n {
                    let first = text.line(i).chars().find(|c| !c.is_whitespace());
                    if first == Some('[') {
                        if let Some(h) = prev {
                            if i - 1 > h {
                                ranges.push((h, i - 1));
                            }
                        }
                        prev = Some(i);
                    }
                }
                if let Some(h) = prev {
                    let mut e = n - 1;
                    while e > h && text.line(e).chars().all(|c| c.is_whitespace()) {
                        e -= 1;
                    }
                    if e > h {
                        ranges.push((h, e));
                    }
                }
            }
            Fold::None => {}
        }
        let mut diff = vec![0i32; n + 1];
        for &(h, e) in &ranges {
            self.folds[h].end = e as u32;
            diff[h + 1] += 1;
            diff[e + 1] -= 1;
            self.folds[e].ends += 1;
        }
        let mut d = 0i32;
        for i in 0..n {
            d += diff[i];
            self.folds[i].depth = d.max(0) as u16;
        }
    }

    pub fn fold_end(&self, line: usize) -> Option<usize> {
        self.folds.get(line).and_then(|f| if f.end as usize > line { Some(f.end as usize) } else { None })
    }

    /// Innermost fold (header, end) containing `line`.
    pub fn enclosing_fold(&self, line: usize) -> Option<(usize, usize)> {
        if self.folds.get(line).is_some_and(|f| f.end as usize > line) {
            return Some((line, self.folds[line].end as usize));
        }
        let mut l = line;
        while l > 0 {
            l -= 1;
            let f = self.folds[l];
            if f.end as usize >= line && f.end as usize > l {
                return Some((l, f.end as usize));
            }
        }
        None
    }
}

fn line_chars(text: &Rope, i: usize, buf: &mut Vec<char>) {
    buf.clear();
    buf.extend(text.line(i).chars().filter(|&c| c != '\n'));
}

fn line_indent(text: &Rope, i: usize, tab: usize) -> Option<usize> {
    let mut col = 0;
    for c in text.line(i).chars() {
        match c {
            ' ' => col += 1,
            '\t' => col = (col / tab + 1) * tab,
            '\n' | '\r' => return None,
            _ => return Some(col),
        }
    }
    None
}

struct Out<'a> {
    runs: Option<&'a mut Runs>,
    base: usize,
}

impl Out<'_> {
    fn emit(&mut self, i: usize, st: St) {
        if let Some(r) = self.runs.as_deref_mut() {
            let p = self.base + i;
            if let Some(last) = r.last_mut() {
                if last.1 == st {
                    return;
                }
                if last.0 == p {
                    last.1 = st;
                    if r.len() >= 2 && r[r.len() - 2].1 == st {
                        r.pop();
                    }
                    return;
                }
            }
            r.push((p, st));
        }
    }
    /// Change style of the run starting at local index `i` (used for JSON property names).
    fn restyle_last(&mut self, st: St) {
        if let Some(r) = self.runs.as_deref_mut() {
            if let Some(last) = r.last_mut() {
                last.1 = st;
            }
        }
    }
}

fn starts_with(s: &[char], i: usize, pat: &str) -> bool {
    let mut j = i;
    for pc in pat.chars() {
        if j >= s.len() || s[j] != pc {
            return false;
        }
        j += 1;
    }
    true
}

fn starts_with_nocase(s: &[char], i: usize, pat: &str) -> bool {
    let mut j = i;
    for pc in pat.chars() {
        if j >= s.len() || !s[j].eq_ignore_ascii_case(&pc) {
            return false;
        }
        j += 1;
    }
    true
}

fn find(s: &[char], from: usize, pat: &str) -> Option<usize> {
    let p: Vec<char> = pat.chars().collect();
    if p.is_empty() || s.len() < p.len() {
        return None;
    }
    (from..=s.len() - p.len()).find(|&i| s[i..i + p.len()] == p[..])
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}
fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

pub fn lex_line(sp: &Spec, kw: &KwSets, s: &[char], state: u32, level: &mut i32, runs: Option<&mut Runs>) -> u32 {
    let mut out = Out { runs, base: 0 };
    match sp.kind {
        LexKind::Plain => {
            out.emit(0, St::Default);
            0
        }
        LexKind::CLike => {
            let was = matches!(mode(state), M_BLOCK | M_DOCBLOCK);
            let st = lex_clike(sp, kw, s, state, level, &mut out);
            let is = matches!(mode(st), M_BLOCK | M_DOCBLOCK);
            if sp.fold == Fold::Brace {
                if !was && is {
                    *level += 1;
                } else if was && !is {
                    *level -= 1;
                }
            }
            st
        }
        LexKind::Markup => lex_markup(sp, s, state, level, &mut out),
        LexKind::Ini => {
            lex_ini(sp, kw, s, &mut out);
            0
        }
        LexKind::Batch => {
            lex_batch(kw, s, &mut out);
            0
        }
        LexKind::Diff => {
            let st = if starts_with(s, 0, "diff ") || starts_with(s, 0, "index ") || starts_with(s, 0, "Index:") {
                St::DiffCmd
            } else if starts_with(s, 0, "---") || starts_with(s, 0, "+++") || starts_with(s, 0, "***") {
                St::DiffHeader
            } else if starts_with(s, 0, "@@") {
                St::DiffPos
            } else if starts_with(s, 0, "-") || starts_with(s, 0, "<") {
                St::DiffDel
            } else if starts_with(s, 0, "+") || starts_with(s, 0, ">") {
                St::DiffAdd
            } else if starts_with(s, 0, "Only in") || starts_with(s, 0, "Binary") {
                St::Comment
            } else {
                St::Default
            };
            out.emit(0, st);
            0
        }
        LexKind::Markdown => lex_markdown(s, state, &mut out),
        LexKind::Yaml => {
            lex_yaml(sp, kw, s, &mut out);
            0
        }
        LexKind::SearchResult => {
            if s.first() == Some(&'\t') {
                let colon = s.iter().position(|&c| c == ':').unwrap_or(0);
                out.emit(0, St::SrLine);
                out.emit(colon + 1, St::Default);
            } else if s.first() == Some(&' ') {
                out.emit(0, St::SrFile);
            } else if !s.is_empty() {
                out.emit(0, St::SrHeader);
            }
            0
        }
    }
}

fn scan_string(s: &[char], from: usize, q: char) -> Option<usize> {
    let mut j = from;
    while j < s.len() {
        if s[j] == '\\' {
            j += 2;
            continue;
        }
        if s[j] == q {
            return Some(j);
        }
        j += 1;
    }
    None
}

fn lex_clike(sp: &Spec, kw: &KwSets, s: &[char], state: u32, level: &mut i32, out: &mut Out) -> u32 {
    let n = s.len();
    let mut i = 0usize;
    match mode(state) {
        M_BLOCK | M_DOCBLOCK => {
            let st = if mode(state) == M_DOCBLOCK { St::CommentDoc } else { St::Comment };
            out.emit(0, st);
            let close = sp.block_comment.map(|b| b.1).unwrap_or("*/");
            match find(s, 0, close) {
                Some(e) => i = e + close.chars().count(),
                None => return state,
            }
        }
        M_STR => {
            let q = char::from_u32((state >> 8) & 0xFFFF).unwrap_or('"');
            out.emit(0, St::String);
            match scan_string(s, 0, q) {
                Some(e) => i = e + 1,
                None => return state,
            }
        }
        M_TRIPLE => {
            let q = char::from_u32((state >> 8) & 0xFFFF).unwrap_or('"');
            out.emit(0, St::Triple);
            let pat: String = std::iter::repeat(q).take(3).collect();
            match find(s, 0, &pat) {
                Some(e) => i = e + 3,
                None => return state,
            }
        }
        M_RAW => {
            let hashes = ((state >> 8) & 0xFF) as usize;
            out.emit(0, St::String);
            let pat: String = std::iter::once('"').chain(std::iter::repeat('#').take(hashes)).collect();
            match find(s, 0, &pat) {
                Some(e) => i = e + pat.len(),
                None => return state,
            }
        }
        _ => {}
    }
    let first_nonws = s.iter().position(|c| !c.is_whitespace());
    let mut prev_word = String::new();
    while i < n {
        let c = s[i];
        // line comments
        if let Some(tok) = sp.line_comment.iter().find(|t| starts_with(s, i, t)) {
            let doc = (*tok == "//" && (starts_with(s, i, "///") || starts_with(s, i, "//!"))) && !starts_with(s, i, "////");
            if !(sp.css) && !(*tok == "#" && sp.lang == crate::lang::Lang::Php && starts_with(s, i, "#[")) {
                out.emit(i, if doc { St::CommentDoc } else { St::CommentLine });
                return M_NORMAL;
            }
        }
        if let Some((open, close)) = sp.block_comment {
            if starts_with(s, i, open) {
                let ol = open.chars().count();
                let doc = open == "/*" && starts_with(s, i, "/**") && !starts_with(s, i, "/**/");
                out.emit(i, if doc { St::CommentDoc } else { St::Comment });
                match find(s, i + ol, close) {
                    Some(e) => {
                        i = e + close.chars().count();
                        continue;
                    }
                    None => return if doc { M_DOCBLOCK } else { M_BLOCK },
                }
            }
        }
        if sp.preproc && Some(i) == first_nonws && c == '#' {
            out.emit(i, St::Preproc);
            if let Some(j) = find(s, i, "//") {
                out.emit(j, St::CommentLine);
            }
            return M_NORMAL;
        }
        if sp.triple && (c == '"' || c == '\'') && i + 2 < n && s[i + 1] == c && s[i + 2] == c {
            out.emit(i, St::Triple);
            let pat: String = std::iter::repeat(c).take(3).collect();
            match find(s, i + 3, &pat) {
                Some(e) => {
                    i = e + 3;
                    continue;
                }
                None => return M_TRIPLE | ((c as u32) << 8),
            }
        }
        if sp.rust && (c == 'r' || c == 'b') && (i == 0 || !is_ident(s[i - 1])) {
            let mut j = i + 1;
            if c == 'b' && j < n && s[j] == 'r' {
                j += 1;
            }
            if c == 'r' || j > i + 1 {
                let mut h = 0;
                while j < n && s[j] == '#' {
                    h += 1;
                    j += 1;
                }
                if j < n && s[j] == '"' && (h > 0 || c == 'r' || j > i + 1) {
                    out.emit(i, St::String);
                    let pat: String = std::iter::once('"').chain(std::iter::repeat('#').take(h)).collect();
                    match find(s, j + 1, &pat) {
                        Some(e) => {
                            i = e + pat.len();
                            continue;
                        }
                        None => return M_RAW | ((h as u32) << 8),
                    }
                }
            }
        }
        if sp.quotes.contains(c) {
            if c == '\'' && sp.rust {
                // char literal or lifetime?
                let is_char = (i + 1 < n && s[i + 1] == '\\') || (i + 2 < n && s[i + 2] == '\'');
                if !is_char {
                    out.emit(i, St::Lifetime);
                    let mut j = i + 1;
                    while j < n && is_ident(s[j]) {
                        j += 1;
                    }
                    i = j.max(i + 1);
                    continue;
                }
            }
            let st = if c == '\'' && sp.char_lit { St::Char } else { St::String };
            out.emit(i, st);
            match scan_string(s, i + 1, c) {
                Some(e) => {
                    i = e + 1;
                    if sp.json {
                        let mut k = i;
                        while k < n && s[k].is_whitespace() {
                            k += 1;
                        }
                        if k < n && s[k] == ':' {
                            out.restyle_last(St::Property);
                        }
                    }
                    continue;
                }
                None => {
                    if sp.ml_quotes.contains(c) || (n > 0 && s[n - 1] == '\\' && c == '"') {
                        return M_STR | ((c as u32) << 8);
                    }
                    return M_NORMAL;
                }
            }
        }
        let prev_is_ident = i > 0 && (is_ident(s[i - 1]) || (sp.css && s[i - 1] == '-'));
        if !prev_is_ident && (c.is_ascii_digit() || (c == '.' && i + 1 < n && s[i + 1].is_ascii_digit())) {
            out.emit(i, St::Number);
            let mut j = i + 1;
            let hex = c == '0' && j < n && (s[j] == 'x' || s[j] == 'X');
            if hex {
                j += 1;
            }
            while j < n {
                let d = s[j];
                if d.is_alphanumeric() || d == '_' || (d == '.' && !hex && j + 1 < n && s[j + 1] != '.') {
                    j += 1;
                } else if (d == '+' || d == '-') && !hex && matches!(s[j - 1], 'e' | 'E') {
                    j += 1;
                } else {
                    break;
                }
            }
            if sp.css && j < n && s[j] == '%' {
                j += 1;
            }
            i = j;
            continue;
        }
        if let Some(vp) = sp.var_prefix {
            if c == vp && i + 1 < n && (is_ident(s[i + 1]) || s[i + 1] == '{' || s[i + 1] == '(') {
                out.emit(i, St::Variable);
                let mut j = i + 1;
                if s[j] == '{' || s[j] == '(' {
                    let close = if s[j] == '{' { '}' } else { ')' };
                    while j < n && s[j] != close {
                        j += 1;
                    }
                    j = (j + 1).min(n);
                } else {
                    while j < n && (is_ident(s[j]) || (sp.lang == Lang::PowerShell && s[j] == ':')) {
                        j += 1;
                    }
                }
                i = j;
                continue;
            }
        }
        if sp.decorator && c == '@' && i + 1 < n && is_ident_start(s[i + 1]) {
            out.emit(i, St::Decorator);
            let mut j = i + 1;
            while j < n && (is_ident(s[j]) || s[j] == '.') {
                j += 1;
            }
            i = j;
            continue;
        }
        if sp.css {
            let inside = *level > 0;
            if c == '@' {
                out.emit(i, St::Directive);
                let mut j = i + 1;
                while j < n && (is_ident(s[j]) || s[j] == '-') {
                    j += 1;
                }
                i = j;
                continue;
            }
            if !inside && (c == '.' || c == '#' || c == ':') && i + 1 < n && (is_ident_start(s[i + 1]) || s[i + 1] == '-' || s[i + 1] == ':') {
                out.emit(i, if c == '.' { St::CssClass } else if c == '#' { St::CssId } else { St::Pseudo });
                let mut j = i + 1;
                while j < n && (is_ident(s[j]) || s[j] == '-' || s[j] == ':') {
                    j += 1;
                }
                i = j;
                continue;
            }
            if inside && c == '#' {
                out.emit(i, St::Number);
                let mut j = i + 1;
                while j < n && s[j].is_ascii_hexdigit() {
                    j += 1;
                }
                i = j;
                continue;
            }
            if c == '!' && starts_with_nocase(s, i, "!important") {
                out.emit(i, St::Important);
                i += 10;
                continue;
            }
        }
        if is_ident_start(c) || (c == '$' && sp.lang == Lang::JavaScript) || (sp.lang == Lang::ObjC && c == '@') || (sp.css && c == '-') {
            let mut j = i + 1;
            while j < n && (is_ident(s[j]) || (sp.css && s[j] == '-') || (sp.lang == Lang::Ruby && s[j] == '?')) {
                j += 1;
            }
            let word: String = s[i..j].iter().collect();
            // Python string prefixes like r"", f'', b""
            if sp.triple && j < n && (s[j] == '"' || s[j] == '\'') && word.len() <= 2 && word.chars().all(|ch| "rRbBfFuU".contains(ch)) {
                out.emit(i, St::String);
                i = j;
                continue;
            }
            let lw = if sp.nocase { word.to_ascii_lowercase() } else { word.clone() };
            let st = if sp.css {
                if *level > 0 {
                    let mut k = j;
                    while k < n && s[k] == ' ' {
                        k += 1;
                    }
                    if k < n && s[k] == ':' {
                        St::Property
                    } else {
                        St::Value
                    }
                } else {
                    St::Tag
                }
            } else if kw.kw.contains(&lw) {
                St::Keyword
            } else if kw.ty.contains(&lw) {
                St::Type
            } else if kw.bi.contains(&lw) {
                St::Builtin
            } else if prev_word == "def" && sp.lang == Lang::Python {
                St::FuncName
            } else if prev_word == "class" && sp.lang == Lang::Python {
                St::ClassName
            } else if sp.rust && j < n && s[j] == '!' {
                j += 1;
                St::Macro
            } else if sp.lang == Lang::PowerShell && word.contains('-') {
                St::Type
            } else {
                St::Default
            };
            out.emit(i, st);
            prev_word = word;
            i = j;
            continue;
        }
        if c == '{' || (sp.fold_square && c == '[') {
            *level += 1;
        } else if c == '}' || (sp.fold_square && c == ']') {
            *level -= 1;
        }
        if c.is_whitespace() {
            out.emit(i, St::Default);
        } else if "+-*/%=<>!&|^~?:;,.(){}[]@\\".contains(c) {
            out.emit(i, St::Operator);
        } else {
            out.emit(i, St::Default);
        }
        i += 1;
    }
    M_NORMAL
}

const VOID_TAGS: &[&str] = &["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source", "track", "wbr", "!doctype"];

fn lex_markup(sp: &Spec, s: &[char], state: u32, level: &mut i32, out: &mut Out) -> u32 {
    let n = s.len();
    let html = sp.lang == Lang::Html;
    let mut i = 0usize;
    let mut embed = (state >> 24) & 0x3; // 1 = script, 2 = style
    let mut tagflag = (state >> 16) & 0x3;
    let mut m = mode(state);
    let mut sub_state = if embed != 0 { state & 0xFFFF } else { 0 };
    // brace depth inside an embedded <script>/<style> block (kept apart from the tag depth)
    let mut emb_depth = if embed != 0 { ((state >> 16) & 0xFF) as i32 } else { 0 };
    let text_st = St::Text;
    loop {
        if i >= n {
            break;
        }
        if embed != 0 && m <= M_RAW {
            let close = if embed == 1 { "</script" } else { "</style" };
            let mut end = n;
            for k in i..n {
                if starts_with_nocase(s, k, close) {
                    end = k;
                    break;
                }
            }
            let lang = if embed == 1 { Lang::JavaScript } else { Lang::Css };
            let esp = spec(lang);
            let ekw = keywords(lang);
            let mut sub = Out { runs: out.runs.as_deref_mut(), base: out.base + i };
            let mut lv = emb_depth;
            let st = lex_clike(esp, ekw, &s[i..end], sub_state, &mut lv, &mut sub);
            *level += lv - emb_depth;
            emb_depth = lv.clamp(0, 255);
            if end == n {
                return (embed << 24) | ((emb_depth as u32) << 16) | (st & 0xFFFF);
            }
            embed = 0;
            m = M_NORMAL;
            i = end;
            continue;
        }
        match m {
            M_HCOMMENT => {
                out.emit(i, St::Comment);
                match find(s, i, "-->") {
                    Some(e) => {
                        i = e + 3;
                        m = M_NORMAL;
                    }
                    None => return M_HCOMMENT,
                }
            }
            M_CDATA => {
                out.emit(i, St::CData);
                match find(s, i, "]]>") {
                    Some(e) => {
                        i = e + 3;
                        m = M_NORMAL;
                    }
                    None => return M_CDATA,
                }
            }
            M_PI => {
                out.emit(i, St::XmlDecl);
                match find(s, i, "?>") {
                    Some(e) => {
                        i = e + 2;
                        m = M_NORMAL;
                    }
                    None => return M_PI,
                }
            }
            M_SGML => {
                out.emit(i, St::Sgml);
                match s[i..].iter().position(|&c| c == '>') {
                    Some(e) => {
                        i += e + 1;
                        m = M_NORMAL;
                    }
                    None => return M_SGML,
                }
            }
            M_ATTR_DQ | M_ATTR_SQ => {
                let q = if m == M_ATTR_DQ { '"' } else { '\'' };
                out.emit(i, St::AttrValue);
                match s[i..].iter().position(|&c| c == q) {
                    Some(e) => {
                        i += e + 1;
                        m = M_TAG;
                    }
                    None => return m | (tagflag << 16),
                }
            }
            M_TAG => {
                let c = s[i];
                if c == '>' {
                    out.emit(i, St::Tag);
                    i += 1;
                    m = M_NORMAL;
                    if html && tagflag != 0 {
                        embed = tagflag;
                        sub_state = 0;
                        emb_depth = 0;
                    }
                    tagflag = 0;
                } else if c == '/' && i + 1 < n && s[i + 1] == '>' {
                    out.emit(i, St::Tag);
                    i += 2;
                    m = M_NORMAL;
                    *level -= 1;
                    tagflag = 0;
                } else if c == '"' {
                    out.emit(i, St::AttrValue);
                    i += 1;
                    m = M_ATTR_DQ;
                } else if c == '\'' {
                    out.emit(i, St::AttrValue);
                    i += 1;
                    m = M_ATTR_SQ;
                } else if c == '=' {
                    out.emit(i, St::Default);
                    i += 1;
                } else if c.is_whitespace() {
                    out.emit(i, St::Default);
                    i += 1;
                } else {
                    out.emit(i, St::Attr);
                    while i < n && !s[i].is_whitespace() && !"=>/\"'".contains(s[i]) {
                        i += 1;
                    }
                }
            }
            _ => {
                let c = s[i];
                if c == '<' {
                    if starts_with(s, i, "<!--") {
                        out.emit(i, St::Comment);
                        m = M_HCOMMENT;
                        i += 4;
                        continue;
                    }
                    if starts_with(s, i, "<![CDATA[") {
                        out.emit(i, St::CData);
                        m = M_CDATA;
                        i += 9;
                        continue;
                    }
                    if starts_with(s, i, "<?") {
                        out.emit(i, St::XmlDecl);
                        m = M_PI;
                        i += 2;
                        continue;
                    }
                    if starts_with(s, i, "<!") {
                        out.emit(i, St::Sgml);
                        m = M_SGML;
                        i += 2;
                        continue;
                    }
                    let closing = i + 1 < n && s[i + 1] == '/';
                    let mut j = i + 1 + closing as usize;
                    let name_start = j;
                    while j < n && !s[j].is_whitespace() && s[j] != '>' && s[j] != '/' {
                        j += 1;
                    }
                    let name: String = s[name_start..j].iter().collect::<String>().to_ascii_lowercase();
                    out.emit(i, St::Tag);
                    if closing {
                        *level -= 1;
                    } else if !(html && VOID_TAGS.contains(&name.as_str())) && !name.is_empty() {
                        *level += 1;
                    }
                    tagflag = if !closing && html && name == "script" {
                        1
                    } else if !closing && html && name == "style" {
                        2
                    } else {
                        0
                    };
                    i = j;
                    m = M_TAG;
                } else if c == '&' {
                    out.emit(i, St::Entity);
                    let mut j = i + 1;
                    while j < n && (s[j].is_alphanumeric() || s[j] == '#') {
                        j += 1;
                    }
                    if j < n && s[j] == ';' {
                        j += 1;
                    }
                    i = j;
                } else {
                    out.emit(i, if c.is_whitespace() { St::Default } else { text_st });
                    i += 1;
                }
            }
        }
    }
    if embed != 0 {
        return embed << 24;
    }
    m | (tagflag << 16)
}

fn lex_ini(sp: &Spec, kw: &KwSets, s: &[char], out: &mut Out) {
    let first = s.iter().position(|c| !c.is_whitespace());
    let Some(f) = first else {
        out.emit(0, St::Default);
        return;
    };
    let c = s[f];
    if sp.line_comment.iter().any(|t| starts_with(s, f, t)) {
        out.emit(0, St::Comment);
        return;
    }
    if c == '[' {
        out.emit(0, St::Section);
        return;
    }
    let sep = s.iter().position(|&c| c == '=' || (sp.lang == Lang::Properties && c == ':'));
    match sep {
        Some(e) => {
            out.emit(0, St::Key);
            out.emit(e, St::Assign);
            if sp.lang == Lang::Toml {
                let tsp = spec(Lang::Json);
                let mut lv = 0;
                let mut sub = Out { runs: out.runs.as_deref_mut(), base: e + 1 };
                lex_clike(tsp, kw, &s[e + 1..], 0, &mut lv, &mut sub);
            } else if e + 1 < s.len() {
                out.emit(e + 1, St::Default);
            }
        }
        None => out.emit(0, St::Default),
    }
}

fn lex_batch(kw: &KwSets, s: &[char], out: &mut Out) {
    let n = s.len();
    let f = s.iter().position(|c| !c.is_whitespace()).unwrap_or(n);
    let mut g = f;
    if g < n && s[g] == '@' {
        g += 1;
    }
    if starts_with(s, g, "::") || (starts_with_nocase(s, g, "rem") && (g + 3 >= n || s[g + 3].is_whitespace())) {
        out.emit(0, St::Comment);
        return;
    }
    if f < n && s[f] == ':' {
        out.emit(0, St::Label);
        return;
    }
    let mut i = 0;
    while i < n {
        let c = s[i];
        if c == '%' || c == '!' {
            let mut j = i + 1;
            if j < n && (s[j].is_ascii_digit() || s[j] == '*') {
                out.emit(i, St::Variable);
                i = j + 1;
                continue;
            }
            if j < n && s[j] == '~' {
                while j < n && (s[j].is_alphanumeric() || s[j] == '~' || s[j] == '$' || s[j] == ':') {
                    j += 1;
                }
                out.emit(i, St::Variable);
                i = j;
                continue;
            }
            if let Some(e) = s[j..].iter().position(|&x| x == c) {
                if e > 0 && s[j..j + e].iter().all(|x| !x.is_whitespace()) {
                    out.emit(i, St::Variable);
                    i = j + e + 1;
                    continue;
                }
            }
            if c == '%' && j < n && s[j] == '%' {
                j += 1;
                while j < n && is_ident(s[j]) {
                    j += 1;
                }
                out.emit(i, St::Variable);
                i = j;
                continue;
            }
            out.emit(i, St::Operator);
            i += 1;
            continue;
        }
        if is_ident_start(c) {
            let mut j = i;
            while j < n && (is_ident(s[j]) || s[j] == '.' || s[j] == '-') {
                j += 1;
            }
            let w: String = s[i..j].iter().collect::<String>().to_ascii_lowercase();
            out.emit(i, if kw.kw.contains(&w) { St::Keyword } else { St::Default });
            i = j;
            continue;
        }
        if "@<>|&()".contains(c) {
            out.emit(i, St::Operator);
        } else {
            out.emit(i, St::Default);
        }
        i += 1;
    }
}

fn lex_markdown(s: &[char], state: u32, out: &mut Out) -> u32 {
    let n = s.len();
    let f = s.iter().position(|c| !c.is_whitespace()).unwrap_or(n);
    if mode(state) == M_FENCE {
        out.emit(0, St::Code);
        return if starts_with(s, f, "```") || starts_with(s, f, "~~~") { 0 } else { M_FENCE };
    }
    if starts_with(s, f, "```") || starts_with(s, f, "~~~") {
        out.emit(0, St::Code);
        return M_FENCE;
    }
    if f < n && s[f] == '#' {
        out.emit(0, St::Heading);
        return 0;
    }
    if f < n && s[f] == '>' {
        out.emit(0, St::Quote);
        return 0;
    }
    if n >= 3 && f < n && (s[f..].iter().all(|&c| c == '-' || c == ' ') || s[f..].iter().all(|&c| c == '=' || c == ' ')) && s[f..].iter().filter(|c| !c.is_whitespace()).count() >= 3 {
        out.emit(0, St::Heading);
        return 0;
    }
    let mut i = 0;
    if f < n && (s[f] == '-' || s[f] == '*' || s[f] == '+') && f + 1 < n && s[f + 1] == ' ' {
        out.emit(f, St::ListMark);
        i = f + 1;
    } else if f < n && s[f].is_ascii_digit() {
        let mut j = f;
        while j < n && s[j].is_ascii_digit() {
            j += 1;
        }
        if j + 1 < n && (s[j] == '.' || s[j] == ')') && s[j + 1] == ' ' {
            out.emit(f, St::ListMark);
            i = j + 1;
        }
    }
    while i < n {
        let c = s[i];
        if c == '`' {
            if let Some(e) = s[i + 1..].iter().position(|&x| x == '`') {
                out.emit(i, St::Code);
                i += e + 2;
                continue;
            }
        }
        if (c == '*' || c == '_') && i + 1 < n && s[i + 1] == c {
            let pat: String = [c, c].iter().collect();
            if let Some(e) = find(s, i + 2, &pat) {
                out.emit(i, St::Strong);
                i = e + 2;
                continue;
            }
        }
        if (c == '*' || c == '_') && i + 1 < n && !s[i + 1].is_whitespace() && (c == '*' || i == 0 || !is_ident(s[i - 1])) {
            if let Some(e) = s[i + 1..].iter().position(|&x| x == c) {
                out.emit(i, St::Emph);
                i += e + 2;
                continue;
            }
        }
        if c == '[' {
            if let Some(e) = s[i..].iter().position(|&x| x == ']') {
                let close = i + e;
                if close + 1 < n && s[close + 1] == '(' {
                    if let Some(p) = s[close..].iter().position(|&x| x == ')') {
                        out.emit(i, St::Link);
                        i = close + p + 1;
                        continue;
                    }
                }
            }
        }
        if c == '<' && starts_with(s, i, "<!--") {
            out.emit(i, St::Comment);
            match find(s, i, "-->") {
                Some(e) => {
                    i = e + 3;
                    continue;
                }
                None => return 0,
            }
        }
        out.emit(i, St::Default);
        i += 1;
    }
    0
}

fn lex_yaml(sp: &Spec, kw: &KwSets, s: &[char], out: &mut Out) {
    let n = s.len();
    let f = s.iter().position(|c| !c.is_whitespace()).unwrap_or(n);
    if f >= n {
        return;
    }
    if s[f] == '#' {
        out.emit(0, St::Comment);
        return;
    }
    if starts_with(s, 0, "---") || starts_with(s, 0, "...") {
        out.emit(0, St::Directive);
        return;
    }
    let mut i = f;
    if s[i] == '-' && (i + 1 >= n || s[i + 1] == ' ') {
        out.emit(i, St::Operator);
        i += 1;
        while i < n && s[i] == ' ' {
            i += 1;
        }
    }
    // key: value
    let mut j = i;
    let mut in_q: Option<char> = None;
    let mut key_end = None;
    while j < n {
        let c = s[j];
        match in_q {
            Some(q) => {
                if c == q {
                    in_q = None;
                }
            }
            None => {
                if c == '"' || c == '\'' {
                    in_q = Some(c);
                } else if c == ':' && (j + 1 >= n || s[j + 1] == ' ') {
                    key_end = Some(j);
                    break;
                } else if c == '#' && j > 0 && s[j - 1] == ' ' {
                    break;
                }
            }
        }
        j += 1;
    }
    if let Some(e) = key_end {
        out.emit(i, St::Key);
        out.emit(e, St::Operator);
        i = e + 1;
    }
    // value
    let rest_start = i;
    while i < n && s[i] == ' ' {
        i += 1;
    }
    if i >= n {
        return;
    }
    let comment = (i..n).find(|&k| s[k] == '#' && k > 0 && s[k - 1] == ' ');
    let vend = comment.unwrap_or(n);
    let val: String = s[i..vend].iter().collect::<String>().trim_end().to_string();
    let st = if val.starts_with('"') || val.starts_with('\'') {
        St::String
    } else if val.starts_with('&') || val.starts_with('*') {
        St::Variable
    } else if kw.kw.contains(&val) {
        St::Keyword
    } else if !val.is_empty() && val.parse::<f64>().is_ok() {
        St::Number
    } else if val == "|" || val == ">" || val == "|-" || val == ">-" {
        St::Operator
    } else {
        St::Default
    };
    let _ = sp;
    out.emit(rest_start, St::Default);
    out.emit(i, st);
    if let Some(c) = comment {
        out.emit(c, St::Comment);
    }
}

#[cfg(test)]
impl Highlighter {
    /// Fully validated (state, level) per line start, for consistency tests.
    pub fn snapshot(&mut self, text: &Rope, lang: Lang) -> Vec<(u32, i32)> {
        let n = text.len_lines();
        self.ensure(text, lang, n);
        self.starts.iter().copied().zip(self.levels.iter().copied()).collect()
    }
}
