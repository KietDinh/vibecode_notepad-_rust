//! Document model: rope text, selections, undo/redo, bookmarks, change history.

use crate::highlight::Highlighter;
use crate::lang::Lang;
use ropey::Rope;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::SystemTime;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Eol {
    Crlf,
    Lf,
    Cr,
}

impl Eol {
    pub fn as_str(self) -> &'static str {
        match self {
            Eol::Crlf => "\r\n",
            Eol::Lf => "\n",
            Eol::Cr => "\r",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Eol::Crlf => "Windows (CR LF)",
            Eol::Lf => "Unix (LF)",
            Eol::Cr => "Macintosh (CR)",
        }
    }
    pub fn code(self) -> u8 {
        match self {
            Eol::Crlf => 0,
            Eol::Lf => 1,
            Eol::Cr => 2,
        }
    }
    pub fn from_code(c: u8) -> Eol {
        match c {
            1 => Eol::Lf,
            2 => Eol::Cr,
            _ => Eol::Crlf,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Encoding {
    Ansi,
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
}

impl Encoding {
    pub fn label(self) -> &'static str {
        match self {
            Encoding::Ansi => "ANSI",
            Encoding::Utf8 => "UTF-8",
            Encoding::Utf8Bom => "UTF-8-BOM",
            Encoding::Utf16Le => "UTF-16 LE BOM",
            Encoding::Utf16Be => "UTF-16 BE BOM",
        }
    }
    pub fn code(self) -> u8 {
        self as u8
    }
    pub fn from_code(c: u8) -> Encoding {
        match c {
            0 => Encoding::Ansi,
            2 => Encoding::Utf8Bom,
            3 => Encoding::Utf16Le,
            4 => Encoding::Utf16Be,
            _ => Encoding::Utf8,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Sel {
    pub anchor: usize,
    pub caret: usize,
    /// Remembered visual column for vertical movement.
    pub vcol: Option<usize>,
}

impl Sel {
    pub fn at(p: usize) -> Sel {
        Sel { anchor: p, caret: p, vcol: None }
    }
    pub fn new(anchor: usize, caret: usize) -> Sel {
        Sel { anchor, caret, vcol: None }
    }
    pub fn start(&self) -> usize {
        self.anchor.min(self.caret)
    }
    pub fn end(&self) -> usize {
        self.anchor.max(self.caret)
    }
    pub fn is_empty(&self) -> bool {
        self.anchor == self.caret
    }
}

/// Rectangular (column) selection expressed in visual columns.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RectSel {
    pub anchor_line: usize,
    pub anchor_col: usize,
    pub caret_line: usize,
    pub caret_col: usize,
}

#[derive(Clone, Debug)]
pub struct Edit {
    pub pos: usize,
    pub del: String,
    pub ins: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EditKind {
    Typing,
    Other,
}

#[derive(Clone, Debug)]
struct Group {
    edits: Vec<Edit>,
    before: Vec<Sel>,
    after: Vec<Sel>,
    id: u64,
    kind: EditKind,
}

#[derive(Default)]
struct Undo {
    groups: Vec<Group>,
    pos: usize,
    next_id: u64,
    open: Option<Group>,
    depth: u32,
    can_coalesce: bool,
}

/// Per-line change-history marker (Notepad++ 8.5+ style).
pub const CH_NONE: u8 = 0;
pub const CH_MODIFIED: u8 = 1;
pub const CH_SAVED: u8 = 2;

pub struct Document {
    pub id: u64,
    pub text: Rope,
    pub path: Option<PathBuf>,
    pub untitled_name: String,
    pub eol: Eol,
    pub encoding: Encoding,
    pub lang: Lang,
    pub sels: Vec<Sel>,
    pub main: usize,
    pub rect: Option<RectSel>,
    pub bookmarks: BTreeSet<usize>,
    pub folded: BTreeSet<usize>,
    pub change: Vec<u8>,
    pub scroll_row: usize,
    pub scroll_x: i32,
    pub read_only: bool,
    pub mtime: Option<SystemTime>,
    pub version: u64,
    pub hl: Highlighter,
    pub tab_width: usize,
    /// Marked char ranges: (start, end, style). Style 0 = find mark, 1..=5 = token styles, 6 = search hit.
    pub marks: Vec<(usize, usize, u8)>,
    /// Version at which a backup snapshot was last written.
    pub backup_version: u64,
    pub backup_file: Option<PathBuf>,
    pub pinned: bool,
    undo: Undo,
    saved_state: u64,
}

static NEXT_DOC_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

pub fn char_cells(c: char) -> usize {
    let u = c as u32;
    if u < 0x20 || u == 0x7F {
        return ctrl_name(c).len();
    }
    if (0x1100..=0x115F).contains(&u)
        || (0x2E80..=0xA4CF).contains(&u)
        || (0xAC00..=0xD7A3).contains(&u)
        || (0xF900..=0xFAFF).contains(&u)
        || (0xFE30..=0xFE4F).contains(&u)
        || (0xFF00..=0xFF60).contains(&u)
        || (0xFFE0..=0xFFE6).contains(&u)
        || (0x1F300..=0x1F64F).contains(&u)
        || (0x1F900..=0x1F9FF).contains(&u)
        || (0x20000..=0x3FFFD).contains(&u)
    {
        2
    } else {
        1
    }
}

pub fn ctrl_name(c: char) -> &'static str {
    const N: [&str; 32] = [
        "NUL", "SOH", "STX", "ETX", "EOT", "ENQ", "ACK", "BEL", "BS", "HT", "LF", "VT", "FF", "CR", "SO", "SI", "DLE",
        "DC1", "DC2", "DC3", "DC4", "NAK", "SYN", "ETB", "CAN", "EM", "SUB", "ESC", "FS", "GS", "RS", "US",
    ];
    let u = c as u32;
    if u < 32 {
        N[u as usize]
    } else if u == 0x7F {
        "DEL"
    } else {
        ""
    }
}

pub fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Character class used for word navigation (Scintilla-like).
pub fn char_class(c: char) -> u8 {
    if c == '\n' {
        1
    } else if c.is_whitespace() {
        0
    } else if is_word_char(c) {
        2
    } else {
        3
    }
}

impl Document {
    pub fn new(untitled_name: String) -> Self {
        let mut d = Document {
            id: NEXT_DOC_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            text: Rope::new(),
            path: None,
            untitled_name,
            eol: Eol::Crlf,
            encoding: Encoding::Utf8,
            lang: Lang::Text,
            sels: vec![Sel::at(0)],
            main: 0,
            rect: None,
            bookmarks: BTreeSet::new(),
            folded: BTreeSet::new(),
            change: vec![CH_NONE],
            scroll_row: 0,
            scroll_x: 0,
            read_only: false,
            mtime: None,
            version: 1,
            hl: Highlighter::default(),
            tab_width: 4,
            marks: Vec::new(),
            backup_version: 0,
            backup_file: None,
            pinned: false,
            undo: Undo { next_id: 1, ..Default::default() },
            saved_state: 0,
        };
        d.hl.reset(1);
        d
    }

    /// Replace the whole content without recording undo (used when loading).
    pub fn set_content(&mut self, s: &str) {
        self.text = Rope::from_str(s);
        self.sels = vec![Sel::at(0)];
        self.main = 0;
        self.rect = None;
        self.bookmarks.clear();
        self.folded.clear();
        self.marks.clear();
        self.change = vec![CH_NONE; self.text.len_lines()];
        self.undo = Undo { next_id: self.undo.next_id, ..Default::default() };
        self.saved_state = 0;
        self.version += 1;
        self.hl.reset(self.text.len_lines());
    }

    pub fn title(&self) -> String {
        match &self.path {
            Some(p) => p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
            None => self.untitled_name.clone(),
        }
    }

    pub fn full_title(&self) -> String {
        match &self.path {
            Some(p) => p.to_string_lossy().into_owned(),
            None => self.untitled_name.clone(),
        }
    }

    // ---------- text queries ----------
    pub fn len(&self) -> usize {
        self.text.len_chars()
    }
    pub fn line_count(&self) -> usize {
        self.text.len_lines()
    }
    pub fn line_of(&self, pos: usize) -> usize {
        self.text.char_to_line(pos.min(self.len()))
    }
    pub fn line_start(&self, line: usize) -> usize {
        self.text.line_to_char(line.min(self.line_count()))
    }
    /// Position of the end of `line`, excluding the newline.
    pub fn line_end(&self, line: usize) -> usize {
        if line + 1 >= self.line_count() {
            self.len()
        } else {
            self.text.line_to_char(line + 1) - 1
        }
    }
    pub fn line_len(&self, line: usize) -> usize {
        self.line_end(line) - self.line_start(line)
    }
    pub fn line_text(&self, line: usize) -> String {
        let s = self.line_start(line);
        let e = self.line_end(line);
        self.text.slice(s..e).to_string()
    }
    pub fn line_chars(&self, line: usize) -> Vec<char> {
        let s = self.line_start(line);
        let e = self.line_end(line);
        self.text.slice(s..e).chars().collect()
    }
    pub fn char_at(&self, pos: usize) -> Option<char> {
        if pos < self.len() {
            Some(self.text.char(pos))
        } else {
            None
        }
    }
    pub fn slice(&self, a: usize, b: usize) -> String {
        let a = a.min(self.len());
        let b = b.min(self.len()).max(a);
        self.text.slice(a..b).to_string()
    }

    /// Visual column of `pos` (tabs and wide chars expanded).
    pub fn col_of(&self, pos: usize) -> usize {
        let line = self.line_of(pos);
        let start = self.line_start(line);
        let mut col = 0;
        for c in self.text.slice(start..pos.min(self.len())).chars() {
            col = self.advance(col, c);
        }
        col
    }

    pub fn advance(&self, col: usize, c: char) -> usize {
        if c == '\t' {
            (col / self.tab_width + 1) * self.tab_width
        } else {
            col + char_cells(c)
        }
    }

    /// Nearest position on `line` to visual column `col`.
    pub fn pos_at_col(&self, line: usize, col: usize) -> usize {
        let start = self.line_start(line);
        let end = self.line_end(line);
        let mut c0 = 0;
        let mut pos = start;
        for ch in self.text.slice(start..end).chars() {
            let c1 = self.advance(c0, ch);
            if col < c1 {
                return if col - c0 > (c1 - c0) / 2 || (c1 - c0 == 1 && col > c0) { pos + 1 } else { pos };
            }
            c0 = c1;
            pos += 1;
        }
        end
    }

    /// Leading whitespace of a line in visual columns.
    pub fn indent_of(&self, line: usize) -> usize {
        let mut col = 0;
        let s = self.line_start(line);
        let e = self.line_end(line);
        for c in self.text.slice(s..e).chars() {
            match c {
                ' ' | '\t' => col = self.advance(col, c),
                _ => break,
            }
        }
        col
    }

    pub fn is_blank_line(&self, line: usize) -> bool {
        let s = self.line_start(line);
        let e = self.line_end(line);
        self.text.slice(s..e).chars().all(|c| c == ' ' || c == '\t')
    }

    pub fn word_at(&self, pos: usize) -> (usize, usize) {
        let mut a = pos;
        let mut b = pos;
        while a > 0 && self.char_at(a - 1).is_some_and(is_word_char) {
            a -= 1;
        }
        while self.char_at(b).is_some_and(is_word_char) {
            b += 1;
        }
        (a, b)
    }

    pub fn word_left(&self, pos: usize) -> usize {
        let mut p = pos;
        while p > 0 && char_class(self.text.char(p - 1)) == 0 {
            p -= 1;
        }
        if p == 0 {
            return 0;
        }
        let cls = char_class(self.text.char(p - 1));
        if cls == 1 {
            return p - 1;
        }
        while p > 0 && char_class(self.text.char(p - 1)) == cls {
            p -= 1;
        }
        p
    }

    pub fn word_right(&self, pos: usize) -> usize {
        let n = self.len();
        let mut p = pos;
        if p >= n {
            return n;
        }
        let cls = char_class(self.text.char(p));
        if cls == 1 {
            return p + 1;
        }
        while p < n && char_class(self.text.char(p)) == cls {
            p += 1;
        }
        while p < n && char_class(self.text.char(p)) == 0 {
            p += 1;
        }
        p
    }

    // ---------- selections ----------
    pub fn main_sel(&self) -> Sel {
        self.sels[self.main.min(self.sels.len() - 1)]
    }
    pub fn caret(&self) -> usize {
        self.main_sel().caret
    }
    pub fn set_sel(&mut self, anchor: usize, caret: usize) {
        let n = self.len();
        self.sels = vec![Sel::new(anchor.min(n), caret.min(n))];
        self.main = 0;
        self.rect = None;
        self.undo.can_coalesce = false;
    }
    pub fn set_caret(&mut self, p: usize) {
        self.set_sel(p, p);
    }
    pub fn has_selection(&self) -> bool {
        self.sels.iter().any(|s| !s.is_empty())
    }
    pub fn selected_text(&self) -> String {
        let mut v: Vec<Sel> = self.sels.clone();
        v.sort_by_key(|s| s.start());
        let parts: Vec<String> = v.iter().filter(|s| !s.is_empty()).map(|s| self.slice(s.start(), s.end())).collect();
        if self.rect.is_some() {
            let mut out = parts.join("\n");
            if !out.is_empty() {
                out.push('\n');
            }
            out
        } else {
            parts.join("\n")
        }
    }

    pub fn break_coalesce(&mut self) {
        self.undo.can_coalesce = false;
    }

    /// Sort and merge overlapping selections, keeping track of the main one.
    pub fn normalize_sels(&mut self) {
        let n = self.len();
        for s in &mut self.sels {
            s.anchor = s.anchor.min(n);
            s.caret = s.caret.min(n);
        }
        if self.sels.len() <= 1 {
            self.main = 0;
            return;
        }
        let main_caret = self.main_sel().caret;
        let mut v = std::mem::take(&mut self.sels);
        v.sort_by_key(|s| (s.start(), s.end()));
        let mut out: Vec<Sel> = Vec::with_capacity(v.len());
        for s in v {
            if let Some(last) = out.last_mut() {
                let overlap = s.start() < last.end() || s.start() == last.start();
                if overlap {
                    let a = last.start().min(s.start());
                    let b = last.end().max(s.end());
                    let forward = last.caret >= last.anchor;
                    *last = if forward { Sel::new(a, b) } else { Sel::new(b, a) };
                    continue;
                }
            }
            out.push(s);
        }
        self.main = out.iter().position(|s| s.caret == main_caret || (s.start() <= main_caret && main_caret <= s.end())).unwrap_or(out.len() - 1);
        self.sels = out;
    }

    /// Rebuild `sels` from the rectangular selection.
    pub fn apply_rect(&mut self) {
        let Some(r) = self.rect else { return };
        let (l0, l1) = (r.anchor_line.min(r.caret_line), r.anchor_line.max(r.caret_line));
        let mut sels = Vec::new();
        for line in l0..=l1.min(self.line_count() - 1) {
            let a = self.pos_at_col(line, r.anchor_col);
            let c = self.pos_at_col(line, r.caret_col);
            sels.push(Sel::new(a, c));
        }
        self.main = if r.caret_line >= r.anchor_line { sels.len() - 1 } else { 0 };
        self.sels = sels;
        self.undo.can_coalesce = false;
    }

    // ---------- editing primitives ----------
    fn raw_replace(&mut self, pos: usize, del_len: usize, ins: &str) -> String {
        let pos = pos.min(self.len());
        let end = (pos + del_len).min(self.len());
        let deleted: String = self.text.slice(pos..end).to_string();
        let line = self.text.char_to_line(pos);
        let removed = deleted.bytes().filter(|&b| b == b'\n').count();
        let added = ins.bytes().filter(|&b| b == b'\n').count();
        if end > pos {
            self.text.remove(pos..end);
        }
        if !ins.is_empty() {
            self.text.insert(pos, ins);
        }
        self.version += 1;
        self.hl.on_edit(line, removed, added);
        shift_set(&mut self.bookmarks, line, removed, added, true);
        shift_set(&mut self.folded, line, removed, added, false);
        let hi = (line + removed + 1).min(self.change.len());
        let lo = line.min(hi);
        self.change.splice(lo..hi, std::iter::repeat(CH_MODIFIED).take(added + 1));
        let nl = self.text.len_lines();
        self.change.resize(nl, CH_NONE);
        if !self.marks.is_empty() {
            let delta = ins.chars().count() as isize - (end - pos) as isize;
            self.marks.retain(|&(a, b, _)| b <= pos || a >= end);
            for m in &mut self.marks {
                if m.0 >= end {
                    m.0 = (m.0 as isize + delta) as usize;
                    m.1 = (m.1 as isize + delta) as usize;
                }
            }
        }
        deleted
    }

    pub fn begin(&mut self, kind: EditKind) {
        if self.undo.depth > 0 {
            self.undo.depth += 1;
            return;
        }
        self.undo.depth = 1;
        let reopen = kind == EditKind::Typing
            && self.undo.can_coalesce
            && self.undo.pos == self.undo.groups.len()
            && self.undo.groups.last().is_some_and(|g| g.kind == EditKind::Typing && g.id != self.saved_state);
        if reopen {
            let g = self.undo.groups.pop().unwrap();
            self.undo.pos -= 1;
            self.undo.open = Some(g);
        } else {
            self.undo.open = Some(Group { edits: Vec::new(), before: self.sels.clone(), after: Vec::new(), id: 0, kind });
        }
    }

    pub fn end(&mut self) {
        if self.undo.depth == 0 {
            return;
        }
        self.undo.depth -= 1;
        if self.undo.depth > 0 {
            return;
        }
        if let Some(mut g) = self.undo.open.take() {
            if g.edits.is_empty() {
                return;
            }
            g.after = self.sels.clone();
            if g.id == 0 {
                g.id = self.undo.next_id;
                self.undo.next_id += 1;
            }
            let kind = g.kind;
            self.undo.groups.truncate(self.undo.pos);
            self.undo.groups.push(g);
            self.undo.pos = self.undo.groups.len();
            self.undo.can_coalesce = kind == EditKind::Typing;
        }
    }

    /// Replace `del_len` chars at `pos` with `ins`, recording undo. Must be inside begin/end.
    pub fn replace(&mut self, pos: usize, del_len: usize, ins: &str) {
        if self.read_only {
            return;
        }
        if del_len == 0 && ins.is_empty() {
            return;
        }
        let auto = self.undo.depth == 0;
        if auto {
            self.begin(EditKind::Other);
        }
        let del = self.raw_replace(pos, del_len, ins);
        if let Some(g) = self.undo.open.as_mut() {
            g.edits.push(Edit { pos, del, ins: ins.to_string() });
        }
        if auto {
            self.end();
        }
    }

    pub fn can_undo(&self) -> bool {
        self.undo.pos > 0
    }
    pub fn can_redo(&self) -> bool {
        self.undo.pos < self.undo.groups.len()
    }

    pub fn undo(&mut self) {
        if self.undo.pos == 0 || self.read_only {
            return;
        }
        let g = self.undo.groups[self.undo.pos - 1].clone();
        for e in g.edits.iter().rev() {
            self.raw_replace(e.pos, e.ins.chars().count(), &e.del);
        }
        self.undo.pos -= 1;
        self.sels = g.before;
        self.main = self.sels.len().saturating_sub(1);
        self.rect = None;
        self.undo.can_coalesce = false;
        self.normalize_sels();
    }

    pub fn redo(&mut self) {
        if self.undo.pos >= self.undo.groups.len() || self.read_only {
            return;
        }
        let g = self.undo.groups[self.undo.pos].clone();
        for e in &g.edits {
            self.raw_replace(e.pos, e.del.chars().count(), &e.ins);
        }
        self.undo.pos += 1;
        self.sels = g.after;
        self.main = self.sels.len().saturating_sub(1);
        self.rect = None;
        self.undo.can_coalesce = false;
        self.normalize_sels();
    }


    fn state_id(&self) -> u64 {
        if self.undo.pos == 0 {
            0
        } else {
            self.undo.groups[self.undo.pos - 1].id
        }
    }

    pub fn is_modified(&self) -> bool {
        self.state_id() != self.saved_state
    }

    pub fn mark_saved(&mut self) {
        self.saved_state = self.state_id();
        self.undo.can_coalesce = false;
        for c in &mut self.change {
            if *c == CH_MODIFIED {
                *c = CH_SAVED;
            }
        }
    }

    /// Force the document into the modified state (e.g. after encoding conversion).
    pub fn mark_dirty(&mut self) {
        self.saved_state = u64::MAX;
    }

    pub fn text_string(&self) -> String {
        self.text.to_string()
    }
}

/// Shift line-indexed set entries after an edit that removed `removed` and added `added` newlines at `line`.
fn shift_set(set: &mut BTreeSet<usize>, line: usize, removed: usize, added: usize, keep_merged: bool) {
    if removed == 0 && added == 0 {
        return;
    }
    if set.iter().all(|&l| l < line) {
        return;
    }
    let old: Vec<usize> = std::mem::take(set).into_iter().collect();
    for l in old {
        if l <= line {
            if !(l == line && !keep_merged && removed > 0) {
                set.insert(l);
            }
        } else if l <= line + removed {
            if keep_merged {
                set.insert(line);
            }
        } else {
            set.insert(l - removed + added);
        }
    }
}
