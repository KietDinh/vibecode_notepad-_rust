//! The custom-drawn editor control (a Scintilla-like view written from scratch).

use crate::app::{app, editor_parts, notify_main};
use crate::config::Settings;
use crate::doc::*;
use crate::highlight::St;
use crate::theme::{self, TextStyle};
use crate::win::*;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::SystemServices::{MK_CONTROL, MK_SHIFT};
use windows_sys::Win32::UI::Controls::{SetScrollInfo, ShowScrollBar};
use windows_sys::Win32::UI::Input::Ime::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

pub const CLASS: &str = "NppRsEditor";
const TIMER_CARET: usize = 1;
const TIMER_SCROLL: usize = 2;

#[derive(Clone, Copy, Debug)]
pub struct Row {
    pub line: usize,
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Copy)]
enum Drag {
    None,
    Text { anchor: usize, word: Option<(usize, usize)>, lines: bool },
    Lines { anchor_line: usize },
    Rect { line: usize, col: usize },
    MultiAdd { anchor: usize },
}

pub struct Margins {
    pub ln_x: i32,
    pub ln_w: i32,
    pub sym_x: i32,
    pub sym_w: i32,
    pub fold_x: i32,
    pub fold_w: i32,
    pub text_x: i32,
}

pub struct EditorView {
    pub hwnd: HWND,
    pub results: bool,
    pub dpi: u32,
    fonts: [HFONT; 4],
    small_font: HFONT,
    pub cw: i32,
    pub lh: i32,
    font_key: (String, i32, i32, u32),
    rows: Vec<Row>,
    identity: bool,
    layout_key: (u64, u64, u64, i32, bool),
    drag: Drag,
    pub caret_on: bool,
    pub overtype: bool,
    pub focused: bool,
    max_w: i32,
    max_w_doc: u64,
    last_dbl: (u32, i32, i32),
    bookmark_icon: HICON,
    bookmark_size: i32,
    mouse: (i32, i32),
    pub begin_select: Option<usize>,
}

impl EditorView {
    pub fn new(results: bool) -> Self {
        EditorView {
            hwnd: std::ptr::null_mut(),
            results,
            dpi: 96,
            fonts: [std::ptr::null_mut(); 4],
            small_font: std::ptr::null_mut(),
            cw: 8,
            lh: 16,
            font_key: (String::new(), 0, 0, 0),
            rows: Vec::new(),
            identity: true,
            layout_key: (0, 0, 0, 0, false),
            drag: Drag::None,
            caret_on: true,
            overtype: false,
            focused: false,
            max_w: 0,
            max_w_doc: 0,
            last_dbl: (0, 0, 0),
            bookmark_icon: std::ptr::null_mut(),
            bookmark_size: 0,
            mouse: (0, 0),
            begin_select: None,
        }
    }

    pub fn create(&mut self, parent: HWND) {
        let cls = w(CLASS);
        unsafe {
            self.hwnd = CreateWindowExW(
                0,
                cls.as_ptr(),
                std::ptr::null(),
                WS_CHILD | WS_VISIBLE | WS_VSCROLL | WS_HSCROLL | WS_CLIPSIBLINGS,
                0,
                0,
                100,
                100,
                parent,
                std::ptr::null_mut(),
                hinstance(),
                std::ptr::null(),
            );
        }
        self.dpi = dpi_of(self.hwnd);
    }

    fn ensure_fonts(&mut self, s: &Settings) {
        let size = (s.font_size * 10 + s.zoom * 10).max(20);
        let key = (s.font.clone(), size, 0, self.dpi);
        if key == self.font_key && !self.fonts[0].is_null() {
            return;
        }
        unsafe {
            for f in self.fonts {
                if !f.is_null() {
                    DeleteObject(f as _);
                }
            }
            if !self.small_font.is_null() {
                DeleteObject(self.small_font as _);
            }
            self.fonts = [
                make_font(&s.font, size, self.dpi, false, false),
                make_font(&s.font, size, self.dpi, true, false),
                make_font(&s.font, size, self.dpi, false, true),
                make_font(&s.font, size, self.dpi, true, true),
            ];
            self.small_font = make_font("Segoe UI", (size * 7 / 10).max(60), self.dpi, false, false);
            let hdc = GetDC(self.hwnd);
            let old = SelectObject(hdc, self.fonts[0] as _);
            let mut tm: TEXTMETRICW = std::mem::zeroed();
            GetTextMetricsW(hdc, &mut tm);
            let mut sz = SIZE { cx: 0, cy: 0 };
            let m: Vec<u16> = "MMMMMMMMMM".encode_utf16().collect();
            GetTextExtentPoint32W(hdc, m.as_ptr(), 10, &mut sz);
            SelectObject(hdc, old);
            ReleaseDC(self.hwnd, hdc);
            self.cw = ((sz.cx + 5) / 10).max(1);
            self.lh = tm.tmHeight.max(1);
        }
        self.font_key = key;
        self.layout_key.0 = 0;
        let bs = scale(12, self.dpi).max(8) as usize;
        if self.bookmark_size != bs as i32 {
            if !self.bookmark_icon.is_null() {
                unsafe { DestroyIcon(self.bookmark_icon) };
            }
            let mut c = crate::icons::Canvas::new(bs);
            c.circle(8.0, 8.0, 7.2, 0x1E4F9A);
            c.circle(8.0, 8.0, 6.2, 0x3D86E8);
            c.circle(6.6, 6.0, 3.0, 0x8CC0FF);
            c.circle(6.2, 5.6, 1.4, 0xE8F2FF);
            self.bookmark_icon = c.to_icon();
            self.bookmark_size = bs as i32;
        }
    }

    pub fn margins(&self, doc: &Document, s: &Settings) -> Margins {
        let d = self.dpi;
        let ln_w = if s.line_numbers && !self.results {
            let digits = doc.line_count().to_string().len().max(3) as i32;
            (digits + 1) * self.cw + scale(4, d)
        } else {
            0
        };
        let sym_w = if s.bookmark_margin && !self.results { scale(16, d) } else { 0 };
        let fold_w = if s.fold_margin { scale(14, d) } else { 0 };
        let pad = if self.results { scale(4, d) } else { scale(1, d) };
        Margins { ln_x: 0, ln_w, sym_x: ln_w, sym_w, fold_x: ln_w + sym_w, fold_w, text_x: ln_w + sym_w + fold_w + pad }
    }

    fn text_width(&self, doc: &Document, s: &Settings) -> i32 {
        let rc = client_rect(self.hwnd);
        (rc.right - self.margins(doc, s).text_x).max(self.cw * 4)
    }

    pub fn visible_rows(&self) -> usize {
        let rc = client_rect(self.hwnd);
        ((rc.bottom - rc.top) / self.lh.max(1)).max(1) as usize
    }

    // ------------------------------------------------------------------ layout

