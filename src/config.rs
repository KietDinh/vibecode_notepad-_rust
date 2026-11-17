//! Persistent settings and session snapshot (JSON in %APPDATA%\notepad-rs).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Settings {
    pub font: String,
    pub font_size: i32,
    pub zoom: i32,
    pub tab_width: usize,
    pub use_spaces: bool,
    pub auto_indent: bool,
    pub word_wrap: bool,
    pub show_ws: bool,
    pub show_eol: bool,
    pub indent_guides: bool,
    pub line_numbers: bool,
    pub bookmark_margin: bool,
    pub fold_margin: bool,
    pub change_history: bool,
    pub smart_highlight: bool,
    pub smart_match_case: bool,
    pub cur_line: bool,
    pub brace_match: bool,
    pub auto_close: bool,
    pub auto_complete: bool,
    pub theme: usize,
    pub toolbar: bool,
    pub statusbar: bool,
    pub tabbar: bool,
    pub always_on_top: bool,
    pub tab_close_button: bool,
    pub dbl_click_close_tab: bool,
    pub remember_session: bool,
    pub snapshot: bool,
    pub caret_width: i32,
    pub edge_column: usize,
    pub recent: Vec<String>,
    pub max_recent: usize,
    pub find_history: Vec<String>,
    pub replace_history: Vec<String>,
    pub match_case: bool,
    pub whole_word: bool,
    pub wrap_around: bool,
    pub search_mode: u8,
    pub dot_nl: bool,
    pub backward: bool,
    pub fif_dir: String,
    pub fif_filter: String,
    pub fif_subfolders: bool,
    pub fif_hidden: bool,
    pub run_history: Vec<String>,
    pub default_eol: u8,
    pub default_encoding: u8,
    pub win: [i32; 4],
    pub maximized: bool,
    pub results_height: i32,
    pub panel_width: i32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            font: "Courier New".into(),
            font_size: 10,
            zoom: 0,
            tab_width: 4,
            use_spaces: false,
            auto_indent: true,
            word_wrap: false,
            show_ws: false,
            show_eol: false,
            indent_guides: true,
            line_numbers: true,
            bookmark_margin: true,
            fold_margin: true,
            change_history: true,
            smart_highlight: true,
            smart_match_case: false,
            cur_line: true,
            brace_match: true,
            auto_close: false,
            auto_complete: true,
            theme: 0,
            toolbar: true,
            statusbar: true,
            tabbar: true,
            always_on_top: false,
            tab_close_button: true,
            dbl_click_close_tab: false,
            remember_session: true,
            snapshot: true,
            caret_width: 1,
            edge_column: 0,
            recent: Vec::new(),
            max_recent: 10,
            find_history: Vec::new(),
            replace_history: Vec::new(),
            match_case: false,
            whole_word: false,
            wrap_around: true,
            search_mode: 0,
            dot_nl: false,
            backward: false,
            fif_dir: String::new(),
            fif_filter: "*.*".into(),
            fif_subfolders: true,
            fif_hidden: false,
            run_history: Vec::new(),
            default_eol: 0,
            default_encoding: 1,
            win: [-1, -1, 1100, 750],
            maximized: false,
            results_height: 180,
            panel_width: 260,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct SessionFile {
    pub path: Option<String>,
    pub untitled: Option<String>,
    pub backup: Option<String>,
    pub lang: String,
    pub encoding: u8,
    pub eol: u8,
    pub anchor: usize,
    pub caret: usize,
    pub scroll_row: usize,
    pub bookmarks: Vec<usize>,
    pub read_only: bool,
    pub pinned: bool,
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct Session {
    pub files: Vec<SessionFile>,
    pub active: usize,
}

pub fn dir() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let d = base.join("notepad-rs");
    let _ = std::fs::create_dir_all(&d);
    d
}

pub fn backup_dir() -> PathBuf {
    let d = dir().join("backup");
    let _ = std::fs::create_dir_all(&d);
    d
}

fn load_json<T: for<'a> Deserialize<'a> + Default>(name: &str) -> T {
    std::fs::read_to_string(dir().join(name)).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

fn save_json<T: Serialize>(name: &str, v: &T) {
    if let Ok(s) = serde_json::to_string_pretty(v) {
        let p = dir().join(name);
        let tmp = p.with_extension("tmp");
        if std::fs::write(&tmp, s).is_ok() {
            let _ = std::fs::rename(&tmp, &p);
        }
    }
}

pub fn load_settings() -> Settings {
    let mut s: Settings = load_json("config.json");
    s.tab_width = s.tab_width.clamp(1, 16);
    s.font_size = s.font_size.clamp(6, 48);
    s.zoom = s.zoom.clamp(-9, 30);
    s.caret_width = s.caret_width.clamp(1, 4);
    s.theme = s.theme.min(crate::theme::THEMES.len() - 1);
    s.max_recent = s.max_recent.clamp(1, 30);
    if s.font.trim().is_empty() {
        s.font = "Courier New".into();
    }
    s
}
pub fn save_settings(s: &Settings) {
    save_json("config.json", s)
}
pub fn load_session() -> Session {
    load_json("session.json")
}
pub fn save_session(s: &Session) {
    save_json("session.json", s)
}

pub fn push_history(list: &mut Vec<String>, item: &str, max: usize) {
    if item.is_empty() {
        return;
    }
    list.retain(|x| x != item);
    list.insert(0, item.to_string());
    list.truncate(max);
}
