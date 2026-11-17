//! Editing operations on a Document (independent of any window).

use crate::doc::*;
use crate::lang::{self, Lang};

pub struct Change {
    pub pos: usize,
    pub del: usize,
    pub ins: String,
    pub anchor: usize,
    pub caret: usize,
}

impl Document {
    /// Apply one change per selection (ascending), tracking position shifts.
    pub fn each_sel(&mut self, kind: EditKind, f: impl Fn(&Document, Sel) -> Option<Change>) {
        if self.read_only {
            return;
        }
        let mut order: Vec<usize> = (0..self.sels.len()).collect();
        order.sort_by_key(|&k| self.sels[k].start());
        let orig = self.sels.clone();
        let mut new = orig.clone();
        let mut delta: isize = 0;
        self.begin(kind);
        for k in order {
            let s = orig[k];
            let adj = Sel::new((s.anchor as isize + delta).max(0) as usize, (s.caret as isize + delta).max(0) as usize);
            match f(self, adj) {
                Some(ch) => {
                    let il = ch.ins.chars().count();
                    self.replace(ch.pos, ch.del, &ch.ins);
                    new[k] = Sel::new(ch.anchor, ch.caret);
                    delta += il as isize - ch.del as isize;
                }
                None => new[k] = adj,
            }
        }
        self.sels = new;
        self.rect = None;
        self.normalize_sels();
        self.end();
    }

    /// Replace every selection by `f(text)`; optionally keep the result selected.
    pub fn replace_sels(&mut self, keep_sel: bool, f: impl Fn(&str) -> String) {
        let rect = self.rect;
        self.each_sel(EditKind::Other, |d, s| {
            let old = d.slice(s.start(), s.end());
            let rep = f(&old);
            if rep == old {
                return None;
            }
            let rl = rep.chars().count();
            let (a, c) = if keep_sel {
                if s.caret >= s.anchor {
                    (s.start(), s.start() + rl)
                } else {
                    (s.start() + rl, s.start())
                }
            } else {
                (s.start() + rl, s.start() + rl)
            };
            Some(Change { pos: s.start(), del: s.end() - s.start(), ins: rep, anchor: a, caret: c })
        });
        if keep_sel && rect.is_some() {
            self.rect = rect;
        }
    }

    pub fn insert_text(&mut self, text: &str, overtype: bool) {
        self.each_sel(EditKind::Typing, |d, s| {
            let mut del = s.end() - s.start();
            if del == 0 && overtype {
                if let Some(c) = d.char_at(s.caret) {
                    if c != '\n' {
                        del = 1;
                    }
                }
            }
            let p = s.start() + text.chars().count();
            Some(Change { pos: s.start(), del, ins: text.to_string(), anchor: p, caret: p })
        });
    }

    /// Paste: with multiple selections and the same number of clipboard lines, distribute them.
    pub fn paste(&mut self, text: &str) {
        if self.read_only {
            return;
        }
        let t = crate::fileio::normalize_eol(text);
        let lines: Vec<&str> = t.trim_end_matches('\n').split('\n').collect();
        if self.sels.len() > 1 && lines.len() == self.sels.len() {
            let mut order: Vec<usize> = (0..self.sels.len()).collect();
            order.sort_by_key(|&k| self.sels[k].start());
            let orig = self.sels.clone();
            let mut new = orig.clone();
            let mut delta: isize = 0;
            self.begin(EditKind::Other);
            for (r, &k) in order.iter().enumerate() {
                let s = orig[k];
                let a = (s.start() as isize + delta) as usize;
                let len = s.end() - s.start();
                let ins = lines[r];
                let il = ins.chars().count();
                self.replace(a, len, ins);
                new[k] = Sel::at(a + il);
                delta += il as isize - len as isize;
            }
            self.sels = new;
            self.rect = None;
            self.normalize_sels();
            self.end();
            return;
        }
        self.each_sel(EditKind::Other, |_, s| {
            let p = s.start() + t.chars().count();
            Some(Change { pos: s.start(), del: s.end() - s.start(), ins: t.clone(), anchor: p, caret: p })
        });
    }

