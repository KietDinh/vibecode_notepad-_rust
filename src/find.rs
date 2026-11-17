//! Search engine (Normal / Extended / Regex) and the tabbed Find / Replace / Find in Files / Mark dialog.

use crate::app::{app, ResultLoc};
use crate::config::push_history;
use crate::dialogs::*;
use crate::doc::{Document, EditKind, Sel};
use crate::win::*;
use regex::{Regex, RegexBuilder};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::SystemServices::SS_ENDELLIPSIS;
use windows_sys::Win32::UI::Controls::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

#[derive(Clone, Default)]
pub struct Opts {
    pub what: String,
    pub with: String,
    pub case: bool,
    pub word: bool,
    pub mode: u8,
    pub dot_nl: bool,
    pub wrap: bool,
    pub backward: bool,
    pub in_sel: bool,
}

pub fn unescape_extended(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('n') => out.push('\n'),
            Some('r') => {
                // "\r\n" means a single line break in our \n-normalized buffers
                let mut look = it.clone();
                if !(look.next() == Some('\\') && look.next() == Some('n')) {
                    out.push('\n');
                }
            }
            Some('t') => out.push('\t'),
            Some('0') => out.push('\0'),
            Some('\\') => out.push('\\'),
            Some('x') => {
                let h: String = (0..2).filter_map(|_| it.next_if(|c| c.is_ascii_hexdigit())).collect();
                if let Some(ch) = u32::from_str_radix(&h, 16).ok().and_then(char::from_u32) {
                    out.push(ch);
                }
            }
            Some('u') => {
                let h: String = (0..4).filter_map(|_| it.next_if(|c| c.is_ascii_hexdigit())).collect();
                if let Some(ch) = u32::from_str_radix(&h, 16).ok().and_then(char::from_u32) {
                    out.push(ch);
                }
            }
            Some(o) => {
                out.push('\\');
                out.push(o);
            }
            None => out.push('\\'),
        }
    }
    out
}

pub fn build_regex(o: &Opts) -> Result<Regex, String> {
    if o.what.is_empty() {
        return Err("empty".into());
    }
    let mut pat = match o.mode {
        0 => regex::escape(&o.what),
        1 => regex::escape(&unescape_extended(&o.what)),
        _ => o.what.replace("\\r\\n", "\\n").replace("\\r", "\\n"),
    };
    if o.word {
        pat = format!(r"\b(?:{pat})\b");
    }
    RegexBuilder::new(&pat)
        .case_insensitive(!o.case)
        .multi_line(true)
        .dot_matches_new_line(o.dot_nl && o.mode == 2)
        .crlf(false)
        .build()
        .map_err(|e| e.to_string())
}

thread_local! {
    static TEXT_CACHE: RefCell<(u64, u64, Rc<String>)> = RefCell::new((0, 0, Rc::new(String::new())));
}

pub fn doc_string(doc: &Document) -> Rc<String> {
    TEXT_CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if c.0 != doc.id || c.1 != doc.version {
            *c = (doc.id, doc.version, Rc::new(doc.text_string()));
        }
        c.2.clone()
    })
}

/// Expand a replacement string for a regex match (supports $1, ${name}, \1, \n, \t).
pub fn expand(caps: &regex::Captures, rep: &str, mode: u8) -> String {
    match mode {
        0 => rep.to_string(),
        1 => unescape_extended(rep),
        _ => {
            let mut out = String::new();
            let chars: Vec<char> = rep.chars().collect();
            let mut i = 0;
            while i < chars.len() {
                let c = chars[i];
                if c == '\\' && i + 1 < chars.len() {
                    let n = chars[i + 1];
                    match n {
                        'n' => out.push('\n'),
                        'r' => {
                            // "\r\n" is a single line break in our \n-normalized buffers
                            if !(i + 3 < chars.len() + 0 && chars.get(i + 2) == Some(&'\\') && chars.get(i + 3) == Some(&'n')) {
                                out.push('\n');
                            }
                        }
                        't' => out.push('\t'),
                        '\\' => out.push('\\'),
                        '0'..='9' => {
                            if let Some(m) = caps.get(n.to_digit(10).unwrap() as usize) {
                                out.push_str(m.as_str());
                            }
                        }
                        _ => {
                            out.push(n);
                        }
                    }
                    i += 2;
                    continue;
                }
                if c == '$' && i + 1 < chars.len() {
                    if chars[i + 1] == '{' {
                        if let Some(end) = chars[i + 2..].iter().position(|&x| x == '}') {
                            let name: String = chars[i + 2..i + 2 + end].iter().collect();
                            let m = name.parse::<usize>().ok().and_then(|k| caps.get(k)).or_else(|| caps.name(&name));
                            if let Some(m) = m {
                                out.push_str(m.as_str());
                            }
                            i += 3 + end;
                            continue;
                        }
                    } else if chars[i + 1].is_ascii_digit() {
                        let k = chars[i + 1].to_digit(10).unwrap() as usize;
                        if let Some(m) = caps.get(k) {
                            out.push_str(m.as_str());
                        }
                        i += 2;
                        continue;
                    }
                }
                out.push(c);
                i += 1;
            }
            out
        }
    }
}