    pub fn ensure_layout(&mut self, doc: &mut Document, s: &Settings) {
        self.ensure_fonts(s);
        let wrap = s.word_wrap && !self.results;
        let width = if wrap { self.text_width(doc, s) } else { 0 };
        let fh = doc.folded.iter().fold(doc.folded.len() as u64, |h, &x| h.wrapping_mul(1_000_003).wrapping_add(x as u64));
        let key = (doc.id, doc.version, fh, width, wrap);
        if key == self.layout_key {
            return;
        }
        self.layout_key = key;
        if !wrap && doc.folded.is_empty() {
            self.identity = true;
            self.rows.clear();
            return;
        }
        self.identity = false;
        self.rows.clear();
        if !doc.folded.is_empty() {
            doc.hl.update_folds(&doc.text, doc.lang, doc.version, doc.tab_width);
            // drop stale fold headers
            let stale: Vec<usize> = doc.folded.iter().copied().filter(|&h| doc.hl.fold_end(h).is_none()).collect();
            for h in stale {
                doc.folded.remove(&h);
            }
        }
        let n = doc.line_count();
        let cols = (width / self.cw).max(8) as usize;
        let folded: Vec<(usize, usize)> = doc.folded.iter().filter_map(|&h| doc.hl.fold_end(h).map(|e| (h, e))).collect();
        let mut fi = 0;
        let mut line = 0;
        while line < n {
            let len = doc.line_len(line);
            if !wrap || len == 0 {
                self.rows.push(Row { line, start: 0, end: len });
            } else {
                let chars = doc.line_chars(line);
                let mut start = 0;
                let mut col0 = 0usize;
                let mut col = 0usize;
                let mut last_break = None;
                let mut i = 0;
                while i < chars.len() {
                    let c = chars[i];
                    let nc = doc.advance(col, c);
                    if nc - col0 > cols && i > start {
                        let br = match last_break {
                            Some(b) if b > start => b,
                            _ => i,
                        };
                        self.rows.push(Row { line, start, end: br });
                        start = br;
                        // recompute col at new start
                        col0 = chars[..start].iter().fold(0, |c, &ch| doc.advance(c, ch));
                        col = col0;
                        last_break = None;
                        i = start;
                        continue;
                    }
                    col = nc;
                    if c == ' ' || c == '\t' {
                        last_break = Some(i + 1);
                    }
                    i += 1;
                }
                self.rows.push(Row { line, start, end: chars.len() });
            }
            // skip folded body
            while fi < folded.len() && folded[fi].0 < line {
                fi += 1;
            }
            if fi < folded.len() && folded[fi].0 == line {
                line = folded[fi].1 + 1;
                fi += 1;
            } else {
                line += 1;
            }
        }
    }

    pub fn nrows(&self, doc: &Document) -> usize {
        if self.identity {
            doc.line_count()
        } else {
            self.rows.len().max(1)
        }
    }

    pub fn row(&self, doc: &Document, r: usize) -> Row {
        if self.identity {
            let line = r.min(doc.line_count() - 1);
            Row { line, start: 0, end: doc.line_len(line) }
        } else if let Some(row) = self.rows.get(r) {
            *row
        } else {
            *self.rows.last().unwrap_or(&Row { line: 0, start: 0, end: 0 })
        }
    }

    pub fn row_of_line(&self, line: usize) -> usize {
        if self.identity {
            return line;
        }
        let idx = self.rows.partition_point(|r| r.line < line);
        if idx < self.rows.len() && self.rows[idx].line == line {
            idx
        } else {
            idx.saturating_sub(1)
        }
    }

    pub fn row_of_pos(&self, doc: &Document, pos: usize) -> usize {
        let line = doc.line_of(pos);
        let mut r = self.row_of_line(line);
        if self.identity {
            return r;
        }
        let off = pos - doc.line_start(line);
        while r + 1 < self.rows.len() && self.rows[r + 1].line == line && self.rows[r + 1].start <= off {
            r += 1;
        }
        r
    }

    fn row_start_col(&self, doc: &Document, row: &Row) -> usize {
        if row.start == 0 {
            0
        } else {
            doc.col_of(doc.line_start(row.line) + row.start)
        }
    }

    /// (row, column within the row) of a position.
    pub fn row_col(&self, doc: &Document, pos: usize) -> (usize, usize) {
        let r = self.row_of_pos(doc, pos);
        let row = self.row(doc, r);
        (r, doc.col_of(pos).saturating_sub(self.row_start_col(doc, &row)))
    }

    pub fn pos_at(&self, doc: &Document, r: usize, xcol: usize) -> usize {
        let row = self.row(doc, r);
        let base = self.row_start_col(doc, &row);
        let ls = doc.line_start(row.line);
        let p = doc.pos_at_col(row.line, base + xcol);
        let last = r + 1 >= self.nrows(doc) || self.row(doc, r + 1).line != row.line;
        let hi = if last || row.end == row.start { ls + row.end } else { ls + row.end - 1 };
        p.clamp(ls + row.start, hi)
    }

    // ------------------------------------------------------------------ scrolling

    pub fn update_scrollbars(&mut self, doc: &Document, s: &Settings) {
        let vis = self.visible_rows();
        let n = self.nrows(doc);
        let rc = client_rect(self.hwnd);
        let m = self.margins(doc, s);
        let text_w = (rc.right - m.text_x).max(1);
        if self.max_w_doc != doc.id {
            self.max_w_doc = doc.id;
            self.max_w = 0;
        }
        unsafe {
            let mut si: SCROLLINFO = std::mem::zeroed();
            si.cbSize = std::mem::size_of::<SCROLLINFO>() as u32;
            si.fMask = SIF_RANGE | SIF_PAGE | SIF_POS | SIF_DISABLENOSCROLL;
            si.nMin = 0;
            si.nMax = (n.saturating_sub(1)) as i32;
            si.nPage = vis as u32;
            si.nPos = doc.scroll_row as i32;
            SetScrollInfo(self.hwnd, SB_VERT, &si, 1);
            let wrap = s.word_wrap && !self.results;
            let content = if wrap { text_w } else { self.max_w.max(text_w + doc.scroll_x).max(text_w) };
            si.fMask = SIF_RANGE | SIF_PAGE | SIF_POS;
            si.nMax = content;
            si.nPage = text_w as u32 + 1;
            si.nPos = doc.scroll_x;
            SetScrollInfo(self.hwnd, SB_HORZ, &si, 1);
            ShowScrollBar(self.hwnd, SB_HORZ, (!wrap && content > text_w) as i32);
        }
    }

    pub fn clamp_scroll(&self, doc: &mut Document) {
        let n = self.nrows(doc);
        let vis = self.visible_rows();
        let max = n.saturating_sub(vis);
        if doc.scroll_row > max {
            doc.scroll_row = max;
        }
        if doc.scroll_x < 0 {
            doc.scroll_x = 0;
        }
    }

    /// Unfold any collapsed fold hiding `line`.
    pub fn reveal_line(&mut self, doc: &mut Document, line: usize) {
        if doc.folded.is_empty() {
            return;
        }
        doc.hl.update_folds(&doc.text, doc.lang, doc.version, doc.tab_width);
        let hidden: Vec<usize> = doc.folded.iter().copied().filter(|&h| h < line && doc.hl.fold_end(h).is_some_and(|e| e >= line)).collect();
        for h in hidden {
            doc.folded.remove(&h);
        }
    }

    pub fn ensure_caret_visible(&mut self, doc: &mut Document, s: &Settings) {
        let caret = doc.caret();
        let line = doc.line_of(caret);
        self.reveal_line(doc, line);
        self.ensure_layout(doc, s);
        let (r, col) = self.row_col(doc, caret);
        let vis = self.visible_rows();
        if r < doc.scroll_row {
            doc.scroll_row = r;
        } else if r >= doc.scroll_row + vis {
            doc.scroll_row = r + 1 - vis;
        }
        if !(s.word_wrap && !self.results) {
            let x = col as i32 * self.cw;
            let tw = self.text_width(doc, s);
            let slop = (tw / 8).max(self.cw * 2);
            if x < doc.scroll_x {
                doc.scroll_x = (x - slop).max(0);
            } else if x > doc.scroll_x + tw - self.cw * 2 {
                doc.scroll_x = x - tw + slop;
            }
        } else {
            doc.scroll_x = 0;
        }
        self.clamp_scroll(doc);
    }

    /// Scroll so that `line` is roughly centered (used by Go To / search).
    pub fn center_line(&mut self, doc: &mut Document, s: &Settings, line: usize) {
        self.reveal_line(doc, line);
        self.ensure_layout(doc, s);
        let r = self.row_of_line(line);
        let vis = self.visible_rows();
        if r < doc.scroll_row || r >= doc.scroll_row + vis {
            doc.scroll_row = r.saturating_sub(vis / 2);
        }
        self.clamp_scroll(doc);
    }

