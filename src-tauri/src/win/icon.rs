//! 进程图标提取：exe 文件 → HICON → RGBA → PNG → base64 data URL。
//!
//! 进程选择器一次要显示上百个条目，逐个 ExtractIconEx 有可感知的耗时，
//! 因此按 exe 路径做进程内缓存，同一个程序只提取一次。

use std::collections::HashMap;
use std::ffi::c_void;
use std::mem::size_of;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use base64::Engine as _;
use windows::core::PCWSTR;
use windows::Win32::Graphics::Gdi::{
    DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER,
    BI_RGB, DIB_RGB_COLORS, HBITMAP, HGDIOBJ,
};
use windows::Win32::Storage::FileSystem::{
    GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
};
use windows::Win32::UI::Shell::ExtractIconExW;
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};

fn cache() -> &'static Mutex<HashMap<String, Option<String>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 取某个 exe 的图标（`data:image/png;base64,...`），带缓存。
pub fn data_url(exe_path: &str) -> Option<String> {
    if exe_path.is_empty() {
        return None;
    }
    let key = exe_path.to_ascii_lowercase();
    if let Ok(map) = cache().lock() {
        if let Some(hit) = map.get(&key) {
            return hit.clone();
        }
    }

    let value = extract(exe_path);
    if let Ok(mut map) = cache().lock() {
        map.insert(key, value.clone());
    }
    value
}

fn name_cache() -> &'static Mutex<HashMap<String, Option<String>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 读 exe 的「文件说明」（资源管理器里显示的应用名），读不到就回退文件名。
///
/// 报告给用户的是应用名而不是 `chrome.exe` 这种可执行名，
/// 规则默认名称、进程选择器、卡片标题都用它。
pub fn app_name(exe_path: &str) -> Option<String> {
    if exe_path.is_empty() {
        return None;
    }
    let key = exe_path.to_ascii_lowercase();
    if let Ok(map) = name_cache().lock() {
        if let Some(hit) = map.get(&key) {
            return hit.clone();
        }
    }

    let value = read_file_description(exe_path).or_else(|| {
        Path::new(exe_path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
    });
    if let Ok(mut map) = name_cache().lock() {
        map.insert(key, value.clone());
    }
    value
}

fn read_file_description(path: &str) -> Option<String> {
    let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let size = GetFileVersionInfoSizeW(PCWSTR(wide.as_ptr()), None);
        if size == 0 {
            return None;
        }
        let mut buf = vec![0u8; size as usize];
        if GetFileVersionInfoW(PCWSTR(wide.as_ptr()), Some(0), size, buf.as_mut_ptr().cast()).is_err()
        {
            return None;
        }

        let block: *const c_void = buf.as_ptr().cast();
        for (lang, cp) in translations(block) {
            let sub = format!("\\StringFileInfo\\{lang:04x}{cp:04x}\\FileDescription");
            if let Some(text) = query_string(block, &sub) {
                let text = text.trim();
                if !text.is_empty() {
                    return Some(text.to_string());
                }
            }
        }
        None
    }
}

/// 取版本信息里的语言/代码页组合，读不到就用英文(美国)兜底。
unsafe fn translations(block: *const c_void) -> Vec<(u16, u16)> {
    let mut out = Vec::new();
    let sub: Vec<u16> = "\\VarFileInfo\\Translation"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let mut ptr: *mut c_void = std::ptr::null_mut();
    let mut len = 0u32;
    if VerQueryValueW(block, PCWSTR(sub.as_ptr()), &mut ptr, &mut len).as_bool()
        && !ptr.is_null()
        && len >= 4
    {
        // Translation 块的 len 是字节数，每项 2 个 WORD
        let count = (len as usize) / 4;
        let arr = std::slice::from_raw_parts(ptr as *const u16, count * 2);
        for i in 0..count {
            out.push((arr[i * 2], arr[i * 2 + 1]));
        }
    }
    if out.is_empty() {
        out.push((0x0409, 0x04b0));
    }
    out
}

unsafe fn query_string(block: *const c_void, sub: &str) -> Option<String> {
    let sub_w: Vec<u16> = sub.encode_utf16().chain(std::iter::once(0)).collect();
    let mut ptr: *mut c_void = std::ptr::null_mut();
    let mut len = 0u32;
    if !VerQueryValueW(block, PCWSTR(sub_w.as_ptr()), &mut ptr, &mut len).as_bool()
        || ptr.is_null()
        || len == 0
    {
        return None;
    }
    // 字符串的 len 是 WCHAR 个数
    let chars = std::slice::from_raw_parts(ptr as *const u16, len as usize);
    let end = chars.iter().position(|c| *c == 0).unwrap_or(chars.len());
    Some(String::from_utf16_lossy(&chars[..end]))
}

fn extract(exe_path: &str) -> Option<String> {
    let wide: Vec<u16> = exe_path.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let mut icon = HICON::default();
        let extracted = ExtractIconExW(PCWSTR(wide.as_ptr()), 0, Some(&mut icon), None, 1);
        if extracted == 0 || icon.0.is_null() {
            return None;
        }
        let url = hicon_to_data_url(icon);
        let _ = DestroyIcon(icon);
        url
    }
}

unsafe fn hicon_to_data_url(icon: HICON) -> Option<String> {
    let mut info = ICONINFO::default();
    if GetIconInfo(icon, &mut info).is_err() {
        return None;
    }
    let color: HBITMAP = info.hbmColor;
    let mask: HBITMAP = info.hbmMask;

    // 彩色位图为空说明是单色图标，直接跳过（极少见）
    let url = if color.0.is_null() {
        None
    } else {
        bitmap_to_png(color)
    };

    if !color.0.is_null() {
        let _ = DeleteObject(HGDIOBJ(color.0));
    }
    if !mask.0.is_null() {
        let _ = DeleteObject(HGDIOBJ(mask.0));
    }
    url
}

unsafe fn bitmap_to_png(hbm: HBITMAP) -> Option<String> {
    let mut bmp = BITMAP::default();
    let got = GetObjectW(
        HGDIOBJ(hbm.0),
        size_of::<BITMAP>() as i32,
        Some(&mut bmp as *mut BITMAP as *mut _),
    );
    if got == 0 || bmp.bmWidth <= 0 || bmp.bmHeight <= 0 {
        return None;
    }
    let width = bmp.bmWidth;
    let height = bmp.bmHeight;

    let mut bi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            // 负数高度 = 顶向下，省一次逐行翻转
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut buf = vec![0u8; (width as usize) * (height as usize) * 4];
    let hdc = GetDC(None);
    if hdc.0.is_null() {
        return None;
    }
    let lines = GetDIBits(
        hdc,
        hbm,
        0,
        height as u32,
        Some(buf.as_mut_ptr() as *mut _),
        &mut bi,
        DIB_RGB_COLORS,
    );
    ReleaseDC(None, hdc);
    if lines == 0 {
        return None;
    }

    // DIB 是 BGRA（顶向下），换成 RGBA；若整幅图 alpha 全 0 则视为不透明
    let mut rgba = Vec::with_capacity(buf.len());
    let mut any_alpha = false;
    for px in buf.chunks_exact(4) {
        if px[3] != 0 {
            any_alpha = true;
        }
        rgba.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
    }
    if !any_alpha {
        for px in rgba.chunks_exact_mut(4) {
            px[3] = 255;
        }
    }

    let mut png_bytes: Vec<u8> = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png_bytes, width as u32, height as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(&rgba).ok()?;
    }

    Some(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&png_bytes)
    ))
}