/// Find the next match relative to the selection. Returns (start, end, wrapped) in chars.
pub fn find_from(doc: &Document, re: &Regex, sel: Sel, backward: bool, wrap: bool, range: Option<(usize, usize)>) -> Option<(usize, usize, bool)> {
    let text = doc_string(doc);
    let b2c = |b: usize| doc.text.byte_to_char(b);
    let (lo, hi) = byte_range(doc, range, text.len());
    let ok = |m: &regex::Match| m.start() >= lo && m.end() <= hi;
    if !backward {
        let mut from = doc.text.char_to_byte(sel.end()).max(lo);
        loop {
            if from > text.len() {
                break;
            }
            match re.find_at(&text, from) {
                Some(m) if ok(&m) => {
                    if m.start() == m.end() && m.start() == doc.text.char_to_byte(sel.end()) && sel.is_empty() && from == m.start() {
                        // skip an empty match at the caret
                        from = next_boundary(&text, from);
                        continue;
                    }
                    return Some((b2c(m.start()), b2c(m.end()), false));
                }
                _ => break,
            }
        }
        if wrap {
            if let Some(m) = re.find_at(&text, lo) {
                if ok(&m) {
                    return Some((b2c(m.start()), b2c(m.end()), true));
                }
            }
        }
        None
    } else {
        let end_b = doc.text.char_to_byte(sel.start());
        let mut best = None;
        let mut last_any = None;
        for m in re.find_iter(&text) {
            if m.start() >= hi {
                break;
            }
            if m.start() < lo || m.end() > hi {
                continue;
            }
            if m.start() < end_b && !(m.start() == m.end() && m.start() == end_b) {
                best = Some(m);
            }
            last_any = Some(m);
        }
        if let Some(m) = best {
            return Some((b2c(m.start()), b2c(m.end()), false));
        }
        if wrap {
            if let Some(m) = last_any {
                return Some((b2c(m.start()), b2c(m.end()), true));
            }
        }
        None
    }
}

/// Clamp an optional char range to the document and convert it to bytes.
fn byte_range(doc: &Document, range: Option<(usize, usize)>, len: usize) -> (usize, usize) {
    match range {
        Some((a, b)) => {
            let n = doc.len();
            let (a, b) = (a.min(n), b.min(n));
            (doc.text.char_to_byte(a.min(b)), doc.text.char_to_byte(b.max(a)))
        }
        None => (0, len),
    }
}

fn next_boundary(s: &str, b: usize) -> usize {
    let mut n = b + 1;
    while n < s.len() && !s.is_char_boundary(n) {
        n += 1;
    }
    n
}

pub fn find_all(doc: &Document, re: &Regex, range: Option<(usize, usize)>) -> Vec<(usize, usize)> {
    let text = doc_string(doc);
    let (lo, hi) = byte_range(doc, range, text.len());
    re.find_iter(&text).take_while(|m| m.start() < hi).filter(|m| m.start() >= lo && m.end() <= hi && m.end() > m.start()).map(|m| (doc.text.byte_to_char(m.start()), doc.text.byte_to_char(m.end()))).collect()
}

/// Replace all matches in a document (one undo step). Returns the count.
pub fn replace_all_in(doc: &mut Document, re: &Regex, o: &Opts, range: Option<(usize, usize)>) -> usize {
    if doc.read_only {
        return 0;
    }
    let text = doc_string(doc);
    let (lo, hi) = byte_range(doc, range, text.len());
    let mut reps: Vec<(usize, usize, String)> = Vec::new();
    for caps in re.captures_iter(&text) {
        let m = caps.get(0).unwrap();
        if m.start() >= hi {
            break;
        }
        if m.start() < lo || m.end() > hi || (m.start() == m.end() && o.mode != 2) {
            continue;
        }
        reps.push((m.start(), m.end(), expand(&caps, &o.with, o.mode)));
    }
    if reps.is_empty() {
        return 0;
    }
    let n = reps.len();
    let caret = doc.caret();
    doc.begin(EditKind::Other);
    if n <= 64 {
        for (a, b, r) in reps.into_iter().rev() {
            let ca = doc.text.byte_to_char(a);
            let cb = doc.text.byte_to_char(b);
            doc.replace(ca, cb - ca, &r);
        }
    } else if reps.iter().all(|(a, b, r)| !text[*a..*b].contains('\n') && !r.contains('\n')) {
        // Line structure unchanged: rewrite only the affected lines (keeps bookmarks, folds, change history exact).
        let mut k = reps.len();
        while k > 0 {
            let line = doc.text.byte_to_line(reps[k - 1].0);
            let lb = doc.text.line_to_byte(line);
            let mut j = k;
            while j > 0 && doc.text.byte_to_line(reps[j - 1].0) == line {
                j -= 1;
            }
            let le_char = doc.line_end(line);
            let le = doc.text.char_to_byte(le_char);
            let mut out = String::new();
            let mut cur = lb;
            for (a, b, r) in &reps[j..k] {
                out.push_str(&text[cur..*a]);
                out.push_str(r);
                cur = *b;
            }
            out.push_str(&text[cur..le]);
            let ls_char = doc.line_start(line);
            doc.replace(ls_char, le_char - ls_char, &out);
            k = j;
        }
    } else {
        // Build the replaced span in one pass and apply it as a single edit (fast on huge files),
        // then map bookmarks from old to new line numbers.
        let old_bm: Vec<usize> = doc.bookmarks.iter().copied().collect();
        let starts: Vec<usize> = reps.iter().map(|(a, _, _)| doc.text.byte_to_line(*a)).collect();
        let ends: Vec<usize> = reps.iter().map(|(_, b, _)| doc.text.byte_to_line(*b)).collect();
        let mut prefix = vec![0isize; n + 1];
        for (i, (a, b, r)) in reps.iter().enumerate() {
            let d = r.matches('\n').count() as isize - text[*a..*b].matches('\n').count() as isize;
            prefix[i + 1] = prefix[i] + d;
        }
        let new_bm: std::collections::BTreeSet<usize> = old_bm
            .iter()
            .map(|&l| {
                let k = ends.partition_point(|&e| e < l);
                let base = if k < n && starts[k] < l { starts[k] } else { l };
                (base as isize + prefix[k]).max(0) as usize
            })
            .collect();
        let span_lo = reps[0].0;
        let span_hi = reps[n - 1].1;
        let mut out = String::with_capacity(span_hi - span_lo);
        let mut cur = span_lo;
        for (a, b, r) in &reps {
            out.push_str(&text[cur..*a]);
            out.push_str(r);
            cur = *b;
        }
        let ca = doc.text.byte_to_char(span_lo);
        let cb = doc.text.byte_to_char(span_hi);
        doc.replace(ca, cb - ca, &out);
        let nl = doc.line_count();
        doc.bookmarks = new_bm.into_iter().filter(|&l| l < nl).collect();
    }
    doc.set_caret(caret.min(doc.len()));
    doc.end();
    n
}


