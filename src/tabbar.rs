//! Custom-drawn document tab bar in the Notepad++ style
//! (orange indicator on the active tab, floppy save-state icons, close buttons).

use crate::app::{self, try_app};
use crate::win::*;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::UI::Controls::WM_MOUSELEAVE;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

pub const CLASS: &str = "NppRsTabBar";

pub struct TabBar {
    pub hwnd: HWND,
    pub dpi: u32,
    font: HFONT,
    icons: [HICON; 4],
    icon_px: i32,
    first: usize,
    rects: Vec<RECT>,
    hover: Option<usize>,
    hover_close: bool,
    hover_arrow: i32,
    drag: Option<usize>,
    drag_moved: bool,
    overflow: bool,
    pub focused_editor: bool,
}

impl TabBar {
    pub fn new() -> Self {
        TabBar {
            hwnd: std::ptr::null_mut(),
            dpi: 96,
            font: std::ptr::null_mut(),
            icons: [std::ptr::null_mut(); 4],
            icon_px: 0,
            first: 0,
            rects: Vec::new(),
            hover: None,
            hover_close: false,
            hover_arrow: 0,
            drag: None,
            drag_moved: false,
            overflow: false,
            focused_editor: true,
        }
    }

    pub fn create(&mut self, parent: HWND) {
        let cls = w(CLASS);
        unsafe {
            self.hwnd = CreateWindowExW(0, cls.as_ptr(), std::ptr::null(), WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS, 0, 0, 100, 24, parent, std::ptr::null_mut(), hinstance(), std::ptr::null());
        }
        self.update_dpi();
    }

    pub fn update_dpi(&mut self) {
        self.dpi = dpi_of(self.hwnd);
        unsafe {
            if !self.font.is_null() {
                DeleteObject(self.font as _);
            }
        }
        self.font = ui_font(9, self.dpi, false);
        let px = scale(16, self.dpi);
        if px != self.icon_px {
            for (k, ic) in self.icons.iter_mut().enumerate() {
                if !ic.is_null() {
                    unsafe { DestroyIcon(*ic) };
                }
                *ic = crate::icons::tab_icon(k, px as usize);
            }
            self.icon_px = px;
        }
    }

    pub fn height(&self) -> i32 {
        scale(25, self.dpi)
    }

    fn arrows_w(&self) -> i32 {
        scale(34, self.dpi)
    }

    fn layout(&mut self, titles: &[(String, usize)], active: usize, close_btn: bool) {
        let hdc = unsafe { GetDC(self.hwnd) };
        let old = unsafe { SelectObject(hdc, self.font as _) };
        let rc = client_rect(self.hwnd);
        let d = self.dpi;
        let h = self.height();
        let mut widths = Vec::with_capacity(titles.len());
        for (i, (t, _)) in titles.iter().enumerate() {
            let tw = text_width(hdc, t);
            let close = if close_btn && (i == active || Some(i) == self.hover) { scale(18, d) } else { scale(18, d) };
            widths.push((scale(8, d) + self.icon_px + scale(5, d) + tw + scale(6, d) + if close_btn { close } else { 0 }).max(scale(60, d)));
        }
        unsafe {
            SelectObject(hdc, old);
            ReleaseDC(self.hwnd, hdc);
        }
        let total: i32 = widths.iter().sum::<i32>() + scale(2, d);
        self.overflow = total > rc.right;
        let avail = if self.overflow { rc.right - self.arrows_w() } else { rc.right };
        if !self.overflow {
            self.first = 0;
        }
        // keep active visible
        if self.overflow {
            if active < self.first {
                self.first = active;
            }
            loop {
                let w_: i32 = widths[self.first..=active.min(widths.len() - 1)].iter().sum();
                if w_ <= avail || self.first >= active {
                    break;
                }
                self.first += 1;
            }
        }
        self.first = self.first.min(titles.len().saturating_sub(1));
        self.rects = vec![rect(-1, -1, -1, -1); titles.len()];
        let mut x = scale(2, d);
        for (i, w_) in widths.iter().enumerate().skip(self.first) {
            self.rects[i] = rect(x, scale(2, d), x + w_, h);
            x += w_;
        }
    }

    fn close_rect(&self, r: &RECT) -> RECT {
        let s = scale(14, self.dpi);
        let cx = r.right - scale(6, self.dpi) - s;
        let cy = (r.top + r.bottom) / 2 - s / 2 + scale(1, self.dpi);
        rect(cx, cy, cx + s, cy + s)
    }

