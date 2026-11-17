//! A tiny anti-aliased vector rasterizer used to draw the toolbar / tab / app icons at any DPI.

use std::f32::consts::PI;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

pub struct Canvas {
    pub size: usize,
    px: Vec<[f32; 4]>, // premultiplied RGBA
    k: f32,            // scale from 16-unit design space to pixels
}

const SS: usize = 4;

impl Canvas {
    pub fn new(size: usize) -> Self {
        Canvas { size, px: vec![[0.0; 4]; size * size], k: size as f32 / 16.0 }
    }

    fn fill_fn(&mut self, color: u32, alpha: f32, inside: impl Fn(f32, f32) -> bool) {
        let r = ((color >> 16) & 0xFF) as f32 / 255.0;
        let g = ((color >> 8) & 0xFF) as f32 / 255.0;
        let b = (color & 0xFF) as f32 / 255.0;
        let n = self.size;
        for py in 0..n {
            for px in 0..n {
                let mut cnt = 0;
                for sy in 0..SS {
                    for sx in 0..SS {
                        let x = (px as f32 + (sx as f32 + 0.5) / SS as f32) / self.k;
                        let y = (py as f32 + (sy as f32 + 0.5) / SS as f32) / self.k;
                        if inside(x, y) {
                            cnt += 1;
                        }
                    }
                }
                if cnt == 0 {
                    continue;
                }
                let a = alpha * cnt as f32 / (SS * SS) as f32;
                let d = &mut self.px[py * n + px];
                d[0] = r * a + d[0] * (1.0 - a);
                d[1] = g * a + d[1] * (1.0 - a);
                d[2] = b * a + d[2] * (1.0 - a);
                d[3] = a + d[3] * (1.0 - a);
            }
        }
    }