    pub fn backspace(&mut self) {
        self.each_sel(EditKind::Other, |d, s| {
            if !s.is_empty() {
                return Some(Change { pos: s.start(), del: s.end() - s.start(), ins: String::new(), anchor: s.start(), caret: s.start() });
            }
            if s.caret == 0 {
                return None;
            }
            let p = s.caret - 1;
            let _ = d;
            Some(Change { pos: p, del: 1, ins: String::new(), anchor: p, caret: p })
        });
    }

    pub fn delete_fwd(&mut self) {
        self.each_sel(EditKind::Other, |d, s| {
            if !s.is_empty() {
                return Some(Change { pos: s.start(), del: s.end() - s.start(), ins: String::new(), anchor: s.start(), caret: s.start() });
            }
            if s.caret >= d.len() {
                return None;
            }
            Some(Change { pos: s.caret, del: 1, ins: String::new(), anchor: s.caret, caret: s.caret })
        });
    }

    pub fn delete_word(&mut self, left: bool) {
        self.each_sel(EditKind::Other, |d, s| {
            if !s.is_empty() {
                return Some(Change { pos: s.start(), del: s.end() - s.start(), ins: String::new(), anchor: s.start(), caret: s.start() });
            }
            let (a, b) = if left { (d.word_left(s.caret), s.caret) } else { (s.caret, d.word_right(s.caret)) };
            if a == b {
                return None;
            }
            Some(Change { pos: a, del: b - a, ins: String::new(), anchor: a, caret: a })
        });
    }

    /// Delete from caret to line start/end (Ctrl+Shift+Backspace / Delete).
    pub fn delete_line_part(&mut self, left: bool) {
        self.each_sel(EditKind::Other, |d, s| {
            let line = d.line_of(s.caret);
            let (a, b) = if left { (d.line_start(line), s.caret) } else { (s.caret, d.line_end(line)) };
            if a == b {
                return None;
            }
            Some(Change { pos: a, del: b - a, ins: String::new(), anchor: a, caret: a })
        });
    }

    pub fn indent_unit(&self, use_spaces: bool) -> String {
        if use_spaces {
            " ".repeat(self.tab_width)
        } else {
            "\t".into()
        }
    }

    pub fn newline(&mut self, auto_indent: bool, use_spaces: bool) {
        let unit = self.indent_unit(use_spaces);
        let lang = self.lang;
        self.each_sel(EditKind::Other, |d, s| {
            let line = d.line_of(s.start());
            let ls = d.line_start(line);
            let mut indent = String::new();
            let mut extra = false;
            let mut close_after = false;
            if auto_indent {
                for c in d.text.slice(ls..s.start()).chars() {
                    if c == ' ' || c == '\t' {
                        indent.push(c);
                    } else {
                        break;
                    }
                }
                let before = d.slice(ls, s.start());
                let t = before.trim_end();
                let sp = lang::spec(lang);
                if sp.fold == lang::Fold::Brace && (t.ends_with('{') || t.ends_with('[') || t.ends_with('(')) {
                    extra = true;
                    let next = d.char_at(s.end());
                    close_after = matches!((t.chars().last(), next), (Some('{'), Some('}')) | (Some('['), Some(']')) | (Some('('), Some(')')));
                } else if lang == Lang::Python && t.ends_with(':') && !t.trim_start().starts_with('#') {
                    extra = true;
                }
            }
            let mut ins = format!("\n{indent}");
            if extra {
                ins.push_str(&unit);
            }
            let caret = s.start() + ins.chars().count();
            if close_after {
                ins.push('\n');
                ins.push_str(&indent);
            }
            Some(Change { pos: s.start(), del: s.end() - s.start(), ins, anchor: caret, caret })
        });
    }

    /// After typing a closing brace on an otherwise blank line, align it with its opening line.
    pub fn align_closing_brace(&mut self) {
        if self.sels.len() != 1 || !self.main_sel().is_empty() {
            return;
        }
        let caret = self.caret();
        if caret == 0 || !matches!(self.char_at(caret - 1), Some('}') | Some(']')) {
            return;
        }
        let line = self.line_of(caret);
        let ls = self.line_start(line);
        let before = self.slice(ls, caret - 1);
        if !before.chars().all(|c| c == ' ' || c == '\t') {
            return;
        }
        let Some(open) = self.match_brace(caret - 1) else { return };
        let ol = self.line_of(open);
        if ol == line {
            return;
        }
        let ind: String = self.line_chars(ol).into_iter().take_while(|c| *c == ' ' || *c == '\t').collect();
        if ind == before {
            return;
        }
        self.begin(EditKind::Typing);
        self.replace(ls, caret - 1 - ls, &ind);
        let p = ls + ind.chars().count() + 1;
        self.sels = vec![Sel::at(p)];
        self.main = 0;
        self.end();
    }

