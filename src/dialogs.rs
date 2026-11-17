//! Native dialogs built from in-memory DLGTEMPLATEEX (no .rc file needed).

use crate::win::*;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::System::SystemServices::{SS_ICON, SS_LEFT, SS_RIGHT};
use windows_sys::Win32::UI::Controls::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, SetFocus};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

pub const IDOK_: u16 = 1;
pub const IDCANCEL_: u16 = 2;

pub const BS_PUSH: u32 = WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_PUSHBUTTON as u32;
pub const BS_DEF: u32 = WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_DEFPUSHBUTTON as u32;
pub const BS_CHECK: u32 = WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_AUTOCHECKBOX as u32;
pub const BS_RADIO1: u32 = WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_GROUP | BS_AUTORADIOBUTTON as u32;
pub const BS_RADIO: u32 = WS_CHILD | WS_VISIBLE | BS_AUTORADIOBUTTON as u32;
pub const BS_GROUPBOX_: u32 = WS_CHILD | WS_VISIBLE | BS_GROUPBOX as u32;
pub const ES_EDIT: u32 = WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_BORDER | ES_AUTOHSCROLL as u32;
pub const SS_TEXT: u32 = WS_CHILD | WS_VISIBLE | SS_LEFT;
pub const SS_RTEXT: u32 = WS_CHILD | WS_VISIBLE | SS_RIGHT;
pub const CB_EDIT: u32 = WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_VSCROLL | CBS_DROPDOWN as u32 | CBS_AUTOHSCROLL as u32;
pub const CB_LIST: u32 = WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_VSCROLL | CBS_DROPDOWNLIST as u32;

pub struct DlgBuilder {
    buf: Vec<u16>,
    count_at: usize,
    count: u16,
}

impl DlgBuilder {
    pub fn new(title: &str, cx: i16, cy: i16) -> Self {
        Self::with_style(title, cx, cy, WS_POPUP | WS_CAPTION | WS_SYSMENU | DS_MODALFRAME as u32, 0)
    }

    pub fn with_style(title: &str, cx: i16, cy: i16, style: u32, ex: u32) -> Self {
        let mut b = DlgBuilder { buf: Vec::new(), count_at: 0, count: 0 };
        let style = style | DS_SETFONT as u32 | DS_FIXEDSYS as u32 | DS_CENTER as u32;
        b.w16(1);
        b.w16(0xFFFF);
        b.w32(0);
        b.w32(ex);
        b.w32(style);
        b.count_at = b.buf.len();
        b.w16(0);
        b.w16(0);
        b.w16(0);
        b.w16(cx as u16);
        b.w16(cy as u16);
        b.w16(0); // menu
        b.w16(0); // class
        b.wstr(title);
        b.w16(9); // point size
        b.w16(400); // weight
        b.w16(0); // italic + charset
        b.wstr("Segoe UI");
        b
    }

