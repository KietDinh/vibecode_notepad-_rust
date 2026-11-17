//! Thin helpers over raw Win32 calls.
#![allow(dead_code)]

use std::ffi::c_void;
use std::ptr::null_mut;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::DataExchange::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Memory::*;
use windows_sys::Win32::System::Ole::CF_UNICODETEXT;
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

/// Null-terminated UTF-16 string.
pub fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

/// 0xRRGGBB -> COLORREF (0x00BBGGRR)
pub const fn rgb(hex: u32) -> COLORREF {
    ((hex & 0xFF) << 16) | (hex & 0xFF00) | ((hex >> 16) & 0xFF)
}

/// Blend two 0xRRGGBB colors; `a` in 0..=255 is the weight of `fg`.
pub fn blend(fg: u32, bg: u32, a: u32) -> u32 {
    let ch = |s: u32| {
        let f = (fg >> s) & 0xFF;
        let b = (bg >> s) & 0xFF;
        ((f * a + b * (255 - a)) / 255) << s
    };
    ch(16) | ch(8) | ch(0)
}

pub fn hinstance() -> HINSTANCE {
    unsafe { GetModuleHandleW(std::ptr::null()) }
}

pub fn loword(v: usize) -> u16 {
    (v & 0xFFFF) as u16
}
pub fn hiword(v: usize) -> u16 {
    ((v >> 16) & 0xFFFF) as u16
}
pub fn x_lparam(l: LPARAM) -> i32 {
    (l & 0xFFFF) as i16 as i32
}
pub fn y_lparam(l: LPARAM) -> i32 {
    ((l >> 16) & 0xFFFF) as i16 as i32
}

pub fn dpi_of(hwnd: HWND) -> u32 {
    let d = unsafe { GetDpiForWindow(hwnd) };
    if d == 0 {
        96
    } else {
        d
    }
}

pub fn scale(v: i32, dpi: u32) -> i32 {
    (v as i64 * dpi as i64 / 96) as i32
}

pub fn get_text(hwnd: HWND) -> String {
    unsafe {
        let len = GetWindowTextLengthW(hwnd);
        let mut buf = vec![0u16; len as usize + 1];
        GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        from_wide(&buf)
    }
}

pub fn set_text(hwnd: HWND, s: &str) {
    let ws = w(s);
    unsafe {
        SetWindowTextW(hwnd, ws.as_ptr());
    }
}

pub fn msgbox(owner: HWND, text: &str, caption: &str, flags: u32) -> i32 {
    let t = w(text);
    let c = w(caption);
    unsafe { MessageBoxW(owner, t.as_ptr(), c.as_ptr(), flags) }
}

pub fn client_rect(hwnd: HWND) -> RECT {
    let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    unsafe {
        GetClientRect(hwnd, &mut r);
    }
    r
}

pub fn rect(l: i32, t: i32, r: i32, b: i32) -> RECT {
    RECT { left: l, top: t, right: r, bottom: b }
}

pub fn fill(hdc: HDC, r: &RECT, color: u32) {
    unsafe {
        let br = CreateSolidBrush(rgb(color));
        FillRect(hdc, r, br);
        DeleteObject(br as _);
    }
}

pub fn frame(hdc: HDC, r: &RECT, color: u32) {
    unsafe {
        let br = CreateSolidBrush(rgb(color));
        FrameRect(hdc, r, br);
        DeleteObject(br as _);
    }
}

pub fn hline(hdc: HDC, x1: i32, x2: i32, y: i32, color: u32) {
    fill(hdc, &rect(x1, y, x2, y + 1), color);
}
pub fn vline(hdc: HDC, x: i32, y1: i32, y2: i32, color: u32) {
    fill(hdc, &rect(x, y1, x + 1, y2), color);
}

pub fn text_out(hdc: HDC, x: i32, y: i32, s: &str) {
    let ws: Vec<u16> = s.encode_utf16().collect();
    unsafe {
        TextOutW(hdc, x, y, ws.as_ptr(), ws.len() as i32);
    }
}

pub fn text_width(hdc: HDC, s: &str) -> i32 {
    let ws: Vec<u16> = s.encode_utf16().collect();
    let mut sz = SIZE { cx: 0, cy: 0 };
    unsafe {
        GetTextExtentPoint32W(hdc, ws.as_ptr(), ws.len() as i32, &mut sz);
    }
    sz.cx
}

/// Create a UI font (Segoe UI) of `pt` points at the given DPI.
pub fn ui_font(pt: i32, dpi: u32, bold: bool) -> HFONT {
    make_font("Segoe UI", pt * 10, dpi, bold, false)
}