// ------------------------------------------------------------------ actions

#[derive(Clone, Copy, PartialEq)]
pub enum StatusKind {
    Found,
    NotFound,
    Info,
    Wrapped,
}

pub fn set_status(msg: &str, kind: StatusKind) {
    let a = app();
    a.find.status_kind = kind;
    if !a.find.hwnd.is_null() {
        set_item_text(a.find.hwnd, ID_STATUS, msg);
    }
    a.set_status_text(msg);
}

/// Find next/previous in the active document. Returns true when found.
pub fn find_next(o: &Opts) -> bool {
    let re = match build_regex(o) {
        Ok(r) => r,
        Err(e) => {
            if !o.what.is_empty() {
                set_status(&format!("Find: Invalid regular expression ({})", e.lines().last().unwrap_or("")), StatusKind::NotFound);
            }
            return false;
        }
    };
    let a = app();
    let range = current_range(o);
    let doc = a.doc_mut();
    let sel = doc.main_sel();
    match find_from(doc, &re, sel, o.backward, o.wrap, range) {
        Some((s, e, wrapped)) => {
            doc.set_sel(if o.backward { e } else { s }, if o.backward { s } else { e });
            let line = doc.line_of(s);
            a.reveal_selection(line);
            if wrapped {
                set_status(
                    &format!("Find: Found the 1st occurrence from the {}. The {} of the document has been reached.", if o.backward { "bottom" } else { "top" }, if o.backward { "start" } else { "end" }),
                    StatusKind::Wrapped,
                );
            } else {
                set_status("", StatusKind::Found);
            }
            true
        }
        None => {
            set_status(&format!("Find: Can't find the text \"{}\"", o.what), StatusKind::NotFound);
            unsafe {
                windows_sys::Win32::System::Diagnostics::Debug::MessageBeep(MB_OK);
            }
            false
        }
    }
}

/// "In selection" range, read from the current selection when the button is pressed.
fn current_range(o: &Opts) -> Option<(usize, usize)> {
    if !o.in_sel {
        return None;
    }
    let a = app();
    let s = a.doc().main_sel();
    Some(a.find.sel_range.unwrap_or((s.start(), s.end())))
}

pub fn count(o: &Opts) {
    match build_regex(o) {
        Ok(re) => {
            let a = app();
            let range = current_range(o);
            let n = find_all(a.doc(), &re, range).len();
            set_status(&format!("Count: {} match{}.", n, if n == 1 { "" } else { "es" }), StatusKind::Info);
        }
        Err(_) => set_status("Count: Invalid search pattern", StatusKind::NotFound),
    }
}

pub fn replace_one(o: &Opts) {
    let Ok(re) = build_regex(o) else { return };
    let a = app();
    let doc = a.doc_mut();
    if doc.read_only {
        set_status("Replace: Cannot replace text. The current document is read only.", StatusKind::NotFound);
        return;
    }
    let s = doc.main_sel();
    let text = doc_string(doc);
    let sb = doc.text.char_to_byte(s.start());
    let eb = doc.text.char_to_byte(s.end());
    let mut replaced = false;
    if let Some(caps) = re.captures_at(&text, sb) {
        let m = caps.get(0).unwrap();
        if m.start() == sb && m.end() == eb && !s.is_empty() {
            let rep = expand(&caps, &o.with, o.mode);
            doc.begin(EditKind::Other);
            doc.replace(s.start(), s.end() - s.start(), &rep);
            let p = s.start() + rep.chars().count();
            doc.set_sel(p, p);
            doc.end();
            replaced = true;
        }
    }
    let found = find_next(o);
    if replaced && found {
        set_status("Replace: 1 occurrence was replaced. The next occurrence found.", StatusKind::Info);
    } else if replaced {
        set_status("Replace: 1 occurrence was replaced. No more occurrence found.", StatusKind::Wrapped);
    }
    a.refresh_editor();
}

pub fn replace_all(o: &Opts, all_docs: bool) {
    let Ok(re) = build_regex(o) else {
        set_status("Replace All: Invalid search pattern", StatusKind::NotFound);
        return;
    };
    let a = app();
    let mut total = 0;
    let mut ndocs = 0;
    if all_docs {
        for d in a.docs.iter_mut() {
            let n = replace_all_in(d, &re, o, None);
            total += n;
            if n > 0 {
                ndocs += 1;
            }
        }
        set_status(&format!("Replace in Opened Files: {total} occurrences were replaced in {ndocs} documents."), StatusKind::Info);
    } else {
        let range = current_range(o);
        let before = a.doc().len();
        total = replace_all_in(a.doc_mut(), &re, o, range);
        if let Some((s0, e0)) = range {
            let d = a.doc_mut();
            let e1 = (e0 as isize + d.len() as isize - before as isize).max(s0 as isize) as usize;
            let e1 = e1.min(d.len());
            d.set_sel(s0, e1);
            a.find.sel_range = Some((s0, e1));
        }
        set_status(&format!("Replace All: {} occurrence{} were replaced{}.", total, if total == 1 { "" } else { "s" }, if o.in_sel { " in selected text" } else { " from top to bottom" }), StatusKind::Info);
    }
    a.refresh_editor();
}

pub fn mark_all(o: &Opts, bookmark: bool, purge: bool) {
    let Ok(re) = build_regex(o) else { return };
    let a = app();
    let range = current_range(o);
    let doc = a.doc_mut();
    if purge {
        doc.marks.retain(|m| m.2 != 0);
    }
    let hits = find_all(doc, &re, range);
    for &(s, e) in &hits {
        doc.marks.push((s, e, 0));
        if bookmark {
            let l = doc.line_of(s);
            doc.bookmarks.insert(l);
        }
    }
    doc.marks.sort();
    doc.marks.dedup();
    let n = hits.len();
    set_status(&format!("Mark: {} match{}.", n, if n == 1 { "" } else { "es" }), StatusKind::Info);
    a.refresh_editor();
}