    pub fn after_change(&mut self, doc: &mut Document, s: &Settings) {
        self.ensure_layout(doc, s);
        self.ensure_caret_visible(doc, s);
        self.update_scrollbars(doc, s);
        self.reset_blink();
        invalidate(self.hwnd);
        notify_main();
    }

    pub fn reset_blink(&mut self) {
        self.caret_on = true;
        unsafe {
            if self.focused {
                SetTimer(self.hwnd, TIMER_CARET, GetCaretBlinkTime(), None);
            }
        }
    }

    // ------------------------------------------------------------------ hit testing

    /// (row, unclamped column) under a client point.
    fn hit(&self, doc: &Document, s: &Settings, x: i32, y: i32) -> (usize, usize) {
        let m = self.margins(doc, s);
        let r = (doc.scroll_row as i64 + (y.max(-1000) / self.lh) as i64 + if y < 0 { -1 } else { 0 }).clamp(0, self.nrows(doc) as i64 - 1) as usize;
        let xx = x - m.text_x + doc.scroll_x;
        let col = if xx <= 0 { 0 } else { ((xx + self.cw / 2) / self.cw) as usize };
        (r, col)
    }

    fn pos_from_point(&self, doc: &Document, s: &Settings, x: i32, y: i32) -> usize {
        let (r, col) = self.hit(doc, s, x, y);
        self.pos_at(doc, r, col)
    }

    // ------------------------------------------------------------------ painting

