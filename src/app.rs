//! Application state, main window, layout, commands, sessions.

use crate::config::{self, push_history, Session, SessionFile, Settings};
use crate::dialogs;
use crate::doc::*;
use crate::editor::EditorView;
use crate::find::{self, FindDlg};
use crate::fileio;
use crate::icons;
use crate::lang::{self, Lang, SPECS};
use crate::menu::{self, Cmd};
use crate::tabbar::TabBar;
use crate::win::*;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Instant;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::UI::Controls::Dialogs::*;
use windows_sys::Win32::UI::Controls::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::System::DataExchange::COPYDATASTRUCT;
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

pub const MAIN_CLASS: &str = "NppRsMain";
pub const PANEL_CLASS: &str = "NppRsPanel";
pub const AC_CLASS: &str = "NppRsAutoComplete";
pub const FUNC_CLASS: &str = "NppRsFuncPanel";
const TIMER_FUNCS: usize = 13;
pub const WM_APP_STATUS: u32 = WM_APP + 1;
pub const WM_APP_RESULT: u32 = WM_APP + 2;
const TIMER_BACKUP: usize = 10;
const TIMER_TAIL: usize = 11;
const TIMER_STATUSMSG: usize = 12;
const ID_TOOLBAR: u16 = 50;
const ID_STATUSBAR: u16 = 51;
const ID_RES_CLEAR: u16 = 7001;
const ID_RES_CLOSE: u16 = 7002;
const ID_RES_COPY: u16 = 7003;
const ID_RES_SELALL: u16 = 7004;
const ID_RES_FOLDALL: u16 = 7005;
const ID_RES_UNFOLDALL: u16 = 7006;

#[derive(Clone, Debug)]
pub struct ResultLoc {
    pub doc_id: u64,
    pub path: Option<PathBuf>,
    pub line: usize,
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug)]
pub enum MacroStep {
    Cmd(Cmd),
    Text(String),
    Key(u16, bool, bool, bool),
}

#[derive(Default)]
pub struct AutoComplete {
    pub hwnd: HWND,
    pub list: HWND,
    pub start: usize,
    pub items: Vec<String>,
}

pub struct App {
    pub hwnd: HWND,
    pub settings: Settings,
    pub docs: Vec<Document>,
    pub active: usize,
    pub view: EditorView,
    pub rview: EditorView,
    pub rdoc: Document,
    pub results_locs: Vec<Option<ResultLoc>>,
    pub results_panel: HWND,
    pub show_results: bool,
    pub toolbar: HWND,
    pub status: HWND,
    pub tabbar: TabBar,
    pub accel: HACCEL,
    pub menus: menu::Built,
    recent_count: usize,
    window_count: usize,
    pub find: FindDlg,
    pub closed: Vec<PathBuf>,
    fullscreen: Option<(WINDOWPLACEMENT, u32, bool)>,
    postit: Option<(u32, bool, bool, bool)>,
    pub recording: bool,
    pub macro_steps: Vec<MacroStep>,
    recording_buf: Vec<MacroStep>,
    playing: bool,
    status_pending: bool,
    status_msg: Option<(String, Instant)>,
    pub monitoring: HashSet<u64>,
    pub ac: AutoComplete,
    checking_files: bool,
    splitter_drag: Option<u8>,
    res_split: RECT,
    func_split: RECT,
    pub func_panel: HWND,
    pub func_list: HWND,
    pub show_funcs: bool,
    func_lines: Vec<usize>,
    func_key: (u64, u64),
    panel_font: HFONT,
    dpi: u32,
    tb_lists: (HIMAGELIST, HIMAGELIST),
    saved_style: Option<(usize, String, i32)>,
    exiting: bool,
    /// -nosession / -multiInst: never read or write the shared session and backups.
    pub no_session: bool,
    pub test: bool,
    /// Text last copied from a rectangular selection (pasted back as a column block).
    rect_clip: Option<String>,
}

static mut APP: *mut App = std::ptr::null_mut();

pub fn app() -> &'static mut App {
    // SAFETY: single-threaded Win32 UI; APP is set once at startup before any window exists.
    unsafe { &mut *APP }
}

pub fn try_app() -> Option<&'static mut App> {
    unsafe {
        let p = APP;
        if p.is_null() {
            None
        } else {
            Some(&mut *p)
        }
    }
}

pub fn editor_parts(results: bool) -> (&'static mut EditorView, &'static mut Document, &'static Settings) {
    let a = app();
    if results {
        (&mut a.rview, &mut a.rdoc, &a.settings)
    } else {
        let i = a.active.min(a.docs.len() - 1);
        (&mut a.view, &mut a.docs[i], &a.settings)
    }
}

/// Ask the main window to refresh status bar / title / tabs (coalesced).
pub fn notify_main() {
    if let Some(a) = try_app() {
        if !a.status_pending && !a.hwnd.is_null() {
            a.status_pending = true;
            unsafe {
                PostMessageW(a.hwnd, WM_APP_STATUS, 0, 0);
            }
        }
    }
}

pub fn exec_cmd(c: Cmd) {
    app().exec(c);
}

pub fn on_editor_focus(_results: bool) {
    let a = app();
    a.tabbar.focused_editor = true;
    invalidate(a.tabbar.hwnd);
}

pub fn record_text(t: &str) {
    let a = app();
    if a.recording {
        if let Some(MacroStep::Text(s)) = a.recording_buf.last_mut() {
            s.push_str(t);
        } else {
            a.recording_buf.push(MacroStep::Text(t.to_string()));
        }
    }
}

pub fn record_key(vk: u16, ctrl: bool, shift: bool, alt: bool) {
    let a = app();
    if a.recording {
        a.recording_buf.push(MacroStep::Key(vk, ctrl, shift, alt));
    }
    if a.ac_visible() && !matches!(vk, VK_UP | VK_DOWN | VK_PRIOR | VK_NEXT) {
        if vk == VK_BACK {
            a.ac_update(false);
        } else {
            a.ac_close();
        }
    }
}

pub fn activate(i: usize) {
    app().activate(i);
}
pub fn close_tab(i: usize) {
    app().close_doc(i);
}
pub fn move_tab(from: usize, to: usize) {
    let a = app();
    if from >= a.docs.len() || to >= a.docs.len() {
        return;
    }
    let d = a.docs.remove(from);
    a.docs.insert(to, d);
    a.active = to;
    invalidate(a.tabbar.hwnd);
}

pub fn tab_context_menu(x: i32, y: i32) {
    let items: &[(Cmd, &str)] = &[
        (Cmd::Close, "Close"),
        (Cmd::CloseOthers, "Close All BUT This"),
        (Cmd::CloseLeft, "Close All to the Left"),
        (Cmd::CloseRight, "Close All to the Right"),
        (Cmd::CloseUnchanged, "Close All Unchanged"),
        (Cmd::Escape, ""),
        (Cmd::Save, "Save"),
        (Cmd::SaveAs, "Save As..."),
        (Cmd::OpenFolder, "Open Containing Folder in Explorer"),
        (Cmd::OpenCmd, "Open Containing Folder in cmd"),
        (Cmd::Rename, "Rename"),
        (Cmd::DeleteFile, "Move to Recycle Bin"),
        (Cmd::Reload, "Reload"),
        (Cmd::Print, "Print"),
        (Cmd::Escape, ""),
        (Cmd::ReadOnly, "Read-Only"),
        (Cmd::Escape, ""),
        (Cmd::CopyFullPath, "Copy Full File Path"),
        (Cmd::CopyFileName, "Copy Filename"),
        (Cmd::CopyDirPath, "Copy Current Dir. Path"),
    ];
    popup_menu(items, x, y);
}

fn popup_menu(items: &[(Cmd, &str)], x: i32, y: i32) {
    let a = app();
    unsafe {
        let m = CreatePopupMenu();
        for (c, label) in items {
            if label.is_empty() {
                AppendMenuW(m, MF_SEPARATOR, 0, std::ptr::null());
            } else {
                let l = w(label);
                AppendMenuW(m, MF_STRING, *c as usize, l.as_ptr());
            }
        }
        a.update_menu_states(m);
        let id = TrackPopupMenu(m, TPM_RETURNCMD | TPM_RIGHTBUTTON, x, y, 0, a.hwnd, std::ptr::null());
        DestroyMenu(m);
        if id > 0 {
            if let Some(c) = Cmd::from_id(id as u16) {
                a.exec(c);
            }
        }
    }
}

pub fn editor_context_menu(results: bool, mut x: i32, mut y: i32) {
    let a = app();
    if x == -1 && y == -1 {
        let mut pt = POINT { x: 0, y: 0 };
        unsafe {
            GetCaretPos(&mut pt);
            ClientToScreen(if results { a.rview.hwnd } else { a.view.hwnd }, &mut pt);
        }
        x = pt.x;
        y = pt.y;
    }
    if results {
        unsafe {
            let m = CreatePopupMenu();
            for (id, l) in [(ID_RES_COPY, "Copy"), (ID_RES_SELALL, "Select All"), (0, ""), (ID_RES_FOLDALL, "Fold All"), (ID_RES_UNFOLDALL, "Unfold All"), (0, ""), (ID_RES_CLEAR, "Clear All"), (ID_RES_CLOSE, "Close")] {
                if id == 0 {
                    AppendMenuW(m, MF_SEPARATOR, 0, std::ptr::null());
                } else {
                    let lw = w(l);
                    AppendMenuW(m, MF_STRING, id as usize, lw.as_ptr());
                }
            }
            let id = TrackPopupMenu(m, TPM_RETURNCMD | TPM_RIGHTBUTTON, x, y, 0, a.hwnd, std::ptr::null());
            DestroyMenu(m);
            a.results_command(id as u16);
        }
        return;
    }
    let items: &[(Cmd, &str)] = &[
        (Cmd::Cut, "Cut"),
        (Cmd::Copy, "Copy"),
        (Cmd::Paste, "Paste"),
        (Cmd::Delete, "Delete"),
        (Cmd::SelectAll, "Select All"),
        (Cmd::BeginEndSelect, "Begin/End Select"),
        (Cmd::Escape, ""),
        (Cmd::Style1, "Style all occurrences of token: 1st style"),
        (Cmd::Style2, "Style all occurrences of token: 2nd style"),
        (Cmd::Style3, "Style all occurrences of token: 3rd style"),
        (Cmd::ClearAllStyles, "Clear all styles"),
        (Cmd::Escape, ""),
        (Cmd::Upper, "UPPERCASE"),
        (Cmd::Lower, "lowercase"),
        (Cmd::Escape, ""),
        (Cmd::OpenSelectedFile, "Open File"),
        (Cmd::SearchInternet, "Search on Internet"),
        (Cmd::Escape, ""),
        (Cmd::ToggleComment, "Toggle Single Line Comment"),
        (Cmd::BlockComment, "Block Comment"),
        (Cmd::BlockUncomment, "Block Uncomment"),
    ];
    popup_menu(items, x, y);
}

pub fn autocomplete_key(vk: u16) -> bool {
    let a = app();
    if !a.ac_visible() {
        return false;
    }
    let lb = a.ac.list;
    let n = send(lb, LB_GETCOUNT, 0, 0);
    let cur = send(lb, LB_GETCURSEL, 0, 0);
    match vk {
        VK_UP | VK_DOWN | VK_PRIOR | VK_NEXT => {
            let step = match vk {
                VK_UP => -1,
                VK_DOWN => 1,
                VK_PRIOR => -8,
                _ => 8,
            };
            let ni = (cur + step).clamp(0, n - 1);
            send(lb, LB_SETCURSEL, ni as usize, 0);
            true
        }
        VK_RETURN | VK_TAB => {
            a.ac_accept();
            true
        }
        VK_ESCAPE => {
            a.ac_close();
            true
        }
        _ => false,
    }
}

/// Close the completion popup unless focus moved into it.
pub fn autocomplete_focus_lost(new_focus: HWND) {
    if let Some(a) = try_app() {
        if new_focus != a.ac.list && new_focus != a.ac.hwnd {
            a.ac_close();
        }
    }
}

pub fn autocomplete_close() {
    if let Some(a) = try_app() {
        a.ac_close();
    }
}

pub fn autocomplete_after_char(c: char) {
    let a = app();
    if is_word_char(c) {
        if a.ac_visible() || a.settings.auto_complete {
            a.ac_update(true);
        }
    } else {
        a.ac_close();
    }
}

pub fn preview_style(theme: usize, font: &str, size: i32) {
    let a = app();
    a.settings.theme = theme;
    a.settings.font = font.to_string();
    a.settings.font_size = size;
    invalidate(a.view.hwnd);
    invalidate(a.rview.hwnd);
}

pub fn browse_folder(owner: HWND, _start: &str) -> Option<String> {
    unsafe {
        let title = w("Select a folder to search from");
        let mut bi: BROWSEINFOW = std::mem::zeroed();
        bi.hwndOwner = owner;
        bi.lpszTitle = title.as_ptr();
        bi.ulFlags = BIF_RETURNONLYFSDIRS | BIF_NEWDIALOGSTYLE;
        let pidl = SHBrowseForFolderW(&bi);
        if pidl.is_null() {
            return None;
        }
        let mut buf = vec![0u16; 1024];
        let ok = SHGetPathFromIDListW(pidl, buf.as_mut_ptr());
        windows_sys::Win32::System::Com::CoTaskMemFree(pidl as _);
        if ok != 0 {
            Some(from_wide(&buf))
        } else {
            None
        }
    }
}

const OPEN_FILTER: &str = "All types (*.*)\0*.*\0Normal text file (*.txt)\0*.txt\0C/C++ source (*.c;*.cpp;*.h;*.hpp)\0*.c;*.cpp;*.cc;*.h;*.hpp\0C# (*.cs)\0*.cs\0Java (*.java)\0*.java\0JavaScript / TypeScript\0*.js;*.mjs;*.ts;*.tsx;*.jsx\0Python (*.py)\0*.py;*.pyw\0Rust (*.rs)\0*.rs\0Go (*.go)\0*.go\0HTML / XML\0*.html;*.htm;*.xml;*.xsl;*.svg\0CSS (*.css)\0*.css\0JSON / YAML / TOML\0*.json;*.yml;*.yaml;*.toml\0Markdown (*.md)\0*.md;*.markdown\0Batch / PowerShell / Shell\0*.bat;*.cmd;*.ps1;*.sh\0SQL (*.sql)\0*.sql\0ini / properties\0*.ini;*.cfg;*.conf;*.properties\0Log files (*.log)\0*.log\0";

pub fn open_file_dialog(owner: HWND, multi: bool, filter: &str) -> Vec<PathBuf> {
    let mut buf = vec![0u16; 65536];
    let filt: Vec<u16> = filter.encode_utf16().chain(std::iter::once(0)).collect();
    let dir = try_app().and_then(|a| a.doc().path.as_ref().and_then(|p| p.parent()).map(|p| w(&p.to_string_lossy())));
    unsafe {
        let mut ofn: OPENFILENAMEW = std::mem::zeroed();
        ofn.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
        ofn.hwndOwner = owner;
        ofn.lpstrFilter = filt.as_ptr();
        ofn.lpstrFile = buf.as_mut_ptr();
        ofn.nMaxFile = buf.len() as u32;
        if let Some(d) = &dir {
            ofn.lpstrInitialDir = d.as_ptr();
        }
        ofn.Flags = OFN_EXPLORER | OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_HIDEREADONLY | if multi { OFN_ALLOWMULTISELECT } else { 0 };
        if GetOpenFileNameW(&mut ofn) == 0 {
            return vec![];
        }
    }
    let parts: Vec<String> = buf.split(|&c| c == 0).take_while(|s| !s.is_empty()).map(String::from_utf16_lossy).collect();
    if parts.len() <= 1 {
        parts.into_iter().map(PathBuf::from).collect()
    } else {
        let dir = PathBuf::from(&parts[0]);
        parts[1..].iter().map(|f| dir.join(f)).collect()
    }
}

fn save_file_dialog(owner: HWND, name: &str, dir: Option<&Path>) -> Option<PathBuf> {
    let mut buf = vec![0u16; 4096];
    for (i, c) in name.encode_utf16().enumerate().take(4000) {
        buf[i] = c;
    }
    let filt: Vec<u16> = OPEN_FILTER.encode_utf16().chain(std::iter::once(0)).collect();
    let d = dir.map(|p| w(&p.to_string_lossy()));
    unsafe {
        let mut ofn: OPENFILENAMEW = std::mem::zeroed();
        ofn.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
        ofn.hwndOwner = owner;
        ofn.lpstrFilter = filt.as_ptr();
        ofn.lpstrFile = buf.as_mut_ptr();
        ofn.nMaxFile = buf.len() as u32;
        if let Some(d) = &d {
            ofn.lpstrInitialDir = d.as_ptr();
        }
        ofn.Flags = OFN_EXPLORER | OFN_OVERWRITEPROMPT | OFN_PATHMUSTEXIST | OFN_HIDEREADONLY;
        if GetSaveFileNameW(&mut ofn) == 0 {
            return None;
        }
    }
    Some(PathBuf::from(from_wide(&buf)))
}