pub fn clear_marks(style: Option<u8>) {
    let a = app();
    let doc = a.doc_mut();
    match style {
        Some(s) => doc.marks.retain(|m| m.2 != s),
        None => doc.marks.retain(|m| m.2 == 6),
    }
    a.refresh_editor();
}

/// Search > Style All Occurrences of Token.
pub fn style_token(style: u8) {
    let a = app();
    let doc = a.doc_mut();
    let s = doc.main_sel();
    let (st, en) = if s.is_empty() { doc.word_at(s.caret) } else { (s.start(), s.end()) };
    if st == en {
        return;
    }
    let word = doc.slice(st, en);
    let o = Opts { what: word, case: true, word: s.is_empty(), mode: 0, ..Default::default() };
    if let Ok(re) = build_regex(&o) {
        for (a_, b) in find_all(doc, &re, None) {
            doc.marks.push((a_, b, style));
        }
        doc.marks.sort();
        doc.marks.dedup();
    }
    a.refresh_editor();
}

fn line_preview(doc: &Document, line: usize) -> String {
    let t = doc.line_text(line);
    let t: String = t.chars().take(1000).collect();
    t
}

/// Search one document, appending lines to the results buffer.
fn search_doc_into(doc: &Document, re: &Regex, out: &mut Vec<(String, Option<ResultLoc>, Vec<(usize, usize)>)>, path: Option<PathBuf>) -> usize {
    let hits = find_all(doc, re, None);
    if hits.is_empty() {
        return 0;
    }
    out.push((format!("  {} ({} hit{})", doc.full_title(), hits.len(), if hits.len() == 1 { "" } else { "s" }), None, vec![]));
    let mut i = 0;
    while i < hits.len() {
        let line = doc.line_of(hits[i].0);
        let ls = doc.line_start(line);
        let prefix = format!("\tLine {}: ", line + 1);
        let pl = prefix.chars().count();
        let mut ranges = Vec::new();
        let first = hits[i];
        while i < hits.len() && doc.line_of(hits[i].0) == line {
            let (s, e) = hits[i];
            let le = doc.line_end(line);
            ranges.push((pl + s - ls, pl + e.min(le) - ls));
            i += 1;
        }
        let loc = ResultLoc { doc_id: doc.id, path: path.clone().or_else(|| doc.path.clone()), line, start: first.0 - ls, end: first.1.min(doc.line_end(line)) - ls };
        out.push((format!("{prefix}{}", line_preview(doc, line)), Some(loc), ranges));
    }
    hits.len()
}

pub fn find_all_docs(o: &Opts, all: bool) {
    let Ok(re) = build_regex(o) else { return };
    let a = app();
    let mut out = Vec::new();
    let mut total = 0;
    let mut files = 0;
    let searched;
    if all {
        searched = a.docs.len();
        for d in &a.docs {
            let n = search_doc_into(d, &re, &mut out, None);
            if n > 0 {
                files += 1;
                total += n;
            }
        }
    } else {
        searched = 1;
        let n = search_doc_into(a.doc(), &re, &mut out, None);
        if n > 0 {
            files = 1;
            total = n;
        }
    }
    let header = format!("Search \"{}\" ({} hit{} in {} file{} of {} searched)", o.what, total, if total == 1 { "" } else { "s" }, files, if files == 1 { "" } else { "s" }, searched);
    a.push_results(header, out);
    set_status(&format!("{total} hits"), StatusKind::Info);
}

fn wildcard(pat: &str, name: &str) -> bool {
    let p: Vec<char> = pat.to_lowercase().chars().collect();
    let n: Vec<char> = name.to_lowercase().chars().collect();
    fn rec(p: &[char], n: &[char]) -> bool {
        match (p.first(), n.first()) {
            (None, None) => true,
            (Some('*'), _) => rec(&p[1..], n) || (!n.is_empty() && rec(p, &n[1..])),
            (Some('?'), Some(_)) => rec(&p[1..], &n[1..]),
            (Some(a), Some(b)) if a == b => rec(&p[1..], &n[1..]),
            _ => false,
        }
    }
    rec(&p, &n)
}

fn collect_files(dir: &Path, filters: &[String], sub: bool, hidden: bool, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        if !hidden && name.starts_with('.') {
            continue;
        }
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_dir() {
            if sub && !matches!(name.as_str(), "node_modules" | ".git" | "target") {
                collect_files(&p, filters, sub, hidden, out);
            }
        } else if filters.is_empty() || filters.iter().any(|f| wildcard(f, &name)) {
            out.push(p);
        }
        if out.len() > 200_000 {
            return;
        }
    }
}