    pub fn rect(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, c: u32) {
        self.fill_fn(c, 1.0, |x, y| x >= x0 && x < x1 && y >= y0 && y < y1);
    }
    pub fn rrect(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, r: f32, c: u32) {
        self.fill_fn(c, 1.0, |x, y| {
            if x < x0 || x >= x1 || y < y0 || y >= y1 {
                return false;
            }
            let cx = x.clamp(x0 + r, x1 - r);
            let cy = y.clamp(y0 + r, y1 - r);
            (x - cx).powi(2) + (y - cy).powi(2) <= r * r
        });
    }
    /// Outlined box: border color `bc`, fill `fc`.
    pub fn boxed(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, bc: u32, fc: u32) {
        self.rect(x0, y0, x1, y1, bc);
        self.rect(x0 + 1.0, y0 + 1.0, x1 - 1.0, y1 - 1.0, fc);
    }
    pub fn circle(&mut self, cx: f32, cy: f32, r: f32, c: u32) {
        self.fill_fn(c, 1.0, |x, y| (x - cx).powi(2) + (y - cy).powi(2) <= r * r);
    }
    pub fn ring(&mut self, cx: f32, cy: f32, r: f32, w: f32, c: u32) {
        self.fill_fn(c, 1.0, |x, y| {
            let d = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
            d <= r && d >= r - w
        });
    }
    pub fn poly(&mut self, pts: &[(f32, f32)], c: u32) {
        let pts = pts.to_vec();
        self.fill_fn(c, 1.0, move |x, y| point_in_poly(&pts, x, y));
    }
    pub fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, w: f32, c: u32) {
        let hw = w / 2.0;
        self.fill_fn(c, 1.0, |x, y| seg_dist(x, y, x0, y0, x1, y1) <= hw);
    }
    pub fn polyline(&mut self, pts: &[(f32, f32)], w: f32, c: u32) {
        let pts = pts.to_vec();
        let hw = w / 2.0;
        self.fill_fn(c, 1.0, move |x, y| pts.windows(2).any(|s| seg_dist(x, y, s[0].0, s[0].1, s[1].0, s[1].1) <= hw));
    }
    pub fn arc(&mut self, cx: f32, cy: f32, r: f32, a0: f32, a1: f32, w: f32, c: u32) {
        let steps = 24;
        let pts: Vec<(f32, f32)> = (0..=steps)
            .map(|i| {
                let a = (a0 + (a1 - a0) * i as f32 / steps as f32) * PI / 180.0;
                (cx + r * a.cos(), cy - r * a.sin())
            })
            .collect();
        self.polyline(&pts, w, c);
    }

    /// Top-down 32bpp premultiplied DIB section.
    pub fn to_bitmap(&self) -> HBITMAP {
        let n = self.size as i32;
        unsafe {
            let mut bi: BITMAPINFO = std::mem::zeroed();
            bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            bi.bmiHeader.biWidth = n;
            bi.bmiHeader.biHeight = -n;
            bi.bmiHeader.biPlanes = 1;
            bi.bmiHeader.biBitCount = 32;
            bi.bmiHeader.biCompression = BI_RGB as u32;
            let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
            let hbm = CreateDIBSection(std::ptr::null_mut(), &bi, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
            if hbm.is_null() || bits.is_null() {
                return hbm;
            }
            let out = std::slice::from_raw_parts_mut(bits as *mut u8, (n * n * 4) as usize);
            for (i, p) in self.px.iter().enumerate() {
                out[i * 4] = (p[2].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                out[i * 4 + 1] = (p[1].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                out[i * 4 + 2] = (p[0].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                out[i * 4 + 3] = (p[3].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            }
            hbm
        }
    }

    pub fn to_icon(&self) -> HICON {
        unsafe {
            let color = self.to_bitmap();
            let n = self.size as i32;
            let mask_bits = vec![0u8; ((n + 15) / 16 * 2 * n) as usize];
            let mask = CreateBitmap(n, n, 1, 1, mask_bits.as_ptr() as _);
            let ii = ICONINFO { fIcon: 1, xHotspot: 0, yHotspot: 0, hbmMask: mask, hbmColor: color };
            let h = CreateIconIndirect(&ii);
            DeleteObject(color as _);
            DeleteObject(mask as _);
            h
        }
    }
}

fn point_in_poly(p: &[(f32, f32)], x: f32, y: f32) -> bool {
    let mut inside = false;
    let mut j = p.len() - 1;
    for i in 0..p.len() {
        let (xi, yi) = p[i];
        let (xj, yj) = p[j];
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn seg_dist(px: f32, py: f32, x0: f32, y0: f32, x1: f32, y1: f32) -> f32 {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let l2 = dx * dx + dy * dy;
    let t = if l2 == 0.0 { 0.0 } else { (((px - x0) * dx + (py - y0) * dy) / l2).clamp(0.0, 1.0) };
    let qx = x0 + t * dx;
    let qy = y0 + t * dy;
    ((px - qx).powi(2) + (py - qy).powi(2)).sqrt()
}

// ---------------------------------------------------------------- icon designs

const INK: u32 = 0x4A5562;
const BLUE: u32 = 0x2F6FD0;
const LBLUE: u32 = 0x8DB8F0;
const RED: u32 = 0xD83A34;
const GREEN: u32 = 0x2E9E3E;
const YEL: u32 = 0xF2C14E;
const DYEL: u32 = 0xC98F1A;

fn page(c: &mut Canvas, x: f32, y: f32, w: f32, h: f32) {
    let f = 4.0_f32.min(w / 2.5);
    c.poly(&[(x, y), (x + w - f, y), (x + w, y + f), (x + w, y + h), (x, y + h)], 0x7A8594);
    c.poly(&[(x + 1.0, y + 1.0), (x + w - f - 0.4, y + 1.0), (x + w - 1.0, y + f + 0.4), (x + w - 1.0, y + h - 1.0), (x + 1.0, y + h - 1.0)], 0xFFFFFF);
    c.poly(&[(x + w - f, y), (x + w - f, y + f), (x + w, y + f)], 0xC9D1DB);
}

fn page_lines(c: &mut Canvas, x: f32, y: f32, w: f32, n: usize, col: u32) {
    for i in 0..n {
        c.rect(x, y + i as f32 * 2.0, x + w - (i % 2) as f32 * 1.5, y + i as f32 * 2.0 + 1.0, col);
    }
}

fn floppy(c: &mut Canvas, x: f32, y: f32, s: f32, body: u32) {
    let k = s / 14.0;
    c.rrect(x, y, x + 14.0 * k, y + 14.0 * k, 1.5 * k, body);
    c.rect(x + 3.0 * k, y + 1.0 * k, x + 11.0 * k, y + 5.5 * k, 0xE8EEF6);
    c.rect(x + 8.5 * k, y + 1.8 * k, x + 10.0 * k, y + 4.8 * k, body);
    c.rect(x + 2.5 * k, y + 8.0 * k, x + 11.5 * k, y + 14.0 * k, 0xFFFFFF);
    c.rect(x + 3.5 * k, y + 9.5 * k, x + 10.5 * k, y + 10.3 * k, 0xB0BCCB);
    c.rect(x + 3.5 * k, y + 11.5 * k, x + 10.5 * k, y + 12.3 * k, 0xB0BCCB);
}

fn red_x(c: &mut Canvas, cx: f32, cy: f32, r: f32) {
    c.circle(cx, cy, r, RED);
    c.line(cx - r * 0.45, cy - r * 0.45, cx + r * 0.45, cy + r * 0.45, 1.3, 0xFFFFFF);
    c.line(cx + r * 0.45, cy - r * 0.45, cx - r * 0.45, cy + r * 0.45, 1.3, 0xFFFFFF);
}

fn magnifier(c: &mut Canvas, cx: f32, cy: f32, r: f32) {
    c.line(cx + r * 0.6, cy + r * 0.6, cx + r * 1.7, cy + r * 1.7, 2.6, 0x6B4A2B);
    c.circle(cx, cy, r, 0x55606E);
    c.circle(cx, cy, r - 1.2, 0xDDEEFF);
    c.arc(cx, cy, r - 2.2, 100.0, 170.0, 0.9, 0xFFFFFF);
}

fn arrow_head(c: &mut Canvas, tip: (f32, f32), dir: f32, size: f32, col: u32) {
    let a = dir * PI / 180.0;
    let back = (tip.0 - size * a.cos(), tip.1 + size * a.sin());
    let perp = (a.sin() * size * 0.75, a.cos() * size * 0.75);
    c.poly(&[tip, (back.0 + perp.0, back.1 + perp.1), (back.0 - perp.0, back.1 - perp.1)], col);
}

pub const TOOLBAR_ICONS: usize = 32;

/// Draw toolbar icon `i` (Notepad++ toolbar order).
pub fn toolbar_icon(i: usize, size: usize) -> Canvas {
    let mut c = Canvas::new(size);
    match i {
        0 => {
            // new
            page(&mut c, 2.0, 0.5, 11.0, 14.5);
            page_lines(&mut c, 4.0, 5.0, 6.0, 3, 0xC2CAD4);
            c.circle(12.0, 12.0, 3.6, GREEN);
            c.rect(10.0, 11.4, 14.0, 12.6, 0xFFFFFF);
            c.rect(11.4, 10.0, 12.6, 14.0, 0xFFFFFF);
        }
        1 => {
            // open
            c.poly(&[(0.5, 3.0), (5.5, 3.0), (7.0, 4.5), (14.0, 4.5), (14.0, 13.5), (0.5, 13.5)], DYEL);
            page(&mut c, 3.0, 1.5, 9.0, 8.0);
            c.poly(&[(2.5, 7.0), (15.5, 7.0), (13.5, 14.0), (0.5, 14.0)], YEL);
            c.polyline(&[(2.5, 7.0), (15.5, 7.0), (13.5, 14.0), (0.5, 14.0), (2.5, 7.0)], 0.7, DYEL);
        }
        2 => floppy(&mut c, 1.0, 1.0, 14.0, BLUE),
        3 => {
            floppy(&mut c, 4.0, 0.5, 11.5, 0x6C95D8);
            floppy(&mut c, 0.5, 4.0, 11.5, BLUE);
        }
        4 => {
            page(&mut c, 1.5, 0.5, 11.0, 14.5);
            page_lines(&mut c, 3.5, 5.0, 6.0, 3, 0xC2CAD4);
            red_x(&mut c, 11.5, 11.5, 4.0);
        }
        5 => {
            page(&mut c, 4.0, 0.0, 10.0, 12.0);
            page(&mut c, 1.0, 2.5, 10.0, 12.5);
            red_x(&mut c, 11.5, 11.5, 4.0);
        }
        6 => {
            // print
            c.rect(4.0, 1.0, 12.0, 6.0, 0xFFFFFF);
            c.polyline(&[(4.0, 6.0), (4.0, 1.0), (12.0, 1.0), (12.0, 6.0)], 0.9, 0x7A8594);
            c.rrect(0.5, 5.5, 15.5, 12.0, 1.5, 0x6E7A89);
            c.rect(2.0, 7.0, 14.0, 8.0, 0x8E99A6);
            c.circle(13.0, 9.0, 0.8, 0x7CFC7C);
            c.rect(3.5, 10.0, 12.5, 15.0, 0xFFFFFF);
            c.polyline(&[(3.5, 10.0), (3.5, 15.0), (12.5, 15.0), (12.5, 10.0)], 0.9, 0x7A8594);
            c.rect(5.0, 12.0, 11.0, 12.8, 0xB0BCCB);
        }
        7 => {
            // cut
            c.line(5.0, 1.0, 10.5, 10.0, 1.5, 0x8A96A6);
            c.line(11.0, 1.0, 5.5, 10.0, 1.5, 0x8A96A6);
            c.ring(4.5, 12.0, 3.0, 1.5, RED);
            c.ring(11.5, 12.0, 3.0, 1.5, RED);
        }
        8 => {
            // copy
            c.boxed(1.0, 1.0, 10.0, 11.0, BLUE, 0xFFFFFF);
            page_lines(&mut c, 2.5, 3.5, 5.0, 3, LBLUE);
            c.boxed(6.0, 5.0, 15.0, 15.0, BLUE, 0xFFFFFF);
            page_lines(&mut c, 7.5, 7.5, 5.0, 3, LBLUE);
        }
        9 => {
            // paste
            c.rrect(1.0, 2.0, 13.0, 15.5, 1.2, 0xB07A3E);
            c.rrect(4.0, 0.5, 10.0, 4.0, 1.0, 0x707C8A);
            c.boxed(6.0, 5.0, 15.0, 15.5, 0x7A8594, 0xFFFFFF);
            page_lines(&mut c, 7.5, 7.5, 5.0, 3, 0xB0BCCB);
        }
        10 => {
            // undo
            c.arc(8.5, 9.0, 5.0, 150.0, -60.0, 2.2, BLUE);
            arrow_head(&mut c, (1.5, 8.0), 250.0, 4.2, BLUE);
        }
        11 => {
            // redo
            c.arc(7.5, 9.0, 5.0, 30.0, 240.0, 2.2, BLUE);
            arrow_head(&mut c, (14.5, 8.0), 290.0, 4.2, BLUE);
        }
        12 => {
            // find: binoculars
            c.rrect(1.0, 6.0, 7.5, 14.5, 2.5, 0x2F3A48);
            c.rrect(8.5, 6.0, 15.0, 14.5, 2.5, 0x2F3A48);
            c.rect(6.0, 7.5, 10.0, 11.0, 0x2F3A48);
            c.rect(2.0, 2.0, 6.0, 7.0, 0x48566A);
            c.rect(10.0, 2.0, 14.0, 7.0, 0x48566A);
            c.circle(4.2, 11.5, 2.0, 0x7FB2F0);
            c.circle(11.8, 11.5, 2.0, 0x7FB2F0);
        }
        13 => {
            // replace
            c.rrect(0.5, 4.0, 5.5, 10.0, 2.0, 0x2F3A48);
            c.rrect(6.0, 4.0, 11.0, 10.0, 2.0, 0x2F3A48);
            c.rect(4.5, 5.0, 7.0, 7.5, 0x2F3A48);
            c.circle(3.0, 7.8, 1.4, 0x7FB2F0);
            c.circle(8.5, 7.8, 1.4, 0x7FB2F0);
            c.arc(10.0, 12.0, 3.0, 180.0, 330.0, 1.6, GREEN);
            arrow_head(&mut c, (15.2, 11.8), 80.0, 3.4, GREEN);
        }
        14 | 15 => {
            magnifier(&mut c, 6.0, 6.0, 5.0);
            c.rect(3.5, 5.3, 8.5, 6.7, 0x2F6FD0);
            if i == 14 {
                c.rect(5.3, 3.5, 6.7, 8.5, 0x2F6FD0);
            }
        }
        16 | 17 => {
            c.boxed(0.5, 1.0, 7.0, 15.0, 0x7A8594, 0xFFFFFF);
            c.boxed(9.0, 1.0, 15.5, 15.0, 0x7A8594, 0xFFFFFF);
            if i == 16 {
                c.line(3.75, 4.0, 3.75, 12.0, 1.3, BLUE);
                arrow_head(&mut c, (3.75, 2.5), 90.0, 3.0, BLUE);
                arrow_head(&mut c, (3.75, 13.5), 270.0, 3.0, BLUE);
                c.line(12.25, 4.0, 12.25, 12.0, 1.3, BLUE);
                arrow_head(&mut c, (12.25, 2.5), 90.0, 3.0, BLUE);
                arrow_head(&mut c, (12.25, 13.5), 270.0, 3.0, BLUE);
            } else {
                c.line(2.0, 8.0, 14.0, 8.0, 1.3, BLUE);
                arrow_head(&mut c, (0.8, 8.0), 180.0, 3.0, BLUE);
                arrow_head(&mut c, (15.2, 8.0), 0.0, 3.0, BLUE);
            }
        }
        18 => {
            // word wrap
            c.rect(1.0, 2.0, 15.0, 3.4, INK);
            c.polyline(&[(1.0, 7.5), (12.0, 7.5)], 1.4, INK);
            c.arc(12.0, 10.0, 2.5, 90.0, -90.0, 1.4, BLUE);
            c.line(12.0, 12.5, 8.5, 12.5, 1.4, BLUE);
            arrow_head(&mut c, (6.5, 12.5), 180.0, 3.0, BLUE);
            c.rect(1.0, 11.8, 5.0, 13.2, INK);
        }
        19 => {
            // pilcrow
            c.circle(6.0, 5.0, 3.6, BLUE);
            c.rect(6.0, 1.4, 13.5, 2.8, BLUE);
            c.rect(8.0, 1.4, 9.4, 15.0, BLUE);
            c.rect(11.5, 1.4, 12.9, 15.0, BLUE);
            c.rect(6.0, 1.4, 8.0, 8.6, BLUE);
        }
        20 => {
            // indent guide
            for k in 0..6 {
                c.rect(2.0, 1.0 + k as f32 * 2.5, 3.0, 2.2 + k as f32 * 2.5, 0x8A96A6);
            }
            c.rect(4.5, 2.0, 15.0, 3.4, INK);
            c.rect(7.0, 6.0, 15.0, 7.4, BLUE);
            c.rect(7.0, 10.0, 13.0, 11.4, BLUE);
            c.rect(4.5, 13.5, 11.0, 14.9, INK);
            for k in 0..3 {
                c.rect(5.3, 5.5 + k as f32 * 2.5, 5.9, 6.5 + k as f32 * 2.5, 0x8A96A6);
            }
        }
        21 => {
            // user defined language
            page(&mut c, 1.5, 0.5, 13.0, 15.0);
            c.rect(3.5, 5.0, 8.0, 6.4, 0x0000FF);
            c.rect(8.8, 5.0, 12.0, 6.4, 0xFF8000);
            c.rect(3.5, 8.0, 6.0, 9.4, 0x8000FF);
            c.rect(6.8, 8.0, 12.0, 9.4, 0x808080);
            c.rect(3.5, 11.0, 10.0, 12.4, 0x008000);
        }
        22 => {
            // document map
            c.boxed(1.5, 0.5, 14.5, 15.5, 0x7A8594, 0xFFFFFF);
            page_lines(&mut c, 3.0, 2.5, 9.0, 6, 0xB0BCCB);
            c.ring(8.0, 7.0, 4.6, 1.2, 0xFF8000);
            c.polyline(&[(2.5, 4.5), (13.5, 4.5), (13.5, 9.5), (2.5, 9.5), (2.5, 4.5)], 1.2, 0xFF8000);
        }
        23 => {
            // document list
            for k in 0..4 {
                let y = 1.5 + k as f32 * 3.6;
                c.boxed(1.0, y, 4.0, y + 3.0, 0x7A8594, 0xFFFFFF);
                c.rect(5.5, y + 0.9, 15.0, y + 2.1, if k == 1 { BLUE } else { INK });
            }
        }
        24 => {
            // function list: f()
            c.arc(6.5, 4.0, 2.5, 20.0, 180.0, 1.5, 0x8000FF);
            c.line(4.0, 4.0, 4.0, 14.0, 1.5, 0x8000FF);
            c.line(2.0, 7.0, 7.0, 7.0, 1.3, 0x8000FF);
            c.arc(13.0, 9.5, 4.5, 130.0, 230.0, 1.3, INK);
            c.arc(8.0, 9.5, 4.5, -50.0, 50.0, 1.3, INK);
        }
        25 => {
            // folder as workspace
            c.poly(&[(0.5, 2.0), (5.5, 2.0), (7.0, 3.5), (14.5, 3.5), (14.5, 12.0), (0.5, 12.0)], DYEL);
            c.rect(0.5, 5.0, 14.5, 12.0, YEL);
            c.rect(3.0, 12.0, 4.2, 15.0, INK);
            c.rect(3.0, 14.0, 9.0, 15.2, INK);
            c.rect(9.0, 12.5, 15.0, 15.5, BLUE);
        }
        26 => {
            // monitoring (eye)
            c.poly(&[(0.5, 8.0), (4.0, 4.0), (8.0, 3.0), (12.0, 4.0), (15.5, 8.0), (12.0, 12.0), (8.0, 13.0), (4.0, 12.0)], 0x55606E);
            c.poly(&[(1.8, 8.0), (4.6, 5.0), (8.0, 4.2), (11.4, 5.0), (14.2, 8.0), (11.4, 11.0), (8.0, 11.8), (4.6, 11.0)], 0xFFFFFF);
            c.circle(8.0, 8.0, 3.2, BLUE);
            c.circle(8.0, 8.0, 1.4, 0x101820);
            c.circle(7.0, 7.0, 0.7, 0xFFFFFF);
        }
        27 => {
            c.circle(8.0, 8.0, 6.0, 0x8A96A6);
            c.circle(8.0, 8.0, 5.0, RED);
            c.circle(6.5, 6.5, 1.5, 0xF08A86);
        }
        28 => c.rrect(2.5, 2.5, 13.5, 13.5, 1.5, 0x55606E),
        29 => c.poly(&[(3.5, 2.0), (13.5, 8.0), (3.5, 14.0)], 0x55606E),
        30 => {
            c.poly(&[(1.0, 2.0), (8.0, 8.0), (1.0, 14.0)], 0x55606E);
            c.poly(&[(8.0, 2.0), (15.0, 8.0), (8.0, 14.0)], BLUE);
        }
        31 => {
            floppy(&mut c, 0.5, 0.5, 13.0, 0x55606E);
            c.circle(12.0, 12.0, 3.5, RED);
        }
        _ => {}
    }
    c
}

/// Tab state icons: 0 saved, 1 modified, 2 read-only, 3 monitoring.
pub fn tab_icon(kind: usize, size: usize) -> HICON {
    let mut c = Canvas::new(size);
    match kind {
        0 => floppy(&mut c, 1.0, 1.0, 14.0, BLUE),
        1 => floppy(&mut c, 1.0, 1.0, 14.0, RED),
        2 => {
            floppy(&mut c, 1.0, 1.0, 14.0, 0x8A96A6);
            c.rrect(8.5, 9.0, 15.0, 15.5, 1.0, 0x55606E);
            c.ring(11.75, 9.0, 2.4, 1.1, 0x55606E);
        }
        _ => {
            floppy(&mut c, 1.0, 1.0, 14.0, GREEN);
        }
    }
    c.to_icon()
}

/// The application icon: a spiral notepad with "++".
pub fn app_icon(size: usize) -> HICON {
    let mut c = Canvas::new(size);
    c.rrect(2.0, 2.0, 14.5, 15.5, 1.2, 0x5B6B7E);
    c.rrect(2.6, 2.6, 13.9, 14.9, 0.8, 0xFFFDF0);
    for k in 0..4 {
        c.rect(4.0, 6.5 + k as f32 * 2.2, 12.5, 7.1 + k as f32 * 2.2, 0xA9C4E8);
    }
    c.rect(2.0, 2.0, 14.5, 4.6, 0x2E9E3E);
    for k in 0..5 {
        let x = 3.6 + k as f32 * 2.3;
        c.rrect(x, 0.5, x + 1.1, 4.0, 0.5, 0x404A56);
    }
    c.rect(7.0, 9.8, 12.0, 11.2, 0xD83A34);
    c.rect(8.8, 8.0, 10.2, 13.0, 0xD83A34);
    c.rect(11.0, 12.6, 15.5, 13.8, 0xD83A34);
    c.rect(12.6, 11.0, 13.8, 15.5, 0xD83A34);
    c.to_icon()
}

/// Build an image list for the toolbar at the given pixel size.
pub fn toolbar_imagelist(size: usize) -> windows_sys::Win32::UI::Controls::HIMAGELIST {
    use windows_sys::Win32::UI::Controls::*;
    unsafe {
        let il = ImageList_Create(size as i32, size as i32, ILC_COLOR32, TOOLBAR_ICONS as i32, 4);
        for i in 0..TOOLBAR_ICONS {
            let bmp = toolbar_icon(i, size).to_bitmap();
            ImageList_Add(il, bmp, std::ptr::null_mut());
            DeleteObject(bmp as _);
        }
        il
    }
}

/// Same icons rendered disabled (desaturated, faded).
pub fn toolbar_imagelist_disabled(size: usize) -> windows_sys::Win32::UI::Controls::HIMAGELIST {
    use windows_sys::Win32::UI::Controls::*;
    unsafe {
        let il = ImageList_Create(size as i32, size as i32, ILC_COLOR32, TOOLBAR_ICONS as i32, 4);
        for i in 0..TOOLBAR_ICONS {
            let mut c = toolbar_icon(i, size);
            for p in c.px.iter_mut() {
                let a = p[3];
                if a > 0.0 {
                    let l = (0.3 * p[0] + 0.59 * p[1] + 0.11 * p[2]) / a;
                    let g = (l * 0.5 + 0.45).min(1.0);
                    let na = a * 0.45;
                    *p = [g * na, g * na, g * na, na];
                }
            }
            let bmp = c.to_bitmap();
            ImageList_Add(il, bmp, std::ptr::null_mut());
            DeleteObject(bmp as _);
        }
        il
    }
}