    /// Lines covered by the main selection (an end at column 0 is excluded).
    pub fn sel_lines(&self) -> (usize, usize) {
        let s = self.main_sel();
        let a = self.line_of(s.start());
        let mut b = self.line_of(s.end());
        if b > a && s.end() == self.line_start(b) {
            b -= 1;
        }
        (a, b)
    }

    fn all_sel_lines(&self) -> Vec<usize> {
        let mut v = Vec::new();
        for s in &self.sels {
            let a = self.line_of(s.start());
            let mut b = self.line_of(s.end());
            if b > a && s.end() == self.line_start(b) {
                b -= 1;
            }
            v.extend(a..=b);
        }
        v.sort();
        v.dedup();
        v
    }

    /// Tab key: indent selected lines, or insert a tab/spaces.
    pub fn tab(&mut self, use_spaces: bool) {
        let multi_line = self.sels.iter().any(|s| self.line_of(s.start()) != self.line_of(s.end()));
        if multi_line {
            self.indent_lines(use_spaces);
            return;
        }
        if use_spaces {
            let tw = self.tab_width;
            self.each_sel(EditKind::Typing, |d, s| {
                let col = d.col_of(s.start());
                let n = tw - col % tw;
                let p = s.start() + n;
                Some(Change { pos: s.start(), del: s.end() - s.start(), ins: " ".repeat(n), anchor: p, caret: p })
            });
        } else {
            self.insert_text("\t", false);
        }
    }

    pub fn indent_lines(&mut self, use_spaces: bool) {
        let unit = self.indent_unit(use_spaces);
        let lines = self.all_sel_lines();
        let (first, last) = (lines[0], *lines.last().unwrap());
        self.begin(EditKind::Other);
        for &l in lines.iter().rev() {
            if self.line_len(l) > 0 {
                let p = self.line_start(l);
                self.replace(p, 0, &unit);
            }
        }
        let a = self.line_start(first);
        let b = if last + 1 < self.line_count() { self.line_start(last + 1) } else { self.len() };
        self.set_sel(a, b);
        self.end();
    }

    pub fn outdent_lines(&mut self) {
        let lines = self.all_sel_lines();
        let (first, last) = (lines[0], *lines.last().unwrap());
        let single_caret = self.sels.len() == 1 && self.main_sel().is_empty();
        let caret = self.caret();
        let caret_col_off = caret - self.line_start(self.line_of(caret));
        self.begin(EditKind::Other);
        let mut removed_on_caret_line = 0;
        for &l in lines.iter().rev() {
            let p = self.line_start(l);
            let chars = self.line_chars(l);
            let mut n = 0;
            if chars.first() == Some(&'\t') {
                n = 1;
            } else {
                while n < self.tab_width && n < chars.len() && chars[n] == ' ' {
                    n += 1;
                }
                if n < self.tab_width && n < chars.len() && chars[n] == '\t' {
                    n += 1;
                }
            }
            if n > 0 {
                self.replace(p, n, "");
                if l == self.line_of(caret.min(self.len())) {
                    removed_on_caret_line = n;
                }
            }
        }
        if single_caret {
            let line = first;
            let p = self.line_start(line) + caret_col_off.saturating_sub(removed_on_caret_line);
            self.set_caret(p.min(self.line_end(line)));
        } else {
            let a = self.line_start(first);
            let b = if last + 1 < self.line_count() { self.line_start(last + 1) } else { self.len() };
            self.set_sel(a, b);
        }
        self.end();
    }

    /// Lines to operate on: selection lines when the selection spans lines, otherwise the whole document.
    pub fn target_lines(&self) -> (usize, usize) {
        let (a, b) = self.sel_lines();
        if a != b {
            (a, b)
        } else {
            (0, self.line_count() - 1)
        }
    }