    fn paint(&mut self) {
        let Some(a) = try_app() else { return };
        let titles: Vec<(String, usize)> = a
            .docs
            .iter()
            .map(|d| (d.title(), if d.read_only { 2 } else if d.is_modified() || (d.path.is_none() && false) { 1 } else { 0 }))
            .collect();
        let active = a.active;
        let close_btn = a.settings.tab_close_button;
        self.layout(&titles, active, close_btn);
        let bp = BufferedPaint::begin(self.hwnd);
        let hdc = bp.hdc;
        let d = self.dpi;
        let bg = 0xF0F0F0;
        fill(hdc, &rect(0, 0, bp.w, bp.h), bg);
        hline(hdc, 0, bp.w, bp.h - 1, 0xA0A0A0);
        unsafe {
            SelectObject(hdc, self.font as _);
            SetBkMode(hdc, TRANSPARENT as i32);
        }
        let limit = if self.overflow { bp.w - self.arrows_w() } else { bp.w };
        unsafe {
            IntersectClipRect(hdc, 0, 0, limit, bp.h);
        }
        for (i, (title, state)) in titles.iter().enumerate() {
            let r = self.rects[i];
            if r.right < 0 {
                continue;
            }
            let is_active = i == active;
            let hovered = self.hover == Some(i);
            let mut tr = r;
            if !is_active {
                tr.top += scale(2, d);
            }
            let fillc = if is_active { 0xFFFFFF } else if hovered { 0xF7F7F7 } else { 0xE3E3E3 };
            fill(hdc, &rect(tr.left, tr.top, tr.right, bp.h - if is_active { 0 } else { 1 }), fillc);
            // borders
            vline(hdc, tr.left, tr.top, bp.h - 1, 0xB4B4B4);
            vline(hdc, tr.right - 1, tr.top, bp.h - 1, 0xB4B4B4);
            hline(hdc, tr.left, tr.right, tr.top, 0xB4B4B4);
            if is_active {
                let bar = if self.focused_editor { 0xFAAA3C } else { 0xFFCAB0 };
                fill(hdc, &rect(tr.left, tr.top, tr.right, tr.top + scale(3, d)), bar);
            }
            let ix = tr.left + scale(6, d);
            let iy = (tr.top + bp.h - self.icon_px) / 2;
            unsafe {
                DrawIconEx(hdc, ix, iy, self.icons[*state], self.icon_px, self.icon_px, 0, std::ptr::null_mut(), DI_NORMAL);
                SetTextColor(hdc, rgb(if is_active { 0x000000 } else { 0x6D6D6D }));
            }
            let tx = ix + self.icon_px + scale(5, d);
            let th = scale(15, d);
            text_out(hdc, tx, (tr.top + bp.h - th) / 2 - scale(1, d) + if is_active { scale(1, d) } else { 0 }, title);
            if close_btn && (is_active || hovered) {
                let cr = self.close_rect(&tr);
                let hot = hovered && self.hover_close;
                if hot {
                    fill(hdc, &cr, 0xE81123);
                }
                let col = if hot { 0xFFFFFF } else if is_active { 0xD83A34 } else { 0x909090 };
                let p = scale(4, d);
                draw_x(hdc, cr.left + p, cr.top + p, cr.right - p, cr.bottom - p, col, scale(1, d).max(1) + (d > 120) as i32);
            }
        }
        unsafe {
            SelectClipRgn(hdc, std::ptr::null_mut());
        }
        if self.overflow {
            let aw = self.arrows_w() / 2;
            let x0 = bp.w - aw * 2;
            for k in 0..2 {
                let r = rect(x0 + k * aw, scale(3, d), x0 + (k + 1) * aw, bp.h - scale(2, d));
                fill(hdc, &r, if self.hover_arrow == k + 1 { 0xDDE8F6 } else { 0xF0F0F0 });
                frame(hdc, &r, 0xB4B4B4);
                let cx = (r.left + r.right) / 2;
                let cy = (r.top + r.bottom) / 2;
                let sz = scale(4, d);
                for j in 0..=sz {
                    let (xa, half) = if k == 0 { (cx - sz / 2 + j, j) } else { (cx + sz / 2 - j, j) };
                    vline(hdc, xa, cy - half, cy + half + 1, 0x404040);
                }
            }
        }
        drop(bp);
    }