    fn paint(&mut self, doc: &mut Document, s: &Settings) {
        self.ensure_layout(doc, s);
        self.clamp_scroll(doc);
        let bp = BufferedPaint::begin(self.hwnd);
        let hdc = bp.hdc;
        let t = theme::theme(s.theme);
        let m = self.margins(doc, s);
        let (cw, lh) = (self.cw, self.lh);
        let width = bp.w;
        let height = bp.h;
        fill(hdc, &rect(0, 0, width, height), t.bg);
        unsafe {
            SetBkMode(hdc, TRANSPARENT as i32);
        }
        let n = self.nrows(doc);
        let vis = (height / lh + 2) as usize;
        let first = doc.scroll_row.min(n.saturating_sub(1));
        let last = (first + vis).min(n);
        let need_folds = s.fold_margin || !doc.folded.is_empty();
        if need_folds {
            doc.hl.update_folds(&doc.text, doc.lang, doc.version, doc.tab_width);
        }
        let main = doc.main_sel();
        let caret_line = doc.line_of(main.caret);
        let mut sels: Vec<Sel> = doc.sels.clone();
        sels.sort_by_key(|s| s.start());
        let brace = if s.brace_match && !self.results { doc.brace_pair() } else { None };
        let active_fold = if need_folds { doc.hl.enclosing_fold(caret_line) } else { None };
        // smart highlight word
        let smart: Option<Vec<char>> = if s.smart_highlight && !self.results && !main.is_empty() && main.end() - main.start() <= 200 {
            let w = doc.slice(main.start(), main.end());
            let ok = w.chars().all(is_word_char)
                && (main.start() == 0 || !doc.char_at(main.start() - 1).is_some_and(is_word_char))
                && !doc.char_at(main.end()).is_some_and(is_word_char);
            if ok {
                Some(w.chars().collect())
            } else {
                None
            }
        } else {
            None
        };
        let show_ws = s.show_ws;
        let show_eol = s.show_eol;
        let tw = doc.tab_width;
        let x0 = m.text_x - doc.scroll_x;
        let mut line_cache: Option<(usize, Vec<char>, Vec<usize>, Vec<St>)> = None;
        let mut widest = 0;
        let ln_font_h = self.fonts[0];

        // indent guide helper data
        for r in first..last {
            let row = self.row(doc, r);
            let line = row.line;
            let y = (r - first) as i32 * lh;
            if line_cache.as_ref().map(|c| c.0) != Some(line) {
                let chars = doc.line_chars(line);
                let mut cols = Vec::with_capacity(chars.len() + 1);
                let mut c = 0;
                for &ch in &chars {
                    cols.push(c);
                    c = doc.advance(c, ch);
                }
                cols.push(c);
                let runs = doc.hl.line_runs(&doc.text, doc.lang, line, &chars);
                let mut styles = vec![St::Default; chars.len()];
                for (k, &(start, st)) in runs.iter().enumerate() {
                    let end = runs.get(k + 1).map(|x| x.0).unwrap_or(chars.len()).min(chars.len());
                    for item in styles.iter_mut().take(end).skip(start.min(chars.len())) {
                        *item = st;
                    }
                }
                line_cache = Some((line, chars, cols, styles));
            }
            let (_, chars, cols, styles) = line_cache.as_ref().unwrap();
            let ls = doc.line_start(line);
            let base_col = cols[row.start.min(cols.len() - 1)];
            let xc = |col: usize| x0 + (col as i32 - base_col as i32) * cw;
            let is_last_row = r + 1 >= n || self.row(doc, r + 1).line != line;
            let row_end_pos = ls + row.end;
            widest = widest.max((cols[row.end.min(cols.len() - 1)] - base_col) as i32 * cw + cw * 2);

            // --- backgrounds
            if s.cur_line && line == caret_line && !self.results || (self.results && line == caret_line) {
                fill(hdc, &rect(m.text_x, y, width, y + lh), t.cur_line);
            }
            // style backgrounds (e.g. XML declaration)
            {
                let mut i = row.start;
                while i < row.end {
                    let st = styles[i];
                    let ts = theme::style(t, doc.lang, st);
                    let mut j = i + 1;
                    while j < row.end && styles[j] == st {
                        j += 1;
                    }
                    if let Some(bg) = ts.bg {
                        let xa = xc(cols[i]);
                        let xb = xc(cols[j]);
                        fill(hdc, &rect(xa.max(m.text_x), y, xb, y + lh), bg);
                    }
                    i = j;
                }
            }
            // marks / token styles
            for &(a, b, style) in doc.marks.iter() {
                if a >= b || b <= ls + row.start || a >= row_end_pos {
                    continue;
                }
                let ra = a.max(ls + row.start) - ls;
                let rb = b.min(row_end_pos) - ls;
                if rb < ra {
                    continue;
                }
                let xa = xc(cols[ra]).max(m.text_x);
                let xb = xc(cols[rb]);
                if xb <= xa {
                    continue;
                }
                let (color, boxed) = match style {
                    0 => (t.find_mark, true),
                    1 => (0x00FFFF, false),
                    2 => (0xFF8000, false),
                    3 => (0xFFFF00, false),
                    4 => (0x8000FF, false),
                    5 => (0x008000, false),
                    _ => (t.sr_hit_bg, false),
                };
                if style == 6 {
                    fill(hdc, &rect(xa, y, xb, y + lh), color);
                } else {
                    fill(hdc, &rect(xa, y, xb, y + lh), blend(color, t.bg, if boxed { 70 } else { 110 }));
                    if boxed {
                        frame(hdc, &rect(xa, y, xb, y + lh), blend(color, t.bg, 200));
                    }
                }
            }
            // smart highlighting
            if let Some(word) = &smart {
                let wl = word.len();
                if wl > 0 && chars.len() >= wl {
                    let mut i = row.start;
                    while i + wl <= row.end {
                        let eq = if s.smart_match_case {
                            chars[i..i + wl] == word[..]
                        } else {
                            chars[i..i + wl].iter().zip(word.iter()).all(|(a, b)| a.to_lowercase().eq(b.to_lowercase()))
                        };
                        if eq && (i == 0 || !is_word_char(chars[i - 1])) && (i + wl >= chars.len() || !is_word_char(chars[i + wl])) {
                            let xa = xc(cols[i]).max(m.text_x);
                            let xb = xc(cols[i + wl]);
                            fill(hdc, &rect(xa, y, xb, y + lh), blend(t.smart, t.bg, 90));
                            frame(hdc, &rect(xa, y, xb, y + lh), blend(t.smart, t.bg, 220));
                            i += wl;
                        } else {
                            i += 1;
                        }
                    }
                }
            }
            // selections
            for sel in &sels {
                if sel.is_empty() {
                    continue;
                }
                if sel.end() < ls + row.start || sel.start() > row_end_pos {
                    continue;
                }
                let a = sel.start().max(ls + row.start) - ls;
                let b = sel.end().min(row_end_pos) - ls;
                let xa = xc(cols[a]).max(m.text_x);
                let mut xb = xc(cols[b]);
                if sel.end() > row_end_pos && is_last_row {
                    xb += cw;
                }
                if xb > xa {
                    fill(hdc, &rect(xa, y, xb, y + lh), t.sel_bg);
                }
            }
            // edge
            if s.edge_column > 0 && !self.results {
                let ex = x0 + s.edge_column as i32 * cw;
                if ex > m.text_x {
                    vline(hdc, ex, y, y + lh, t.edge);
                }
            }

            // --- indent guides
            if s.indent_guides && !self.results && row.start == 0 {
                let ind = guide_indent(doc, line, chars, tw);
                let mut k = tw;
                while k < ind {
                    let gx = xc(k);
                    if gx >= m.text_x {
                        let mut yy = y;
                        while yy < y + lh {
                            if (yy & 1) == 0 {
                                fill(hdc, &rect(gx, yy, gx + 1, yy + 1), t.guide);
                            }
                            yy += 1;
                        }
                    }
                    k += tw;
                }
            }

            // --- text
            unsafe {
                IntersectClipRect(hdc, m.text_x, y, width, y + lh);
            }
            let mut buf: Vec<u16> = Vec::with_capacity(256);
            let mut dx: Vec<i32> = Vec::with_capacity(256);
            let mut run_x = 0;
            let mut run_style: Option<TextStyle> = None;
            let flush = |buf: &mut Vec<u16>, dx: &mut Vec<i32>, run_x: i32, st: Option<TextStyle>| {
                if buf.is_empty() {
                    return;
                }
                if let Some(ts) = st {
                    let fi = ts.bold as usize + ts.italic as usize * 2;
                    unsafe {
                        SelectObject(hdc, self.fonts[fi] as _);
                        SetTextColor(hdc, rgb(ts.fg));
                        ExtTextOutW(hdc, run_x, y, 0, std::ptr::null(), buf.as_ptr(), buf.len() as u32, dx.as_ptr());
                    }
                }
                buf.clear();
                dx.clear();
            };
            let vis_right = width + cw;
            for i in row.start..row.end {
                let cx = xc(cols[i]);
                if cx > vis_right {
                    break;
                }
                let c = chars[i];
                let next_x = xc(cols[i + 1]);
                if next_x < m.text_x - cw * 2 {
                    continue;
                }
                let p = ls + i;
                let mut ts = theme::style(t, doc.lang, styles[i]);
                if let Some((bp, mp)) = brace {
                    if p == bp || Some(p) == mp {
                        ts = TextStyle { fg: if mp.is_some() { t.brace } else { t.bad_brace }, bg: None, bold: true, italic: false };
                    }
                }
                if c == '\t' || (c as u32) < 0x20 || c as u32 == 0x7F {
                    flush(&mut buf, &mut dx, run_x, run_style);
                    run_style = None;
                    if c == '\t' {
                        if show_ws {
                            let mid = y + lh / 2;
                            let xa = cx + 2;
                            let xb = next_x - 3;
                            if xb > xa {
                                hline(hdc, xa, xb, mid, t.ws);
                                for k in 1..=3 {
                                    fill(hdc, &rect(xb - k, mid - k, xb - k + 1, mid + k + 1), t.ws);
                                }
                            }
                        }
                    } else {
                        let name = ctrl_name(c);
                        let r2 = rect(cx + 1, y + 1, next_x - 1, y + lh - 1);
                        fill(hdc, &r2, t.fg);
                        unsafe {
                            SelectObject(hdc, self.small_font as _);
                            SetTextColor(hdc, rgb(t.bg));
                        }
                        let tw_ = text_width(hdc, name);
                        text_out(hdc, (r2.left + r2.right - tw_) / 2, y + (lh - self.lh * 8 / 10) / 2, name);
                    }
                    continue;
                }
                if c == ' ' && show_ws {
                    let d = scale(1, self.dpi).max(1);
                    let mx = (cx + next_x) / 2;
                    let my = y + lh / 2;
                    fill(hdc, &rect(mx - d / 2 - (d & 1 ^ 1) / 2, my - d / 2, mx - d / 2 + d + 1, my - d / 2 + d + 1), t.ws);
                }
                let same = run_style.is_some_and(|r| r.fg == ts.fg && r.bold == ts.bold && r.italic == ts.italic);
                if !same {
                    flush(&mut buf, &mut dx, run_x, run_style);
                    run_style = Some(ts);
                    run_x = cx;
                }
                let mut tmp = [0u16; 2];
                let units = c.encode_utf16(&mut tmp);
                buf.push(units[0]);
                dx.push(next_x - cx);
                if units.len() > 1 {
                    buf.push(units[1]);
                    dx.push(0);
                }
            }
            flush(&mut buf, &mut dx, run_x, run_style);
            // EOL marker
            if show_eol && is_last_row && line + 1 < doc.line_count() {
                let ex = xc(cols[chars.len()]) + 2;
                let label = match doc.eol {
                    Eol::Crlf => "CRLF",
                    Eol::Lf => "LF",
                    Eol::Cr => "CR",
                };
                unsafe {
                    SelectObject(hdc, self.small_font as _);
                }
                let lw = text_width(hdc, label) + scale(6, self.dpi);
                let r2 = rect(ex, y + 2, ex + lw, y + lh - 2);
                fill(hdc, &r2, t.eol);
                unsafe {
                    SetTextColor(hdc, rgb(if t.dark { 0x202020 } else { 0x404040 }));
                }
                let th = {
                    let mut tm: TEXTMETRICW = unsafe { std::mem::zeroed() };
                    unsafe { GetTextMetricsW(hdc, &mut tm) };
                    tm.tmHeight
                };
                text_out(hdc, ex + scale(3, self.dpi), y + (lh - th) / 2, label);
            }
            // collapsed fold line
            if is_last_row && doc.folded.contains(&line) && doc.hl.fold_end(line).is_some() {
                hline(hdc, m.text_x, width, y + lh - 1, t.fold_fg);
            }
            // carets
            if self.caret_on && (self.focused || self.results) {
                for (k, sel) in doc.sels.iter().enumerate() {
                    let cp = sel.caret;
                    if cp < ls + row.start || cp > row_end_pos {
                        continue;
                    }
                    if cp == row_end_pos && !is_last_row {
                        continue;
                    }
                    if !self.focused {
                        continue;
                    }
                    let cx = xc(cols[cp - ls]);
                    let color = if k == doc.main { t.caret } else { if t.dark { 0xC0C0C0 } else { 0x404040 } };
                    if self.overtype {
                        fill(hdc, &rect(cx, y + lh - scale(2, self.dpi), cx + cw, y + lh), color);
                    } else {
                        let cwid = scale(s.caret_width.max(1), self.dpi).max(1);
                        fill(hdc, &rect(cx, y, cx + cwid, y + lh), color);
                    }
                }
            }
            unsafe {
                SelectClipRgn(hdc, std::ptr::null_mut());
            }

            // --- margins (drawn after text so they cover any overflow)
            if m.ln_w > 0 {
                fill(hdc, &rect(m.ln_x, y, m.ln_x + m.ln_w, y + lh), t.ln_bg);
                if row.start == 0 {
                    let s_ = (line + 1).to_string();
                    unsafe {
                        SelectObject(hdc, ln_font_h as _);
                        SetTextColor(hdc, rgb(t.ln_fg));
                    }
                    let tw_ = s_.len() as i32 * cw;
                    text_out(hdc, m.ln_x + m.ln_w - tw_ - scale(4, self.dpi), y, &s_);
                }
            }
            if m.sym_w > 0 {
                fill(hdc, &rect(m.sym_x, y, m.sym_x + m.sym_w, y + lh), t.sym_bg);
                if s.change_history {
                    let ch = doc.change.get(line).copied().unwrap_or(CH_NONE);
                    if ch != CH_NONE {
                        let bw = scale(3, self.dpi);
                        let cx = m.sym_x + m.sym_w - bw - scale(1, self.dpi);
                        fill(hdc, &rect(cx, y, cx + bw, y + lh), if ch == CH_MODIFIED { t.ch_modified } else { t.ch_saved });
                    }
                }
                if row.start == 0 && doc.bookmarks.contains(&line) {
                    let bs = self.bookmark_size;
                    unsafe {
                        DrawIconEx(hdc, m.sym_x + (m.sym_w - bs) / 2 - scale(1, self.dpi), y + (lh - bs) / 2, self.bookmark_icon, bs, bs, 0, std::ptr::null_mut(), DI_NORMAL);
                    }
                }
            }
            if m.fold_w > 0 {
                fill(hdc, &rect(m.fold_x, y, m.fold_x + m.fold_w, y + lh), t.fold_margin);
                let info = doc.hl.folds.get(line).copied().unwrap_or_default();
                let active = active_fold.is_some_and(|(h, e)| line >= h && line <= e);
                let col = if active { t.fold_active } else { t.fold_fg };
                let cx = m.fold_x + m.fold_w / 2;
                let bsz = (scale(9, self.dpi) | 1).min(lh - 2);
                let by = y + (lh - bsz) / 2;
                let is_header = (info.end as usize) > line;
                if row.start > 0 {
                    if info.depth > 0 || is_header {
                        vline(hdc, cx, y, y + lh, col);
                    }
                } else if is_header {
                    let collapsed = doc.folded.contains(&line);
                    let outer_col = if active && active_fold.map(|a| a.0) != Some(line) { t.fold_active } else if active { t.fold_active } else { t.fold_fg };
                    if info.depth > 0 {
                        let pc = if active && active_fold.map(|a| a.0) == Some(line) { t.fold_fg } else { col };
                        vline(hdc, cx, y, by, pc);
                    }
                    if !collapsed {
                        vline(hdc, cx, by + bsz, y + lh, outer_col);
                    } else if info.depth > 0 {
                        vline(hdc, cx, by + bsz, y + lh, t.fold_fg);
                    }
                    let br = rect(cx - bsz / 2, by, cx + bsz / 2 + 1, by + bsz);
                    fill(hdc, &br, t.fold_fill);
                    frame(hdc, &br, outer_col);
                    let arm = bsz / 2 - scale(2, self.dpi);
                    hline(hdc, cx - arm, cx + arm + 1, by + bsz / 2, outer_col);
                    if collapsed {
                        vline(hdc, cx, by + bsz / 2 - arm, by + bsz / 2 + arm + 1, outer_col);
                    }
                } else if info.ends > 0 {
                    let my = y + lh / 2;
                    vline(hdc, cx, y, my + 1, col);
                    hline(hdc, cx, cx + bsz / 2 + 1, my, col);
                    if info.depth > info.ends {
                        let oc = if active && active_fold.is_some_and(|(_, e)| e == line) { t.fold_fg } else { col };
                        vline(hdc, cx, my, y + lh, oc);
                    }
                } else if info.depth > 0 {
                    vline(hdc, cx, y, y + lh, col);
                }
            }
        }
        // empty area below last row: margins continue
        let used = (last - first) as i32 * lh;
        if used < height {
            if m.ln_w > 0 {
                fill(hdc, &rect(m.ln_x, used, m.ln_x + m.ln_w, height), t.ln_bg);
            }
            if m.sym_w > 0 {
                fill(hdc, &rect(m.sym_x, used, m.sym_x + m.sym_w, height), t.sym_bg);
            }
            if m.fold_w > 0 {
                fill(hdc, &rect(m.fold_x, used, m.fold_x + m.fold_w, height), t.fold_margin);
            }
        }
        if widest + doc.scroll_x > self.max_w {
            self.max_w = widest + doc.scroll_x;
        }
        drop(bp);
    }