    pub fn replace_lines(&mut self, first: usize, last: usize, lines: &[String]) {
        let a = self.line_start(first);
        let b = self.line_end(last);
        let new = lines.join("\n");
        if self.slice(a, b) == new {
            return;
        }
        self.begin(EditKind::Other);
        self.replace(a, b - a, &new);
        let nb = a + new.chars().count();
        self.set_sel(a, nb);
        self.end();
    }

    pub fn map_lines(&mut self, f: impl Fn(Vec<String>) -> Vec<String>) {
        let (a, b) = self.target_lines();
        let lines: Vec<String> = (a..=b).map(|l| self.line_text(l)).collect();
        let out = f(lines.clone());
        if out.len() != lines.len() {
            self.replace_lines(a, b, &out);
            return;
        }
        // Same line count: only touch lines that changed (keeps bookmarks, folds, change history).
        let sels = self.sels.clone();
        let main = self.main;
        self.begin(EditKind::Other);
        for (k, (old, new)) in lines.iter().zip(out.iter()).enumerate().rev() {
            if old != new {
                let l = a + k;
                let s = self.line_start(l);
                self.replace(s, old.chars().count(), new);
            }
        }
        let n = self.len();
        self.sels = sels.into_iter().map(|s| Sel::new(s.anchor.min(n), s.caret.min(n))).collect();
        self.main = main;
        self.normalize_sels();
        self.end();
    }

    pub fn duplicate_lines(&mut self) {
        let s = self.main_sel();
        if !s.is_empty() && self.line_of(s.start()) == self.line_of(s.end()) {
            let t = self.slice(s.start(), s.end());
            self.begin(EditKind::Other);
            self.replace(s.end(), 0, &t);
            self.end();
            return;
        }
        let (a, b) = self.sel_lines();
        let text = self.slice(self.line_start(a), self.line_end(b));
        let at = self.line_end(b);
        let caret = self.caret();
        self.begin(EditKind::Other);
        self.replace(at, 0, &format!("\n{text}"));
        self.set_caret(caret);
        self.end();
    }

    pub fn delete_lines(&mut self) {
        let (a, b) = self.sel_lines();
        let col = self.col_of(self.caret());
        let start = self.line_start(a);
        let (pos, len) = if b + 1 < self.line_count() {
            (start, self.line_start(b + 1) - start)
        } else if a > 0 {
            (self.line_end(a - 1), self.len() - self.line_end(a - 1))
        } else {
            (0, self.len())
        };
        self.begin(EditKind::Other);
        self.replace(pos, len, "");
        let l = a.min(self.line_count() - 1);
        let p = self.pos_at_col(l, col);
        self.set_caret(p);
        self.end();
    }

    pub fn move_lines(&mut self, up: bool) {
        let (a, b) = self.sel_lines();
        if (up && a == 0) || (!up && b + 1 >= self.line_count()) {
            return;
        }
        let s = self.main_sel();
        let block = self.slice(self.line_start(a), self.line_end(b));
        self.begin(EditKind::Other);
        if up {
            let prev = self.line_text(a - 1);
            let start = self.line_start(a - 1);
            let end = self.line_end(b);
            self.replace(start, end - start, &format!("{block}\n{prev}"));
            let shift = prev.chars().count() + 1;
            self.sels = vec![Sel::new(s.anchor - shift, s.caret - shift)];
        } else {
            let next = self.line_text(b + 1);
            let start = self.line_start(a);
            let end = self.line_end(b + 1);
            self.replace(start, end - start, &format!("{next}\n{block}"));
            let shift = next.chars().count() + 1;
            self.sels = vec![Sel::new(s.anchor + shift, s.caret + shift)];
        }
        self.main = 0;
        self.end();
    }

    pub fn transpose_line(&mut self) {
        let l = self.line_of(self.caret());
        if l == 0 {
            return;
        }
        let caret = self.caret();
        let a = self.line_text(l - 1);
        let b = self.line_text(l);
        let start = self.line_start(l - 1);
        let end = self.line_end(l);
        self.begin(EditKind::Other);
        self.replace(start, end - start, &format!("{b}\n{a}"));
        self.set_caret(caret);
        self.end();
    }