// toolbar layout: (command, icon index); Cmd::Escape = separator, None command = disabled placeholder
const TOOLBAR: &[(Option<Cmd>, i32)] = &[
    (Some(Cmd::New), 0),
    (Some(Cmd::Open), 1),
    (Some(Cmd::Save), 2),
    (Some(Cmd::SaveAll), 3),
    (Some(Cmd::Close), 4),
    (Some(Cmd::CloseAll), 5),
    (Some(Cmd::Print), 6),
    (None, -1),
    (Some(Cmd::Cut), 7),
    (Some(Cmd::Copy), 8),
    (Some(Cmd::Paste), 9),
    (None, -1),
    (Some(Cmd::Undo), 10),
    (Some(Cmd::Redo), 11),
    (None, -1),
    (Some(Cmd::Find), 12),
    (Some(Cmd::Replace), 13),
    (None, -1),
    (Some(Cmd::ZoomIn), 14),
    (Some(Cmd::ZoomOut), 15),
    (None, -1),
    (Some(Cmd::WordWrap), 18),
    (Some(Cmd::ShowAll), 19),
    (Some(Cmd::ShowIndentGuide), 20),
    (None, -1),
    (Some(Cmd::StyleConfigurator), 21),
    (Some(Cmd::WindowsList), 23),
    (Some(Cmd::FuncList), 24),
    (Some(Cmd::FindInFiles), 25),
    (None, -1),
    (Some(Cmd::Monitoring), 26),
    (None, -1),
    (Some(Cmd::MacroRecord), 27),
    (Some(Cmd::MacroStop), 28),
    (Some(Cmd::MacroPlay), 29),
    (Some(Cmd::MacroRunMulti), 30),
];

fn tooltip_for(c: Cmd) -> &'static str {
    match c {
        Cmd::New => "New",
        Cmd::Open => "Open",
        Cmd::Save => "Save",
        Cmd::SaveAll => "Save All",
        Cmd::Close => "Close",
        Cmd::CloseAll => "Close All",
        Cmd::Print => "Print",
        Cmd::Cut => "Cut",
        Cmd::Copy => "Copy",
        Cmd::Paste => "Paste",
        Cmd::Undo => "Undo",
        Cmd::Redo => "Redo",
        Cmd::Find => "Find",
        Cmd::Replace => "Replace",
        Cmd::ZoomIn => "Zoom In",
        Cmd::ZoomOut => "Zoom Out",
        Cmd::WordWrap => "Word Wrap",
        Cmd::ShowAll => "Show All Characters",
        Cmd::ShowIndentGuide => "Show Indent Guide",
        Cmd::StyleConfigurator => "Style Configurator",
        Cmd::Summary => "Summary",
        Cmd::FuncList => "Function List",
        Cmd::WindowsList => "Document List",
        Cmd::FindInFiles => "Find in Files",
        Cmd::Monitoring => "Monitoring (tail -f)",
        Cmd::MacroRecord => "Start Recording",
        Cmd::MacroStop => "Stop Recording",
        Cmd::MacroPlay => "Playback",
        Cmd::MacroRunMulti => "Run a Macro Multiple Times",
        _ => "",
    }
}

impl App {
    pub fn doc(&self) -> &Document {
        &self.docs[self.active.min(self.docs.len() - 1)]
    }
    pub fn doc_mut(&mut self) -> &mut Document {
        let i = self.active.min(self.docs.len() - 1);
        &mut self.docs[i]
    }

    fn new_document(&mut self) -> Document {
        let mut n = 1;
        loop {
            let name = format!("new {n}");
            if !self.docs.iter().any(|d| d.path.is_none() && d.untitled_name == name) {
                let mut d = Document::new(name);
                d.eol = Eol::from_code(self.settings.default_eol);
                d.encoding = Encoding::from_code(self.settings.default_encoding);
                d.tab_width = self.settings.tab_width;
                return d;
            }
            n += 1;
        }
    }

    pub fn refresh_editor(&mut self) {
        let i = self.active.min(self.docs.len() - 1);
        let (v, d, s) = (&mut self.view, &mut self.docs[i], &self.settings);
        d.normalize_sels();
        v.after_change(d, s);
    }

    pub fn reveal_selection(&mut self, line: usize) {
        let i = self.active;
        let (v, d, s) = (&mut self.view, &mut self.docs[i], &self.settings);
        v.center_line(d, s, line);
        v.after_change(d, s);
    }

    pub fn set_status_text(&mut self, msg: &str) {
        if msg.is_empty() {
            return;
        }
        self.status_msg = Some((msg.to_string(), Instant::now()));
        unsafe {
            SetTimer(self.hwnd, TIMER_STATUSMSG, 4000, None);
        }
        self.update_status();
    }

    pub fn activate(&mut self, i: usize) {
        if i >= self.docs.len() {
            return;
        }
        self.ac_close();
        self.active = i;
        let (v, d, s) = (&mut self.view, &mut self.docs[i], &self.settings);
        v.ensure_layout(d, s);
        v.clamp_scroll(d);
        v.update_scrollbars(d, s);
        invalidate(v.hwnd);
        invalidate(self.tabbar.hwnd);
        if !self.test {
            unsafe {
                SetFocus(self.view.hwnd);
            }
        }
        self.update_status();
    }

    // ------------------------------------------------------------------ status

    pub fn update_status(&mut self) {
        self.status_pending = false;
        if self.docs.is_empty() || self.status.is_null() {
            return;
        }
        let d = &self.docs[self.active];
        let sp = lang::spec(d.lang);
        let caret = d.caret();
        let line = d.line_of(caret);
        let col = d.col_of(caret) + 1;
        let crlf = d.eol == Eol::Crlf;
        let lines = d.line_count();
        let len_bytes = d.text.len_bytes() + if crlf { lines - 1 } else { 0 };
        let pos = d.text.char_to_byte(caret) + if crlf { line } else { 0 } + 1;
        let first = match &self.status_msg {
            Some((m, t)) if t.elapsed().as_secs() < 4 => m.clone(),
            _ => sp.desc.to_string(),
        };
        let sel_info = if d.has_selection() {
            let chars: usize = d.sels.iter().map(|s| s.end() - s.start()).sum();
            let sl: usize = d.sels.iter().filter(|s| !s.is_empty()).map(|s| d.line_of(s.end()) - d.line_of(s.start()) + 1).sum();
            format!("Sel: {} | {}", fmt_num(chars), fmt_num(sl))
        } else {
            format!("Pos: {}", fmt_num(pos))
        };
        let parts = [
            first,
            format!("length: {}    lines: {}", fmt_num(len_bytes), fmt_num(lines)),
            format!("Ln: {}    Col: {}    {}", fmt_num(line + 1), fmt_num(col), sel_info),
            d.eol.label().to_string(),
            d.encoding.label().to_string(),
            if self.view.overtype { "OVR".into() } else { "INS".into() },
        ];
        for (i, p) in parts.iter().enumerate() {
            let ws = w(p);
            send(self.status, SB_SETTEXTW, i, ws.as_ptr() as isize);
        }
        // title
        let mut title = String::new();
        if d.is_modified() {
            title.push('*');
        }
        title.push_str(&d.full_title());
        if d.read_only {
            title.push_str(" [Read Only]");
        }
        if self.monitoring.contains(&d.id) {
            title.push_str(" [Monitoring]");
        }
        title.push_str(" - Notepad++ RS");
        if self.recording {
            title.push_str("  (Recording...)");
        }
        set_text(self.hwnd, &title);
        if self.show_funcs && (d.id, d.version) != self.func_key {
            let delay = if d.id != self.func_key.0 {
                10
            } else if d.line_count() > 20_000 {
                2500
            } else {
                700
            };
            unsafe {
                SetTimer(self.hwnd, TIMER_FUNCS, delay, None);
            }
        }
        invalidate(self.tabbar.hwnd);
        self.update_toolbar();
    }

    fn update_toolbar(&mut self) {
        if self.toolbar.is_null() {
            return;
        }
        let d = &self.docs[self.active];
        let enable = |c: Cmd, on: bool| {
            send(self.toolbar, TB_ENABLEBUTTON, c as usize, on as isize);
        };
        let check = |c: Cmd, on: bool| {
            send(self.toolbar, TB_CHECKBUTTON, c as usize, on as isize);
        };
        enable(Cmd::Save, d.is_modified() || d.path.is_none());
        enable(Cmd::SaveAll, self.docs.iter().any(|d| d.is_modified()));
        enable(Cmd::Undo, d.can_undo());
        enable(Cmd::Redo, d.can_redo());
        let has_sel = d.has_selection();
        enable(Cmd::Cut, has_sel && !d.read_only);
        enable(Cmd::Copy, has_sel);
        enable(Cmd::Paste, !d.read_only);
        check(Cmd::WordWrap, self.settings.word_wrap);
        check(Cmd::ShowAll, self.settings.show_ws && self.settings.show_eol);
        check(Cmd::ShowIndentGuide, self.settings.indent_guides);
        check(Cmd::Monitoring, self.monitoring.contains(&d.id));
        check(Cmd::FuncList, self.show_funcs);
        enable(Cmd::MacroRecord, !self.recording && !self.playing);
        enable(Cmd::MacroStop, self.recording);
        enable(Cmd::MacroPlay, !self.recording && !self.macro_steps.is_empty());
        enable(Cmd::MacroRunMulti, !self.recording && !self.macro_steps.is_empty());
    }

    pub fn update_menu_states(&mut self, m: HMENU) {
        let d = &self.docs[self.active];
        let s = &self.settings;
        let set = |c: Cmd, on: bool| unsafe {
            CheckMenuItem(m, c as u32, if on { MF_CHECKED } else { MF_UNCHECKED });
        };
        let en = |c: Cmd, on: bool| unsafe {
            EnableMenuItem(m, c as u32, if on { MF_ENABLED } else { MF_GRAYED });
        };
        set(Cmd::AlwaysOnTop, s.always_on_top);
        set(Cmd::FullScreen, self.fullscreen.is_some());
        set(Cmd::PostIt, self.postit.is_some());
        set(Cmd::ShowWs, s.show_ws);
        set(Cmd::ShowEol, s.show_eol);
        set(Cmd::ShowAll, s.show_ws && s.show_eol);
        set(Cmd::ShowIndentGuide, s.indent_guides);
        set(Cmd::WordWrap, s.word_wrap);
        set(Cmd::ToggleToolbar, s.toolbar);
        set(Cmd::ToggleStatusbar, s.statusbar);
        set(Cmd::ToggleTabbar, s.tabbar);
        set(Cmd::ReadOnly, d.read_only);
        set(Cmd::Monitoring, self.monitoring.contains(&d.id));
        set(Cmd::SearchResults, self.show_results);
        set(Cmd::EolCrlf, d.eol == Eol::Crlf);
        set(Cmd::EolLf, d.eol == Eol::Lf);
        set(Cmd::EolCr, d.eol == Eol::Cr);
        set(Cmd::EncAnsi, d.encoding == Encoding::Ansi);
        set(Cmd::EncUtf8, d.encoding == Encoding::Utf8);
        set(Cmd::EncUtf8Bom, d.encoding == Encoding::Utf8Bom);
        set(Cmd::EncUtf16Be, d.encoding == Encoding::Utf16Be);
        set(Cmd::EncUtf16Le, d.encoding == Encoding::Utf16Le);
        en(Cmd::Undo, d.can_undo());
        en(Cmd::Redo, d.can_redo());
        en(Cmd::Cut, d.has_selection() && !d.read_only);
        en(Cmd::Copy, d.has_selection());
        en(Cmd::Paste, !d.read_only);
        en(Cmd::Reload, d.path.is_some());
        en(Cmd::Rename, true);
        en(Cmd::DeleteFile, d.path.is_some());
        en(Cmd::OpenFolder, d.path.is_some());
        en(Cmd::OpenCmd, d.path.is_some());
        en(Cmd::MacroRecord, !self.recording);
        en(Cmd::MacroStop, self.recording);
        en(Cmd::MacroPlay, !self.recording && !self.macro_steps.is_empty());
        en(Cmd::MacroRunMulti, !self.recording && !self.macro_steps.is_empty());
        en(Cmd::RestoreClosed, !self.closed.is_empty());
        en(Cmd::DocMap, false);
        set(Cmd::FuncList, self.show_funcs);
        set(Cmd::BeginEndSelect, self.view.begin_select.is_some());
        // language radio
        if !self.menus.languages.is_null() {
            let idx = SPECS.iter().position(|sp| sp.lang == d.lang).unwrap_or(0);
            for i in 0..SPECS.len() {
                unsafe {
                    CheckMenuItem(self.menus.bar, (menu::ID_LANG as usize + i) as u32, if i == idx { MF_CHECKED } else { MF_UNCHECKED });
                }
            }
        }
    }

    fn refresh_dynamic_menus(&mut self) {
        let recent: Vec<String> = self.settings.recent.clone();
        if let Some(&(m, at)) = self.menus.recent.first() {
            self.recent_count = menu::set_dynamic(m, at, menu::ID_RECENT, self.recent_count, &recent);
        }
        let names: Vec<String> = self.docs.iter().map(|d| d.full_title()).collect();
        if let Some(&(m, at)) = self.menus.windows.first() {
            self.window_count = menu::set_dynamic(m, at, menu::ID_WINDOW, self.window_count, &names);
            unsafe {
                CheckMenuItem(m, (menu::ID_WINDOW as usize + self.active) as u32, MF_CHECKED);
            }
        }
    }

    // ------------------------------------------------------------------ layout

    pub fn layout(&mut self) {
        if self.hwnd.is_null() {
            return;
        }
        let rc = client_rect(self.hwnd);
        let d = dpi_of(self.hwnd);
        let mut top = 0;
        let mut bottom = rc.bottom;
        unsafe {
            if self.settings.toolbar {
                ShowWindow(self.toolbar, SW_SHOW);
                let sz = send(self.toolbar, TB_GETBUTTONSIZE, 0, 0);
                let th = hiword(sz as usize) as i32 + scale(4, d);
                MoveWindow(self.toolbar, 0, 0, rc.right, th, 1);
                top += th;
            } else {
                ShowWindow(self.toolbar, SW_HIDE);
            }
            if self.settings.statusbar {
                ShowWindow(self.status, SW_SHOW);
                send(self.status, WM_SIZE, 0, 0);
                let mut sr = RECT { left: 0, top: 0, right: 0, bottom: 0 };
                GetWindowRect(self.status, &mut sr);
                bottom -= sr.bottom - sr.top;
                let s = |v: i32| scale(v, d);
                let wdt = rc.right;
                let p5 = -1;
                let p4 = wdt - s(42);
                let p3 = p4 - s(110);
                let p2 = p3 - s(120);
                let p1 = p2 - s(250);
                let p0 = p1 - s(210);
                let parts = [p0.max(s(60)), p1, p2, p3, p4, p5];
                send(self.status, SB_SETPARTS, parts.len(), parts.as_ptr() as isize);
            } else {
                ShowWindow(self.status, SW_HIDE);
            }
            let mut right = rc.right;
            if self.show_funcs {
                let pw = scale(self.settings.panel_width, d).clamp(scale(120, d), (rc.right / 2).max(scale(120, d)));
                let split = scale(4, d);
                MoveWindow(self.func_panel, right - pw, top, pw, (bottom - top).max(0), 1);
                ShowWindow(self.func_panel, SW_SHOW);
                right -= pw + split;
                self.func_split = rect(right, top, right + split, bottom);
            } else {
                ShowWindow(self.func_panel, SW_HIDE);
                self.func_split = rect(0, 0, 0, 0);
            }
            if self.settings.tabbar {
                ShowWindow(self.tabbar.hwnd, SW_SHOW);
                let th = self.tabbar.height();
                MoveWindow(self.tabbar.hwnd, 0, top, right, th, 1);
                top += th;
            } else {
                ShowWindow(self.tabbar.hwnd, SW_HIDE);
            }
            if self.show_results {
                let ph = scale(self.settings.results_height, d).clamp(scale(60, d), (bottom - top - scale(80, d)).max(scale(60, d)));
                let split = scale(4, d);
                MoveWindow(self.results_panel, 0, bottom - ph, right, ph, 1);
                ShowWindow(self.results_panel, SW_SHOW);
                bottom -= ph + split;
                self.res_split = rect(0, bottom, right, bottom + split);
            } else {
                ShowWindow(self.results_panel, SW_HIDE);
                self.res_split = rect(0, 0, 0, 0);
            }
            MoveWindow(self.view.hwnd, 0, top, right, (bottom - top).max(0), 1);
        }
    }