    // ------------------------------------------------------------------ navigation

    fn move_carets(&mut self, doc: &mut Document, s: &Settings, extend: bool, f: impl Fn(&Self, &Document, Sel) -> (usize, Option<usize>)) {
        let mut sels = doc.sels.clone();
        for sel in sels.iter_mut() {
            let (p, vcol) = f(self, doc, *sel);
            sel.caret = p;
            if !extend {
                sel.anchor = p;
            }
            sel.vcol = vcol;
        }
        doc.sels = sels;
        doc.rect = None;
        doc.normalize_sels();
        doc.break_coalesce();
        self.after_change(doc, s);
    }

    fn vertical(&self, doc: &Document, sel: Sel, delta: isize) -> (usize, Option<usize>) {
        let (r, col) = self.row_col(doc, sel.caret);
        let vcol = sel.vcol.unwrap_or(col);
        let n = self.nrows(doc) as isize;
        let nr = r as isize + delta;
        if nr < 0 {
            return (0, Some(vcol));
        }
        if nr >= n {
            return (doc.len(), Some(vcol));
        }
        (self.pos_at(doc, nr as usize, vcol), Some(vcol))
    }

    fn rect_extend(&mut self, doc: &mut Document, s: &Settings, dl: isize, dc: isize) {
        let r = match doc.rect {
            Some(r) => r,
            None => {
                let c = doc.caret();
                let line = doc.line_of(c);
                let col = doc.col_of(c);
                RectSel { anchor_line: line, anchor_col: col, caret_line: line, caret_col: col }
            }
        };
        let nl = (r.caret_line as isize + dl).clamp(0, doc.line_count() as isize - 1) as usize;
        let nc = (r.caret_col as isize + dc).max(0) as usize;
        doc.rect = Some(RectSel { caret_line: nl, caret_col: nc, ..r });
        doc.apply_rect();
        self.after_change(doc, s);
    }