    pub fn join_lines(&mut self) {
        let (a, b) = self.sel_lines();
        if a == b {
            return;
        }
        let lines: Vec<String> = (a..=b).map(|l| self.line_text(l)).collect();
        let mut out = lines[0].trim_end().to_string();
        for l in &lines[1..] {
            let t = l.trim();
            if !t.is_empty() {
                if !out.is_empty() {
                    out.push(' ');
                }
                out.push_str(t);
            }
        }
        self.replace_lines(a, b, &[out]);
    }

    pub fn split_lines(&mut self, width: usize) {
        let (a, b) = self.sel_lines();
        let mut out = Vec::new();
        for l in a..=b {
            let text = self.line_text(l);
            let mut cur = String::new();
            for word in text.split(' ') {
                if !cur.is_empty() && cur.chars().count() + 1 + word.chars().count() > width {
                    out.push(std::mem::take(&mut cur));
                }
                if !cur.is_empty() {
                    cur.push(' ');
                }
                cur.push_str(word);
            }
            out.push(cur);
        }
        self.replace_lines(a, b, &out);
    }

    pub fn insert_blank_line(&mut self, above: bool) {
        let l = self.line_of(self.caret());
        self.begin(EditKind::Other);
        if above {
            let p = self.line_start(l);
            self.replace(p, 0, "\n");
            self.set_caret(p);
        } else {
            let p = self.line_end(l);
            self.replace(p, 0, "\n");
            self.set_caret(p + 1);
        }
        self.end();
    }

    /// Single-line comment operations. mode: 0 toggle, 1 comment, 2 uncomment.
    pub fn line_comment(&mut self, mode: u8) {
        let Some(tok) = lang::comment_token(self.lang) else {
            if mode != 2 {
                self.block_comment(true);
            } else {
                self.block_comment(false);
            }
            return;
        };
        let (a, b) = self.sel_lines();
        let lines: Vec<String> = (a..=b).map(|l| self.line_text(l)).collect();
        let lead = |l: &str| l.len() - l.trim_start_matches([' ', '\t']).len();
        let is_commented = |l: &str| l[lead(l)..].starts_with(tok);
        let all = lines.iter().filter(|l| !l.trim().is_empty()).all(|l| is_commented(l));
        let uncomment = mode == 2 || (mode == 0 && all);
        let min_indent = lines.iter().filter(|l| !l.trim().is_empty()).map(|l| lead(l)).min().unwrap_or(0);
        let first_prefix = lines.iter().find(|l| !l.trim().is_empty()).map(|l| l[..min_indent].to_string()).unwrap_or_default();
        let common_prefix = lines.iter().filter(|l| !l.trim().is_empty()).all(|l| l.starts_with(&first_prefix));
        let out: Vec<String> = lines
            .iter()
            .map(|l| {
                if l.trim().is_empty() {
                    return l.clone();
                }
                if uncomment {
                    let i = lead(l);
                    if l[i..].starts_with(tok) {
                        {
                            let mut rest = &l[i + tok.len()..];
                            if rest.starts_with(' ') {
                                rest = &rest[1..];
                            }
                            return format!("{}{}", &l[..i], rest);
                        }
                    }
                    l.clone()
                } else {
                    // align at the common indentation when lines share it, else at each line's own indent
                    let ind = if common_prefix { min_indent } else { lead(l) };
                    format!("{}{} {}", &l[..ind], tok, &l[ind..])
                }
            })
            .collect();
        let caret_line = a;
        self.replace_lines(a, b, &out);
        if a == b {
            let p = self.line_end(caret_line);
            self.set_caret(p);
        }
    }