    pub fn refresh_funcs(&mut self, force: bool) {
        if !self.show_funcs || self.func_list.is_null() {
            return;
        }
        let d = &self.docs[self.active];
        let key = (d.id, d.version);
        if !force && key == self.func_key {
            return;
        }
        self.func_key = key;
        let lb = self.func_list;
        let top = send(lb, LB_GETTOPINDEX, 0, 0);
        send(lb, WM_SETREDRAW, 0, 0);
        send(lb, LB_RESETCONTENT, 0, 0);
        self.func_lines.clear();
        match crate::funclist::parse(d) {
            Some(entries) if !entries.is_empty() => {
                for e in entries {
                    let ws = w(&e.label);
                    send(lb, LB_ADDSTRING, 0, ws.as_ptr() as isize);
                    self.func_lines.push(e.line);
                }
            }
            Some(_) => {
                let ws = w("(no functions found)");
                send(lb, LB_ADDSTRING, 0, ws.as_ptr() as isize);
            }
            None => {
                let ws = w(&format!("(no function list for {})", lang::spec(d.lang).name));
                send(lb, LB_ADDSTRING, 0, ws.as_ptr() as isize);
            }
        }
        send(lb, LB_SETTOPINDEX, top.max(0) as usize, 0);
        send(lb, WM_SETREDRAW, 1, 0);
        unsafe {
            InvalidateRect(lb, std::ptr::null(), 1);
        }
    }

    fn func_jump(&mut self) {
        let i = send(self.func_list, LB_GETCURSEL, 0, 0);
        let Some(&line) = (i >= 0).then(|| self.func_lines.get(i as usize)).flatten() else { return };
        let d = self.doc_mut();
        if line >= d.line_count() {
            return;
        }
        let p = d.line_start(line);
        let first = p + d.line_chars(line).iter().take_while(|c| c.is_whitespace()).count();
        d.set_caret(first);
        self.reveal_selection(line);
    }

    fn panel_layout(&mut self) {
        let rc = client_rect(self.results_panel);
        let cap = scale(22, dpi_of(self.results_panel));
        unsafe {
            MoveWindow(self.rview.hwnd, 0, cap, rc.right, (rc.bottom - cap).max(0), 1);
        }
    }

    // ------------------------------------------------------------------ files

    pub fn open_paths(&mut self, paths: &[PathBuf]) {
        let mut last = None;
        for p in paths {
            if p.is_dir() {
                continue;
            }
            match self.open_one(p) {
                Ok(i) => last = Some(i),
                Err(e) => {
                    if !p.exists() {
                        let q = format!("\"{}\" doesn't exist. Create it?", p.display());
                        if msgbox(self.hwnd, &q, "Create new file", MB_YESNO | MB_ICONQUESTION) == IDYES {
                            if std::fs::write(p, b"").is_ok() {
                                if let Ok(i) = self.open_one(p) {
                                    last = Some(i);
                                }
                            }
                        }
                    } else {
                        msgbox(self.hwnd, &format!("Can not open file \"{}\".\n{e}", p.display()), "Open", MB_OK | MB_ICONWARNING);
                    }
                }
            }
        }
        if let Some(i) = last {
            self.activate(i);
        }
    }

    fn open_one(&mut self, p: &Path) -> std::io::Result<usize> {
        let full = std::fs::canonicalize(p).map(|c| strip_unc(&c)).unwrap_or_else(|_| p.to_path_buf());
        if let Some(i) = self.docs.iter().position(|d| d.path.as_deref().is_some_and(|dp| same_path(dp, &full))) {
            return Ok(i);
        }
        let loaded = fileio::load(&full)?;
        let mut d = self.new_document();
        d.path = Some(full.clone());
        d.set_content(&loaded.text);
        d.encoding = loaded.encoding;
        d.eol = loaded.eol;
        d.lang = lang::detect(&full);
        if d.lang == Lang::Text {
            if let Some(l) = lang::detect_content(&d.line_text(0)) {
                d.lang = l;
            }
        }
        d.hl.reset(d.line_count());
        d.mtime = fileio::mtime(&full);
        d.read_only = std::fs::metadata(&full).map(|m| m.permissions().readonly()).unwrap_or(false);
        push_history(&mut self.settings.recent, &full.to_string_lossy(), self.settings.max_recent);
        self.closed.retain(|c| !same_path(c, &full));
        // replace a pristine "new 1"
        if self.docs.len() == 1 && self.docs[0].path.is_none() && !self.docs[0].is_modified() && self.docs[0].len() == 0 {
            self.docs[0] = d;
            return Ok(0);
        }
        self.docs.push(d);
        Ok(self.docs.len() - 1)
    }

    pub fn save_doc(&mut self, i: usize) -> bool {
        let Some(path) = self.docs[i].path.clone() else {
            return self.save_as(i, false);
        };
        let d = &mut self.docs[i];
        match fileio::save(&path, &d.text_string(), d.eol, d.encoding) {
            Ok(()) => {
                d.mark_saved();
                d.mtime = fileio::mtime(&path);
                remove_backup(d);
                invalidate(self.view.hwnd);
                self.update_status();
                true
            }
            Err(e) => {
                msgbox(self.hwnd, &format!("Please check whether if this file is opened in another program.\n\n{}\n{e}", path.display()), "Save failed", MB_OK | MB_ICONWARNING);
                false
            }
        }
    }

    pub fn save_as(&mut self, i: usize, copy: bool) -> bool {
        let d = &self.docs[i];
        let name = d.title();
        let dir = d.path.as_ref().and_then(|p| p.parent().map(|p| p.to_path_buf()));
        let Some(path) = save_file_dialog(self.hwnd, &name, dir.as_deref()) else { return false };
        if !copy {
            if let Some(j) = self.docs.iter().position(|x| x.path.as_deref().is_some_and(|p| same_path(p, &path))) {
                if j != i {
                    msgbox(self.hwnd, "The file is already opened in Notepad++ RS.", "Save As", MB_OK | MB_ICONWARNING);
                    return false;
                }
            }
        }
        let d = &mut self.docs[i];
        match fileio::save(&path, &d.text_string(), d.eol, d.encoding) {
            Ok(()) => {
                if !copy {
                    d.path = Some(path.clone());
                    d.mark_saved();
                    d.mtime = fileio::mtime(&path);
                    d.read_only = false;
                    remove_backup(d);
                    let nl = lang::detect(&path);
                    if nl != d.lang && (d.lang == Lang::Text || nl != Lang::Text) {
                        d.lang = nl;
                        d.hl.reset(d.line_count());
                    }
                    push_history(&mut self.settings.recent, &path.to_string_lossy(), self.settings.max_recent);
                }
                invalidate(self.view.hwnd);
                self.update_status();
                true
            }
            Err(e) => {
                msgbox(self.hwnd, &format!("Failed to save file.\n{e}"), "Save As", MB_OK | MB_ICONWARNING);
                false
            }
        }
    }

    /// Returns false if the user cancelled.
    fn confirm_close(&mut self, i: usize) -> bool {
        if !self.docs[i].is_modified() {
            return true;
        }
        self.activate(i);
        let q = format!("Save file \"{}\" ?", self.docs[i].full_title());
        match msgbox(self.hwnd, &q, "Save", MB_YESNOCANCEL | MB_ICONQUESTION) {
            IDYES => self.save_doc(i),
            IDNO => true,
            _ => false,
        }
    }

    pub fn close_doc(&mut self, i: usize) -> bool {
        if i >= self.docs.len() || !self.confirm_close(i) {
            return false;
        }
        self.ac_close();
        let d = self.docs.remove(i);
        remove_backup(&d);
        self.monitoring.remove(&d.id);
        if let Some(p) = d.path {
            self.closed.push(p);
        }
        if self.docs.is_empty() {
            let nd = self.new_document();
            self.docs.push(nd);
        }
        if self.active > i || self.active >= self.docs.len() {
            self.active = self.active.saturating_sub(1);
        }
        let a = self.active.min(self.docs.len() - 1);
        self.activate(a);
        true
    }

    fn close_where(&mut self, pred: impl Fn(usize, &Document) -> bool) {
        let ids: Vec<u64> = self.docs.iter().enumerate().filter(|(i, d)| pred(*i, d)).map(|(_, d)| d.id).collect();
        for id in ids {
            if let Some(i) = self.docs.iter().position(|d| d.id == id) {
                if !self.close_doc(i) {
                    break;
                }
            }
        }
    }

    fn reload(&mut self, ask: bool) {
        let i = self.active;
        let Some(path) = self.docs[i].path.clone() else { return };
        if ask && self.docs[i].is_modified() {
            let q = format!("Are you sure you want to reload the current file and lose the changes made in Notepad++ RS?\n\n{}", path.display());
            if msgbox(self.hwnd, &q, "Reload", MB_YESNO | MB_ICONQUESTION) != IDYES {
                return;
            }
        }
        match fileio::load(&path) {
            Ok(l) => {
                let d = &mut self.docs[i];
                let caret = d.caret();
                let scroll = d.scroll_row;
                let bm = d.bookmarks.clone();
                d.set_content(&l.text);
                d.encoding = l.encoding;
                d.eol = l.eol;
                d.mtime = fileio::mtime(&path);
                d.set_caret(caret.min(d.len()));
                d.scroll_row = scroll;
                d.bookmarks = bm.into_iter().filter(|&b| b < d.line_count()).collect();
                remove_backup(d);
                self.refresh_editor();
            }
            Err(e) => {
                msgbox(self.hwnd, &format!("Cannot reload file.\n{e}"), "Reload", MB_OK | MB_ICONWARNING);
            }
        }
    }

    /// Detect files changed on disk (called when the app regains focus).
    fn check_files(&mut self) {
        // never run inside a modal loop (e.g. a "Save?" prompt): indices could shift under it
        if self.checking_files || unsafe { IsWindowEnabled(self.hwnd) } == 0 {
            return;
        }
        self.checking_files = true;
        let mut i = 0;
        while i < self.docs.len() {
            let Some(path) = self.docs[i].path.clone() else {
                i += 1;
                continue;
            };
            let now = fileio::mtime(&path);
            let known = self.docs[i].mtime;
            if now.is_none() && known.is_some() {
                self.docs[i].mtime = None;
                self.activate(i);
                let q = format!("\"{}\"\n\nThis file doesn't exist anymore.\nKeep this file in editor?", path.display());
                if msgbox(self.hwnd, &q, "Keep non existing file", MB_YESNO | MB_ICONWARNING) == IDNO {
                    let d = self.docs.remove(i);
                    remove_backup(&d);
                    if self.docs.is_empty() {
                        let nd = self.new_document();
                        self.docs.push(nd);
                    }
                    self.active = self.active.min(self.docs.len() - 1);
                    self.activate(self.active);
                    continue;
                }
                self.docs[i].mark_dirty();
            } else if now.is_some() && known.is_some() && now != known {
                self.docs[i].mtime = now;
                if self.monitoring.contains(&self.docs[i].id) {
                    i += 1;
                    continue;
                }
                self.activate(i);
                let q = if self.docs[i].is_modified() {
                    format!("{}\n\nThis file has been modified by another program.\nDo you want to reload it and lose the changes made in Notepad++ RS?", path.display())
                } else {
                    format!("{}\n\nThis file has been modified by another program.\nDo you want to reload it?", path.display())
                };
                if msgbox(self.hwnd, &q, "Reload", MB_YESNO | MB_ICONQUESTION) == IDYES {
                    self.reload(false);
                }
            }
            i += 1;
        }
        self.checking_files = false;
        self.update_status();
    }

