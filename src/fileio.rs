//! Loading and saving files with encoding / EOL detection.

use crate::doc::{Encoding, Eol};
use std::path::Path;
use windows_sys::Win32::Globalization::{MultiByteToWideChar, WideCharToMultiByte, CP_ACP};

pub struct Loaded {
    pub text: String,
    pub encoding: Encoding,
    pub eol: Eol,
}

pub fn decode(bytes: &[u8]) -> Loaded {
    let (text, encoding) = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        (String::from_utf8_lossy(&bytes[3..]).into_owned(), Encoding::Utf8Bom)
    } else if bytes.starts_with(&[0xFF, 0xFE]) {
        (utf16(&bytes[2..], true), Encoding::Utf16Le)
    } else if bytes.starts_with(&[0xFE, 0xFF]) {
        (utf16(&bytes[2..], false), Encoding::Utf16Be)
    } else if looks_utf16le(bytes) {
        (utf16(bytes, true), Encoding::Utf16Le)
    } else {
        match std::str::from_utf8(bytes) {
            Ok(s) => (s.to_string(), Encoding::Utf8),
            Err(_) => (ansi_decode(bytes), Encoding::Ansi),
        }
    };
    let eol = detect_eol(&text);
    let text = normalize_eol(&text);
    Loaded { text, encoding, eol }
}

fn looks_utf16le(b: &[u8]) -> bool {
    if b.len() < 4 || b.len() % 2 != 0 {
        return false;
    }
    let sample = &b[..b.len().min(1024)];
    let zeros_odd = sample.iter().skip(1).step_by(2).filter(|&&x| x == 0).count();
    let zeros_even = sample.iter().step_by(2).filter(|&&x| x == 0).count();
    zeros_odd * 10 > sample.len() / 2 * 9 && zeros_even == 0
}

fn utf16(b: &[u8], le: bool) -> String {
    let units: Vec<u16> = b
        .chunks_exact(2)
        .map(|c| if le { u16::from_le_bytes([c[0], c[1]]) } else { u16::from_be_bytes([c[0], c[1]]) })
        .collect();
    String::from_utf16_lossy(&units)
}

pub fn ansi_decode(b: &[u8]) -> String {
    if b.is_empty() {
        return String::new();
    }
    unsafe {
        let n = MultiByteToWideChar(CP_ACP, 0, b.as_ptr(), b.len() as i32, std::ptr::null_mut(), 0);
        let mut buf = vec![0u16; n as usize];
        MultiByteToWideChar(CP_ACP, 0, b.as_ptr(), b.len() as i32, buf.as_mut_ptr(), n);
        String::from_utf16_lossy(&buf)
    }
}

pub fn ansi_encode(s: &str) -> Vec<u8> {
    let w: Vec<u16> = s.encode_utf16().collect();
    if w.is_empty() {
        return Vec::new();
    }
    unsafe {
        let n = WideCharToMultiByte(CP_ACP, 0, w.as_ptr(), w.len() as i32, std::ptr::null_mut(), 0, std::ptr::null(), std::ptr::null_mut());
        let mut buf = vec![0u8; n as usize];
        WideCharToMultiByte(CP_ACP, 0, w.as_ptr(), w.len() as i32, buf.as_mut_ptr(), n, std::ptr::null(), std::ptr::null_mut());
        buf
    }
}

pub fn detect_eol(s: &str) -> Eol {
    let b = s.as_bytes();
    let (mut crlf, mut lf, mut cr) = (0, 0, 0);
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'\r' if i + 1 < b.len() && b[i + 1] == b'\n' => {
                crlf += 1;
                i += 1;
            }
            b'\r' => cr += 1,
            b'\n' => lf += 1,
            _ => {}
        }
        i += 1;
    }
    if crlf == 0 && lf == 0 && cr == 0 {
        Eol::Crlf
    } else if lf > crlf && lf >= cr {
        Eol::Lf
    } else if cr > crlf && cr > lf {
        Eol::Cr
    } else {
        Eol::Crlf
    }
}

pub fn normalize_eol(s: &str) -> String {
    if !s.contains('\r') {
        return s.to_string();
    }
    s.replace("\r\n", "\n").replace('\r', "\n")
}

pub fn encode(text: &str, eol: Eol, enc: Encoding) -> Vec<u8> {
    let t = if eol == Eol::Lf { std::borrow::Cow::Borrowed(text) } else { std::borrow::Cow::Owned(text.replace('\n', eol.as_str())) };
    match enc {
        Encoding::Utf8 => t.as_bytes().to_vec(),
        Encoding::Utf8Bom => {
            let mut v = vec![0xEF, 0xBB, 0xBF];
            v.extend_from_slice(t.as_bytes());
            v
        }
        Encoding::Utf16Le => {
            let mut v = vec![0xFF, 0xFE];
            for u in t.encode_utf16() {
                v.extend_from_slice(&u.to_le_bytes());
            }
            v
        }
        Encoding::Utf16Be => {
            let mut v = vec![0xFE, 0xFF];
            for u in t.encode_utf16() {
                v.extend_from_slice(&u.to_be_bytes());
            }
            v
        }
        Encoding::Ansi => ansi_encode(&t),
    }
}

pub fn load(path: &Path) -> std::io::Result<Loaded> {
    let bytes = std::fs::read(path)?;
    Ok(decode(&bytes))
}

pub fn save(path: &Path, text: &str, eol: Eol, enc: Encoding) -> std::io::Result<()> {
    use std::io::Write;
    let bytes = encode(text, eol, enc);
    if path.exists() {
        // Open the existing file (works for hidden/system files, keeps attributes and ACLs).
        let mut f = std::fs::OpenOptions::new().write(true).open(path)?;
        f.set_len(0)?;
        f.write_all(&bytes)?;
        f.sync_all()
    } else {
        std::fs::write(path, bytes)
    }
}

pub fn mtime(path: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_utf8_crlf() {
        let l = decode(b"a\r\nb\r\n");
        assert_eq!(l.text, "a\nb\n");
        assert_eq!(l.eol, Eol::Crlf);
        assert_eq!(encode(&l.text, l.eol, l.encoding), b"a\r\nb\r\n");
    }

    #[test]
    fn detects_bom_and_utf16() {
        assert_eq!(decode(&[0xEF, 0xBB, 0xBF, b'x']).encoding, Encoding::Utf8Bom);
        let l = decode(&[0xFF, 0xFE, b'h', 0, b'i', 0]);
        assert_eq!(l.text, "hi");
        assert_eq!(l.encoding, Encoding::Utf16Le);
        assert_eq!(decode(b"a\nb").eol, Eol::Lf);
    }
}