    pub fn block_comment(&mut self, comment: bool) {
        let Some((open, close)) = lang::spec(self.lang).block_comment else { return };
        let s = self.main_sel();
        if comment {
            let (a, b) = if s.is_empty() {
                let l = self.line_of(s.caret);
                (self.line_start(l), self.line_end(l))
            } else {
                (s.start(), s.end())
            };
            self.begin(EditKind::Other);
            self.replace(b, 0, &format!(" {close}"));
            self.replace(a, 0, &format!("{open} "));
            let ol = open.chars().count() + 1;
            self.set_sel(a, b + ol + close.chars().count() + 1);
            self.end();
        } else {
            let (a, b) = if s.is_empty() {
                let l = self.line_of(s.caret);
                (self.line_start(l), self.line_end(l))
            } else {
                (s.start(), s.end())
            };
            let t = self.slice(a, b);
            let tt = t.trim();
            if tt.starts_with(open) && tt.ends_with(close) && tt.len() >= open.len() + close.len() {
                let lead = t.len() - t.trim_start().len();
                let inner = &tt[open.len()..tt.len() - close.len()];
                let inner = inner.strip_prefix(' ').unwrap_or(inner);
                let inner = inner.strip_suffix(' ').unwrap_or(inner);
                let new = format!("{}{}", &t[..lead], inner);
                self.begin(EditKind::Other);
                self.replace(a, b - a, &new);
                self.set_sel(a, a + new.chars().count());
                self.end();
            }
        }
    }

    pub fn convert_case(&mut self, kind: u8) {
        if !self.has_selection() {
            return;
        }
        self.replace_sels(true, |t| match kind {
            0 => t.to_uppercase(),
            1 => t.to_lowercase(),
            2 => {
                let mut out = String::new();
                let mut start = true;
                for c in t.chars() {
                    if c.is_alphabetic() {
                        if start {
                            out.extend(c.to_uppercase());
                        } else {
                            out.extend(c.to_lowercase());
                        }
                        start = false;
                    } else {
                        out.push(c);
                        start = !(c.is_alphanumeric() || c == '\'');
                    }
                }
                out
            }
            3 => {
                let mut out = String::new();
                let mut start = true;
                for c in t.chars() {
                    if c.is_alphabetic() {
                        if start {
                            out.extend(c.to_uppercase());
                            start = false;
                        } else {
                            out.extend(c.to_lowercase());
                        }
                    } else {
                        if ".!?\n".contains(c) {
                            start = true;
                        }
                        out.push(c);
                    }
                }
                out
            }
            4 => t.chars().map(|c| if c.is_uppercase() { c.to_lowercase().next().unwrap_or(c) } else { c.to_uppercase().next().unwrap_or(c) }).collect(),
            _ => {
                let mut seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(7) | 1;
                t.chars()
                    .map(|c| {
                        seed ^= seed << 13;
                        seed ^= seed >> 7;
                        seed ^= seed << 17;
                        if seed & 1 == 0 {
                            c.to_uppercase().next().unwrap_or(c)
                        } else {
                            c.to_lowercase().next().unwrap_or(c)
                        }
                    })
                    .collect()
            }
        });
    }

    pub fn match_brace(&self, pos: usize) -> Option<usize> {
        let c = self.char_at(pos)?;
        let (open, close, fwd) = match c {
            '(' => ('(', ')', true),
            '[' => ('[', ']', true),
            '{' => ('{', '}', true),
            ')' => ('(', ')', false),
            ']' => ('[', ']', false),
            '}' => ('{', '}', false),
            _ => return None,
        };
        let mut depth = 0i32;
        let limit = 200_000usize;
        if fwd {
            let mut it = self.text.chars_at(pos);
            let mut p = pos;
            for ch in it.by_ref().take(limit) {
                if ch == open {
                    depth += 1;
                } else if ch == close {
                    depth -= 1;
                    if depth == 0 {
                        return Some(p);
                    }
                }
                p += 1;
            }
        } else {
            let mut it = self.text.chars_at(pos + 1);
            let mut p = pos + 1;
            let mut n = 0;
            while let Some(ch) = it.prev() {
                p -= 1;
                n += 1;
                if n > limit {
                    break;
                }
                if ch == close {
                    depth += 1;
                } else if ch == open {
                    depth -= 1;
                    if depth == 0 {
                        return Some(p);
                    }
                }
            }
        }
        None
    }

    /// Brace at/before the caret and its match: (brace_pos, Some(match)).
    pub fn brace_pair(&self) -> Option<(usize, Option<usize>)> {
        let c = self.caret();
        let is_brace = |p: usize| self.char_at(p).is_some_and(|ch| "()[]{}".contains(ch));
        let p = if c > 0 && is_brace(c - 1) {
            c - 1
        } else if is_brace(c) {
            c
        } else {
            return None;
        };
        Some((p, self.match_brace(p)))
    }