    fn tail_tick(&mut self) {
        let ids: Vec<u64> = self.monitoring.iter().copied().collect();
        for id in ids {
            let Some(i) = self.docs.iter().position(|d| d.id == id) else {
                self.monitoring.remove(&id);
                continue;
            };
            let Some(path) = self.docs[i].path.clone() else { continue };
            let now = fileio::mtime(&path);
            if now != self.docs[i].mtime {
                if let Ok(l) = fileio::load(&path) {
                    let d = &mut self.docs[i];
                    d.set_content(&l.text);
                    d.mtime = now;
                    let end = d.len();
                    d.set_caret(end);
                    if i == self.active {
                        self.refresh_editor();
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------------ session

    fn backup_tick(&mut self) {
        if !self.settings.snapshot || self.no_session {
            return;
        }
        let dir = config::backup_dir();
        let mut changed = false;
        for d in self.docs.iter_mut() {
            let needs = d.is_modified() || (d.path.is_none() && d.len() > 0);
            if !needs {
                if d.backup_file.is_some() {
                    remove_backup(d);
                    changed = true;
                }
                continue;
            }
            if d.backup_version == d.version && d.backup_file.is_some() {
                continue;
            }
            let file = d.backup_file.clone().unwrap_or_else(|| {
                let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|x| x.as_millis()).unwrap_or(0);
                dir.join(format!("{}@{}-{}", sanitize(&d.title()), ts, d.id))
            });
            if std::fs::write(&file, d.text_string()).is_ok() {
                d.backup_file = Some(file);
                d.backup_version = d.version;
                changed = true;
            }
        }
        if changed {
            self.save_session();
        }
    }

    fn save_session(&mut self) {
        let mut s = Session { files: Vec::new(), active: self.active };
        for d in &self.docs {
            if d.path.is_none() && d.backup_file.is_none() {
                continue;
            }
            let m = d.main_sel();
            s.files.push(SessionFile {
                path: d.path.as_ref().map(|p| p.to_string_lossy().into_owned()),
                untitled: if d.path.is_none() { Some(d.untitled_name.clone()) } else { None },
                backup: if d.is_modified() || d.path.is_none() { d.backup_file.as_ref().map(|p| p.to_string_lossy().into_owned()) } else { None },
                lang: lang::spec(d.lang).name.to_string(),
                encoding: d.encoding.code(),
                eol: d.eol.code(),
                anchor: m.anchor,
                caret: m.caret,
                scroll_row: d.scroll_row,
                bookmarks: d.bookmarks.iter().copied().collect(),
                read_only: d.read_only,
                pinned: d.pinned,
            });
        }
        s.active = s.active.min(s.files.len().saturating_sub(1));
        config::save_session(&s);
    }

    fn restore_session(&mut self) {
        let s = config::load_session();
        for f in &s.files {
            let backup = f.backup.as_ref().map(PathBuf::from).filter(|p| p.exists());
            let mut d = if let Some(p) = &f.path {
                let pb = PathBuf::from(p);
                if backup.is_none() && !pb.exists() {
                    continue;
                }
                let mut d = self.new_document();
                d.path = Some(pb.clone());
                d.mtime = fileio::mtime(&pb);
                if let Some(b) = &backup {
                    let t = std::fs::read_to_string(b).unwrap_or_default();
                    d.set_content(&t);
                    d.mark_dirty();
                } else if let Ok(l) = fileio::load(&pb) {
                    d.set_content(&l.text);
                } else {
                    continue;
                }
                d
            } else {
                let Some(b) = &backup else { continue };
                let mut d = self.new_document();
                if let Some(n) = &f.untitled {
                    if !self.docs.iter().any(|x| x.path.is_none() && &x.untitled_name == n) {
                        d.untitled_name = n.clone();
                    }
                }
                let t = std::fs::read_to_string(b).unwrap_or_default();
                d.set_content(&t);
                d.mark_dirty();
                d
            };
            d.backup_file = backup;
            d.backup_version = if d.backup_file.is_some() { d.version } else { 0 };
            d.encoding = Encoding::from_code(f.encoding);
            d.eol = Eol::from_code(f.eol);
            d.lang = SPECS.iter().find(|sp| sp.name == f.lang).map(|sp| sp.lang).unwrap_or(Lang::Text);
            d.hl.reset(d.line_count());
            let n = d.len();
            d.set_sel(f.anchor.min(n), f.caret.min(n));
            d.scroll_row = f.scroll_row;
            d.bookmarks = f.bookmarks.iter().copied().filter(|&b| b < d.line_count()).collect();
            d.read_only = f.read_only;
            d.pinned = f.pinned;
            self.docs.push(d);
        }
        if !self.docs.is_empty() {
            self.active = s.active.min(self.docs.len() - 1);
        }
    }

    /// Returns false if exit was cancelled.
    fn on_exit(&mut self) -> bool {
        if self.settings.snapshot && self.settings.remember_session && !self.no_session {
            self.backup_tick();
        } else {
            for i in 0..self.docs.len() {
                if !self.confirm_close(i) {
                    return false;
                }
            }
            for d in &mut self.docs {
                remove_backup(d);
            }
        }
        unsafe {
            let mut wp: WINDOWPLACEMENT = std::mem::zeroed();
            wp.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
            if self.fullscreen.is_none() && GetWindowPlacement(self.hwnd, &mut wp) != 0 {
                let r = wp.rcNormalPosition;
                self.settings.win = [r.left, r.top, r.right - r.left, r.bottom - r.top];
                self.settings.maximized = wp.showCmd == SW_SHOWMAXIMIZED as u32;
            }
        }
        if let Some((t, f, s)) = self.saved_style.take() {
            self.settings.theme = t;
            self.settings.font = f;
            self.settings.font_size = s;
        }
        if self.no_session {
            // leave the shared session and its backups untouched
        } else if self.settings.remember_session {
            self.save_session();
        } else {
            config::save_session(&Session::default());
        }
        if !self.test {
            config::save_settings(&self.settings);
        }
        self.exiting = true;
        true
    }

    // ------------------------------------------------------------------ results panel

    pub fn push_results(&mut self, header: String, lines: Vec<(String, Option<ResultLoc>, Vec<(usize, usize)>)>) {
        let mut text = header.clone();
        let mut locs: Vec<Option<ResultLoc>> = vec![None];
        let mut marks: Vec<(usize, usize, usize)> = Vec::new(); // (line idx, start col, end col)
        for (i, (l, loc, ranges)) in lines.into_iter().enumerate() {
            text.push('\n');
            text.push_str(&l);
            locs.push(loc);
            for (a, b) in ranges {
                marks.push((i + 1, a, b));
            }
        }
        // prepend to previous results, keep the history bounded
        let old = self.rdoc.text_string();
        let old_locs = std::mem::take(&mut self.results_locs);
        let old_marks: Vec<(usize, usize, u8)> = self.rdoc.marks.clone();
        let new_lines = locs.len();
        let mut full = text.clone();
        let keep_old = !old.is_empty() && old_locs.len() < 50_000;
        if keep_old {
            full.push('\n');
            full.push_str(&old);
        }
        self.rdoc.read_only = false;
        self.rdoc.set_content(&full);
        self.rdoc.read_only = true;
        self.rdoc.lang = Lang::SearchResult;
        self.rdoc.tab_width = 4;
        self.rdoc.hl.reset(self.rdoc.line_count());
        let offset_chars = text.chars().count() + 1;
        let mut m = Vec::new();
        for (li, a, b) in marks {
            let ls = self.rdoc.line_start(li);
            m.push((ls + a, ls + b, 6u8));
        }
        if keep_old {
            for (a, b, s) in old_marks {
                m.push((a + offset_chars, b + offset_chars, s));
            }
        }
        self.rdoc.marks = m;
        self.results_locs = locs;
        if keep_old {
            self.results_locs.extend(old_locs);
            // fold older searches
            self.rdoc.hl.update_folds(&self.rdoc.text, Lang::SearchResult, self.rdoc.version, 4);
            let mut l = new_lines;
            while l < self.rdoc.line_count() {
                if self.rdoc.line_chars(l).first().is_some_and(|c| *c == 'S') {
                    self.rdoc.folded.insert(l);
                }
                l += 1;
            }
        }
        self.rdoc.set_caret(0);
        self.rdoc.scroll_row = 0;
        self.show_results = true;
        self.layout();
        let (v, d, s) = (&mut self.rview, &mut self.rdoc, &self.settings);
        v.ensure_layout(d, s);
        v.update_scrollbars(d, s);
        invalidate(v.hwnd);
        invalidate(self.results_panel);
    }

    fn results_jump(&mut self, line: usize) {
        let Some(Some(loc)) = self.results_locs.get(line).cloned() else {
            // toggle fold on header lines
            self.rdoc.hl.update_folds(&self.rdoc.text, Lang::SearchResult, self.rdoc.version, 4);
            if self.rdoc.hl.fold_end(line).is_some() && !self.rdoc.folded.remove(&line) {
                self.rdoc.folded.insert(line);
            }
            let (v, d, s) = (&mut self.rview, &mut self.rdoc, &self.settings);
            v.ensure_layout(d, s);
            v.update_scrollbars(d, s);
            invalidate(v.hwnd);
            return;
        };
        self.rdoc.set_caret(self.rdoc.line_start(line));
        invalidate(self.rview.hwnd);
        let idx = self.docs.iter().position(|d| d.id == loc.doc_id).or_else(|| {
            let p = loc.path.clone()?;
            self.open_paths(&[p.clone()]);
            self.docs.iter().position(|d| d.path.as_deref().is_some_and(|dp| same_path(dp, &p)))
        });
        let Some(i) = idx else { return };
        self.activate(i);
        let d = &mut self.docs[i];
        if loc.line >= d.line_count() {
            return;
        }
        let ls = d.line_start(loc.line);
        let le = d.line_end(loc.line);
        d.set_sel((ls + loc.start).min(le), (ls + loc.end).min(le));
        self.reveal_selection(loc.line);
        unsafe {
            SetFocus(self.view.hwnd);
        }
    }

    fn results_step(&mut self, forward: bool) {
        if self.results_locs.is_empty() {
            return;
        }
        let n = self.results_locs.len();
        let cur = self.rdoc.line_of(self.rdoc.caret());
        let mut l = cur;
        for _ in 0..n {
            l = if forward { (l + 1) % n } else { (l + n - 1) % n };
            if self.results_locs[l].is_some() {
                self.rview.reveal_line(&mut self.rdoc, l);
                self.results_jump(l);
                let (v, d, s) = (&mut self.rview, &mut self.rdoc, &self.settings);
                v.ensure_caret_visible(d, s);
                v.update_scrollbars(d, s);
                invalidate(v.hwnd);
                return;
            }
        }
    }

    fn results_command(&mut self, id: u16) {
        match id {
            ID_RES_COPY => {
                let t = self.rdoc.selected_text();
                let t = if t.is_empty() { self.rdoc.line_text(self.rdoc.line_of(self.rdoc.caret())) } else { t };
                set_clipboard(self.hwnd, &t.replace('\n', "\r\n"));
            }
            ID_RES_SELALL => {
                let n = self.rdoc.len();
                self.rdoc.set_sel(0, n);
                invalidate(self.rview.hwnd);
            }
            ID_RES_CLEAR => {
                self.rdoc.read_only = false;
                self.rdoc.set_content("");
                self.rdoc.read_only = true;
                self.results_locs.clear();
                invalidate(self.rview.hwnd);
            }
            ID_RES_CLOSE => {
                self.show_results = false;
                self.layout();
            }
            ID_RES_FOLDALL | ID_RES_UNFOLDALL => {
                self.rdoc.hl.update_folds(&self.rdoc.text, Lang::SearchResult, self.rdoc.version, 4);
                self.rdoc.folded.clear();
                if id == ID_RES_FOLDALL {
                    for l in 0..self.rdoc.line_count() {
                        if self.rdoc.hl.fold_end(l).is_some() && self.rdoc.line_chars(l).first() == Some(&'S') {
                            self.rdoc.folded.insert(l);
                        }
                    }
                }
                let (v, d, s) = (&mut self.rview, &mut self.rdoc, &self.settings);
                v.ensure_layout(d, s);
                v.clamp_scroll(d);
                v.update_scrollbars(d, s);
                invalidate(v.hwnd);
            }
            _ => {}
        }
    }

    // ------------------------------------------------------------------ autocomplete

    fn ac_visible(&self) -> bool {
        !self.ac.hwnd.is_null() && unsafe { IsWindowVisible(self.ac.hwnd) } != 0
    }

    pub fn ac_close(&mut self) {
        if self.ac_visible() {
            unsafe {
                ShowWindow(self.ac.hwnd, SW_HIDE);
            }
        }
    }

    fn ac_update(&mut self, auto: bool) {
        let d = &self.docs[self.active];
        if d.sels.len() != 1 {
            self.ac_close();
            return;
        }
        let caret = d.caret();
        let (ws, _) = d.word_at(caret);
        let prefix = d.slice(ws, caret);
        let min = if auto { 2 } else { 1 };
        if prefix.chars().count() < min && !(prefix.is_empty() && !auto) {
            self.ac_close();
            return;
        }
        let pl = prefix.to_lowercase();
        let mut words: Vec<String> = Vec::new();
        // scan a window around the caret so typing stays fast in huge files
        let lo = caret.saturating_sub(128 * 1024);
        let hi = (caret + 128 * 1024).min(d.len());
        let window = d.slice(lo, hi);
        let text = window.as_str();
        let mut seen = HashSet::new();
        let kw = lang::keywords(d.lang);
        for w_ in text.split(|c: char| !is_word_char(c)).chain(kw.kw.iter().map(|s| s.as_str())).chain(kw.ty.iter().map(|s| s.as_str())) {
            if w_.len() > prefix.len() && w_.len() < 80 && w_.to_lowercase().starts_with(&pl) && seen.insert(w_.to_string()) {
                words.push(w_.to_string());
                if words.len() > 2000 {
                    break;
                }
            }
        }
        if words.is_empty() {
            self.ac_close();
            return;
        }
        words.sort_by_key(|a| a.to_lowercase());
        words.truncate(300);
        self.ac.start = ws;
        self.ac.items = words;
        self.ac_show();
    }

    fn ac_show(&mut self) {
        if self.ac.hwnd.is_null() {
            let cls = w(AC_CLASS);
            unsafe {
                self.ac.hwnd = CreateWindowExW(WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE, cls.as_ptr(), std::ptr::null(), WS_POPUP | WS_BORDER, 0, 0, 200, 150, self.hwnd, std::ptr::null_mut(), hinstance(), std::ptr::null());
                let lb = w("LISTBOX");
                self.ac.list = CreateWindowExW(0, lb.as_ptr(), std::ptr::null(), WS_CHILD | WS_VISIBLE | WS_VSCROLL | LBS_NOTIFY as u32 | LBS_NOINTEGRALHEIGHT as u32, 0, 0, 200, 150, self.ac.hwnd, std::ptr::null_mut(), hinstance(), std::ptr::null());
                send(self.ac.list, WM_SETFONT, self.panel_font as usize, 0);
            }
        }
        let lb = self.ac.list;
        send(lb, WM_SETREDRAW, 0, 0);
        send(lb, LB_RESETCONTENT, 0, 0);
        for s in &self.ac.items {
            let ws = w(s);
            send(lb, LB_ADDSTRING, 0, ws.as_ptr() as isize);
        }
        send(lb, LB_SETCURSEL, 0, 0);
        send(lb, WM_SETREDRAW, 1, 0);
        let d = &self.docs[self.active];
        let (r, col) = self.view.row_col(d, d.caret());
        let m = self.view.margins(d, &self.settings);
        let dpi = dpi_of(self.hwnd);
        let x = m.text_x + (col as i32 - (d.caret() - self.ac.start) as i32) * self.view.cw - d.scroll_x;
        let y = (r as i32 - d.scroll_row as i32 + 1) * self.view.lh;
        let mut pt = POINT { x, y };
        unsafe {
            ClientToScreen(self.view.hwnd, &mut pt);
            let item_h = send(lb, LB_GETITEMHEIGHT, 0, 0).max(12) as i32;
            let h = (self.ac.items.len().min(10) as i32) * item_h + 4;
            let wdt = scale(220, dpi);
            SetWindowPos(self.ac.hwnd, HWND_TOPMOST, pt.x, pt.y + 2, wdt, h, SWP_NOACTIVATE | SWP_SHOWWINDOW);
            MoveWindow(lb, 0, 0, wdt - 2, h - 2, 1);
        }
    }

    fn ac_accept(&mut self) {
        let sel = send(self.ac.list, LB_GETCURSEL, 0, 0);
        if sel < 0 || sel as usize >= self.ac.items.len() {
            self.ac_close();
            return;
        }
        let word = self.ac.items[sel as usize].clone();
        let start = self.ac.start;
        self.ac_close();
        let i = self.active;
        let d = &mut self.docs[i];
        let caret = d.caret();
        let (ws, we) = d.word_at(caret);
        if d.sels.len() != 1 || ws != start || start > caret {
            return;
        }
        d.begin(EditKind::Other);
        d.replace(start, we.max(caret) - start, &word);
        let p = start + word.chars().count();
        d.set_caret(p);
        d.end();
        self.refresh_editor();
        unsafe {
            SetFocus(self.view.hwnd);
        }
    }

    // ------------------------------------------------------------------ misc helpers

    fn selection_or_word(&self) -> String {
        let d = self.doc();
        let s = d.main_sel();
        if !s.is_empty() {
            d.slice(s.start(), s.end())
        } else {
            let (a, b) = d.word_at(s.caret);
            d.slice(a, b)
        }
    }

    fn fold_all(&mut self, fold: bool, level: Option<usize>) {
        let i = self.active;
        let d = &mut self.docs[i];
        d.hl.update_folds(&d.text, d.lang, d.version, d.tab_width);
        if !fold && level.is_none() {
            d.folded.clear();
        } else {
            for l in 0..d.line_count() {
                let f = d.hl.folds[l];
                if f.end as usize > l && level.is_none_or(|lv| f.depth as usize + 1 == lv) {
                    if fold {
                        d.folded.insert(l);
                    } else {
                        d.folded.remove(&l);
                    }
                }
            }
        }
        let caret_line = d.line_of(d.caret());
        // move caret out of hidden region
        if let Some(&h) = d.folded.iter().filter(|&&h| h < caret_line && d.hl.fold_end(h).is_some_and(|e| e >= caret_line)).min() {
            let p = d.line_start(h);
            d.set_caret(p);
        }
        self.refresh_editor();
    }

    fn fold_current(&mut self, fold: bool) {
        let i = self.active;
        let d = &mut self.docs[i];
        d.hl.update_folds(&d.text, d.lang, d.version, d.tab_width);
        let line = d.line_of(d.caret());
        if let Some((h, _)) = d.hl.enclosing_fold(line) {
            if fold {
                d.folded.insert(h);
                let p = d.line_start(h);
                d.set_caret(p);
            } else {
                d.folded.remove(&h);
            }
        }
        self.refresh_editor();
    }

    fn set_lang(&mut self, l: Lang) {
        let d = self.doc_mut();
        d.lang = l;
        d.hl.reset(d.line_count());
        d.folded.clear();
        self.refresh_editor();
        self.update_status();
    }

    fn zoom(&mut self, delta: i32) {
        self.settings.zoom = if delta == 0 { 0 } else { (self.settings.zoom + delta).clamp(-9, 30) };
        invalidate(self.view.hwnd);
        self.refresh_editor();
    }

    fn toggle_setting(&mut self, f: impl FnOnce(&mut Settings)) {
        f(&mut self.settings);
        self.layout();
        self.refresh_editor();
        invalidate(self.rview.hwnd);
        self.update_status();
    }

    fn change_encoding(&mut self, enc: Encoding, convert: bool) {
        let i = self.active;
        if !convert {
            // reinterpret the bytes on disk
            if let Some(p) = self.docs[i].path.clone() {
                if !self.docs[i].is_modified() {
                    if let Ok(bytes) = std::fs::read(&p) {
                        let body: &[u8] = match enc {
                            Encoding::Utf8Bom if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) => &bytes[3..],
                            Encoding::Utf16Le | Encoding::Utf16Be if bytes.len() >= 2 && (bytes[..2] == [0xFF, 0xFE] || bytes[..2] == [0xFE, 0xFF]) => &bytes[2..],
                            _ => &bytes,
                        };
                        let text = match enc {
                            Encoding::Ansi => fileio::ansi_decode(body),
                            Encoding::Utf8 | Encoding::Utf8Bom => String::from_utf8_lossy(body).into_owned(),
                            Encoding::Utf16Le => String::from_utf16_lossy(&body.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<_>>()),
                            Encoding::Utf16Be => String::from_utf16_lossy(&body.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect::<Vec<_>>()),
                        };
                        let d = &mut self.docs[i];
                        let caret = d.caret();
                        d.set_content(&fileio::normalize_eol(&text));
                        d.set_caret(caret.min(d.len()));
                    }
                }
            }
            self.docs[i].encoding = enc;
        } else if self.docs[i].encoding != enc {
            self.docs[i].encoding = enc;
            self.docs[i].mark_dirty();
        }
        self.refresh_editor();
        self.update_status();
    }

    fn set_eol(&mut self, eol: Eol) {
        let d = self.doc_mut();
        if d.eol != eol {
            d.eol = eol;
            d.mark_dirty();
        }
        invalidate(self.view.hwnd);
        self.update_status();
    }

    fn toggle_fullscreen(&mut self) {
        unsafe {
            if let Some((wp, style, menu_on)) = self.fullscreen.take() {
                SetWindowLongW(self.hwnd, GWL_STYLE, style as i32);
                if menu_on {
                    SetMenu(self.hwnd, self.menus.bar);
                }
                SetWindowPlacement(self.hwnd, &wp);
                SetWindowPos(self.hwnd, std::ptr::null_mut(), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED);
            } else {
                let mut wp: WINDOWPLACEMENT = std::mem::zeroed();
                wp.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
                GetWindowPlacement(self.hwnd, &mut wp);
                let style = GetWindowLongW(self.hwnd, GWL_STYLE) as u32;
                let mon = MonitorFromWindow(self.hwnd, MONITOR_DEFAULTTONEAREST);
                let mut mi: MONITORINFO = std::mem::zeroed();
                mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
                GetMonitorInfoW(mon, &mut mi);
                SetWindowLongW(self.hwnd, GWL_STYLE, (style & !WS_OVERLAPPEDWINDOW) as i32);
                SetMenu(self.hwnd, std::ptr::null_mut());
                let r = mi.rcMonitor;
                SetWindowPos(self.hwnd, HWND_TOP, r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_FRAMECHANGED);
                self.fullscreen = Some((wp, style, true));
            }
        }
        self.layout();
    }

    fn toggle_postit(&mut self) {
        unsafe {
            if let Some((style, tb, sb, tabs)) = self.postit.take() {
                SetWindowLongW(self.hwnd, GWL_STYLE, style as i32);
                SetMenu(self.hwnd, self.menus.bar);
                self.settings.toolbar = tb;
                self.settings.statusbar = sb;
                self.settings.tabbar = tabs;
                SetWindowPos(self.hwnd, if self.settings.always_on_top { HWND_TOPMOST } else { HWND_NOTOPMOST }, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_FRAMECHANGED);
            } else {
                let style = GetWindowLongW(self.hwnd, GWL_STYLE) as u32;
                self.postit = Some((style, self.settings.toolbar, self.settings.statusbar, self.settings.tabbar));
                SetWindowLongW(self.hwnd, GWL_STYLE, ((style & !WS_CAPTION) | WS_THICKFRAME) as i32);
                SetMenu(self.hwnd, std::ptr::null_mut());
                self.settings.toolbar = false;
                self.settings.statusbar = false;
                self.settings.tabbar = false;
                SetWindowPos(self.hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_FRAMECHANGED);
            }
        }
        self.layout();
    }

    fn summary(&self) {
        let d = self.doc();
        let text = d.text_string();
        let words = text.split(|c: char| !is_word_char(c)).filter(|w| !w.is_empty()).count();
        let chars_no_ws = text.chars().filter(|c| !c.is_whitespace()).count();
        let lines = d.line_count();
        let crlf = d.eol == Eol::Crlf;
        let bytes = fileio::encode(&text, d.eol, d.encoding).len();
        let mut info = String::new();
        if let Some(p) = &d.path {
            info.push_str(&format!("Full file path: {}\n", p.display()));
            if let Ok(m) = std::fs::metadata(p) {
                if let Ok(t) = m.created() {
                    info.push_str(&format!("Created: {}\n", fmt_time(t)));
                }
                if let Ok(t) = m.modified() {
                    info.push_str(&format!("Modified: {}\n", fmt_time(t)));
                }
            }
            info.push('\n');
        }
        let _ = crlf;
        info.push_str(&format!(
            "Characters (without line endings): {}\nWords: {}\nLines: {}\nDocument length: {}\nCharacters (without blanks): {}\n",
            fmt_num(d.len() - (lines - 1)),
            fmt_num(words),
            fmt_num(lines),
            fmt_num(bytes),
            fmt_num(chars_no_ws)
        ));
        let s = d.main_sel();
        if !s.is_empty() {
            info.push_str(&format!("\n{} selected characters ({} bytes) in {} ranges", fmt_num(s.end() - s.start()), fmt_num(d.slice(s.start(), s.end()).len()), d.sels.iter().filter(|x| !x.is_empty()).count()));
        }
        msgbox(self.hwnd, &info, "Summary", MB_OK | MB_ICONINFORMATION);
    }

    fn run_command(&mut self) {
        let hist = self.settings.run_history.clone();
        let Some(cmdline) = dialogs::run_dialog(self.hwnd, &hist) else { return };
        if cmdline.trim().is_empty() {
            return;
        }
        push_history(&mut self.settings.run_history, &cmdline, 15);
        let d = self.doc();
        let full = d.path.as_ref().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
        let dir = d.path.as_ref().and_then(|p| p.parent()).map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
        let fname = d.title();
        let (name_part, ext_part) = match fname.rfind('.') {
            Some(i) => (fname[..i].to_string(), fname[i..].to_string()),
            None => (fname.clone(), String::new()),
        };
        let caret = d.caret();
        let line = d.line_of(caret) + 1;
        let col = d.col_of(caret) + 1;
        let word = self.selection_or_word();
        let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_string_lossy().into_owned())).unwrap_or_default();
        let expanded = cmdline
            .replace("$(FULL_CURRENT_PATH)", &full)
            .replace("$(CURRENT_DIRECTORY)", &dir)
            .replace("$(FILE_NAME)", &fname)
            .replace("$(NAME_PART)", &name_part)
            .replace("$(EXT_PART)", &ext_part)
            .replace("$(CURRENT_WORD)", &word)
            .replace("$(CURRENT_LINE)", &line.to_string())
            .replace("$(CURRENT_COLUMN)", &col.to_string())
            .replace("$(NPP_DIRECTORY)", &exe_dir);
        let (file, args) = split_cmdline(&expanded);
        shell_exec(&file, &args, if dir.is_empty() { None } else { Some(&dir) });
    }

    fn print(&mut self) {
        use windows_sys::Win32::Storage::Xps::*;
        let d = self.doc();
        unsafe {
            let mut pd: PRINTDLGW = std::mem::zeroed();
            pd.lStructSize = std::mem::size_of::<PRINTDLGW>() as u32;
            pd.hwndOwner = self.hwnd;
            pd.Flags = PD_RETURNDC | PD_NOSELECTION | PD_NOPAGENUMS;
            if PrintDlgW(&mut pd) == 0 || pd.hDC.is_null() {
                return;
            }
            let hdc = pd.hDC;
            let title = w(&d.title());
            let di = DOCINFOW { cbSize: std::mem::size_of::<DOCINFOW>() as i32, lpszDocName: title.as_ptr(), lpszOutput: std::ptr::null(), lpszDatatype: std::ptr::null(), fwType: 0 };
            if StartDocW(hdc, &di) <= 0 {
                DeleteDC(hdc);
                return;
            }
            let dpi_y = GetDeviceCaps(hdc, LOGPIXELSY as i32) as u32;
            let pw = GetDeviceCaps(hdc, HORZRES as i32);
            let ph = GetDeviceCaps(hdc, VERTRES as i32);
            let font = make_font(&self.settings.font, self.settings.font_size * 10, dpi_y, false, false);
            let hfont = make_font("Segoe UI", 80, dpi_y, false, true);
            let old = SelectObject(hdc, font as _);
            let mut tm: TEXTMETRICW = std::mem::zeroed();
            GetTextMetricsW(hdc, &mut tm);
            let lh = tm.tmHeight + tm.tmExternalLeading;
            let margin = (dpi_y as i32) / 2;
            let cw = text_width(hdc, "M").max(1);
            let cols = ((pw - margin * 2) / cw).max(10) as usize;
            let header_h = lh * 2;
            let lines_per_page = ((ph - margin * 2 - header_h * 2) / lh).max(1) as usize;
            let mut rows: Vec<String> = Vec::new();
            for l in 0..d.line_count() {
                let t = d.line_text(l).replace('\t', &" ".repeat(d.tab_width));
                let chars: Vec<char> = t.chars().collect();
                if chars.is_empty() {
                    rows.push(String::new());
                }
                for chunk in chars.chunks(cols) {
                    rows.push(chunk.iter().collect());
                }
            }
            let pages = rows.len().div_ceil(lines_per_page).max(1);
            for p in 0..pages {
                StartPage(hdc);
                SelectObject(hdc, hfont as _);
                text_out(hdc, margin, margin, &d.full_title());
                let pn = format!("Page {} of {}", p + 1, pages);
                text_out(hdc, pw - margin - text_width(hdc, &pn), ph - margin - lh, &pn);
                SelectObject(hdc, font as _);
                for (k, row) in rows.iter().skip(p * lines_per_page).take(lines_per_page).enumerate() {
                    text_out(hdc, margin, margin + header_h + k as i32 * lh, row);
                }
                EndPage(hdc);
            }
            EndDoc(hdc);
            SelectObject(hdc, old);
            DeleteObject(font as _);
            DeleteObject(hfont as _);
            DeleteDC(hdc);
        }
    }

    fn macro_play(&mut self, times: Option<usize>) {
        if self.macro_steps.is_empty() {
            return;
        }
        self.playing = true;
        let steps = self.macro_steps.clone();
        let mut n = 0;
        loop {
            let before = (self.doc().caret(), self.doc().version);
            for st in &steps {
                match st {
                    MacroStep::Cmd(c) => self.exec(*c),
                    MacroStep::Text(t) => {
                        let ov = self.view.overtype;
                        self.doc_mut().insert_text(t, ov);
                    }
                    MacroStep::Key(vk, c, s, a) => {
                        let (v, d, set) = editor_parts(false);
                        v.key(d, set, *vk, *c, *s, *a);
                    }
                }
            }
            n += 1;
            let after = (self.doc().caret(), self.doc().version);
            match times {
                Some(t) if n >= t => break,
                None => {
                    let d = self.doc();
                    if d.caret() >= d.len() || after == before || n > 1_000_000 {
                        break;
                    }
                }
                _ => {}
            }
        }
        self.playing = false;
        self.refresh_editor();
    }

    fn insert_column(&mut self, ci: dialogs::ColumnInsert) {
        let d = self.doc_mut();
        let (first, last, col) = if let Some(r) = d.rect {
            (r.anchor_line.min(r.caret_line), r.anchor_line.max(r.caret_line), r.anchor_col.min(r.caret_col))
        } else if d.sels.len() > 1 {
            let mut lines: Vec<usize> = d.sels.iter().map(|s| d.line_of(s.caret)).collect();
            lines.sort();
            (lines[0], *lines.last().unwrap(), d.col_of(d.sels.iter().map(|s| s.start()).min().unwrap_or(0)))
        } else {
            let c = d.caret();
            (d.line_of(c), d.line_count() - 1, d.col_of(c))
        };
        let count = last - first + 1;
        let texts: Vec<String> = match ci {
            dialogs::ColumnInsert::Text(t) => vec![t; count],
            dialogs::ColumnInsert::Numbers { init, step, repeat, leading, base } => {
                let nums: Vec<i64> = (0..count).map(|k| init + step * (k / repeat.max(1)) as i64).collect();
                let fmt = |n: i64| match base {
                    16 => format!("{:X}", n),
                    8 => format!("{:o}", n),
                    2 => format!("{:b}", n),
                    _ => n.to_string(),
                };
                let width = nums.iter().map(|&n| fmt(n).len()).max().unwrap_or(1);
                nums.iter()
                    .map(|&n| match leading {
                        1 => format!("{:0>width$}", fmt(n), width = width),
                        2 => format!("{:>width$}", fmt(n), width = width),
                        _ => fmt(n),
                    })
                    .collect()
            }
        };
        d.insert_column(first, last, col, &texts);
        self.refresh_editor();
    }

    fn transform_selection(&mut self, f: impl Fn(&str) -> Option<String>) {
        let d = self.doc_mut();
        if !d.has_selection() {
            let n = d.len();
            d.set_sel(0, n);
        }
        let ok = std::cell::Cell::new(true);
        d.replace_sels(true, |t| match f(t) {
            Some(r) => r,
            None => {
                ok.set(false);
                t.to_string()
            }
        });
        if !ok.get() {
            msgbox(self.hwnd, "The selection could not be converted.", "Notepad++ RS", MB_OK | MB_ICONWARNING);
        }
        self.refresh_editor();
    }

    // ------------------------------------------------------------------ command dispatch

    pub fn exec(&mut self, c: Cmd) {
        if self.recording && is_recordable(c) {
            self.recording_buf.push(MacroStep::Cmd(c));
        }
        let i = self.active;
        let hwnd = self.hwnd;
        match c {
            Cmd::New => {
                let d = self.new_document();
                self.docs.push(d);
                let n = self.docs.len() - 1;
                self.activate(n);
            }
            Cmd::Open => {
                let files = open_file_dialog(hwnd, true, OPEN_FILTER);
                self.open_paths(&files);
            }
            Cmd::Reload => self.reload(true),
            Cmd::Save => {
                self.save_doc(i);
            }
            Cmd::SaveAs => {
                self.save_as(i, false);
            }
            Cmd::SaveCopy => {
                self.save_as(i, true);
            }
            Cmd::SaveAll => {
                for k in 0..self.docs.len() {
                    if self.docs[k].is_modified() && !self.save_doc(k) {
                        break;
                    }
                }
            }
            Cmd::Rename => {
                let d = &self.docs[i];
                if let Some(p) = d.path.clone() {
                    let name = d.title();
                    if let Some(nn) = dialogs::input_box(hwnd, "Rename Current Document", "New name :", &name) {
                        let np = p.with_file_name(nn.trim());
                        if np != p {
                            match std::fs::rename(&p, &np) {
                                Ok(()) => {
                                    let d = &mut self.docs[i];
                                    d.path = Some(np.clone());
                                    d.mtime = fileio::mtime(&np);
                                    let nl = lang::detect(&np);
                                    if nl != d.lang {
                                        d.lang = nl;
                                        d.hl.reset(d.line_count());
                                    }
                                }
                                Err(e) => {
                                    msgbox(hwnd, &format!("Rename failed: {e}"), "Rename", MB_OK | MB_ICONWARNING);
                                }
                            }
                        }
                    }
                } else if let Some(nn) = dialogs::input_box(hwnd, "Rename Current Document", "New name :", &d.untitled_name.clone()) {
                    if !nn.trim().is_empty() {
                        self.docs[i].untitled_name = nn.trim().to_string();
                    }
                }
                self.update_status();
            }
            Cmd::Close => {
                self.close_doc(i);
            }
            Cmd::CloseAll => self.close_where(|_, _| true),
            Cmd::CloseOthers => {
                let id = self.docs[i].id;
                self.close_where(|_, d| d.id != id);
            }
            Cmd::CloseLeft => self.close_where(|k, _| k < i),
            Cmd::CloseRight => self.close_where(|k, _| k > i),
            Cmd::CloseUnchanged => self.close_where(|_, d| !d.is_modified()),
            Cmd::DeleteFile => {
                if let Some(p) = self.docs[i].path.clone() {
                    let q = format!("The file \"{}\"\nwill be moved to your Recycle Bin and this document will be closed.\nContinue?", p.display());
                    if msgbox(hwnd, &q, "Delete file", MB_YESNO | MB_ICONQUESTION) == IDYES && recycle(&p) {
                        self.docs[i].mark_saved();
                        self.close_doc(i);
                    }
                }
            }
            Cmd::Print => self.print(),
            Cmd::RestoreClosed => {
                if let Some(p) = self.closed.pop() {
                    self.open_paths(&[p]);
                }
            }
            Cmd::OpenAllRecent => {
                let r: Vec<PathBuf> = self.settings.recent.iter().map(PathBuf::from).collect();
                self.open_paths(&r);
            }
            Cmd::EmptyRecent => self.settings.recent.clear(),
            Cmd::Exit => unsafe {
                PostMessageW(hwnd, WM_CLOSE, 0, 0);
            },
            Cmd::OpenFolder | Cmd::OpenExplorerSelect => {
                if let Some(p) = &self.docs[i].path {
                    shell_exec("explorer.exe", &format!("/select,\"{}\"", p.display()), None);
                }
            }
            Cmd::OpenCmd => {
                if let Some(dir) = self.docs[i].path.as_ref().and_then(|p| p.parent()) {
                    shell_exec("cmd.exe", "", Some(&dir.to_string_lossy()));
                }
            }
            // ---- edit
            Cmd::Undo => {
                self.doc_mut().undo();
                self.refresh_editor();
            }
            Cmd::Redo => {
                self.doc_mut().redo();
                self.refresh_editor();
            }
            Cmd::Cut | Cmd::Copy => {
                let d = self.doc();
                let (text, whole_line) = if d.has_selection() {
                    (d.selected_text(), false)
                } else {
                    let l = d.line_of(d.caret());
                    (format!("{}\n", d.line_text(l)), true)
                };
                let eol = d.eol.as_str();
                set_clipboard(hwnd, &text.replace('\n', eol));
                self.rect_clip = if d.rect.is_some() { Some(text.clone()) } else { None };
                if c == Cmd::Cut && !self.doc().read_only {
                    if whole_line {
                        self.doc_mut().delete_lines();
                    } else {
                        self.doc_mut().replace_sels(false, |_| String::new());
                    }
                    self.refresh_editor();
                }
            }
            Cmd::Paste => {
                if let Some(t) = get_clipboard(hwnd) {
                    let norm = fileio::normalize_eol(&t);
                    let rect = self.rect_clip.as_deref() == Some(norm.as_str());
                    let d = self.doc_mut();
                    if rect && d.sels.len() == 1 && !d.read_only {
                        // paste a block copied from a column selection as a column
                        let lines: Vec<String> = norm.trim_end_matches('\n').split('\n').map(String::from).collect();
                        d.begin(EditKind::Other);
                        let s = d.main_sel();
                        if !s.is_empty() {
                            d.replace(s.start(), s.end() - s.start(), "");
                            d.set_caret(s.start());
                        }
                        let caret = d.caret();
                        let line = d.line_of(caret);
                        let col = d.col_of(caret);
                        let need = line + lines.len();
                        while d.line_count() < need {
                            let n = d.len();
                            d.replace(n, 0, "\n");
                        }
                        d.insert_column(line, line + lines.len() - 1, col, &lines);
                        d.end();
                    } else {
                        d.paste(&t);
                    }
                    self.refresh_editor();
                }
            }
            Cmd::Delete => {
                self.doc_mut().delete_fwd();
                self.refresh_editor();
            }
            Cmd::SelectAll => {
                let d = self.doc_mut();
                let n = d.len();
                d.set_sel(0, n);
                self.refresh_editor();
            }
            Cmd::BeginEndSelect => {
                let caret = self.doc().caret();
                match self.view.begin_select.take() {
                    Some(a) => self.doc_mut().set_sel(a, caret),
                    None => self.view.begin_select = Some(caret),
                }
                self.refresh_editor();
            }
            Cmd::InsertDateShort | Cmd::InsertDateLong => {
                let t = local_time_string(c == Cmd::InsertDateLong);
                self.doc_mut().insert_text(&t, false);
                self.refresh_editor();
            }
            Cmd::CopyFullPath => {
                let t = self.docs[i].full_title();
                set_clipboard(hwnd, &t);
            }
            Cmd::CopyFileName => {
                let t = self.docs[i].title();
                set_clipboard(hwnd, &t);
            }
            Cmd::CopyDirPath => {
                let t = self.docs[i].path.as_ref().and_then(|p| p.parent()).map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
                set_clipboard(hwnd, &t);
            }
            Cmd::Indent => {
                let sp = self.settings.use_spaces;
                self.doc_mut().indent_lines(sp);
                self.refresh_editor();
            }
            Cmd::Outdent => {
                self.doc_mut().outdent_lines();
                self.refresh_editor();
            }
            Cmd::Upper | Cmd::Lower | Cmd::Proper | Cmd::Sentence | Cmd::Invert | Cmd::RandomCase => {
                let k = match c {
                    Cmd::Upper => 0,
                    Cmd::Lower => 1,
                    Cmd::Proper => 2,
                    Cmd::Sentence => 3,
                    Cmd::Invert => 4,
                    _ => 5,
                };
                self.doc_mut().convert_case(k);
                self.refresh_editor();
            }
            Cmd::DupLine => {
                self.doc_mut().duplicate_lines();
                self.refresh_editor();
            }
            Cmd::RemoveDupLines => {
                self.doc_mut().map_lines(|v| {
                    let mut seen = HashSet::new();
                    v.into_iter().filter(|l| seen.insert(l.clone())).collect()
                });
                self.refresh_editor();
            }
            Cmd::RemoveConsecDup => {
                self.doc_mut().map_lines(|mut v| {
                    v.dedup();
                    v
                });
                self.refresh_editor();
            }
            Cmd::SplitLines => {
                let w_ = if self.settings.edge_column > 0 { self.settings.edge_column } else { 80 };
                self.doc_mut().split_lines(w_);
                self.refresh_editor();
            }
            Cmd::JoinLines => {
                self.doc_mut().join_lines();
                self.refresh_editor();
            }
            Cmd::MoveUp | Cmd::MoveDown => {
                self.doc_mut().move_lines(c == Cmd::MoveUp);
                self.refresh_editor();
            }
            Cmd::RemoveEmptyLines => {
                self.doc_mut().map_lines(|v| v.into_iter().filter(|l| !l.is_empty()).collect());
                self.refresh_editor();
            }
            Cmd::RemoveEmptyLinesWs => {
                self.doc_mut().map_lines(|v| v.into_iter().filter(|l| !l.trim().is_empty()).collect());
                self.refresh_editor();
            }
            Cmd::BlankAbove | Cmd::BlankBelow => {
                self.doc_mut().insert_blank_line(c == Cmd::BlankAbove);
                self.refresh_editor();
            }
            Cmd::ReverseLines => {
                self.doc_mut().map_lines(|mut v| {
                    v.reverse();
                    v
                });
                self.refresh_editor();
            }
            Cmd::SortAsc | Cmd::SortDesc | Cmd::SortAscCi | Cmd::SortDescCi | Cmd::SortNumAsc | Cmd::SortNumDesc | Cmd::SortLenAsc => {
                let k = c as u16 - Cmd::SortAsc as u16;
                self.doc_mut().map_lines(|v| crate::ops::sort_lines(v, k as u8));
                self.refresh_editor();
            }
            Cmd::DeleteLine => {
                self.doc_mut().delete_lines();
                self.refresh_editor();
            }
            Cmd::Transpose => {
                self.doc_mut().transpose_line();
                self.refresh_editor();
            }
            Cmd::ToggleComment | Cmd::LineComment | Cmd::LineUncomment => {
                let m = match c {
                    Cmd::ToggleComment => 0,
                    Cmd::LineComment => 1,
                    _ => 2,
                };
                self.doc_mut().line_comment(m);
                self.refresh_editor();
            }
            Cmd::BlockComment | Cmd::BlockUncomment => {
                self.doc_mut().block_comment(c == Cmd::BlockComment);
                self.refresh_editor();
            }
            Cmd::CompleteWord => self.ac_update(false),
            Cmd::EolCrlf => self.set_eol(Eol::Crlf),
            Cmd::EolLf => self.set_eol(Eol::Lf),
            Cmd::EolCr => self.set_eol(Eol::Cr),
            Cmd::TrimTrailing => {
                self.doc_mut().map_lines(|v| v.into_iter().map(|l| l.trim_end().to_string()).collect());
                self.refresh_editor();
            }
            Cmd::TrimLeading => {
                self.doc_mut().map_lines(|v| v.into_iter().map(|l| l.trim_start().to_string()).collect());
                self.refresh_editor();
            }
            Cmd::TrimBoth => {
                self.doc_mut().map_lines(|v| v.into_iter().map(|l| l.trim().to_string()).collect());
                self.refresh_editor();
            }
            Cmd::EolToSpace | Cmd::TrimAndEolToSpace => {
                let trim = c == Cmd::TrimAndEolToSpace;
                self.doc_mut().map_lines(|v| vec![v.iter().map(|l| if trim { l.trim().to_string() } else { l.clone() }).collect::<Vec<_>>().join(" ")]);
                self.refresh_editor();
            }
            Cmd::TabToSpace => {
                self.doc_mut().tabs_to_spaces();
                self.refresh_editor();
            }
            Cmd::SpaceToTabAll | Cmd::SpaceToTabLeading => {
                self.doc_mut().spaces_to_tabs(c == Cmd::SpaceToTabLeading);
                self.refresh_editor();
            }
            Cmd::ColumnEditor => {
                if let Some(ci) = dialogs::column_editor(hwnd) {
                    self.insert_column(ci);
                }
            }
            Cmd::OpenSelectedFile => {
                let t = self.selection_or_word_path();
                let p = PathBuf::from(t.trim());
                let p = if p.is_relative() { self.docs[i].path.as_ref().and_then(|d| d.parent()).map(|d| d.join(&p)).unwrap_or(p) } else { p };
                if p.is_file() {
                    self.open_paths(&[p]);
                }
            }
            Cmd::SearchInternet => {
                let q = self.selection_or_word();
                if !q.is_empty() {
                    shell_open(&format!("https://www.google.com/search?q={}", url_encode(&q)));
                }
            }
            Cmd::ReadOnly => {
                let d = self.doc_mut();
                d.read_only = !d.read_only;
                self.update_status();
            }
            // ---- search
            Cmd::Find => find::show_dialog(0),
            Cmd::Replace => find::show_dialog(1),
            Cmd::FindInFiles => find::show_dialog(2),
            Cmd::Mark => find::show_dialog(3),
            Cmd::FindNext | Cmd::FindPrev => {
                let mut o = find::opts_from_settings();
                if o.what.is_empty() {
                    find::show_dialog(0);
                    return;
                }
                o.backward = c == Cmd::FindPrev;
                find::find_next(&o);
            }
            Cmd::SelectFindNext | Cmd::SelectFindPrev => {
                let word = self.selection_or_word();
                if word.is_empty() {
                    return;
                }
                let d = self.doc_mut();
                let s = d.main_sel();
                if s.is_empty() {
                    let (a, b) = d.word_at(s.caret);
                    d.set_sel(a, b);
                }
                push_history(&mut self.settings.find_history, &word, 20);
                let o = find::Opts { what: word, case: self.settings.match_case, mode: 0, wrap: true, backward: c == Cmd::SelectFindPrev, ..Default::default() };
                find::find_next(&o);
            }
            Cmd::SearchResults => {
                self.show_results = !self.show_results;
                self.layout();
            }
            Cmd::NextResult | Cmd::PrevResult => self.results_step(c == Cmd::NextResult),
            Cmd::Goto => {
                let d = self.doc();
                let caret = d.caret();
                if let Some((offset, v)) = dialogs::goto_dialog(hwnd, d.line_of(caret) + 1, d.line_count(), caret, d.len()) {
                    let d = self.doc_mut();
                    let p = if offset { v.min(d.len()) } else { d.line_start(v.clamp(1, d.line_count()) - 1) };
                    d.set_caret(p);
                    let line = d.line_of(p);
                    self.reveal_selection(line);
                }
            }
            Cmd::GotoBrace | Cmd::SelectBrace => {
                let d = self.doc_mut();
                if c == Cmd::GotoBrace {
                    if let Some((bp, Some(mp))) = d.brace_pair() {
                        let caret = d.caret();
                        let target = if caret == bp + 1 { mp + 1 } else { mp };
                        d.set_caret(target);
                    }
                } else {
                    let caret = d.caret();
                    // find enclosing bracket pair
                    let mut p = caret;
                    let mut depth: i32 = 0;
                    let mut open = None;
                    while p > 0 {
                        p -= 1;
                        let ch = d.char_at(p).unwrap_or(' ');
                        if ")]}".contains(ch) {
                            depth += 1;
                        } else if "([{".contains(ch) {
                            if depth == 0 {
                                open = Some(p);
                                break;
                            }
                            depth -= 1;
                        }
                    }
                    if let Some(o) = open {
                        if let Some(m) = d.match_brace(o) {
                            d.set_sel(o + 1, m);
                        }
                    }
                }
                self.refresh_editor();
            }
            Cmd::Style1 | Cmd::Style2 | Cmd::Style3 | Cmd::Style4 | Cmd::Style5 => find::style_token((c as u16 - Cmd::Style1 as u16 + 1) as u8),
            Cmd::ClearStyle1 | Cmd::ClearStyle2 | Cmd::ClearStyle3 | Cmd::ClearStyle4 | Cmd::ClearStyle5 => find::clear_marks(Some((c as u16 - Cmd::ClearStyle1 as u16 + 1) as u8)),
            Cmd::ClearAllStyles => find::clear_marks(None),
            Cmd::ToggleBookmark => {
                let d = self.doc_mut();
                let l = d.line_of(d.caret());
                if !d.bookmarks.remove(&l) {
                    d.bookmarks.insert(l);
                }
                invalidate(self.view.hwnd);
            }
            Cmd::NextBookmark | Cmd::PrevBookmark => {
                let d = self.doc_mut();
                let l = d.line_of(d.caret());
                let target = if c == Cmd::NextBookmark {
                    d.bookmarks.range(l + 1..).next().or_else(|| d.bookmarks.iter().next()).copied()
                } else {
                    d.bookmarks.range(..l).next_back().or_else(|| d.bookmarks.iter().next_back()).copied()
                };
                if let Some(t) = target {
                    let p = d.line_start(t);
                    d.set_caret(p);
                    self.reveal_selection(t);
                }
            }
            Cmd::ClearBookmarks => {
                self.doc_mut().bookmarks.clear();
                invalidate(self.view.hwnd);
            }
            Cmd::CopyBookmarked | Cmd::CutBookmarked => {
                let t = self.doc().bookmarked_lines_text();
                set_clipboard(hwnd, &t.replace('\n', "\r\n"));
                if c == Cmd::CutBookmarked {
                    self.doc_mut().remove_lines_where(true);
                    self.refresh_editor();
                }
            }
            Cmd::RemoveBookmarked => {
                self.doc_mut().remove_lines_where(true);
                self.refresh_editor();
            }
            Cmd::RemoveUnbookmarked => {
                self.doc_mut().remove_lines_where(false);
                let d = self.doc_mut();
                let n = d.line_count();
                d.bookmarks = (0..n.saturating_sub(1).max(1)).filter(|&l| l < n).collect();
                if d.len() == 0 {
                    d.bookmarks.clear();
                }
                self.refresh_editor();
            }
            Cmd::InverseBookmark => {
                let d = self.doc_mut();
                let n = d.line_count();
                d.bookmarks = (0..n).filter(|l| !d.bookmarks.contains(l)).collect();
                invalidate(self.view.hwnd);
            }
            // ---- view
            Cmd::AlwaysOnTop => {
                self.settings.always_on_top = !self.settings.always_on_top;
                unsafe {
                    SetWindowPos(hwnd, if self.settings.always_on_top { HWND_TOPMOST } else { HWND_NOTOPMOST }, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
                }
            }
            Cmd::FullScreen => self.toggle_fullscreen(),
            Cmd::PostIt => self.toggle_postit(),
            Cmd::ShowWs => self.toggle_setting(|s| s.show_ws = !s.show_ws),
            Cmd::ShowEol => self.toggle_setting(|s| s.show_eol = !s.show_eol),
            Cmd::ShowAll => self.toggle_setting(|s| {
                let on = !(s.show_ws && s.show_eol);
                s.show_ws = on;
                s.show_eol = on;
            }),
            Cmd::ShowIndentGuide => self.toggle_setting(|s| s.indent_guides = !s.indent_guides),
            Cmd::ZoomIn => self.zoom(1),
            Cmd::ZoomOut => self.zoom(-1),
            Cmd::ZoomRestore => self.zoom(0),
            Cmd::WordWrap => self.toggle_setting(|s| s.word_wrap = !s.word_wrap),
            Cmd::FoldAll => self.fold_all(true, None),
            Cmd::UnfoldAll => self.fold_all(false, None),
            Cmd::FoldCurrent => self.fold_current(true),
            Cmd::UnfoldCurrent => self.fold_current(false),
            Cmd::FoldLevel1 | Cmd::FoldLevel2 | Cmd::FoldLevel3 | Cmd::FoldLevel4 | Cmd::FoldLevel5 | Cmd::FoldLevel6 | Cmd::FoldLevel7 | Cmd::FoldLevel8 => {
                self.fold_all(true, Some((c as u16 - Cmd::FoldLevel1 as u16 + 1) as usize))
            }
            Cmd::UnfoldLevel1 | Cmd::UnfoldLevel2 | Cmd::UnfoldLevel3 | Cmd::UnfoldLevel4 | Cmd::UnfoldLevel5 | Cmd::UnfoldLevel6 | Cmd::UnfoldLevel7 | Cmd::UnfoldLevel8 => {
                self.fold_all(false, Some((c as u16 - Cmd::UnfoldLevel1 as u16 + 1) as usize))
            }
            Cmd::Summary => self.summary(),
            Cmd::FuncList => {
                self.show_funcs = !self.show_funcs;
                self.layout();
                self.refresh_funcs(true);
            }
            Cmd::DocMap => {}
            Cmd::NextTab | Cmd::PrevTab => {
                let n = self.docs.len();
                let t = if c == Cmd::NextTab { (i + 1) % n } else { (i + n - 1) % n };
                self.activate(t);
            }
            Cmd::MoveTabFwd => {
                if i + 1 < self.docs.len() {
                    move_tab(i, i + 1);
                }
            }
            Cmd::MoveTabBack => {
                if i > 0 {
                    move_tab(i, i - 1);
                }
            }
            Cmd::Tab1 | Cmd::Tab2 | Cmd::Tab3 | Cmd::Tab4 | Cmd::Tab5 | Cmd::Tab6 | Cmd::Tab7 | Cmd::Tab8 | Cmd::Tab9 => {
                let k = (c as u16 - Cmd::Tab1 as u16) as usize;
                let k = if c == Cmd::Tab9 { self.docs.len() - 1 } else { k };
                self.activate(k);
            }
            Cmd::Monitoring => {
                let d = self.doc();
                if d.path.is_none() {
                    return;
                }
                let id = d.id;
                if !self.monitoring.remove(&id) {
                    if self.doc().is_modified() {
                        msgbox(hwnd, "The document is modified. Please save it before monitoring.", "Monitoring", MB_OK | MB_ICONWARNING);
                        return;
                    }
                    self.monitoring.insert(id);
                    self.doc_mut().read_only = true;
                    let d = self.doc_mut();
                    let end = d.len();
                    d.set_caret(end);
                    self.refresh_editor();
                } else {
                    self.doc_mut().read_only = false;
                }
                self.update_status();
            }
            // ---- encoding
            Cmd::EncAnsi => self.change_encoding(Encoding::Ansi, false),
            Cmd::EncUtf8 => self.change_encoding(Encoding::Utf8, false),
            Cmd::EncUtf8Bom => self.change_encoding(Encoding::Utf8Bom, false),
            Cmd::EncUtf16Be => self.change_encoding(Encoding::Utf16Be, false),
            Cmd::EncUtf16Le => self.change_encoding(Encoding::Utf16Le, false),
            Cmd::ConvAnsi => self.change_encoding(Encoding::Ansi, true),
            Cmd::ConvUtf8 => self.change_encoding(Encoding::Utf8, true),
            Cmd::ConvUtf8Bom => self.change_encoding(Encoding::Utf8Bom, true),
            Cmd::ConvUtf16Be => self.change_encoding(Encoding::Utf16Be, true),
            Cmd::ConvUtf16Le => self.change_encoding(Encoding::Utf16Le, true),
            // ---- settings
            Cmd::Preferences => {
                if let Some(ns) = dialogs::preferences(hwnd, &self.settings) {
                    let tw = ns.tab_width;
                    self.settings = ns;
                    for d in &mut self.docs {
                        d.tab_width = tw;
                        d.hl.reset(d.line_count());
                    }
                    self.layout();
                    self.refresh_editor();
                    invalidate(self.tabbar.hwnd);
                }
            }
            Cmd::StyleConfigurator => {
                let old = (self.settings.theme, self.settings.font.clone(), self.settings.font_size);
                self.saved_style = Some(old.clone());
                match dialogs::style_configurator(hwnd, old.0, &old.1, old.2) {
                    Some((t, f, s)) => preview_style(t, &f, s),
                    None => preview_style(old.0, &old.1, old.2),
                }
                self.saved_style = None;
                self.refresh_editor();
            }
            Cmd::ShortcutMapper => {
                let items: Vec<String> = menu::shortcut_list().into_iter().map(|(n, s)| format!("{n}\t{s}")).collect();
                dialogs::list_dialog(hwnd, "Shortcut Mapper", items, false, &[]);
            }
            Cmd::ToggleToolbar => self.toggle_setting(|s| s.toolbar = !s.toolbar),
            Cmd::ToggleStatusbar => self.toggle_setting(|s| s.statusbar = !s.statusbar),
            Cmd::ToggleTabbar => self.toggle_setting(|s| s.tabbar = !s.tabbar),
            // ---- tools
            Cmd::HashMd5 | Cmd::HashSha256 => {
                let d = self.doc();
                let t = if d.has_selection() { d.selected_text() } else { d.text_string() };
                let h = if c == Cmd::HashMd5 { crate::hash::md5_hex(t.as_bytes()) } else { crate::hash::sha256_hex(t.as_bytes()) };
                set_clipboard(hwnd, &h);
                self.set_status_text(&format!("{} copied to clipboard: {h}", if c == Cmd::HashMd5 { "MD5" } else { "SHA-256" }));
            }
            // ---- macro
            Cmd::MacroRecord => {
                self.recording = true;
                self.recording_buf.clear();
                self.update_status();
            }
            Cmd::MacroStop => {
                if self.recording {
                    self.recording = false;
                    // drop the trailing "stop" command itself
                    self.macro_steps = std::mem::take(&mut self.recording_buf);
                }
                self.update_status();
            }
            Cmd::MacroPlay => self.macro_play(Some(1)),
            Cmd::MacroRunMulti => {
                if let Some(t) = dialogs::macro_multi(hwnd) {
                    self.macro_play(t);
                }
            }
            Cmd::TrimSave => {
                self.exec(Cmd::TrimTrailing);
                self.exec(Cmd::Save);
            }
            // ---- run
            Cmd::Run => self.run_command(),
            Cmd::LaunchBrowser => {
                if let Some(p) = &self.docs[i].path {
                    shell_open(&p.to_string_lossy());
                }
            }
            Cmd::Wikipedia => {
                let q = self.selection_or_word();
                if !q.is_empty() {
                    shell_open(&format!("https://en.wikipedia.org/wiki/Special:Search?search={}", url_encode(&q)));
                }
            }
            // ---- plugins
            Cmd::Base64Enc => self.transform_selection(|t| Some(crate::hash::base64_encode(t.as_bytes()))),
            Cmd::Base64Dec => self.transform_selection(|t| crate::hash::base64_decode(t.trim()).and_then(|b| String::from_utf8(b).ok())),
            Cmd::UrlEnc => self.transform_selection(|t| Some(url_encode(t))),
            Cmd::UrlDec => self.transform_selection(|t| Some(url_decode(t))),
            Cmd::AsciiToHex => self.transform_selection(|t| Some(t.bytes().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(""))),
            Cmd::HexToAscii => self.transform_selection(|t| {
                let h: String = t.chars().filter(|c| c.is_ascii_hexdigit()).collect();
                if h.len() % 2 != 0 {
                    return None;
                }
                let bytes: Option<Vec<u8>> = (0..h.len()).step_by(2).map(|k| u8::from_str_radix(&h[k..k + 2], 16).ok()).collect();
                bytes.and_then(|b| String::from_utf8(b).ok())
            }),
            // ---- window
            Cmd::WindowsList => {
                let items: Vec<String> = self.docs.iter().map(|d| format!("{}{}\t{}", if d.is_modified() { "*" } else { "" }, d.title(), d.path.as_ref().and_then(|p| p.parent()).map(|p| p.display().to_string()).unwrap_or_default())).collect();
                if let Some((btn, sel)) = dialogs::list_dialog(hwnd, "Windows", items, true, &[(1, "Activate"), (10, "Save"), (11, "Close window(s)"), (12, "Sort tabs")]) {
                    let ids: Vec<u64> = sel.iter().filter_map(|&k| self.docs.get(k).map(|d| d.id)).collect();
                    match btn {
                        1 => {
                            if let Some(&k) = sel.first() {
                                self.activate(k);
                            }
                        }
                        10 => {
                            for id in ids {
                                if let Some(k) = self.docs.iter().position(|d| d.id == id) {
                                    self.save_doc(k);
                                }
                            }
                        }
                        11 => self.close_where(|_, d| ids.contains(&d.id)),
                        12 => self.exec(Cmd::SortTabsName),
                        _ => {}
                    }
                }
            }
            Cmd::SortTabsName | Cmd::SortTabsPath => {
                let id = self.docs[i].id;
                if c == Cmd::SortTabsName {
                    self.docs.sort_by_key(|d| d.title().to_lowercase());
                } else {
                    self.docs.sort_by_key(|d| d.full_title().to_lowercase());
                }
                let k = self.docs.iter().position(|d| d.id == id).unwrap_or(0);
                self.activate(k);
            }
            // ---- help
            Cmd::Homepage => shell_open("https://notepad-plus-plus.org/"),
            Cmd::UserManual => shell_open("https://npp-user-manual.org/"),
            Cmd::DebugInfo => {
                let info = format!(
                    "Notepad++ RS v{}   (64-bit)\nBuild with: rustc / windows-sys 0.59\nPath: {}\nConfig: {}\nDocuments open: {}\nTheme: {}",
                    env!("CARGO_PKG_VERSION"),
                    std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default(),
                    config::dir().display(),
                    self.docs.len(),
                    crate::theme::theme(self.settings.theme).name
                );
                set_clipboard(hwnd, &info);
                msgbox(hwnd, &format!("{info}\n\n(copied to clipboard)"), "Debug Info", MB_OK | MB_ICONINFORMATION);
            }
            Cmd::About => dialogs::about(hwnd),
            Cmd::CmdLineHelp => {
                msgbox(hwnd, "Usage:\n\nnotepad-rs [-n<line>] [-c<column>] [-l<Language>] [-ro] [-nosession] [-multiInst] [filePath ...]\n\n-n : Scroll to indicated line on filePath\n-c : Scroll to indicated column on filePath\n-l : Set the language (e.g. -lPython)\n-ro : Make the filePath read only\n-nosession : Launch without previous session\n-multiInst : Launch another instance", "Command Line Arguments", MB_OK | MB_ICONINFORMATION);
            }
            Cmd::Escape | Cmd::Last => {}
        }
        notify_main();
    }

    fn selection_or_word_path(&self) -> String {
        let d = self.doc();
        let s = d.main_sel();
        if !s.is_empty() {
            return d.slice(s.start(), s.end());
        }
        let line = d.line_text(d.line_of(s.caret));
        let col = s.caret - d.line_start(d.line_of(s.caret));
        let chars: Vec<char> = line.chars().collect();
        let ok = |c: char| !c.is_whitespace() && !"\"'<>|*?()[]{},;".contains(c);
        let mut a = col.min(chars.len());
        let mut b = a;
        while a > 0 && ok(chars[a - 1]) {
            a -= 1;
        }
        while b < chars.len() && ok(chars[b]) {
            b += 1;
        }
        chars[a..b].iter().collect()
    }
}

fn is_recordable(c: Cmd) -> bool {
    !matches!(
        c,
        Cmd::MacroRecord
            | Cmd::MacroStop
            | Cmd::MacroPlay
            | Cmd::MacroRunMulti
            | Cmd::Open
            | Cmd::Find
            | Cmd::Replace
            | Cmd::FindInFiles
            | Cmd::Mark
            | Cmd::Preferences
            | Cmd::StyleConfigurator
            | Cmd::ShortcutMapper
            | Cmd::About
            | Cmd::Exit
            | Cmd::Print
            | Cmd::Run
            | Cmd::WindowsList
            | Cmd::Goto
            | Cmd::ColumnEditor
    )
}

fn remove_backup(d: &Document) {
    if let Some(b) = &d.backup_file {
        let _ = std::fs::remove_file(b);
    }
}

fn sanitize(s: &str) -> String {
    s.chars().map(|c| if "\\/:*?\"<>|".contains(c) { '_' } else { c }).collect()
}

fn strip_unc(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        if !rest.starts_with("UNC") {
            return PathBuf::from(rest);
        }
    }
    p.to_path_buf()
}

pub fn same_path(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
}

pub fn fmt_num(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn fmt_time(t: std::time::SystemTime) -> String {
    use windows_sys::Win32::Foundation::{FILETIME, SYSTEMTIME};
    use windows_sys::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime};
    let d = t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let ft100 = d.as_nanos() / 100 + 116_444_736_000_000_000;
    let ft = FILETIME { dwLowDateTime: ft100 as u32, dwHighDateTime: (ft100 >> 32) as u32 };
    unsafe {
        let mut st: SYSTEMTIME = std::mem::zeroed();
        let mut lt: SYSTEMTIME = std::mem::zeroed();
        FileTimeToSystemTime(&ft, &mut st);
        SystemTimeToTzSpecificLocalTime(std::ptr::null(), &st, &mut lt);
        format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", lt.wYear, lt.wMonth, lt.wDay, lt.wHour, lt.wMinute, lt.wSecond)
    }
}

fn local_time_string(long: bool) -> String {
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;
    unsafe {
        let mut st = std::mem::zeroed();
        GetLocalTime(&mut st);
        let st: windows_sys::Win32::Foundation::SYSTEMTIME = st;
        const DAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
        const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
        if long {
            format!("{:02}:{:02} {}, {} {}, {}", st.wHour, st.wMinute, DAYS[st.wDayOfWeek as usize % 7], MONTHS[(st.wMonth as usize + 11) % 12], st.wDay, st.wYear)
        } else {
            format!("{:02}:{:02} {}/{}/{}", st.wHour, st.wMinute, st.wMonth, st.wDay, st.wYear)
        }
    }
}

fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn url_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() + 0 && i + 2 <= b.len() - 1 {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(if b[i] == b'+' { b' ' } else { b[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn split_cmdline(s: &str) -> (String, String) {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix('"') {
        if let Some(e) = rest.find('"') {
            return (rest[..e].to_string(), rest[e + 1..].trim().to_string());
        }
    }
    match s.find(' ') {
        Some(i) => (s[..i].to_string(), s[i + 1..].to_string()),
        None => (s.to_string(), String::new()),
    }
}

fn recycle(p: &Path) -> bool {
    let from: Vec<u16> = p.to_string_lossy().encode_utf16().chain([0, 0]).collect();
    unsafe {
        let mut op: SHFILEOPSTRUCTW = std::mem::zeroed();
        op.wFunc = FO_DELETE;
        op.pFrom = from.as_ptr();
        op.fFlags = (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_SILENT) as u16;
        SHFileOperationW(&mut op) == 0
    }
}

// ---------------------------------------------------------------------- window creation

pub struct Launch {
    pub files: Vec<PathBuf>,
    pub line: Option<usize>,
    pub col: Option<usize>,
    pub lang: Option<String>,
    pub read_only: bool,
    pub no_session: bool,
    /// Automated test mode: off-screen, never activated, persists nothing.
    pub test: bool,
}

pub fn create_app(launch: Launch) -> HWND {
    let settings = config::load_settings();
    let app_box = Box::new(App {
        hwnd: std::ptr::null_mut(),
        settings,
        docs: Vec::new(),
        active: 0,
        view: EditorView::new(false),
        rview: EditorView::new(true),
        rdoc: Document::new("Search results".into()),
        results_locs: Vec::new(),
        results_panel: std::ptr::null_mut(),
        show_results: false,
        toolbar: std::ptr::null_mut(),
        status: std::ptr::null_mut(),
        tabbar: TabBar::new(),
        accel: menu::accelerators(),
        menus: menu::build(),
        recent_count: 0,
        window_count: 0,
        find: FindDlg::new(),
        closed: Vec::new(),
        fullscreen: None,
        postit: None,
        recording: false,
        macro_steps: Vec::new(),
        recording_buf: Vec::new(),
        playing: false,
        status_pending: false,
        status_msg: None,
        monitoring: HashSet::new(),
        ac: AutoComplete::default(),
        checking_files: false,
        splitter_drag: None,
        res_split: RECT { left: 0, top: 0, right: 0, bottom: 0 },
        func_split: RECT { left: 0, top: 0, right: 0, bottom: 0 },
        func_panel: std::ptr::null_mut(),
        func_list: std::ptr::null_mut(),
        show_funcs: false,
        func_lines: Vec::new(),
        func_key: (0, 0),
        panel_font: std::ptr::null_mut(),
        dpi: 96,
        tb_lists: (0, 0),
        saved_style: None,
        exiting: false,
        no_session: launch.no_session,
        test: launch.test,
        rect_clip: None,
    });
    unsafe {
        APP = Box::into_raw(app_box);
    }
    let a = app();
    a.rdoc.lang = Lang::SearchResult;
    a.rdoc.read_only = true;
    if a.settings.remember_session && !launch.no_session {
        a.restore_session();
    }
    if a.docs.is_empty() {
        let d = a.new_document();
        a.docs.push(d);
    }
    for d in &mut a.docs {
        d.tab_width = a.settings.tab_width;
    }

    let cls = w(MAIN_CLASS);
    let title = w("Notepad++ RS");
    let [x, y, wd, ht] = if launch.test { [-4000, 0, 1100, 750] } else { a.settings.win };
    let (x, y) = if x == -1 { (CW_USEDEFAULT, CW_USEDEFAULT) } else { (x, y) };
    unsafe {
        let hwnd = CreateWindowExW(
            WS_EX_ACCEPTFILES,
            cls.as_ptr(),
            title.as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
            x,
            y,
            wd.max(400),
            ht.max(300),
            std::ptr::null_mut(),
            a.menus.bar,
            hinstance(),
            std::ptr::null(),
        );
        a.hwnd = hwnd;
        create_children(hwnd);
        if !launch.files.is_empty() {
            a.open_paths(&launch.files);
            let d = a.doc_mut();
            if launch.read_only {
                d.read_only = true;
            }
            if let Some(l) = &launch.lang {
                if let Some(sp) = SPECS.iter().find(|s| s.name.eq_ignore_ascii_case(l)) {
                    d.lang = sp.lang;
                    d.hl.reset(d.line_count());
                }
            }
            if let Some(line) = launch.line {
                let l = line.clamp(1, d.line_count()) - 1;
                let p = d.pos_at_col(l, launch.col.unwrap_or(1).saturating_sub(1));
                d.set_caret(p);
                a.reveal_selection(l);
            }
        }
        if a.settings.always_on_top {
            SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
        }
        ShowWindow(hwnd, if launch.test { SW_SHOWNOACTIVATE } else if a.settings.maximized { SW_SHOWMAXIMIZED } else { SW_SHOWNORMAL });
        a.layout();
        let i = a.active;
        a.activate(i);
        SetTimer(hwnd, TIMER_BACKUP, 7000, None);
        SetTimer(hwnd, TIMER_TAIL, 1000, None);
        hwnd
    }
}

unsafe fn create_children(hwnd: HWND) {
    let a = app();
    let dpi = dpi_of(hwnd);
    a.dpi = dpi;
    a.panel_font = ui_font(9, dpi, false);
    // toolbar
    let tbc = w("ToolbarWindow32");
    a.toolbar = CreateWindowExW(
        0,
        tbc.as_ptr(),
        std::ptr::null(),
        WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS | TBSTYLE_FLAT | TBSTYLE_TOOLTIPS | CCS_NODIVIDER as u32 | CCS_NOPARENTALIGN as u32 | CCS_NORESIZE as u32,
        0,
        0,
        100,
        30,
        hwnd,
        ID_TOOLBAR as usize as HMENU,
        hinstance(),
        std::ptr::null(),
    );
    setup_toolbar(dpi);
    // tab bar
    a.tabbar.create(hwnd);
    // editor
    a.view.create(hwnd);
    // results panel
    let pc = w(PANEL_CLASS);
    a.results_panel = CreateWindowExW(0, pc.as_ptr(), std::ptr::null(), WS_CHILD | WS_CLIPCHILDREN, 0, 0, 100, 100, hwnd, std::ptr::null_mut(), hinstance(), std::ptr::null());
    a.rview.create(a.results_panel);
    let fc = w(FUNC_CLASS);
    a.func_panel = CreateWindowExW(0, fc.as_ptr(), std::ptr::null(), WS_CHILD | WS_CLIPCHILDREN, 0, 0, 100, 100, hwnd, std::ptr::null_mut(), hinstance(), std::ptr::null());
    let lbc = w("LISTBOX");
    a.func_list = CreateWindowExW(0, lbc.as_ptr(), std::ptr::null(), WS_CHILD | WS_VISIBLE | WS_VSCROLL | WS_HSCROLL | LBS_NOTIFY as u32 | LBS_NOINTEGRALHEIGHT as u32, 0, 0, 100, 100, a.func_panel, std::ptr::null_mut(), hinstance(), std::ptr::null());
    SendMessageW(a.func_list, WM_SETFONT, a.panel_font as usize, 0);
    // status bar
    let sbc = w("msctls_statusbar32");
    a.status = CreateWindowExW(0, sbc.as_ptr(), std::ptr::null(), WS_CHILD | WS_VISIBLE | SBARS_SIZEGRIP, 0, 0, 0, 0, hwnd, ID_STATUSBAR as usize as HMENU, hinstance(), std::ptr::null());
    let icon_big = icons::app_icon(scale(32, dpi) as usize);
    let icon_small = icons::app_icon(scale(16, dpi) as usize);
    SendMessageW(hwnd, WM_SETICON, ICON_BIG as usize, icon_big as isize);
    SendMessageW(hwnd, WM_SETICON, ICON_SMALL as usize, icon_small as isize);
    DragAcceptFiles(hwnd, 1);
}

unsafe fn setup_toolbar(dpi: u32) {
    let a = app();
    let tb = a.toolbar;
    SendMessageW(tb, TB_BUTTONSTRUCTSIZE, std::mem::size_of::<TBBUTTON>(), 0);
    // remove existing buttons (DPI change)
    while SendMessageW(tb, TB_BUTTONCOUNT, 0, 0) > 0 {
        SendMessageW(tb, TB_DELETEBUTTON, 0, 0);
    }
    let px = scale(16, dpi) as usize;
    let il = icons::toolbar_imagelist(px);
    let ild = icons::toolbar_imagelist_disabled(px);
    let old = SendMessageW(tb, TB_SETIMAGELIST, 0, il as isize);
    let oldd = SendMessageW(tb, TB_SETDISABLEDIMAGELIST, 0, ild as isize);
    if old != 0 {
        ImageList_Destroy(old as HIMAGELIST);
    }
    if oldd != 0 {
        ImageList_Destroy(oldd as HIMAGELIST);
    }
    a.tb_lists = (il, ild);
    let mut buttons: Vec<TBBUTTON> = Vec::new();
    for &(cmd, icon) in TOOLBAR {
        let mut b: TBBUTTON = std::mem::zeroed();
        match cmd {
            Some(c) => {
                b.iBitmap = icon;
                b.idCommand = c as i32;
                b.fsState = TBSTATE_ENABLED as u8;
                b.fsStyle = (BTNS_BUTTON | if matches!(c, Cmd::WordWrap | Cmd::ShowAll | Cmd::ShowIndentGuide | Cmd::Monitoring | Cmd::FuncList) { BTNS_CHECK } else { 0 }) as u8;
            }
            None => {
                b.fsStyle = BTNS_SEP as u8;
                b.iBitmap = scale(6, dpi);
            }
        }
        buttons.push(b);
    }
    SendMessageW(tb, TB_ADDBUTTONSW, buttons.len(), buttons.as_ptr() as isize);
    let bs = px as i32 + scale(7, dpi);
    SendMessageW(tb, TB_SETBUTTONSIZE, 0, ((bs << 16) | bs) as isize);
    SendMessageW(tb, TB_AUTOSIZE, 0, 0);
}

pub fn register() {
    unsafe {
        let cls = w(MAIN_CLASS);
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: 0,
            lpfnWndProc: Some(main_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance(),
            hIcon: std::ptr::null_mut(),
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
            hbrBackground: (COLOR_BTNFACE + 1) as usize as HBRUSH,
            lpszMenuName: std::ptr::null(),
            lpszClassName: cls.as_ptr(),
            hIconSm: std::ptr::null_mut(),
        };
        RegisterClassExW(&wc);
        let pc = w(PANEL_CLASS);
        let wc2 = WNDCLASSEXW { lpfnWndProc: Some(panel_proc), lpszClassName: pc.as_ptr(), style: 0, ..wc };
        RegisterClassExW(&wc2);
        let fc = w(FUNC_CLASS);
        let wc4 = WNDCLASSEXW { lpfnWndProc: Some(func_proc), lpszClassName: fc.as_ptr(), style: 0, ..wc };
        RegisterClassExW(&wc4);
        let ac = w(AC_CLASS);
        let wc3 = WNDCLASSEXW { lpfnWndProc: Some(ac_proc), lpszClassName: ac.as_ptr(), style: CS_DROPSHADOW, hbrBackground: (COLOR_WINDOW + 1) as usize as HBRUSH, ..wc };
        RegisterClassExW(&wc3);
    }
}

unsafe extern "system" fn ac_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => MA_NOACTIVATE as isize,
        WM_COMMAND => {
            if hiword(wp) as u32 == LBN_DBLCLK {
                app().ac_accept();
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe extern "system" fn func_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let Some(a) = try_app() else { return DefWindowProcW(hwnd, msg, wp, lp) };
    let d = dpi_of(hwnd);
    let cap = scale(22, d);
    match msg {
        WM_SIZE => {
            let rc = client_rect(hwnd);
            MoveWindow(a.func_list, 0, cap, rc.right, (rc.bottom - cap).max(0), 1);
            0
        }
        WM_PAINT => {
            let bp = BufferedPaint::begin(hwnd);
            let hdc = bp.hdc;
            fill(hdc, &rect(0, 0, bp.w, cap), 0xF0F0F0);
            fill(hdc, &rect(0, 0, bp.w, scale(2, d)), 0xFAAA3C);
            hline(hdc, 0, bp.w, cap - 1, 0xC8C8C8);
            SelectObject(hdc, a.panel_font as _);
            SetBkMode(hdc, TRANSPARENT as i32);
            SetTextColor(hdc, rgb(0x202020));
            text_out(hdc, scale(8, d), scale(4, d), "Function List");
            let x0 = bp.w - scale(20, d);
            let p = scale(6, d);
            for k in 0..(cap - 2 * p) {
                fill(hdc, &rect(x0 + p + k, p + k, x0 + p + k + 2, p + k + 1), 0x606060);
                fill(hdc, &rect(x0 + cap - 2 - p - k - 2, p + k, x0 + cap - p - k - 2, p + k + 1), 0x606060);
            }
            drop(bp);
            0
        }
        WM_ERASEBKGND => 1,
        WM_LBUTTONDOWN => {
            let rc = client_rect(hwnd);
            if y_lparam(lp) < cap && x_lparam(lp) > rc.right - scale(22, d) {
                a.show_funcs = false;
                a.layout();
                a.update_status();
            }
            0
        }
        WM_COMMAND => {
            let code = hiword(wp) as u32;
            if lp as HWND == a.func_list && (code == LBN_DBLCLK || code == LBN_SELCHANGE) {
                a.func_jump();
                if code == LBN_DBLCLK {
                    SetFocus(a.view.hwnd);
                }
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe extern "system" fn panel_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let Some(a) = try_app() else { return DefWindowProcW(hwnd, msg, wp, lp) };
    let d = dpi_of(hwnd);
    let cap = scale(22, d);
    match msg {
        WM_SIZE => {
            if !a.rview.hwnd.is_null() {
                a.panel_layout();
            }
            0
        }
        WM_PAINT => {
            let bp = BufferedPaint::begin(hwnd);
            let hdc = bp.hdc;
            fill(hdc, &rect(0, 0, bp.w, cap), 0xF0F0F0);
            fill(hdc, &rect(0, 0, bp.w, scale(2, d)), 0xFAAA3C);
            hline(hdc, 0, bp.w, cap - 1, 0xC8C8C8);
            SelectObject(hdc, a.panel_font as _);
            SetBkMode(hdc, TRANSPARENT as i32);
            SetTextColor(hdc, rgb(0x202020));
            let hits: usize = a.results_locs.iter().filter(|l| l.is_some()).count();
            text_out(hdc, scale(8, d), scale(4, d), &format!("Search results - ({} lines)", fmt_num(hits)));
            let x0 = bp.w - scale(20, d);
            let p = scale(6, d);
            for k in 0..(cap - 2 * p) {
                fill(hdc, &rect(x0 + p + k, p + k, x0 + p + k + 2, p + k + 1), 0x606060);
                fill(hdc, &rect(x0 + cap - 2 - p - k - 2, p + k, x0 + cap - p - k - 2, p + k + 1), 0x606060);
            }
            drop(bp);
            0
        }
        WM_ERASEBKGND => 1,
        WM_LBUTTONDOWN => {
            let x = x_lparam(lp);
            let y = y_lparam(lp);
            let rc = client_rect(hwnd);
            if y < cap && x > rc.right - scale(22, d) {
                a.show_results = false;
                a.layout();
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe extern "system" fn main_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let Some(a) = try_app() else { return DefWindowProcW(hwnd, msg, wp, lp) };
    if a.hwnd.is_null() {
        return DefWindowProcW(hwnd, msg, wp, lp);
    }
    match msg {
        WM_SIZE => {
            a.layout();
            0
        }
        WM_SETFOCUS => {
            SetFocus(a.view.hwnd);
            0
        }
        WM_ACTIVATEAPP => {
            if wp != 0 && !a.exiting {
                PostMessageW(hwnd, WM_APP + 3, 0, 0);
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        x if x == WM_APP + 3 => {
            a.check_files();
            0
        }
        WM_APP_STATUS => {
            a.update_status();
            0
        }
        WM_APP_RESULT => {
            a.results_jump(wp);
            0
        }
        WM_COMMAND => {
            let id = loword(wp);
            if let Some(c) = Cmd::from_id(id) {
                a.exec(c);
            } else if (menu::ID_RECENT..menu::ID_RECENT + 100).contains(&id) {
                if let Some(p) = a.settings.recent.get((id - menu::ID_RECENT) as usize).cloned() {
                    a.open_paths(&[PathBuf::from(p)]);
                }
            } else if (menu::ID_LANG..menu::ID_LANG + 200).contains(&id) {
                if let Some(sp) = SPECS.get((id - menu::ID_LANG) as usize) {
                    a.set_lang(sp.lang);
                }
            } else if (menu::ID_WINDOW..menu::ID_WINDOW + 1000).contains(&id) {
                a.activate((id - menu::ID_WINDOW) as usize);
            } else if (ID_RES_CLEAR..=ID_RES_UNFOLDALL).contains(&id) {
                a.results_command(id);
            }
            0
        }
        WM_INITMENUPOPUP => {
            a.refresh_dynamic_menus();
            a.update_menu_states(wp as HMENU);
            0
        }
        WM_NOTIFY => {
            let nm = &*(lp as *const NMHDR);
            if nm.code == TTN_GETDISPINFOW {
                let di = &mut *(lp as *mut NMTTDISPINFOW);
                if let Some(c) = Cmd::from_id(nm.idFrom as u16) {
                    let tip = tooltip_for(c);
                    thread_local! { static TIP: std::cell::RefCell<Vec<u16>> = const { std::cell::RefCell::new(Vec::new()) }; }
                    TIP.with(|t| {
                        let mut t = t.borrow_mut();
                        *t = w(tip);
                        di.lpszText = t.as_mut_ptr();
                    });
                }
            }
            0
        }
        WM_DROPFILES => {
            let hdrop = wp as HDROP;
            let n = DragQueryFileW(hdrop, 0xFFFF_FFFF, std::ptr::null_mut(), 0);
            let mut files = Vec::new();
            for k in 0..n {
                let mut buf = vec![0u16; 1024];
                DragQueryFileW(hdrop, k, buf.as_mut_ptr(), 1024);
                files.push(PathBuf::from(from_wide(&buf)));
            }
            DragFinish(hdrop);
            a.open_paths(&files);
            0
        }
        WM_COPYDATA => {
            let cds = &*(lp as *const COPYDATASTRUCT);
            if cds.dwData == 0x4E50 && !cds.lpData.is_null() {
                let units = std::slice::from_raw_parts(cds.lpData as *const u16, cds.cbData as usize / 2);
                let s = String::from_utf16_lossy(units);
                let files: Vec<PathBuf> = s.split('\0').filter(|x| !x.is_empty()).map(PathBuf::from).collect();
                a.open_paths(&files);
                if IsIconic(hwnd) != 0 {
                    ShowWindow(hwnd, SW_RESTORE);
                }
                SetForegroundWindow(hwnd);
            }
            1
        }
        WM_TIMER => {
            match wp {
                TIMER_BACKUP => a.backup_tick(),
                TIMER_TAIL => {
                    if !a.monitoring.is_empty() {
                        a.tail_tick();
                    }
                }
                TIMER_FUNCS => {
                    KillTimer(hwnd, TIMER_FUNCS);
                    a.refresh_funcs(false);
                }
                TIMER_STATUSMSG => {
                    KillTimer(hwnd, TIMER_STATUSMSG);
                    a.status_msg = None;
                    a.update_status();
                }
                _ => {}
            }
            0
        }
        WM_LBUTTONDOWN => {
            let pt = POINT { x: x_lparam(lp), y: y_lparam(lp) };
            if PtInRect(&a.res_split, pt) != 0 {
                a.splitter_drag = Some(1);
                SetCapture(hwnd);
            } else if PtInRect(&a.func_split, pt) != 0 {
                a.splitter_drag = Some(2);
                SetCapture(hwnd);
            }
            0
        }
        WM_MOUSEMOVE => {
            if a.splitter_drag == Some(2) {
                let rc = client_rect(hwnd);
                let d = dpi_of(hwnd);
                a.settings.panel_width = ((rc.right - x_lparam(lp)) * 96 / d as i32).clamp(120, 1200);
                a.layout();
            } else if a.splitter_drag == Some(1) {
                let rc = client_rect(hwnd);
                let mut sb = 0;
                if a.settings.statusbar {
                    let mut sr = RECT { left: 0, top: 0, right: 0, bottom: 0 };
                    GetWindowRect(a.status, &mut sr);
                    sb = sr.bottom - sr.top;
                }
                let h = rc.bottom - sb - y_lparam(lp);
                let d = dpi_of(hwnd);
                a.settings.results_height = (h * 96 / d as i32).clamp(60, 2000);
                a.layout();
            }
            0
        }
        WM_LBUTTONUP => {
            if a.splitter_drag.take().is_some() {
                ReleaseCapture();
            }
            0
        }
        WM_CAPTURECHANGED => {
            a.splitter_drag = None;
            0
        }
        WM_SETCURSOR => {
            if wp as HWND == hwnd && loword(lp as usize) as u32 == HTCLIENT {
                let mut pt = POINT { x: 0, y: 0 };
                GetCursorPos(&mut pt);
                ScreenToClient(hwnd, &mut pt);
                let cur = if PtInRect(&a.func_split, pt) != 0 { IDC_SIZEWE } else { IDC_SIZENS };
                SetCursor(LoadCursorW(std::ptr::null_mut(), cur));
                return 1;
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_DPICHANGED => {
            let r = &*(lp as *const RECT);
            let dpi = hiword(wp) as u32;
            a.dpi = dpi;
            SetWindowPos(hwnd, std::ptr::null_mut(), r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_NOZORDER | SWP_NOACTIVATE);
            setup_toolbar(dpi);
            a.tabbar.update_dpi();
            a.panel_font = ui_font(9, dpi, false);
            a.layout();
            0
        }
        WM_CLOSE => {
            if a.on_exit() {
                DestroyWindow(hwnd);
            }
            0
        }
        WM_QUERYENDSESSION => {
            if a.on_exit() {
                1
            } else {
                0
            }
        }
        WM_ENDSESSION => {
            if wp == 0 {
                // shutdown was cancelled by another application
                a.exiting = false;
            }
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