pub fn find_in_files(o: &Opts, dir: &str, filter: &str, sub: bool, hidden: bool, replace: bool) {
    let Ok(re) = build_regex(o) else {
        set_status("Find in Files: Invalid search pattern", StatusKind::NotFound);
        return;
    };
    let dir = PathBuf::from(dir.trim());
    if !dir.is_dir() {
        set_status("Find in Files: The directory is invalid", StatusKind::NotFound);
        return;
    }
    let filters: Vec<String> = filter.split([';', ' ']).map(|s| s.trim().to_string()).filter(|s| !s.is_empty() && s != "*.*").collect();
    let mut files = Vec::new();
    collect_files(&dir, &filters, sub, hidden, &mut files);
    let a = app();
    unsafe {
        SetCursor(LoadCursorW(std::ptr::null_mut(), IDC_WAIT));
    }
    let mut out = Vec::new();
    let mut total = 0;
    let mut nfiles = 0;
    let mut replaced_files = 0;
    let mut skipped = 0;
    for f in &files {
        let Ok(meta) = std::fs::metadata(f) else { continue };
        if meta.len() > 64 * 1024 * 1024 {
            continue;
        }
        let Ok(bytes) = std::fs::read(f) else { continue };
        if bytes.iter().take(8000).any(|&b| b == 0) && !bytes.starts_with(&[0xFF, 0xFE]) && !bytes.starts_with(&[0xFE, 0xFF]) {
            continue;
        }
        let loaded = crate::fileio::decode(&bytes);
        let mut d = Document::new(String::new());
        d.path = Some(f.clone());
        d.set_content(&loaded.text);
        if replace {
            let open_modified = a.docs.iter().any(|x| x.path.as_deref().is_some_and(|p| crate::app::same_path(p, f)) && x.is_modified());
            let lossless = crate::fileio::encode(&loaded.text, loaded.eol, loaded.encoding) == bytes;
            if open_modified || !lossless {
                skipped += 1;
                continue;
            }
            let n = replace_all_in(&mut d, &re, o, None);
            if n > 0 && crate::fileio::save(f, &d.text_string(), loaded.eol, loaded.encoding).is_ok() {
                total += n;
                replaced_files += 1;
                // refresh an open copy
                if let Some(od) = a.docs.iter_mut().find(|x| x.path.as_deref().is_some_and(|p| crate::app::same_path(p, f))) {
                    if !od.is_modified() {
                        let caret = od.caret();
                        od.set_content(&d.text_string());
                        od.set_caret(caret.min(od.len()));
                        od.mtime = crate::fileio::mtime(f);
                    }
                }
            }
        } else {
            let n = search_doc_into(&d, &re, &mut out, Some(f.clone()));
            if n > 0 {
                total += n;
                nfiles += 1;
            }
        }
    }
    if replace {
        let note = if skipped > 0 { format!(" ({skipped} files skipped: unsaved in editor, mixed line endings or undecodable bytes)") } else { String::new() };
        set_status(&format!("Replace in Files: {total} occurrences were replaced in {replaced_files} files.{note}"), StatusKind::Info);
        a.refresh_editor();
        return;
    }
    let header = format!("Search \"{}\" ({} hit{} in {} file{} of {} searched)", o.what, total, if total == 1 { "" } else { "s" }, nfiles, if nfiles == 1 { "" } else { "s" }, files.len());
    a.push_results(header, out);
    set_status(&format!("Find in Files: {total} hits in {nfiles} files"), StatusKind::Info);
}

// ------------------------------------------------------------------ dialog

pub const ID_TAB: u16 = 100;
const ID_FIND_LBL: u16 = 101;
pub const ID_FIND: u16 = 102;
const ID_REPL_LBL: u16 = 103;
const ID_REPL: u16 = 104;
const ID_DIR_LBL: u16 = 105;
const ID_DIR: u16 = 106;
const ID_DIR_BTN: u16 = 107;
const ID_FILT_LBL: u16 = 108;
const ID_FILT: u16 = 109;
const ID_SUB: u16 = 110;
const ID_HIDDEN: u16 = 111;
const ID_INSEL: u16 = 112;
const ID_BACK: u16 = 113;
const ID_WORD: u16 = 114;
const ID_CASE: u16 = 115;
const ID_WRAP: u16 = 116;
const ID_GRP_MODE: u16 = 117;
const ID_NORMAL: u16 = 118;
const ID_EXT: u16 = 119;
const ID_REGEX: u16 = 120;
const ID_DOTNL: u16 = 121;
const ID_GRP_TRANS: u16 = 122;
const ID_TRANS: u16 = 123;
const ID_LOSE: u16 = 124;
const ID_ALWAYS: u16 = 125;
const ID_SLIDER: u16 = 126;
const ID_B1: u16 = 131;
const ID_B2: u16 = 132;
const ID_B3: u16 = 133;
const ID_B4: u16 = 134;
pub const ID_STATUS: u16 = 140;
const ID_BOOKMARK: u16 = 141;
const ID_PURGE: u16 = 142;

pub struct FindDlg {
    pub hwnd: HWND,
    pub tab: usize,
    pub status_kind: StatusKind,
    /// Range used by "In selection" (snapshotted when the dialog opens or the box is ticked).
    pub sel_range: Option<(usize, usize)>,
}

impl FindDlg {
    pub fn new() -> Self {
        FindDlg { hwnd: std::ptr::null_mut(), tab: 0, status_kind: StatusKind::Info, sel_range: None }
    }
}