    pub fn tabs_to_spaces(&mut self) {
        let tw = self.tab_width;
        self.map_lines(|lines| {
            lines
                .into_iter()
                .map(|l| {
                    let mut out = String::new();
                    let mut col = 0;
                    for c in l.chars() {
                        if c == '\t' {
                            let n = tw - col % tw;
                            out.push_str(&" ".repeat(n));
                            col += n;
                        } else {
                            out.push(c);
                            col += 1;
                        }
                    }
                    out
                })
                .collect()
        });
    }

    pub fn spaces_to_tabs(&mut self, leading_only: bool) {
        let tw = self.tab_width;
        self.map_lines(|lines| {
            lines
                .into_iter()
                .map(|l| {
                    let mut out = String::new();
                    let mut col = 0;
                    let mut pending = 0;
                    let mut leading = true;
                    for c in l.chars() {
                        if c == ' ' && (leading || !leading_only) {
                            pending += 1;
                            col += 1;
                            if col % tw == 0 {
                                out.push(if pending > 1 { '\t' } else { ' ' });
                                pending = 0;
                            }
                            continue;
                        }
                        out.push_str(&" ".repeat(pending));
                        pending = 0;
                        if c == '\t' {
                            col = (col / tw + 1) * tw;
                        } else {
                            col += 1;
                            leading = false;
                        }
                        out.push(c);
                    }
                    out.push_str(&" ".repeat(pending));
                    out
                })
                .collect()
        });
    }

    pub fn bookmarked_lines_text(&self) -> String {
        let mut out = String::new();
        for &l in &self.bookmarks {
            if l < self.line_count() {
                out.push_str(&self.line_text(l));
                out.push('\n');
            }
        }
        out
    }

    /// Remove lines that are (or are not) bookmarked.
    pub fn remove_lines_where(&mut self, bookmarked: bool) {
        let n = self.line_count();
        let keep: Vec<String> = (0..n).filter(|l| self.bookmarks.contains(l) != bookmarked).map(|l| self.line_text(l)).collect();
        self.begin(EditKind::Other);
        let all = self.len();
        self.replace(0, all, &keep.join("\n"));
        self.set_caret(0);
        self.end();
        if bookmarked {
            self.bookmarks.clear();
        }
    }

    /// Insert text at a visual column for each line in [first, last], padding short lines.
    pub fn insert_column(&mut self, first: usize, last: usize, col: usize, texts: &[String]) {
        self.begin(EditKind::Other);
        let lines: Vec<usize> = (first..=last).collect();
        for (i, &l) in lines.iter().enumerate().rev() {
            let Some(t) = texts.get(i) else { continue };
            let end_col = self.col_of(self.line_end(l));
            let (pos, pad) = if end_col < col { (self.line_end(l), " ".repeat(col - end_col)) } else { (self.pos_at_col(l, col), String::new()) };
            self.replace(pos, 0, &format!("{pad}{t}"));
        }
        let p = self.pos_at_col(first, col);
        self.set_caret(p);
        self.end();
    }
}

pub fn sort_lines(mut v: Vec<String>, kind: u8) -> Vec<String> {
    match kind {
        0 => v.sort(),
        1 => {
            v.sort();
            v.reverse();
        }
        2 => v.sort_by_key(|a| a.to_lowercase()),
        3 => {
            v.sort_by_key(|a| a.to_lowercase());
            v.reverse();
        }
        4 | 5 => {
            let key = |s: &String| s.trim().parse::<f64>().unwrap_or(f64::NEG_INFINITY);
            v.sort_by(|a, b| key(a).partial_cmp(&key(b)).unwrap_or(std::cmp::Ordering::Equal));
            if kind == 5 {
                v.reverse();
            }
        }
        _ => v.sort_by_key(|a| a.chars().count()),
    }
    v
}

#[cfg(test)]
mod tests {
    use crate::doc::*;
    use crate::highlight::Highlighter;
    use crate::lang::Lang;

    fn doc(text: &str) -> Document {
        let mut d = Document::new("t".into());
        d.set_content(text);
        d
    }