    fn w16(&mut self, v: u16) {
        self.buf.push(v);
    }
    fn w32(&mut self, v: u32) {
        self.buf.push((v & 0xFFFF) as u16);
        self.buf.push((v >> 16) as u16);
    }
    fn wstr(&mut self, s: &str) {
        self.buf.extend(s.encode_utf16());
        self.buf.push(0);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn item(&mut self, class: &str, text: &str, id: u16, x: i16, y: i16, cx: i16, cy: i16, style: u32) -> &mut Self {
        if self.buf.len() % 2 != 0 {
            self.buf.push(0);
        }
        self.w32(0);
        self.w32(0);
        self.w32(style);
        self.w16(x as u16);
        self.w16(y as u16);
        self.w16(cx as u16);
        self.w16(cy as u16);
        self.w32(id as u32);
        self.wstr(class);
        self.wstr(text);
        self.w16(0);
        self.count += 1;
        self
    }

    pub fn build(mut self) -> Vec<u32> {
        self.buf[self.count_at] = self.count;
        if self.buf.len() % 2 != 0 {
            self.buf.push(0);
        }
        self.buf.chunks(2).map(|c| c[0] as u32 | ((c[1] as u32) << 16)).collect()
    }
}

pub trait DlgHandler {
    fn init(&mut self, _h: HWND) {}
    /// Return Some(result) to close the dialog.
    fn command(&mut self, _h: HWND, _id: u16, _code: u16) -> Option<isize> {
        None
    }
    fn notify(&mut self, _h: HWND, _nm: &NMHDR) -> isize {
        0
    }
    fn other(&mut self, _h: HWND, _msg: u32, _wp: WPARAM, _lp: LPARAM) -> Option<isize> {
        None
    }
}

pub fn modal(owner: HWND, tpl: &[u32], handler: &mut dyn DlgHandler) -> isize {
    let mut holder: &mut dyn DlgHandler = handler;
    let ptr = &mut holder as *mut &mut dyn DlgHandler;
    unsafe { DialogBoxIndirectParamW(hinstance(), tpl.as_ptr() as *const DLGTEMPLATE, owner, Some(modal_proc), ptr as isize) }
}

unsafe extern "system" fn modal_proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> isize {
    if msg == WM_INITDIALOG {
        SetWindowLongPtrW(h, GWLP_USERDATA, lp);
        let hd = &mut **(lp as *mut &mut dyn DlgHandler);
        hd.init(h);
        return 1;
    }
    let p = GetWindowLongPtrW(h, GWLP_USERDATA) as *mut &mut dyn DlgHandler;
    if p.is_null() {
        return 0;
    }
    let hd = &mut **p;
    match msg {
        WM_COMMAND => {
            let id = loword(wp);
            let code = hiword(wp);
            if let Some(r) = hd.command(h, id, code) {
                EndDialog(h, r);
            } else if id == IDCANCEL_ {
                EndDialog(h, IDCANCEL_ as isize);
            }
            1
        }
        WM_NOTIFY => {
            let nm = &*(lp as *const NMHDR);
            hd.notify(h, nm)
        }
        WM_CLOSE => {
            EndDialog(h, IDCANCEL_ as isize);
            1
        }
        _ => hd.other(h, msg, wp, lp).unwrap_or(0),
    }
}

// ----------------------------------------------------------------- helpers

pub fn item(h: HWND, id: u16) -> HWND {
    unsafe { GetDlgItem(h, id as i32) }
}
pub fn get_item_text(h: HWND, id: u16) -> String {
    get_text(item(h, id))
}
pub fn set_item_text(h: HWND, id: u16, s: &str) {
    set_text(item(h, id), s)
}
pub fn checked(h: HWND, id: u16) -> bool {
    unsafe { IsDlgButtonChecked(h, id as i32) == BST_CHECKED }
}
pub fn set_check(h: HWND, id: u16, on: bool) {
    unsafe {
        CheckDlgButton(h, id as i32, if on { BST_CHECKED } else { BST_UNCHECKED });
    }
}
pub fn enable(h: HWND, id: u16, on: bool) {
    unsafe {
        EnableWindow(item(h, id), on as i32);
    }
}
pub fn show(h: HWND, id: u16, on: bool) {
    unsafe {
        ShowWindow(item(h, id), if on { SW_SHOW } else { SW_HIDE });
    }
}
pub fn combo_add(h: HWND, id: u16, s: &str) {
    let ws = w(s);
    send(item(h, id), CB_ADDSTRING, 0, ws.as_ptr() as isize);
}
pub fn combo_clear(h: HWND, id: u16) {
    send(item(h, id), CB_RESETCONTENT, 0, 0);
}
pub fn combo_sel(h: HWND, id: u16) -> isize {
    send(item(h, id), CB_GETCURSEL, 0, 0)
}
pub fn combo_set_sel(h: HWND, id: u16, i: usize) {
    send(item(h, id), CB_SETCURSEL, i, 0);
}
pub fn list_add(h: HWND, id: u16, s: &str) {
    let ws = w(s);
    send(item(h, id), LB_ADDSTRING, 0, ws.as_ptr() as isize);
}

// ----------------------------------------------------------------- input box

struct InputBox {
    prompt: String,
    value: String,
    result: Option<String>,
}

impl DlgHandler for InputBox {
    fn init(&mut self, h: HWND) {
        set_item_text(h, 100, &self.prompt);
        set_item_text(h, 101, &self.value);
        unsafe {
            SetFocus(item(h, 101));
        }
        send(item(h, 101), EM_SETSEL as u32, 0, -1);
    }
    fn command(&mut self, h: HWND, id: u16, _code: u16) -> Option<isize> {
        if id == IDOK_ {
            self.result = Some(get_item_text(h, 101));
            return Some(1);
        }
        None
    }
}

pub fn input_box(owner: HWND, title: &str, prompt: &str, value: &str) -> Option<String> {
    let mut b = DlgBuilder::new(title, 240, 70);
    b.item("STATIC", prompt, 100, 8, 8, 224, 10, SS_TEXT)
        .item("EDIT", "", 101, 8, 22, 224, 13, ES_EDIT)
        .item("BUTTON", "OK", IDOK_, 128, 48, 50, 14, BS_DEF)
        .item("BUTTON", "Cancel", IDCANCEL_, 182, 48, 50, 14, BS_PUSH);
    let tpl = b.build();
    let mut hd = InputBox { prompt: prompt.into(), value: value.into(), result: None };
    modal(owner, &tpl, &mut hd);
    hd.result
}

// ----------------------------------------------------------------- Go To

struct GotoDlg {
    cur_line: usize,
    lines: usize,
    cur_pos: usize,
    len: usize,
    result: Option<(bool, usize)>,
}

impl GotoDlg {
    fn refresh(&self, h: HWND) {
        let offset = checked(h, 102);
        set_item_text(h, 105, &(if offset { self.cur_pos } else { self.cur_line }).to_string());
        set_item_text(h, 109, &(if offset { self.len } else { self.lines }).to_string());
    }
}

impl DlgHandler for GotoDlg {
    fn init(&mut self, h: HWND) {
        set_check(h, 101, true);
        self.refresh(h);
        unsafe {
            SetFocus(item(h, 107));
        }
    }
    fn command(&mut self, h: HWND, id: u16, _code: u16) -> Option<isize> {
        match id {
            101 | 102 => {
                self.refresh(h);
                None
            }
            IDOK_ => {
                let v: usize = get_item_text(h, 107).trim().parse().ok()?;
                self.result = Some((checked(h, 102), v));
                Some(1)
            }
            _ => None,
        }
    }
}

/// Returns (is_offset, value).
pub fn goto_dialog(owner: HWND, cur_line: usize, lines: usize, cur_pos: usize, len: usize) -> Option<(bool, usize)> {
    let mut b = DlgBuilder::new("Go To...", 220, 96);
    b.item("BUTTON", "&Line", 101, 30, 8, 50, 10, BS_RADIO1)
        .item("BUTTON", "&Offset", 102, 100, 8, 50, 10, BS_RADIO)
        .item("STATIC", "You are here :", 104, 8, 26, 100, 10, SS_RTEXT)
        .item("STATIC", "", 105, 114, 26, 60, 10, SS_TEXT)
        .item("STATIC", "You want to go to :", 106, 8, 42, 100, 10, SS_RTEXT)
        .item("EDIT", "", 107, 114, 40, 60, 13, ES_EDIT | ES_NUMBER as u32)
        .item("STATIC", "You can't go further than :", 108, 8, 58, 100, 10, SS_RTEXT)
        .item("STATIC", "", 109, 114, 58, 60, 10, SS_TEXT)
        .item("BUTTON", "&Go", IDOK_, 50, 76, 60, 14, BS_DEF)
        .item("BUTTON", "I'm going nowhere", IDCANCEL_, 114, 76, 80, 14, BS_PUSH);
    let tpl = b.build();
    let mut hd = GotoDlg { cur_line, lines, cur_pos, len, result: None };
    modal(owner, &tpl, &mut hd);
    hd.result
}

// ----------------------------------------------------------------- About

struct AboutDlg;
impl DlgHandler for AboutDlg {
    fn init(&mut self, h: HWND) {
        let a = crate::app::app();
        let f = ui_font(16, dpi_of(h), true);
        send(item(h, 100), WM_SETFONT, f as usize, 1);
        let _ = a;
        unsafe {
            let icon = crate::icons::app_icon(scale(48, dpi_of(h)) as usize);
            SendMessageW(item(h, 99), STM_SETICON, icon as usize, 0);
        }
    }
}

pub fn about(owner: HWND) {
    let text = concat!(
        "A native Windows text editor modeled on Notepad++, written from scratch in Rust.\r\n\r\n",
        "Notepad++ is created and maintained by Don Ho. This project is independent and not affiliated with it; ",
        "it reproduces the familiar workflow (tabs, menus, Find/Replace, folding, bookmarks, macros, sessions) ",
        "with its own implementation and artwork.\r\n\r\n",
        "Built with: windows-sys, ropey, regex, serde.\r\n",
        "License: MIT"
    );
    let mut b = DlgBuilder::new("About Notepad++ RS", 260, 160);
    b.item("STATIC", "", 99, 12, 10, 32, 32, WS_CHILD | WS_VISIBLE | SS_ICON as u32)
        .item("STATIC", concat!("Notepad++ RS  v", env!("CARGO_PKG_VERSION"), "   (64-bit)"), 100, 50, 12, 200, 14, SS_TEXT)
        .item("STATIC", "Build: Rust / Win32 / GDI", 101, 50, 28, 200, 10, SS_TEXT)
        .item("EDIT", text, 102, 12, 46, 236, 86, WS_CHILD | WS_VISIBLE | WS_BORDER | WS_VSCROLL | ES_MULTILINE as u32 | ES_READONLY as u32)
        .item("BUTTON", "OK", IDOK_, 105, 140, 50, 14, BS_DEF);
    let tpl = b.build();
    struct H;
    impl DlgHandler for H {
        fn init(&mut self, h: HWND) {
            AboutDlg.init(h)
        }
        fn command(&mut self, _h: HWND, id: u16, _c: u16) -> Option<isize> {
            if id == IDOK_ {
                Some(1)
            } else {
                None
            }
        }
    }
    modal(owner, &tpl, &mut H);
}

// ----------------------------------------------------------------- Run

struct RunDlg {
    history: Vec<String>,
    result: Option<String>,
}
impl DlgHandler for RunDlg {
    fn init(&mut self, h: HWND) {
        for s in &self.history {
            combo_add(h, 101, s);
        }
        if let Some(s) = self.history.first() {
            set_item_text(h, 101, s);
        }
    }
    fn command(&mut self, h: HWND, id: u16, _c: u16) -> Option<isize> {
        match id {
            102 => {
                if let Some(p) = crate::app::open_file_dialog(h, false, "Programs (*.exe;*.bat;*.cmd)\0*.exe;*.bat;*.cmd\0All files\0*.*\0").into_iter().next() {
                    set_item_text(h, 101, &format!("\"{}\"", p.display()));
                }
                None
            }
            IDOK_ => {
                self.result = Some(get_item_text(h, 101));
                Some(1)
            }
            _ => None,
        }
    }
}

pub fn run_dialog(owner: HWND, history: &[String]) -> Option<String> {
    let mut b = DlgBuilder::new("Run...", 250, 64);
    b.item("STATIC", "The Program to Run", 100, 8, 6, 200, 10, SS_TEXT)
        .item("COMBOBOX", "", 101, 8, 18, 210, 100, CB_EDIT)
        .item("BUTTON", "...", 102, 222, 17, 20, 14, BS_PUSH)
        .item("BUTTON", "Run!", IDOK_, 136, 42, 50, 14, BS_DEF)
        .item("BUTTON", "Cancel", IDCANCEL_, 192, 42, 50, 14, BS_PUSH);
    let tpl = b.build();
    let mut hd = RunDlg { history: history.to_vec(), result: None };
    modal(owner, &tpl, &mut hd);
    hd.result
}

// ----------------------------------------------------------------- Column editor

pub enum ColumnInsert {
    Text(String),
    Numbers { init: i64, step: i64, repeat: usize, leading: u8, base: u32 },
}

struct ColumnDlg {
    result: Option<ColumnInsert>,
}
impl ColumnDlg {
    fn sync(&self, h: HWND) {
        let text = checked(h, 101);
        enable(h, 102, text);
        for id in [104u16, 105, 106, 107, 108, 109, 110, 111, 112, 113, 114, 115, 116, 117] {
            enable(h, id, !text);
        }
    }
}
impl DlgHandler for ColumnDlg {
    fn init(&mut self, h: HWND) {
        set_check(h, 101, true);
        set_check(h, 114, true);
        set_item_text(h, 105, "1");
        set_item_text(h, 107, "1");
        set_item_text(h, 109, "1");
        for s in ["None", "Zeros", "Spaces"] {
            combo_add(h, 111, s);
        }
        combo_set_sel(h, 111, 0);
        self.sync(h);
        unsafe {
            SetFocus(item(h, 102));
        }
    }
    fn command(&mut self, h: HWND, id: u16, _c: u16) -> Option<isize> {
        match id {
            101 | 103 => {
                self.sync(h);
                None
            }
            IDOK_ => {
                if checked(h, 101) {
                    self.result = Some(ColumnInsert::Text(get_item_text(h, 102)));
                } else {
                    let p = |id| get_item_text(h, id).trim().parse::<i64>().unwrap_or(0);
                    let base = if checked(h, 115) {
                        16
                    } else if checked(h, 116) {
                        8
                    } else if checked(h, 117) {
                        2
                    } else {
                        10
                    };
                    self.result = Some(ColumnInsert::Numbers { init: p(105), step: p(107), repeat: p(109).max(1) as usize, leading: combo_sel(h, 111).max(0) as u8, base });
                }
                Some(1)
            }
            _ => None,
        }
    }
}

pub fn column_editor(owner: HWND) -> Option<ColumnInsert> {
    let mut b = DlgBuilder::new("Column / Multi-Selection Editor", 230, 186);
    b.item("BUTTON", "Text to Insert", 100, 6, 4, 218, 34, BS_GROUPBOX_)
        .item("BUTTON", "", 101, 12, 18, 10, 10, BS_RADIO1)
        .item("EDIT", "", 102, 26, 17, 190, 13, ES_EDIT)
        .item("BUTTON", "Number to Insert", 118, 6, 42, 218, 118, BS_GROUPBOX_)
        .item("BUTTON", "", 103, 12, 56, 10, 10, BS_RADIO)
        .item("STATIC", "Initial number :", 104, 26, 57, 80, 10, SS_RTEXT)
        .item("EDIT", "", 105, 110, 55, 60, 13, ES_EDIT)
        .item("STATIC", "Increase by :", 106, 26, 73, 80, 10, SS_RTEXT)
        .item("EDIT", "", 107, 110, 71, 60, 13, ES_EDIT)
        .item("STATIC", "Repeat :", 108, 26, 89, 80, 10, SS_RTEXT)
        .item("EDIT", "", 109, 110, 87, 60, 13, ES_EDIT)
        .item("STATIC", "Leading :", 110, 26, 105, 80, 10, SS_RTEXT)
        .item("COMBOBOX", "", 111, 110, 103, 60, 60, CB_LIST)
        .item("BUTTON", "Format", 112, 20, 122, 196, 30, BS_GROUPBOX_)
        .item("BUTTON", "Dec", 114, 30, 135, 36, 10, BS_RADIO1)
        .item("BUTTON", "Hex", 115, 76, 135, 36, 10, BS_RADIO)
        .item("BUTTON", "Oct", 116, 122, 135, 36, 10, BS_RADIO)
        .item("BUTTON", "Bin", 117, 168, 135, 36, 10, BS_RADIO)
        .item("BUTTON", "OK", IDOK_, 116, 166, 50, 14, BS_DEF)
        .item("BUTTON", "Cancel", IDCANCEL_, 172, 166, 50, 14, BS_PUSH);
    let tpl = b.build();
    let mut hd = ColumnDlg { result: None };
    modal(owner, &tpl, &mut hd);
    hd.result
}

// ----------------------------------------------------------------- Preferences

struct PrefsDlg {
    s: crate::config::Settings,
    ok: bool,
}

const PREF_CHECKS: &[(u16, &str)] = &[
    (110, "Replace by space"),
    (111, "Auto-indent"),
    (112, "Auto-insert matching ( [ { \" '"),
    (113, "Word auto-completion"),
    (114, "Smart highlighting"),
    (115, "Highlight current line"),
    (116, "Highlight matching braces"),
    (120, "Display line number"),
    (121, "Display bookmark margin"),
    (122, "Display fold margin"),
    (123, "Display change history"),
    (124, "Show indent guides"),
    (130, "Show close button on each tab"),
    (131, "Double click to close document"),
    (140, "Remember current session for next launch"),
    (141, "Enable session snapshot and periodic backup"),
];

impl PrefsDlg {
    fn get(&self, id: u16) -> bool {
        let s = &self.s;
        match id {
            110 => s.use_spaces,
            111 => s.auto_indent,
            112 => s.auto_close,
            113 => s.auto_complete,
            114 => s.smart_highlight,
            115 => s.cur_line,
            116 => s.brace_match,
            120 => s.line_numbers,
            121 => s.bookmark_margin,
            122 => s.fold_margin,
            123 => s.change_history,
            124 => s.indent_guides,
            130 => s.tab_close_button,
            131 => s.dbl_click_close_tab,
            140 => s.remember_session,
            141 => s.snapshot,
            _ => false,
        }
    }
    fn set(&mut self, id: u16, v: bool) {
        let s = &mut self.s;
        match id {
            110 => s.use_spaces = v,
            111 => s.auto_indent = v,
            112 => s.auto_close = v,
            113 => s.auto_complete = v,
            114 => s.smart_highlight = v,
            115 => s.cur_line = v,
            116 => s.brace_match = v,
            120 => s.line_numbers = v,
            121 => s.bookmark_margin = v,
            122 => s.fold_margin = v,
            123 => s.change_history = v,
            124 => s.indent_guides = v,
            130 => s.tab_close_button = v,
            131 => s.dbl_click_close_tab = v,
            140 => s.remember_session = v,
            141 => s.snapshot = v,
            _ => {}
        }
    }
}

impl DlgHandler for PrefsDlg {
    fn init(&mut self, h: HWND) {
        for &(id, _) in PREF_CHECKS {
            set_check(h, id, self.get(id));
        }
        set_item_text(h, 151, &self.s.tab_width.to_string());
        set_item_text(h, 153, &self.s.caret_width.to_string());
        set_item_text(h, 155, &self.s.edge_column.to_string());
        for s in ["Windows (CR LF)", "Unix (LF)", "Macintosh (CR)"] {
            combo_add(h, 157, s);
        }
        combo_set_sel(h, 157, self.s.default_eol as usize);
        for s in ["ANSI", "UTF-8", "UTF-8-BOM", "UTF-16 LE BOM", "UTF-16 BE BOM"] {
            combo_add(h, 159, s);
        }
        let enc_idx = match self.s.default_encoding {
            0 => 0,
            2 => 2,
            3 => 3,
            4 => 4,
            _ => 1,
        };
        combo_set_sel(h, 159, enc_idx);
    }
    fn command(&mut self, h: HWND, id: u16, _c: u16) -> Option<isize> {
        if id == IDOK_ {
            for &(cid, _) in PREF_CHECKS {
                let v = checked(h, cid);
                self.set(cid, v);
            }
            self.s.tab_width = get_item_text(h, 151).trim().parse::<usize>().unwrap_or(4).clamp(1, 16);
            self.s.caret_width = get_item_text(h, 153).trim().parse::<i32>().unwrap_or(1).clamp(1, 4);
            self.s.edge_column = get_item_text(h, 155).trim().parse::<usize>().unwrap_or(0).min(1000);
            self.s.default_eol = combo_sel(h, 157).max(0) as u8;
            self.s.default_encoding = match combo_sel(h, 159) {
                0 => 0,
                2 => 2,
                3 => 3,
                4 => 4,
                _ => 1,
            };
            self.ok = true;
            return Some(1);
        }
        None
    }
}

pub fn preferences(owner: HWND, s: &crate::config::Settings) -> Option<crate::config::Settings> {
    let mut b = DlgBuilder::new("Preferences", 330, 252);
    b.item("BUTTON", "Editing", 100, 6, 4, 156, 128, BS_GROUPBOX_);
    let mut y = 16;
    for &(id, label) in &PREF_CHECKS[..7] {
        b.item("BUTTON", label, id, 12, y, 146, 10, BS_CHECK);
        y += 12;
    }
    b.item("STATIC", "Tab size :", 150, 12, y + 2, 60, 10, SS_TEXT).item("EDIT", "", 151, 76, y, 30, 13, ES_EDIT | ES_NUMBER as u32);
    b.item("STATIC", "Caret width :", 152, 12, y + 18, 60, 10, SS_TEXT).item("EDIT", "", 153, 76, y + 16, 30, 13, ES_EDIT | ES_NUMBER as u32);
    b.item("BUTTON", "Margins / Display", 101, 168, 4, 156, 102, BS_GROUPBOX_);
    let mut y = 16;
    for &(id, label) in &PREF_CHECKS[7..12] {
        b.item("BUTTON", label, id, 174, y, 146, 10, BS_CHECK);
        y += 12;
    }
    b.item("STATIC", "Vertical edge column (0 = off) :", 154, 174, y + 4, 110, 10, SS_TEXT).item("EDIT", "", 155, 286, y + 2, 30, 13, ES_EDIT | ES_NUMBER as u32);
    b.item("BUTTON", "Tab Bar", 102, 168, 110, 156, 40, BS_GROUPBOX_);
    b.item("BUTTON", PREF_CHECKS[12].1, 130, 174, 122, 146, 10, BS_CHECK);
    b.item("BUTTON", PREF_CHECKS[13].1, 131, 174, 134, 146, 10, BS_CHECK);
    b.item("BUTTON", "New Document", 103, 6, 136, 156, 52, BS_GROUPBOX_);
    b.item("STATIC", "Format (Line ending) :", 156, 12, 150, 70, 10, SS_TEXT).item("COMBOBOX", "", 157, 84, 148, 72, 80, CB_LIST);
    b.item("STATIC", "Encoding :", 158, 12, 168, 70, 10, SS_TEXT).item("COMBOBOX", "", 159, 84, 166, 72, 80, CB_LIST);
    b.item("BUTTON", "Backup", 104, 168, 154, 156, 40, BS_GROUPBOX_);
    b.item("BUTTON", PREF_CHECKS[14].1, 140, 174, 166, 146, 10, BS_CHECK);
    b.item("BUTTON", PREF_CHECKS[15].1, 141, 174, 178, 146, 10, BS_CHECK);
    b.item("BUTTON", "OK", IDOK_, 216, 232, 50, 14, BS_DEF).item("BUTTON", "Cancel", IDCANCEL_, 272, 232, 50, 14, BS_PUSH);
    let tpl = b.build();
    let mut hd = PrefsDlg { s: s.clone(), ok: false };
    modal(owner, &tpl, &mut hd);
    if hd.ok {
        Some(hd.s)
    } else {
        None
    }
}

// ----------------------------------------------------------------- Style configurator

pub const FONTS: &[&str] = &["Courier New", "Consolas", "Cascadia Mono", "Cascadia Code", "Lucida Console", "JetBrains Mono", "Fira Code", "Source Code Pro", "Hack", "DejaVu Sans Mono"];

struct StyleDlg {
    theme: usize,
    font: String,
    size: i32,
    ok: bool,
}
impl DlgHandler for StyleDlg {
    fn init(&mut self, h: HWND) {
        for t in crate::theme::THEMES {
            combo_add(h, 101, t.name);
        }
        combo_set_sel(h, 101, self.theme);
        for f in FONTS {
            combo_add(h, 103, f);
        }
        set_item_text(h, 103, &self.font);
        for s in [8, 9, 10, 11, 12, 14, 16, 18, 20, 22, 24] {
            combo_add(h, 105, &s.to_string());
        }
        set_item_text(h, 105, &self.size.to_string());
    }
    fn command(&mut self, h: HWND, id: u16, code: u16) -> Option<isize> {
        let apply = |s: &mut Self| {
            s.theme = combo_sel(h, 101).max(0) as usize;
            let mut f = get_item_text(h, 103);
            if code == CBN_SELCHANGE as u16 && id == 103 {
                let i = combo_sel(h, 103);
                if i >= 0 {
                    f = FONTS[i as usize].to_string();
                }
            }
            if !f.trim().is_empty() {
                s.font = f.trim().to_string();
            }
            let mut sz = get_item_text(h, 105);
            if code == CBN_SELCHANGE as u16 && id == 105 {
                let i = combo_sel(h, 105);
                let sizes = [8, 9, 10, 11, 12, 14, 16, 18, 20, 22, 24];
                if i >= 0 {
                    sz = sizes[i as usize].to_string();
                }
            }
            s.size = sz.trim().parse().unwrap_or(s.size).clamp(6, 48);
            crate::app::preview_style(s.theme, &s.font, s.size);
        };
        match id {
            101 | 103 | 105 if code == CBN_SELCHANGE as u16 || code == CBN_KILLFOCUS as u16 => {
                apply(self);
                None
            }
            IDOK_ => {
                apply(self);
                self.ok = true;
                Some(1)
            }
            _ => None,
        }
    }
}

/// Returns (theme, font, size) when accepted.
pub fn style_configurator(owner: HWND, theme: usize, font: &str, size: i32) -> Option<(usize, String, i32)> {
    let mut b = DlgBuilder::new("Style Configurator", 250, 104);
    b.item("STATIC", "Select theme :", 100, 8, 10, 70, 10, SS_TEXT)
        .item("COMBOBOX", "", 101, 80, 8, 160, 120, CB_LIST)
        .item("BUTTON", "Global Styles - Default Style", 106, 6, 28, 238, 48, BS_GROUPBOX_)
        .item("STATIC", "Font name :", 102, 14, 44, 60, 10, SS_TEXT)
        .item("COMBOBOX", "", 103, 80, 42, 100, 120, CB_EDIT)
        .item("STATIC", "Font size :", 104, 14, 60, 60, 10, SS_TEXT)
        .item("COMBOBOX", "", 105, 80, 58, 50, 120, CB_EDIT)
        .item("BUTTON", "Save && Close", IDOK_, 134, 84, 54, 14, BS_DEF)
        .item("BUTTON", "Cancel", IDCANCEL_, 192, 84, 50, 14, BS_PUSH);
    let tpl = b.build();
    let mut hd = StyleDlg { theme, font: font.into(), size, ok: false };
    modal(owner, &tpl, &mut hd);
    if hd.ok {
        Some((hd.theme, hd.font, hd.size))
    } else {
        None
    }
}

// ----------------------------------------------------------------- list dialogs

struct ListDlg {
    items: Vec<String>,
    multi: bool,
    buttons: Vec<(u16, String)>,
    result: Option<(u16, Vec<usize>)>,
}
impl DlgHandler for ListDlg {
    fn init(&mut self, h: HWND) {
        let tabs = [140i32];
        send(item(h, 100), LB_SETTABSTOPS, 1, tabs.as_ptr() as isize);
        for s in &self.items {
            list_add(h, 100, s);
        }
        send(item(h, 100), LB_SETCURSEL, 0, 0);
    }
    fn command(&mut self, h: HWND, id: u16, code: u16) -> Option<isize> {
        let selected = || -> Vec<usize> {
            let lb = item(h, 100);
            if self.multi {
                let n = send(lb, LB_GETSELCOUNT, 0, 0).max(0) as usize;
                let mut buf = vec![0i32; n];
                send(lb, LB_GETSELITEMS, n, buf.as_mut_ptr() as isize);
                buf.into_iter().map(|x| x as usize).collect()
            } else {
                let i = send(lb, LB_GETCURSEL, 0, 0);
                if i >= 0 {
                    vec![i as usize]
                } else {
                    vec![]
                }
            }
        };
        if id == 100 && code == LBN_DBLCLK as u16 {
            if let Some(b) = self.buttons.first() {
                self.result = Some((b.0, selected()));
                return Some(1);
            }
            return None;
        }
        if self.buttons.iter().any(|b| b.0 == id) {
            self.result = Some((id, selected()));
            return Some(1);
        }
        None
    }
}

/// Generic list dialog; returns (button id, selected indices).
pub fn list_dialog(owner: HWND, title: &str, items: Vec<String>, multi: bool, buttons: &[(u16, &str)]) -> Option<(u16, Vec<usize>)> {
    let mut b = DlgBuilder::new(title, 300, 200);
    let style = WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_BORDER | WS_VSCROLL | WS_HSCROLL | LBS_NOTIFY as u32 | LBS_USETABSTOPS as u32 | LBS_NOINTEGRALHEIGHT as u32 | if multi { LBS_EXTENDEDSEL as u32 } else { 0 };
    b.item("LISTBOX", "", 100, 6, 6, 226, 188, style);
    let mut y = 6;
    for (k, (id, label)) in buttons.iter().enumerate() {
        b.item("BUTTON", label, *id, 238, y, 56, 14, if k == 0 { BS_DEF } else { BS_PUSH });
        y += 18;
    }
    b.item("BUTTON", "Close", IDCANCEL_, 238, 180, 56, 14, BS_PUSH);
    let tpl = b.build();
    let mut hd = ListDlg { items, multi, buttons: buttons.iter().map(|(i, s)| (*i, s.to_string())).collect(), result: None };
    modal(owner, &tpl, &mut hd);
    hd.result
}

// ----------------------------------------------------------------- macro run multiple

struct MacroDlg {
    result: Option<Option<usize>>,
}
impl DlgHandler for MacroDlg {
    fn init(&mut self, h: HWND) {
        set_check(h, 101, true);
        set_item_text(h, 102, "1");
    }
    fn command(&mut self, h: HWND, id: u16, _c: u16) -> Option<isize> {
        if id == IDOK_ {
            self.result = Some(if checked(h, 103) { None } else { Some(get_item_text(h, 102).trim().parse().unwrap_or(1)) });
            return Some(1);
        }
        None
    }
}

/// Some(Some(n)) = run n times, Some(None) = until end of file.
pub fn macro_multi(owner: HWND) -> Option<Option<usize>> {
    let mut b = DlgBuilder::new("Run a Macro Multiple Times", 200, 70);
    b.item("BUTTON", "Run", 101, 10, 10, 30, 10, BS_RADIO1)
        .item("EDIT", "", 102, 44, 8, 30, 13, ES_EDIT | ES_NUMBER as u32)
        .item("STATIC", "times", 104, 78, 10, 40, 10, SS_TEXT)
        .item("BUTTON", "Run until the end of file", 103, 10, 28, 150, 10, BS_RADIO)
        .item("BUTTON", "Run", IDOK_, 88, 50, 50, 14, BS_DEF)
        .item("BUTTON", "Cancel", IDCANCEL_, 142, 50, 50, 14, BS_PUSH);
    let tpl = b.build();
    let mut hd = MacroDlg { result: None };
    modal(owner, &tpl, &mut hd);
    hd.result
}