fn template() -> Vec<u32> {
    let mut b = DlgBuilder::with_style("Find", 372, 222, WS_POPUP | WS_CAPTION | WS_SYSMENU, WS_EX_LAYERED);
    let tab_style = WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_CLIPSIBLINGS;
    b.item("SysTabControl32", "", ID_TAB, 4, 4, 364, 14, tab_style)
        .item("STATIC", "Find what :", ID_FIND_LBL, 6, 26, 62, 10, SS_RTEXT)
        .item("COMBOBOX", "", ID_FIND, 72, 24, 196, 160, CB_EDIT)
        .item("STATIC", "Replace with :", ID_REPL_LBL, 6, 44, 62, 10, SS_RTEXT)
        .item("COMBOBOX", "", ID_REPL, 72, 42, 196, 160, CB_EDIT)
        .item("STATIC", "Filters :", ID_FILT_LBL, 6, 62, 62, 10, SS_RTEXT)
        .item("EDIT", "", ID_FILT, 72, 60, 196, 13, ES_EDIT)
        .item("STATIC", "Directory :", ID_DIR_LBL, 6, 80, 62, 10, SS_RTEXT)
        .item("EDIT", "", ID_DIR, 72, 78, 176, 13, ES_EDIT)
        .item("BUTTON", "...", ID_DIR_BTN, 252, 77, 16, 14, BS_PUSH)
        .item("BUTTON", "In all su&b-folders", ID_SUB, 180, 96, 90, 10, BS_CHECK)
        .item("BUTTON", "In &hidden folders", ID_HIDDEN, 180, 108, 90, 10, BS_CHECK)
        .item("BUTTON", "In selection", ID_INSEL, 72, 60, 80, 10, BS_CHECK)
        .item("BUTTON", "Bookmark line", ID_BOOKMARK, 8, 60, 62, 10, BS_CHECK)
        .item("BUTTON", "Purge for each search", ID_PURGE, 8, 72, 90, 10, BS_CHECK)
        .item("BUTTON", "Backward direction", ID_BACK, 8, 98, 120, 10, BS_CHECK)
        .item("BUTTON", "Match &whole word only", ID_WORD, 8, 110, 120, 10, BS_CHECK)
        .item("BUTTON", "Match &case", ID_CASE, 8, 122, 120, 10, BS_CHECK)
        .item("BUTTON", "Wra&p around", ID_WRAP, 8, 134, 120, 10, BS_CHECK)
        .item("BUTTON", "Search Mode", ID_GRP_MODE, 4, 150, 190, 52, BS_GROUPBOX_)
        .item("BUTTON", "&Normal", ID_NORMAL, 10, 162, 120, 10, BS_RADIO1)
        .item("BUTTON", "E&xtended (\\n, \\r, \\t, \\0, \\x...)", ID_EXT, 10, 174, 150, 10, BS_RADIO)
        .item("BUTTON", "Re&gular expression", ID_REGEX, 10, 186, 90, 10, BS_RADIO)
        .item("BUTTON", "&. matches newline", ID_DOTNL, 104, 186, 86, 10, BS_CHECK)
        .item("BUTTON", "", ID_GRP_TRANS, 198, 150, 170, 52, BS_GROUPBOX_)
        .item("BUTTON", "Tra&nsparency", ID_TRANS, 204, 150, 64, 10, BS_CHECK)
        .item("BUTTON", "On losing focus", ID_LOSE, 210, 166, 80, 10, BS_RADIO1)
        .item("BUTTON", "Always", ID_ALWAYS, 210, 180, 80, 10, BS_RADIO)
        .item("msctls_trackbar32", "", ID_SLIDER, 292, 170, 70, 14, WS_CHILD | WS_VISIBLE | WS_TABSTOP)
        .item("BUTTON", "Find Next", ID_B1, 276, 23, 92, 14, BS_DEF)
        .item("BUTTON", "Count", ID_B2, 276, 41, 92, 14, BS_PUSH)
        .item("BUTTON", "Find All in All &Opened Documents", ID_B3, 276, 59, 92, 22, BS_PUSH | BS_MULTILINE as u32)
        .item("BUTTON", "Find All in Current Document", ID_B4, 276, 85, 92, 22, BS_PUSH | BS_MULTILINE as u32)
        .item("BUTTON", "Close", IDCANCEL_, 276, 111, 92, 14, BS_PUSH)
        .item("STATIC", "", ID_STATUS, 6, 208, 362, 10, SS_TEXT | SS_ENDELLIPSIS as u32);
    b.build()
}

const TAB_NAMES: [&str; 4] = ["Find", "Replace", "Find in Files", "Mark"];

fn set_tab(h: HWND, tab: usize) {
    app().find.tab = tab;
    send(item(h, ID_TAB), TCM_SETCURSEL, tab, 0);
    set_text(h, TAB_NAMES[tab]);
    let (repl, fif, mark) = (tab == 1 || tab == 2, tab == 2, tab == 3);
    for id in [ID_REPL_LBL, ID_REPL] {
        show(h, id, repl);
    }
    for id in [ID_DIR_LBL, ID_DIR, ID_DIR_BTN, ID_FILT_LBL, ID_FILT, ID_SUB, ID_HIDDEN] {
        show(h, id, fif);
    }
    show(h, ID_INSEL, tab == 1 || mark);
    show(h, ID_BOOKMARK, mark);
    show(h, ID_PURGE, mark);
    show(h, ID_BACK, !fif);
    show(h, ID_WRAP, !fif);
    let labels: [&str; 4] = match tab {
        0 => ["Find Next", "Count", "Find All in All &Opened Documents", "Find All in Current Document"],
        1 => ["Find Next", "&Replace", "Replace &All", "Replace All in All &Opened Documents"],
        2 => ["Find All", "Replace in Files", "", ""],
        _ => ["Mark All", "Clear all marks", "Copy Marked Text", ""],
    };
    for (k, id) in [ID_B1, ID_B2, ID_B3, ID_B4].into_iter().enumerate() {
        set_item_text(h, id, labels[k]);
        show(h, id, !labels[k].is_empty());
    }
    if fif {
        let a = app();
        if get_item_text(h, ID_DIR).is_empty() {
            let dir = if !a.settings.fif_dir.is_empty() {
                a.settings.fif_dir.clone()
            } else {
                a.doc().path.as_ref().and_then(|p| p.parent()).map(|p| p.to_string_lossy().into_owned()).unwrap_or_default()
            };
            set_item_text(h, ID_DIR, &dir);
        }
        if get_item_text(h, ID_FILT).is_empty() {
            set_item_text(h, ID_FILT, &a.settings.fif_filter.clone());
        }
    }
    unsafe {
        InvalidateRect(h, std::ptr::null(), 1);
    }
}

fn read_opts(h: HWND) -> Opts {
    let a = app();
    let s = &mut a.settings;
    let what = get_item_text(h, ID_FIND);
    let with = get_item_text(h, ID_REPL);
    s.match_case = checked(h, ID_CASE);
    s.whole_word = checked(h, ID_WORD);
    s.wrap_around = checked(h, ID_WRAP);
    s.backward = checked(h, ID_BACK);
    s.search_mode = if checked(h, ID_REGEX) {
        2
    } else if checked(h, ID_EXT) {
        1
    } else {
        0
    };
    s.dot_nl = checked(h, ID_DOTNL);
    let in_sel = checked(h, ID_INSEL) && (a.find.tab == 1 || a.find.tab == 3);
    push_history(&mut s.find_history, &what, 20);
    if a.find.tab == 1 || a.find.tab == 2 {
        push_history(&mut s.replace_history, &with, 20);
    }
    refresh_history(h);
    Opts { what, with, case: s.match_case, word: s.whole_word, mode: s.search_mode, dot_nl: s.dot_nl, wrap: s.wrap_around, backward: s.backward, in_sel }
}

