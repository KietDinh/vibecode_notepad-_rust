#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! Notepad++ RS — a native Windows Notepad++-style editor written in Rust.

mod app;
mod config;
mod dialogs;
mod doc;
mod editor;
mod fileio;
mod find;
mod funclist;
mod hash;
mod highlight;
mod icons;
mod lang;
mod menu;
mod ops;
mod tabbar;
mod theme;
mod win;

use std::path::PathBuf;
use windows_sys::Win32::UI::Controls::*;
use windows_sys::Win32::UI::HiDpi::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

fn parse_args() -> (app::Launch, bool) {
    let mut l = app::Launch { files: Vec::new(), line: None, col: None, lang: None, read_only: false, no_session: false, test: false };
    let mut multi = false;
    for a in std::env::args().skip(1) {
        let lower = a.to_lowercase();
        if let Some(n) = a.strip_prefix("-n").filter(|s| s.chars().all(|c| c.is_ascii_digit()) && !s.is_empty()) {
            l.line = n.parse().ok();
        } else if let Some(n) = a.strip_prefix("-c").filter(|s| s.chars().all(|c| c.is_ascii_digit()) && !s.is_empty()) {
            l.col = n.parse().ok();
        } else if lower == "-ro" {
            l.read_only = true;
        } else if lower == "-nosession" {
            l.no_session = true;
        } else if lower == "-test" {
            l.test = true;
            l.no_session = true;
            multi = true;
        } else if lower == "-multiinst" {
            multi = true;
        } else if let Some(lang) = a.strip_prefix("-l") {
            l.lang = Some(lang.to_string());
        } else if !a.starts_with('-') {
            let p = PathBuf::from(&a);
            let p = if p.is_relative() { std::env::current_dir().map(|d| d.join(&p)).unwrap_or(p) } else { p };
            l.files.push(p);
        }
    }
    (l, multi)
}

/// If another instance is running, hand our files to it and return true.
fn forward_to_existing(files: &[PathBuf]) -> bool {
    let cls = win::w(app::MAIN_CLASS);
    unsafe {
        let h = FindWindowW(cls.as_ptr(), std::ptr::null());
        if h.is_null() {
            return false;
        }
        let joined: Vec<u16> = files.iter().map(|p| p.to_string_lossy().into_owned()).collect::<Vec<_>>().join("\0").encode_utf16().chain([0]).collect();
        let cds = windows_sys::Win32::System::DataExchange::COPYDATASTRUCT { dwData: 0x4E50, cbData: (joined.len() * 2) as u32, lpData: joined.as_ptr() as _ };
        SendMessageW(h, WM_COPYDATA, 0, &cds as *const _ as isize);
        if IsIconic(h) != 0 {
            ShowWindow(h, SW_RESTORE);
        }
        SetForegroundWindow(h);
        true
    }
}

fn main() {
    let (mut launch, multi) = parse_args();
    if multi {
        launch.no_session = true;
    }
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    if !multi && forward_to_existing(&launch.files) {
        return;
    }
    unsafe {
        let icc = INITCOMMONCONTROLSEX { dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32, dwICC: ICC_BAR_CLASSES | ICC_TAB_CLASSES | ICC_STANDARD_CLASSES | ICC_WIN95_CLASSES };
        InitCommonControlsEx(&icc);
    }
    app::register();
    editor::register();
    tabbar::register();
    let hwnd = app::create_app(launch);
    unsafe {
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            let a = app::app();
            if !a.find.hwnd.is_null() && IsDialogMessageW(a.find.hwnd, &msg) != 0 {
                continue;
            }
            let target_main = msg.hwnd == hwnd || IsChild(hwnd, msg.hwnd) != 0;
            if target_main && TranslateAcceleratorW(hwnd, a.accel, &msg) != 0 {
                continue;
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