    fn hit(&self, x: i32, y: i32) -> (Option<usize>, bool, i32) {
        let rc = client_rect(self.hwnd);
        if self.overflow && x >= rc.right - self.arrows_w() {
            let aw = self.arrows_w() / 2;
            return (None, false, if x < rc.right - aw { 1 } else { 2 });
        }
        for (i, r) in self.rects.iter().enumerate() {
            if r.right >= 0 && x >= r.left && x < r.right && y >= 0 && y < rc.bottom {
                let cr = self.close_rect(r);
                let on_close = x >= cr.left && x < cr.right && y >= cr.top && y < cr.bottom;
                return (Some(i), on_close, 0);
            }
        }
        (None, false, 0)
    }
}

fn draw_x(hdc: HDC, x0: i32, y0: i32, x1: i32, y1: i32, color: u32, thick: i32) {
    let n = (x1 - x0).max(1);
    for i in 0..n {
        let y = y0 + i * (y1 - y0) / n;
        fill(hdc, &rect(x0 + i, y, x0 + i + thick, y + thick), color);
        fill(hdc, &rect(x1 - 1 - i, y, x1 - 1 - i + thick, y + thick), color);
    }
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
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: cls.as_ptr(),
            hIconSm: std::ptr::null_mut(),
        };
        RegisterClassExW(&wc);
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let Some(a) = try_app() else { return DefWindowProcW(hwnd, msg, wp, lp) };
    let tb = &mut a.tabbar;
    match msg {
        WM_PAINT => {
            tb.paint();
            0
        }
        WM_ERASEBKGND => 1,
        WM_MOUSEMOVE => {
            let (x, y) = (x_lparam(lp), y_lparam(lp));
            let (h, c, arrow) = tb.hit(x, y);
            if let Some(from) = tb.drag {
                if let Some(to) = h {
                    if to != from {
                        tb.drag = Some(to);
                        tb.drag_moved = true;
                        app::move_tab(from, to);
                        return 0;
                    }
                }
            }
            if h != tb.hover || c != tb.hover_close || arrow != tb.hover_arrow {
                tb.hover = h;
                tb.hover_close = c;
                tb.hover_arrow = arrow;
                invalidate(hwnd);
            }
            let mut tme = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
            TrackMouseEvent(&mut tme);
            0
        }
        WM_MOUSELEAVE => {
            tb.hover = None;
            tb.hover_close = false;
            tb.hover_arrow = 0;
            invalidate(hwnd);
            0
        }
        WM_LBUTTONDOWN => {
            let (x, y) = (x_lparam(lp), y_lparam(lp));
            let (h, c, arrow) = tb.hit(x, y);
            if arrow == 1 {
                tb.first = tb.first.saturating_sub(1);
                invalidate(hwnd);
                return 0;
            }
            if arrow == 2 {
                if tb.first + 1 < a.docs.len() {
                    tb.first += 1;
                }
                invalidate(hwnd);
                return 0;
            }
            if let Some(i) = h {
                if c {
                    app::close_tab(i);
                    return 0;
                }
                tb.drag = Some(i);
                tb.drag_moved = false;
                SetCapture(hwnd);
                app::activate(i);
            }
            0
        }
        WM_LBUTTONUP => {
            if tb.drag.take().is_some() {
                ReleaseCapture();
            }
            0
        }
        WM_LBUTTONDBLCLK => {
            let (h, _, arrow) = tb.hit(x_lparam(lp), y_lparam(lp));
            match h {
                None if arrow == 0 => app::exec_cmd(crate::menu::Cmd::New),
                Some(i) if a.settings.dbl_click_close_tab => app::close_tab(i),
                _ => {}
            }
            0
        }
        WM_MBUTTONUP => {
            if let (Some(i), _, _) = tb.hit(x_lparam(lp), y_lparam(lp)) {
                app::close_tab(i);
            }
            0
        }
        WM_RBUTTONUP => {
            if let (Some(i), _, _) = tb.hit(x_lparam(lp), y_lparam(lp)) {
                app::activate(i);
                let mut pt = POINT { x: x_lparam(lp), y: y_lparam(lp) };
                ClientToScreen(hwnd, &mut pt);
                app::tab_context_menu(pt.x, pt.y);
            }
            0
        }
        WM_MOUSEWHEEL => {
            let delta = (wp >> 16) as u16 as i16;
            let n = a.docs.len();
            if n > 1 {
                let i = if delta > 0 { (a.active + n - 1) % n } else { (a.active + 1) % n };
                app::activate(i);
            }
            0
        }
        WM_CAPTURECHANGED => {
            tb.drag = None;
            0
        }
        _ => {
            let _ = GetKeyState(0);
            DefWindowProcW(hwnd, msg, wp, lp)
        }
    }
}