    /// Handle a navigation / editing key with explicit modifiers (also used by macro playback).
    pub fn key(&mut self, doc: &mut Document, s: &Settings, vk: u16, ctrl: bool, shift: bool, alt: bool) -> bool {
        let extend = shift || self.begin_select.is_some();
        let page = self.visible_rows().saturating_sub(1).max(1) as isize;
        match vk {
            VK_LEFT | VK_RIGHT | VK_UP | VK_DOWN if alt && shift => {
                let (dl, dc) = match vk {
                    VK_LEFT => (0, -1),
                    VK_RIGHT => (0, 1),
                    VK_UP => (-1, 0),
                    _ => (1, 0),
                };
                self.rect_extend(doc, s, dl, dc);
            }
            VK_LEFT => {
                self.move_carets(doc, s, extend, |_, d, sel| {
                    if !extend && !sel.is_empty() && !ctrl {
                        return (sel.start(), None);
                    }
                    if ctrl {
                        (d.word_left(sel.caret), None)
                    } else {
                        (sel.caret.saturating_sub(1), None)
                    }
                });
            }
            VK_RIGHT => {
                self.move_carets(doc, s, extend, |_, d, sel| {
                    if !extend && !sel.is_empty() && !ctrl {
                        return (sel.end(), None);
                    }
                    if ctrl {
                        (d.word_right(sel.caret), None)
                    } else {
                        ((sel.caret + 1).min(d.len()), None)
                    }
                });
            }
            VK_UP | VK_DOWN if ctrl => {
                let delta: isize = if vk == VK_UP { -1 } else { 1 };
                self.scroll_rows(doc, s, delta);
            }
            VK_UP => self.move_carets(doc, s, extend, |v, d, sel| v.vertical(d, sel, -1)),
            VK_DOWN => self.move_carets(doc, s, extend, |v, d, sel| v.vertical(d, sel, 1)),
            VK_PRIOR | VK_NEXT => {
                let delta = if vk == VK_PRIOR { -page } else { page };
                let n = self.nrows(doc);
                let vis = self.visible_rows();
                doc.scroll_row = (doc.scroll_row as isize + delta).clamp(0, n.saturating_sub(vis) as isize) as usize;
                self.move_carets(doc, s, extend, |v, d, sel| v.vertical(d, sel, delta));
            }
            VK_HOME => {
                self.move_carets(doc, s, extend, |v, d, sel| {
                    if ctrl {
                        return (0, None);
                    }
                    let r = v.row_of_pos(d, sel.caret);
                    let row = v.row(d, r);
                    let ls = d.line_start(row.line);
                    if row.start > 0 {
                        return (ls + row.start, None);
                    }
                    let first = ls + d.line_chars(row.line).iter().take_while(|c| **c == ' ' || **c == '\t').count();
                    (if sel.caret == first { ls } else { first }, None)
                });
            }
            VK_END => {
                self.move_carets(doc, s, extend, |v, d, sel| {
                    if ctrl {
                        return (d.len(), None);
                    }
                    let r = v.row_of_pos(d, sel.caret);
                    let row = v.row(d, r);
                    let last = r + 1 >= v.nrows(d) || v.row(d, r + 1).line != row.line;
                    let ls = d.line_start(row.line);
                    (if last { ls + row.end } else { ls + row.end - 1 }, Some(usize::MAX / 2))
                });
            }
            VK_BACK => {
                if doc.read_only {
                    return true;
                }
                if ctrl && shift {
                    doc.delete_line_part(true);
                } else if ctrl {
                    doc.delete_word(true);
                } else if alt {
                    doc.undo();
                } else {
                    doc.backspace();
                }
                self.after_change(doc, s);
            }
            VK_DELETE => {
                if doc.read_only {
                    return true;
                }
                if ctrl && shift {
                    doc.delete_line_part(false);
                } else if ctrl {
                    doc.delete_word(false);
                } else {
                    doc.delete_fwd();
                }
                self.after_change(doc, s);
            }
            VK_RETURN if !ctrl && !alt => {
                if self.results {
                    let line = doc.line_of(doc.caret());
                    unsafe {
                        PostMessageW(app().hwnd, crate::app::WM_APP_RESULT, line, 0);
                    }
                    return true;
                }
                if doc.read_only {
                    return true;
                }
                doc.newline(s.auto_indent, s.use_spaces);
                self.after_change(doc, s);
            }
            VK_TAB if !ctrl && !alt => {
                if doc.read_only {
                    return true;
                }
                if shift {
                    doc.outdent_lines();
                } else {
                    doc.tab(s.use_spaces);
                }
                self.after_change(doc, s);
            }
            VK_INSERT if !ctrl && !shift => {
                self.overtype = !self.overtype;
                invalidate(self.hwnd);
                notify_main();
            }
            VK_ESCAPE => {
                if doc.sels.len() > 1 || doc.rect.is_some() {
                    let m = doc.main_sel();
                    doc.sels = vec![Sel::at(m.caret)];
                    doc.main = 0;
                    doc.rect = None;
                    self.after_change(doc, s);
                }
                self.begin_select = None;
                return false;
            }
            _ => return false,
        }
        true
    }

    pub fn scroll_rows(&mut self, doc: &mut Document, s: &Settings, delta: isize) {
        if !self.results {
            crate::app::autocomplete_close();
        }
        self.ensure_layout(doc, s);
        let n = self.nrows(doc);
        let vis = self.visible_rows();
        let max = n.saturating_sub(vis) as isize;
        doc.scroll_row = (doc.scroll_row as isize + delta).clamp(0, max.max(0)) as usize;
        self.update_scrollbars(doc, s);
        invalidate(self.hwnd);
    }

    fn scroll_x(&mut self, doc: &mut Document, s: &Settings, px: i32) {
        if s.word_wrap && !self.results {
            return;
        }
        let tw = self.text_width(doc, s);
        let max = (self.max_w - tw).max(doc.scroll_x).max(0);
        doc.scroll_x = (doc.scroll_x + px).clamp(0, max.max(doc.scroll_x + px.max(0)));
        self.update_scrollbars(doc, s);
        invalidate(self.hwnd);
    }

    fn mouse_select(&mut self, doc: &mut Document, s: &Settings, x: i32, y: i32) {
        match self.drag {
            Drag::Text { anchor, word, lines } => {
                let p = self.pos_from_point(doc, s, x, y);
                let main = doc.main;
                let (a, c) = if lines {
                    let al = doc.line_of(anchor);
                    let pl = doc.line_of(p);
                    if pl >= al {
                        (doc.line_start(al), if pl + 1 < doc.line_count() { doc.line_start(pl + 1) } else { doc.len() })
                    } else {
                        (if al + 1 < doc.line_count() { doc.line_start(al + 1) } else { doc.len() }, doc.line_start(pl))
                    }
                } else if let Some((wa, wb)) = word {
                    if p >= wa {
                        let (_, e) = doc.word_at(p);
                        (wa, e.max(wb))
                    } else {
                        let (st, _) = doc.word_at(p);
                        (wb, st)
                    }
                } else {
                    (anchor, p)
                };
                if let Some(sel) = doc.sels.get_mut(main) {
                    sel.anchor = a;
                    sel.caret = c;
                    sel.vcol = None;
                }
            }
            Drag::MultiAdd { anchor } => {
                let p = self.pos_from_point(doc, s, x, y);
                if let Some(sel) = doc.sels.last_mut() {
                    sel.anchor = anchor;
                    sel.caret = p;
                }
            }
            Drag::Lines { anchor_line } => {
                let (r, _) = self.hit(doc, s, x, y);
                let line = self.row(doc, r).line;
                let (a, c) = if line >= anchor_line {
                    (doc.line_start(anchor_line), if line + 1 < doc.line_count() { doc.line_start(line + 1) } else { doc.len() })
                } else {
                    (if anchor_line + 1 < doc.line_count() { doc.line_start(anchor_line + 1) } else { doc.len() }, doc.line_start(line))
                };
                doc.set_sel(a, c);
            }
            Drag::Rect { line, col } => {
                let (r, c) = self.hit(doc, s, x, y);
                let row = self.row(doc, r);
                let base = self.row_start_col(doc, &row);
                doc.rect = Some(RectSel { anchor_line: line, anchor_col: col, caret_line: row.line, caret_col: base + c });
                doc.apply_rect();
            }
            Drag::None => {}
        }
        doc.normalize_sels();
        doc.break_coalesce();
    }
}

/// Indentation (in columns) to draw guides for; blank lines borrow from neighbours.
fn guide_indent(doc: &Document, line: usize, chars: &[char], tw: usize) -> usize {
    if chars.iter().any(|c| *c != ' ' && *c != '\t') {
        let mut col = 0;
        for &c in chars {
            match c {
                ' ' => col += 1,
                '\t' => col = (col / tw + 1) * tw,
                _ => break,
            }
        }
        return col;
    }
    let n = doc.line_count();
    let mut prev = 0;
    let mut next = 0;
    for l in (line.saturating_sub(200)..line).rev() {
        if !doc.is_blank_line(l) {
            prev = doc.indent_of(l);
            break;
        }
    }
    for l in line + 1..(line + 200).min(n) {
        if !doc.is_blank_line(l) {
            next = doc.indent_of(l);
            break;
        }
    }
    let _ = tw;
    prev.max(next)
}

pub fn register() {
    let cls = w(CLASS);
    unsafe {
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_DBLCLKS,
            lpfnWndProc: Some(wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance(),
            hIcon: std::ptr::null_mut(),
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_IBEAM),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: cls.as_ptr(),
            hIconSm: std::ptr::null_mut(),
        };
        RegisterClassExW(&wc);
    }
}