fn refresh_history(h: HWND) {
    let a = app();
    for (id, list) in [(ID_FIND, a.settings.find_history.clone()), (ID_REPL, a.settings.replace_history.clone())] {
        let cur = get_item_text(h, id);
        combo_clear(h, id);
        for s in &list {
            combo_add(h, id, s);
        }
        set_item_text(h, id, &cur);
    }
}

pub fn opts_from_settings() -> Opts {
    let s = &app().settings;
    Opts {
        what: s.find_history.first().cloned().unwrap_or_default(),
        with: s.replace_history.first().cloned().unwrap_or_default(),
        case: s.match_case,
        word: s.whole_word,
        mode: s.search_mode,
        dot_nl: s.dot_nl,
        wrap: s.wrap_around,
        backward: s.backward,
        in_sel: false,
    }
}

/// Open (or focus) the find dialog on a given tab.
pub fn show_dialog(tab: usize) {
    let a = app();
    let doc = &a.docs[a.active];
    let s = doc.main_sel();
    let seed = if !s.is_empty() && doc.line_of(s.start()) == doc.line_of(s.end()) && s.end() - s.start() < 1000 {
        Some(doc.slice(s.start(), s.end()))
    } else if s.is_empty() {
        let (x, y) = doc.word_at(s.caret);
        if y > x {
            Some(doc.slice(x, y))
        } else {
            None
        }
    } else {
        None
    };
    let multi_line_sel = !s.is_empty() && doc.line_of(s.start()) != doc.line_of(s.end());
    if a.find.hwnd.is_null() {
        let tpl = template();
        unsafe {
            a.find.hwnd = CreateDialogIndirectParamW(hinstance(), tpl.as_ptr() as *const DLGTEMPLATE, a.hwnd, Some(find_proc), 0);
        }
    }
    let h = a.find.hwnd;
    set_tab(h, tab);
    if let Some(t) = seed {
        set_item_text(h, ID_FIND, &t);
    }
    set_check(h, ID_INSEL, multi_line_sel);
    a.find.sel_range = if multi_line_sel { Some((s.start(), s.end())) } else { None };
    set_item_text(h, ID_STATUS, "");
    unsafe {
        if a.test {
            ShowWindow(h, SW_SHOWNOACTIVATE);
        } else {
            ShowWindow(h, SW_SHOW);
            SetForegroundWindow(h);
        }
        if !a.test {
            SetFocus(item(h, ID_FIND));
        }
    }
    send(item(h, ID_FIND), CB_SETEDITSEL, 0, 0xFFFF_0000u32 as isize);
}

fn apply_transparency(h: HWND, active: bool) {
    let on = checked(h, ID_TRANS);
    let always = checked(h, ID_ALWAYS);
    let alpha = send(item(h, ID_SLIDER), WM_USER, 0, 0).clamp(20, 255) as u8;
    let a = if on && (always || !active) { alpha } else { 255 };
    unsafe {
        SetLayeredWindowAttributes(h, 0, a, LWA_ALPHA);
    }
}