    #[test]
    fn typing_undo_redo_and_modified_flag() {
        let mut d = doc("hello\n");
        d.set_caret(5);
        for c in [" ", "w", "o", "r", "l", "d"] {
            d.insert_text(c, false);
        }
        assert_eq!(d.text_string(), "hello world\n");
        assert!(d.is_modified());
        d.undo();
        assert_eq!(d.text_string(), "hello\n");
        assert!(!d.is_modified());
        d.redo();
        assert_eq!(d.text_string(), "hello world\n");
        d.mark_saved();
        assert!(!d.is_modified());
        d.backspace();
        assert!(d.is_modified());
        d.undo();
        assert!(!d.is_modified());
    }

    #[test]
    fn multi_caret_insert_and_backspace() {
        let mut d = doc("ab\ncd\nef");
        d.sels = vec![Sel::at(1), Sel::at(4), Sel::at(7)];
        d.insert_text("X", false);
        assert_eq!(d.text_string(), "aXb\ncXd\neXf");
        d.backspace();
        assert_eq!(d.text_string(), "ab\ncd\nef");
        d.paste("1\n2\n3\n");
        assert_eq!(d.text_string(), "a1b\nc2d\ne3f");
    }

    #[test]
    fn toggle_comment_roundtrip() {
        let mut d = doc("  int a;\n\tint b;\n");
        d.lang = Lang::Cpp;
        d.set_sel(0, 15);
        d.line_comment(0);
        assert_eq!(d.text_string(), "  // int a;\n\t// int b;\n");
        d.set_sel(0, d.line_end(1));
        d.line_comment(0);
        assert_eq!(d.text_string(), "  int a;\n\tint b;\n");
        // unicode whitespace must not panic
        let mut d = doc("\u{a0}foo\n\tbar");
        d.lang = Lang::Cpp;
        d.set_sel(0, d.len());
        d.line_comment(0);
    }

    #[test]
    fn move_and_duplicate_lines() {
        let mut d = doc("1\n2\n3");
        d.set_caret(2);
        d.move_lines(true);
        assert_eq!(d.text_string(), "2\n1\n3");
        d.move_lines(false);
        d.move_lines(false);
        assert_eq!(d.text_string(), "1\n3\n2");
        d.duplicate_lines();
        assert_eq!(d.text_string(), "1\n3\n2\n2");
    }

    #[test]
    fn per_line_ops_keep_bookmarks() {
        let mut d = doc("a  \nb\nc  \n");
        d.bookmarks.insert(2);
        d.map_lines(|v| v.into_iter().map(|l| l.trim_end().to_string()).collect());
        assert_eq!(d.text_string(), "a\nb\nc\n");
        assert!(d.bookmarks.contains(&2));
        assert_eq!(d.change[1], CH_NONE);
    }

    #[test]
    fn incremental_highlighter_matches_full_lex() {
        let mut d = doc("int main() {\n  /* x\n  y */\n  return 0; // c\n}\nchar *s = \"a\";\n");
        d.lang = Lang::Cpp;
        d.hl.reset(d.line_count());
        let edits: &[(usize, usize, &str)] = &[(13, 0, "/*"), (0, 0, "{\n"), (20, 3, ""), (5, 0, "\"\n\"\n"), (2, 0, "*/"), (30, 4, "x\ny\n{")];
        for &(pos, del, ins) in edits {
            let pos = pos.min(d.len());
            let del = del.min(d.len() - pos);
            d.replace(pos, del, ins);
            let inc = d.hl.snapshot(&d.text, d.lang);
            let mut fresh = Highlighter::default();
            fresh.reset(d.line_count());
            assert_eq!(inc, fresh.snapshot(&d.text, d.lang), "after edit at {pos}");
        }
    }

    #[test]
    fn large_file_timing() {
        let mut s = String::new();
        for i in 0..100_000 {
            s.push_str(&format!("int f{i}(int a) {{ /* c */ return a + {i}; }}\n"));
        }
        let mut d = doc(&s);
        d.lang = Lang::Cpp;
        d.hl.reset(d.line_count());
        let t = std::time::Instant::now();
        d.hl.update_folds(&d.text, d.lang, d.version, 4);
        let full = t.elapsed();
        d.replace(10, 0, "x");
        let t = std::time::Instant::now();
        d.hl.update_folds(&d.text, d.lang, d.version, 4);
        let incr = t.elapsed();
        println!("100k lines: full lex+folds {full:?}, after small edit {incr:?}");
    }
}