fn is_results(hwnd: HWND) -> Option<bool> {
    let a = crate::app::try_app()?;
    if a.view.hwnd == hwnd {
        Some(false)
    } else if a.rview.hwnd == hwnd {
        Some(true)
    } else {
        None
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let Some(results) = is_results(hwnd) else {
        return DefWindowProcW(hwnd, msg, wp, lp);
    };
    let (v, doc, s) = editor_parts(results);
    match msg {
        WM_PAINT => {
            v.paint(doc, s);
            v.update_scrollbars(doc, s);
            0
        }
        WM_ERASEBKGND => 1,
        WM_SIZE => {
            v.layout_key.0 = 0;
            v.ensure_layout(doc, s);
            v.clamp_scroll(doc);
            v.update_scrollbars(doc, s);
            invalidate(hwnd);
            0
        }
        WM_DPICHANGED_AFTERPARENT => {
            v.dpi = dpi_of(hwnd);
            v.font_key.3 = 0;
            invalidate(hwnd);
            0
        }
        WM_SETFOCUS => {
            v.focused = true;
            v.reset_blink();
            invalidate(hwnd);
            notify_main();
            if !results {
                crate::app::on_editor_focus(false);
            }
            0
        }
        WM_KILLFOCUS => {
            crate::app::autocomplete_focus_lost(wp as HWND);
            v.focused = false;
            KillTimer(hwnd, TIMER_CARET);
            invalidate(hwnd);
            0
        }
        WM_TIMER => {
            if wp == TIMER_CARET {
                v.caret_on = !v.caret_on;
                invalidate(hwnd);
            } else if wp == TIMER_SCROLL {
                let rc = client_rect(hwnd);
                let (x, y) = v.mouse;
                let mut changed = false;
                if y < 0 {
                    v.scroll_rows(doc, s, -1);
                    changed = true;
                } else if y > rc.bottom {
                    v.scroll_rows(doc, s, 1);
                    changed = true;
                }
                let m = v.margins(doc, s);
                if x < m.text_x && !matches!(v.drag, Drag::Lines { .. }) {
                    v.scroll_x(doc, s, -v.cw * 2);
                    changed = true;
                } else if x > rc.right {
                    v.scroll_x(doc, s, v.cw * 2);
                    changed = true;
                }
                if changed {
                    v.mouse_select(doc, s, x, y);
                    invalidate(hwnd);
                    notify_main();
                }
            }
            0
        }
        WM_VSCROLL => {
            let code = loword(wp) as i32;
            let page = v.visible_rows().saturating_sub(1) as isize;
            let delta: isize = match code {
                x if x == SB_LINEUP as i32 => -1,
                x if x == SB_LINEDOWN as i32 => 1,
                x if x == SB_PAGEUP as i32 => -page,
                x if x == SB_PAGEDOWN as i32 => page,
                x if x == SB_THUMBTRACK as i32 || x == SB_THUMBPOSITION as i32 => {
                    let mut si: SCROLLINFO = std::mem::zeroed();
                    si.cbSize = std::mem::size_of::<SCROLLINFO>() as u32;
                    si.fMask = SIF_TRACKPOS;
                    GetScrollInfo(hwnd, SB_VERT as i32, &mut si);
                    si.nTrackPos as isize - doc.scroll_row as isize
                }
                x if x == SB_TOP as i32 => -(doc.scroll_row as isize),
                x if x == SB_BOTTOM as i32 => isize::MAX / 4,
                _ => 0,
            };
            v.scroll_rows(doc, s, delta);
            0
        }
        WM_HSCROLL => {
            let code = loword(wp) as i32;
            let tw = v.text_width(doc, s);
            let delta = match code {
                x if x == SB_LINELEFT as i32 => -v.cw * 3,
                x if x == SB_LINERIGHT as i32 => v.cw * 3,
                x if x == SB_PAGELEFT as i32 => -tw,
                x if x == SB_PAGERIGHT as i32 => tw,
                x if x == SB_THUMBTRACK as i32 || x == SB_THUMBPOSITION as i32 => {
                    let mut si: SCROLLINFO = std::mem::zeroed();
                    si.cbSize = std::mem::size_of::<SCROLLINFO>() as u32;
                    si.fMask = SIF_TRACKPOS;
                    GetScrollInfo(hwnd, SB_HORZ as i32, &mut si);
                    si.nTrackPos - doc.scroll_x
                }
                _ => 0,
            };
            v.scroll_x(doc, s, delta);
            0
        }
        WM_MOUSEWHEEL => {
            let delta = (wp >> 16) as u16 as i16 as i32;
            let keys = loword(wp) as u32;
            if keys & MK_CONTROL as u32 != 0 {
                crate::app::exec_cmd(if delta > 0 { crate::menu::Cmd::ZoomIn } else { crate::menu::Cmd::ZoomOut });
            } else if keys & MK_SHIFT as u32 != 0 {
                v.scroll_x(doc, s, -delta * v.cw * 3 / 120);
            } else {
                let mut lines: u32 = 3;
                SystemParametersInfoW(SPI_GETWHEELSCROLLLINES, 0, &mut lines as *mut u32 as _, 0);
                let rows = (-(delta as i64) * lines.min(100) as i64 / 120) as isize;
                v.scroll_rows(doc, s, if rows == 0 { -delta.signum() as isize } else { rows });
            }
            0
        }
        WM_MOUSEHWHEEL => {
            let delta = (wp >> 16) as u16 as i16 as i32;
            v.scroll_x(doc, s, delta * v.cw * 3 / 120);
            0
        }
        WM_SETCURSOR => {
            if loword(lp as usize) as u32 == HTCLIENT {
                let mut pt = POINT { x: 0, y: 0 };
                GetCursorPos(&mut pt);
                ScreenToClient(hwnd, &mut pt);
                let m = v.margins(doc, s);
                let cur = if pt.x < m.text_x { IDC_ARROW } else { IDC_IBEAM };
                SetCursor(LoadCursorW(std::ptr::null_mut(), cur));
                return 1;
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => {
            crate::app::autocomplete_close();
            SetFocus(hwnd);
            let x = x_lparam(lp);
            let y = y_lparam(lp);
            let keys = wp as u32;
            let shift = keys & MK_SHIFT as u32 != 0;
            let ctrl = keys & MK_CONTROL as u32 != 0;
            let alt = GetKeyState(VK_MENU as i32) < 0;
            v.ensure_layout(doc, s);
            let m = v.margins(doc, s);
            let (r, _) = v.hit(doc, s, x, y);
            let line = v.row(doc, r).line;
            let now = GetMessageTime() as u32;
            let triple = msg == WM_LBUTTONDOWN
                && now.wrapping_sub(v.last_dbl.0) < GetDoubleClickTime()
                && (x - v.last_dbl.1).abs() < 4
                && (y - v.last_dbl.2).abs() < 4;
            if x < m.text_x {
                if x >= m.fold_x && x < m.fold_x + m.fold_w && m.fold_w > 0 {
                    doc.hl.update_folds(&doc.text, doc.lang, doc.version, doc.tab_width);
                    if doc.hl.fold_end(line).is_some() {
                        if !doc.folded.remove(&line) {
                            doc.folded.insert(line);
                            if ctrl || shift {
                                // fold/unfold recursively: also collapse children
                            }
                        }
                        let cl = doc.line_of(doc.caret());
                        if doc.folded.contains(&line) && cl > line && doc.hl.fold_end(line).is_some_and(|e| cl <= e) {
                            let p = doc.line_start(line);
                            doc.set_caret(p);
                        }
                        v.ensure_layout(doc, s);
                        v.clamp_scroll(doc);
                        v.update_scrollbars(doc, s);
                        invalidate(hwnd);
                        notify_main();
                    }
                    return 0;
                }
                if x >= m.sym_x && x < m.sym_x + m.sym_w && m.sym_w > 0 {
                    if !doc.bookmarks.remove(&line) {
                        doc.bookmarks.insert(line);
                    }
                    invalidate(hwnd);
                    return 0;
                }
                // line number margin: select line(s)
                SetCapture(hwnd);
                if shift {
                    let anchor_line = doc.line_of(doc.main_sel().anchor);
                    v.drag = Drag::Lines { anchor_line };
                } else {
                    v.drag = Drag::Lines { anchor_line: line };
                }
                v.mouse_select(doc, s, x, y);
                v.after_change(doc, s);
                return 0;
            }
            let p = v.pos_from_point(doc, s, x, y);
            SetCapture(hwnd);
            SetTimer(hwnd, TIMER_SCROLL, 50, None);
            v.mouse = (x, y);
            if msg == WM_LBUTTONDBLCLK {
                v.last_dbl = (now, x, y);
                if results {
                    PostMessageW(app().hwnd, crate::app::WM_APP_RESULT, line, 0);
                }
                let (a, b) = doc.word_at(p);
                let (a, b) = if a == b { (p, (p + 1).min(doc.line_end(line))) } else { (a, b) };
                doc.set_sel(a, b);
                v.drag = Drag::Text { anchor: a, word: Some((a, b)), lines: false };
            } else if triple {
                let a = doc.line_start(line);
                let b = if line + 1 < doc.line_count() { doc.line_start(line + 1) } else { doc.len() };
                doc.set_sel(a, b);
                v.drag = Drag::Text { anchor: a, word: None, lines: true };
            } else if alt {
                let (r2, c) = v.hit(doc, s, x, y);
                let row = v.row(doc, r2);
                let col = v.row_start_col(doc, &row) + c;
                v.drag = Drag::Rect { line: row.line, col };
                doc.rect = Some(RectSel { anchor_line: row.line, anchor_col: col, caret_line: row.line, caret_col: col });
                doc.apply_rect();
            } else if ctrl && !shift {
                doc.sels.push(Sel::at(p));
                doc.main = doc.sels.len() - 1;
                doc.rect = None;
                v.drag = Drag::MultiAdd { anchor: p };
            } else if shift {
                let anchor = doc.main_sel().anchor;
                doc.set_sel(anchor, p);
                v.drag = Drag::Text { anchor, word: None, lines: false };
            } else {
                doc.set_caret(p);
                v.drag = Drag::Text { anchor: p, word: None, lines: false };
            }
            v.begin_select = None;
            doc.normalize_sels();
            doc.break_coalesce();
            v.reset_blink();
            v.update_scrollbars(doc, s);
            invalidate(hwnd);
            notify_main();
            0
        }
        WM_MOUSEMOVE => {
            let x = x_lparam(lp);
            let y = y_lparam(lp);
            v.mouse = (x, y);
            if !matches!(v.drag, Drag::None) && GetCapture() == hwnd {
                v.mouse_select(doc, s, x, y);
                invalidate(hwnd);
                notify_main();
            }
            0
        }
        WM_LBUTTONUP => {
            if GetCapture() == hwnd {
                ReleaseCapture();
            }
            KillTimer(hwnd, TIMER_SCROLL);
            if !matches!(v.drag, Drag::None) {
                v.drag = Drag::None;
                doc.normalize_sels();
                v.after_change(doc, s);
            }
            0
        }
        WM_CAPTURECHANGED => {
            KillTimer(hwnd, TIMER_SCROLL);
            v.drag = Drag::None;
            0
        }
        WM_RBUTTONDOWN => {
            SetFocus(hwnd);
            let x = x_lparam(lp);
            let y = y_lparam(lp);
            let m = v.margins(doc, s);
            if x >= m.text_x {
                let p = v.pos_from_point(doc, s, x, y);
                let inside = doc.sels.iter().any(|sl| !sl.is_empty() && p >= sl.start() && p <= sl.end());
                if !inside {
                    doc.set_caret(p);
                    v.after_change(doc, s);
                }
            }
            0
        }
        WM_CONTEXTMENU => {
            crate::app::editor_context_menu(results, x_lparam(lp), y_lparam(lp));
            0
        }
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            if crate::app::autocomplete_key(wp as u16) {
                return 0;
            }
            let ctrl = GetKeyState(VK_CONTROL as i32) < 0;
            let shift = GetKeyState(VK_SHIFT as i32) < 0;
            let alt = GetKeyState(VK_MENU as i32) < 0;
            if v.key(doc, s, wp as u16, ctrl, shift, alt) {
                crate::app::record_key(wp as u16, ctrl, shift, alt);
                return 0;
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_CHAR => {
            let c = wp as u32;
            if c < 0x20 || c == 0x7F || doc.read_only || results {
                return 0;
            }
            thread_local! { static HIGH: std::cell::Cell<u16> = const { std::cell::Cell::new(0) }; }
            let ch: Option<char> = if (0xD800..0xDC00).contains(&c) {
                HIGH.with(|h| h.set(c as u16));
                None
            } else if (0xDC00..0xE000).contains(&c) {
                let hi = HIGH.with(|h| h.get());
                char::decode_utf16([hi, c as u16]).next().and_then(|r| r.ok())
            } else {
                char::from_u32(c)
            };
            if let Some(ch) = ch {
                let mut text = ch.to_string();
                if s.auto_close {
                    let close = match ch {
                        '(' => Some(')'),
                        '[' => Some(']'),
                        '{' => Some('}'),
                        '"' => Some('"'),
                        '\'' => Some('\''),
                        _ => None,
                    };
                    let next = doc.char_at(doc.caret());
                    if matches!(ch, ')' | ']' | '}' | '"' | '\'') && next == Some(ch) && doc.sels.len() == 1 && doc.main_sel().is_empty() {
                        let p = doc.caret() + 1;
                        doc.set_caret(p);
                        v.after_change(doc, s);
                        return 0;
                    }
                    if let Some(cl) = close {
                        if doc.sels.len() == 1 && doc.main_sel().is_empty() && next.is_none_or(|n| n.is_whitespace() || ")]}".contains(n)) {
                            text.push(cl);
                            doc.insert_text(&text, false);
                            let p = doc.caret() - 1;
                            doc.set_caret(p);
                            v.after_change(doc, s);
                            crate::app::record_text(&ch.to_string());
                            return 0;
                        }
                    }
                }
                doc.insert_text(&text, v.overtype);
                if s.auto_indent && (ch == '}' || ch == ']') && crate::lang::spec(doc.lang).fold == crate::lang::Fold::Brace {
                    doc.align_closing_brace();
                }
                v.after_change(doc, s);
                crate::app::record_text(&text);
                crate::app::autocomplete_after_char(ch);
            }
            0
        }
        WM_IME_STARTCOMPOSITION => {
            let himc = ImmGetContext(hwnd);
            if !himc.is_null() {
                let (r, col) = v.row_col(doc, doc.caret());
                let m = v.margins(doc, s);
                let x = m.text_x + col as i32 * v.cw - doc.scroll_x;
                let y = (r as i32 - doc.scroll_row as i32) * v.lh;
                let cf = COMPOSITIONFORM { dwStyle: CFS_POINT, ptCurrentPos: POINT { x, y }, rcArea: rect(0, 0, 0, 0) };
                ImmSetCompositionWindow(himc, &cf);
                let mut lf: LOGFONTW = std::mem::zeroed();
                GetObjectW(v.fonts[0] as _, std::mem::size_of::<LOGFONTW>() as i32, &mut lf as *mut _ as _);
                ImmSetCompositionFontW(himc, &lf);
                ImmReleaseContext(hwnd, himc);
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_GETDLGCODE => (DLGC_WANTALLKEYS | DLGC_WANTCHARS | DLGC_WANTTAB) as isize,
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