/// `tenths` is the size in tenths of a point (so zoom can be fractional).
pub fn make_font(face: &str, tenths: i32, dpi: u32, bold: bool, italic: bool) -> HFONT {
    let f = w(face);
    let h = -((tenths as i64 * dpi as i64) / 720) as i32;
    unsafe {
        CreateFontW(
            h,
            0,
            0,
            0,
            if bold { FW_BOLD as i32 } else { FW_NORMAL as i32 },
            italic as u32,
            0,
            0,
            DEFAULT_CHARSET as u32,
            OUT_DEFAULT_PRECIS as u32,
            CLIP_DEFAULT_PRECIS as u32,
            CLEARTYPE_QUALITY as u32,
            (FIXED_PITCH | FF_MODERN) as u32,
            f.as_ptr(),
        )
    }
}

pub fn set_clipboard(hwnd: HWND, text: &str) -> bool {
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        if OpenClipboard(hwnd) == 0 {
            return false;
        }
        EmptyClipboard();
        let bytes = wide.len() * 2;
        let h = GlobalAlloc(GMEM_MOVEABLE, bytes);
        if h.is_null() {
            CloseClipboard();
            return false;
        }
        let p = GlobalLock(h) as *mut u16;
        std::ptr::copy_nonoverlapping(wide.as_ptr(), p, wide.len());
        GlobalUnlock(h);
        SetClipboardData(CF_UNICODETEXT as u32, h as HANDLE);
        CloseClipboard();
        true
    }
}

pub fn get_clipboard(hwnd: HWND) -> Option<String> {
    unsafe {
        if IsClipboardFormatAvailable(CF_UNICODETEXT as u32) == 0 {
            return None;
        }
        if OpenClipboard(hwnd) == 0 {
            return None;
        }
        let h = GetClipboardData(CF_UNICODETEXT as u32);
        let mut out = None;
        if !h.is_null() {
            let p = GlobalLock(h as HGLOBAL) as *const u16;
            if !p.is_null() {
                let mut n = 0usize;
                while *p.add(n) != 0 {
                    n += 1;
                }
                out = Some(String::from_utf16_lossy(std::slice::from_raw_parts(p, n)));
                GlobalUnlock(h as HGLOBAL);
            }
        }
        CloseClipboard();
        out
    }
}

pub fn send(hwnd: HWND, msg: u32, wp: usize, lp: isize) -> isize {
    unsafe { SendMessageW(hwnd, msg, wp, lp) }
}

pub fn invalidate(hwnd: HWND) {
    unsafe {
        InvalidateRect(hwnd, std::ptr::null(), 0);
    }
}

pub fn null() -> *mut c_void {
    null_mut()
}

/// Double-buffered painting helper: draws into an off-screen bitmap and blits on drop.
pub struct BufferedPaint {
    pub hwnd: HWND,
    pub ps: PAINTSTRUCT,
    pub hdc_win: HDC,
    pub hdc: HDC,
    bmp: HBITMAP,
    old: HGDIOBJ,
    pub w: i32,
    pub h: i32,
}

impl BufferedPaint {
    pub fn begin(hwnd: HWND) -> Self {
        unsafe {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let hdc_win = BeginPaint(hwnd, &mut ps);
            let rc = client_rect(hwnd);
            let w = (rc.right - rc.left).max(1);
            let h = (rc.bottom - rc.top).max(1);
            let hdc = CreateCompatibleDC(hdc_win);
            let bmp = CreateCompatibleBitmap(hdc_win, w, h);
            let old = SelectObject(hdc, bmp as _);
            BufferedPaint { hwnd, ps, hdc_win, hdc, bmp, old, w, h }
        }
    }
}

impl Drop for BufferedPaint {
    fn drop(&mut self) {
        unsafe {
            BitBlt(self.hdc_win, 0, 0, self.w, self.h, self.hdc, 0, 0, SRCCOPY);
            SelectObject(self.hdc, self.old);
            DeleteObject(self.bmp as _);
            DeleteDC(self.hdc);
            EndPaint(self.hwnd, &self.ps);
        }
    }
}

pub fn shell_open(path: &str) {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    let op = w("open");
    let p = w(path);
    unsafe {
        ShellExecuteW(null_mut(), op.as_ptr(), p.as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL);
    }
}

pub fn shell_exec(file: &str, params: &str, dir: Option<&str>) {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    let op = w("open");
    let f = w(file);
    let p = w(params);
    let d = dir.map(w);
    unsafe {
        ShellExecuteW(
            null_mut(),
            op.as_ptr(),
            f.as_ptr(),
            p.as_ptr(),
            d.as_ref().map(|d| d.as_ptr()).unwrap_or(std::ptr::null()),
            SW_SHOWNORMAL,
        );
    }
}