unsafe extern "system" fn find_proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> isize {
    match msg {
        WM_INITDIALOG => {
            SetLayeredWindowAttributes(h, 0, 255, LWA_ALPHA);
            let tab = item(h, ID_TAB);
            for (i, name) in TAB_NAMES.iter().enumerate() {
                let mut wname = w(name);
                let mut ti: TCITEMW = std::mem::zeroed();
                ti.mask = TCIF_TEXT;
                ti.pszText = wname.as_mut_ptr();
                SendMessageW(tab, TCM_INSERTITEMW, i, &ti as *const _ as isize);
            }
            let s = &app().settings;
            set_check(h, ID_CASE, s.match_case);
            set_check(h, ID_WORD, s.whole_word);
            set_check(h, ID_WRAP, s.wrap_around);
            set_check(h, ID_BACK, s.backward);
            set_check(h, ID_DOTNL, s.dot_nl);
            set_check(h, ID_SUB, s.fif_subfolders);
            set_check(h, ID_HIDDEN, s.fif_hidden);
            set_check(h, ID_TRANS, true);
            set_check(h, ID_LOSE, true);
            set_check(h, ID_PURGE, true);
            set_check(h, match s.search_mode {
                1 => ID_EXT,
                2 => ID_REGEX,
                _ => ID_NORMAL,
            }, true);
            enable(h, ID_DOTNL, s.search_mode == 2);
            SendMessageW(item(h, ID_SLIDER), TBM_SETRANGE, 1, (255isize << 16) | 20);
            SendMessageW(item(h, ID_SLIDER), TBM_SETPOS, 1, 150);
            refresh_history(h);
            // place near the top-right of the main window
            let a = app();
            let mut mr = RECT { left: 0, top: 0, right: 0, bottom: 0 };
            let mut dr = mr;
            GetWindowRect(a.hwnd, &mut mr);
            GetWindowRect(h, &mut dr);
            let dw = dr.right - dr.left;
            SetWindowPos(h, std::ptr::null_mut(), (mr.right - dw - 40).max(mr.left), mr.top + scale(110, dpi_of(h)), 0, 0, SWP_NOSIZE | SWP_NOZORDER);
            1
        }
        WM_ACTIVATE => {
            apply_transparency(h, loword(wp) as u32 != WA_INACTIVE);
            0
        }
        WM_HSCROLL => {
            apply_transparency(h, true);
            0
        }
        WM_NOTIFY => {
            let nm = &*(lp as *const NMHDR);
            if nm.idFrom == ID_TAB as usize && nm.code == TCN_SELCHANGE {
                let i = SendMessageW(nm.hwndFrom, TCM_GETCURSEL, 0, 0).max(0) as usize;
                set_tab(h, i);
            }
            0
        }
        WM_CTLCOLORSTATIC => {
            if lp as HWND == item(h, ID_STATUS) {
                let hdc = wp as HDC;
                let color = match app().find.status_kind {
                    StatusKind::NotFound => 0xFF0000,
                    StatusKind::Wrapped => 0x008000,
                    _ => 0x0000FF,
                };
                SetTextColor(hdc, rgb(color));
                SetBkMode(hdc, TRANSPARENT as i32);
                return GetSysColorBrush(COLOR_BTNFACE) as isize;
            }
            0
        }
        WM_COMMAND => {
            let id = loword(wp);
            let code = hiword(wp);
            let a = app();
            match id {
                IDCANCEL_ => {
                    ShowWindow(h, SW_HIDE);
                    SetFocus(a.view.hwnd);
                }
                ID_NORMAL | ID_EXT | ID_REGEX => enable(h, ID_DOTNL, checked(h, ID_REGEX)),
                ID_INSEL => {
                    let s = a.doc().main_sel();
                    a.find.sel_range = if checked(h, ID_INSEL) && !s.is_empty() { Some((s.start(), s.end())) } else { None };
                }
                ID_TRANS | ID_LOSE | ID_ALWAYS => apply_transparency(h, true),
                ID_DIR_BTN => {
                    if let Some(d) = crate::app::browse_folder(h, &get_item_text(h, ID_DIR)) {
                        set_item_text(h, ID_DIR, &d);
                    }
                }
                ID_B1 | ID_B2 | ID_B3 | ID_B4 if code == BN_CLICKED as u16 => {
                    let o = read_opts(h);
                    if o.what.is_empty() {
                        return 1;
                    }
                    let tab = a.find.tab;
                    match (tab, id) {
                        (0 | 1, ID_B1) => {
                            find_next(&o);
                        }
                        (0, ID_B2) => count(&o),
                        (0, ID_B3) => find_all_docs(&o, true),
                        (0, ID_B4) => find_all_docs(&o, false),
                        (1, ID_B2) => replace_one(&o),
                        (1, ID_B3) => replace_all(&o, false),
                        (1, ID_B4) => {
                            if msgbox(h, "Are you sure you want to replace all occurrences in all opened documents?", "Replace All", MB_OKCANCEL | MB_ICONQUESTION) == IDOK {
                                replace_all(&o, true);
                            }
                        }
                        (2, ID_B1) | (2, ID_B2) => {
                            let dir = get_item_text(h, ID_DIR);
                            let filt = get_item_text(h, ID_FILT);
                            a.settings.fif_dir = dir.clone();
                            a.settings.fif_filter = filt.clone();
                            a.settings.fif_subfolders = checked(h, ID_SUB);
                            a.settings.fif_hidden = checked(h, ID_HIDDEN);
                            let replace = id == ID_B2;
                            if replace
                                && msgbox(h, &format!("Are you sure you want to replace all occurrences in :\n{dir}\nfor file type :\n{filt}"), "Replace in Files", MB_OKCANCEL | MB_ICONQUESTION) != IDOK
                            {
                                return 1;
                            }
                            find_in_files(&o, &dir, &filt, checked(h, ID_SUB), checked(h, ID_HIDDEN), replace);
                        }
                        (3, ID_B1) => mark_all(&o, checked(h, ID_BOOKMARK), checked(h, ID_PURGE)),
                        (3, ID_B2) => clear_marks(Some(0)),
                        (3, ID_B3) => {
                            let d = a.doc();
                            let t: Vec<String> = d.marks.iter().filter(|m| m.2 == 0).map(|m| d.slice(m.0, m.1)).collect();
                            set_clipboard(a.hwnd, &t.join("\r\n"));
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
            1
        }
        WM_CLOSE => {
            ShowWindow(h, SW_HIDE);
            1
        }
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(text: &str) -> Document {
        let mut d = Document::new("t".into());
        d.set_content(text);
        d
    }

    #[test]
    fn big_replace_all_keeps_bookmarks() {
        let text: String = (0..200).map(|i| format!("foo {i}\n")).collect();
        let mut d = doc(&text);
        d.bookmarks.insert(10);
        d.bookmarks.insert(150);
        let o = Opts { what: "foo".into(), with: "bar".into(), ..Default::default() };
        let re = build_regex(&o).unwrap();
        assert_eq!(replace_all_in(&mut d, &re, &o, None), 200);
        assert!(d.text_string().starts_with("bar 0\nbar 1\n"));
        assert_eq!(d.bookmarks.iter().copied().collect::<Vec<_>>(), vec![10, 150]);
        // line-count-changing replacement remaps bookmarks
        let o = Opts { what: "bar".into(), with: "x\ny".into(), mode: 1, ..Default::default() };
        let re = build_regex(&o).unwrap();
        replace_all_in(&mut d, &re, &o, None);
        assert_eq!(d.line_count(), 401);
        assert_eq!(d.bookmarks.iter().copied().collect::<Vec<_>>(), vec![20, 300]);
    }

    #[test]
    fn regex_in_selection_respects_real_line_end() {
        let mut d = doc("a  b  \nc");
        let o = Opts { what: r"\s+$".into(), with: "".into(), mode: 2, ..Default::default() };
        let re = build_regex(&o).unwrap();
        // selection ends in the middle of line 1 (after "a  "): no match should be found
        assert_eq!(replace_all_in(&mut d, &re, &o, Some((0, 3))), 0);
        assert_eq!(replace_all_in(&mut d, &re, &o, None), 1);
        assert_eq!(d.text_string(), "a  b\nc");
    }

    #[test]
    fn crlf_in_replacement_is_one_line_break() {
        let mut d = doc("a,b");
        let o = Opts { what: ",".into(), with: r"\r\n".into(), mode: 2, ..Default::default() };
        let re = build_regex(&o).unwrap();
        replace_all_in(&mut d, &re, &o, None);
        assert_eq!(d.text_string(), "a\nb");
    }
}
