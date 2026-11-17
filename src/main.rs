// looks-like-youtube — 純 Win32 + GDI+ 實作
//
// 全透明、永遠置頂、滑鼠點穿的覆蓋層，在畫面頂端畫假 YouTube 標題、
// 底端畫假直播控制列。中間留空，真實遊戲畫面直接透出（不擷取、不合成、不縮放）。
//
// 關鍵：WS_EX_LAYERED + WS_EX_TRANSPARENT 讓滑鼠事件整個穿透，
// WS_EX_NOACTIVATE 保證永遠不搶遊戲焦點。繪製走 GDI+ → 32bpp PARGB DIB →
// UpdateLayeredWindow，沒有任何子視窗、沒有瀏覽器引擎。
//
// 視窗從頭到尾不隱藏，開關只是換一張（透明的）點陣圖。
// 這樣可以完全避開「反覆 hide/show 讓合成層重建而變黑」的老問題。

#![windows_subsystem = "windows"]

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use std::ptr;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, GetMonitorInfoW,
    MonitorFromPoint, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    BLENDFUNCTION, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ, MONITORINFO, MONITOR_DEFAULTTOPRIMARY,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN,
    VK_DOWN, VK_ESCAPE, VK_RETURN, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, LoadCursorW, RegisterClassExW,
    SetTimer, SetWindowPos, TranslateMessage, UpdateLayeredWindow, CW_USEDEFAULT, HWND_TOPMOST,
    IDC_ARROW, MSG, SWP_NOACTIVATE, SWP_SHOWWINDOW, ULW_ALPHA, WM_DESTROY, WM_DISPLAYCHANGE,
    WM_HOTKEY, WM_TIMER, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP, WS_VISIBLE,
};


// ---- GDI+ 直接 FFI（省掉 windows crate 的型別瑣碎） ----
#[link(name = "gdiplus")]
extern "system" {
    fn GdiplusStartup(token: *mut usize, input: *const GdiplusStartupInput, output: *mut core::ffi::c_void) -> i32;
    fn GdiplusShutdown(token: usize);
    fn GdipCreateBitmapFromScan0(width: i32, height: i32, stride: i32, format: i32, scan0: *mut u32, bitmap: *mut *mut core::ffi::c_void) -> i32;
    fn GdipGetImageGraphicsContext(image: *mut core::ffi::c_void, g: *mut *mut core::ffi::c_void) -> i32;
    fn GdipDisposeImage(image: *mut core::ffi::c_void) -> i32;
    fn GdipDeleteGraphics(g: *mut core::ffi::c_void) -> i32;
    fn GdipGraphicsClear(g: *mut core::ffi::c_void, color: u32) -> i32;
    fn GdipSetTextRenderingHint(g: *mut core::ffi::c_void, hint: i32) -> i32;
    fn GdipSetSmoothingMode(g: *mut core::ffi::c_void, mode: i32) -> i32;
    fn GdipSetPixelOffsetMode(g: *mut core::ffi::c_void, mode: i32) -> i32;
    fn GdipCreateSolidFill(color: u32, brush: *mut *mut core::ffi::c_void) -> i32;
    fn GdipDeleteBrush(brush: *mut core::ffi::c_void) -> i32;
    fn GdipFillRectangleI(g: *mut core::ffi::c_void, brush: *mut core::ffi::c_void, x: i32, y: i32, w: i32, h: i32) -> i32;
    fn GdipFillEllipse(g: *mut core::ffi::c_void, brush: *mut core::ffi::c_void, x: f32, y: f32, w: f32, h: f32) -> i32;
    fn GdipCreateFontFamilyFromName(name: *const u16, collection: *mut core::ffi::c_void, fam: *mut *mut core::ffi::c_void) -> i32;
    fn GdipDeleteFontFamily(fam: *mut core::ffi::c_void) -> i32;
    fn GdipCreateFont(fam: *mut core::ffi::c_void, size: f32, style: i32, unit: i32, font: *mut *mut core::ffi::c_void) -> i32;
    fn GdipDeleteFont(font: *mut core::ffi::c_void) -> i32;
    fn GdipDrawString(g: *mut core::ffi::c_void, text: *const u16, len: i32, font: *mut core::ffi::c_void, layout: *const RectF, format: *mut core::ffi::c_void, brush: *mut core::ffi::c_void) -> i32;
    // 路徑：圖示用實際輪廓畫，才不會像方塊拼貼
    fn GdipCreatePath(fill_mode: i32, path: *mut *mut core::ffi::c_void) -> i32;
    fn GdipDeletePath(path: *mut core::ffi::c_void) -> i32;
    fn GdipAddPathRectangle(path: *mut core::ffi::c_void, x: f32, y: f32, w: f32, h: f32) -> i32;
    fn GdipAddPathEllipse(path: *mut core::ffi::c_void, x: f32, y: f32, w: f32, h: f32) -> i32;
    fn GdipAddPathLine(path: *mut core::ffi::c_void, x1: f32, y1: f32, x2: f32, y2: f32) -> i32;
    fn GdipStartPathFigure(path: *mut core::ffi::c_void) -> i32;
    fn GdipClosePathFigure(path: *mut core::ffi::c_void) -> i32;
    fn GdipFillPath(g: *mut core::ffi::c_void, brush: *mut core::ffi::c_void, path: *mut core::ffi::c_void) -> i32;
    fn GdipDrawPath(g: *mut core::ffi::c_void, pen: *mut core::ffi::c_void, path: *mut core::ffi::c_void) -> i32;
    fn GdipCreatePen1(color: u32, width: f32, unit: i32, pen: *mut *mut core::ffi::c_void) -> i32;
    fn GdipDeletePen(pen: *mut core::ffi::c_void) -> i32;
    fn GdipSetPenStartCap(pen: *mut core::ffi::c_void, cap: i32) -> i32;
    fn GdipSetPenEndCap(pen: *mut core::ffi::c_void, cap: i32) -> i32;
    fn GdipSetPenLineJoin(pen: *mut core::ffi::c_void, join: i32) -> i32;
}

#[repr(C)]
#[derive(Default)]
struct GdiplusStartupInput {
    version: u32,
    callback: *mut core::ffi::c_void,
    callback_arg: *mut core::ffi::c_void,
    suppress_bg: i32,
    suppress_ext: i32,
}

#[repr(C)]
struct RectF { x: f32, y: f32, w: f32, h: f32 }

const HINT_ANTIALIAS_GRIDFIT: i32 = 3;
// PixelFormat32bppPARGB — 預乘 alpha，正好是 UpdateLayeredWindow(AC_SRC_ALPHA) 要的格式
const PIXFMT_PARGB: i32 = 0x000E_200B;
const SMOOTH_ANTIALIAS: i32 = 4;
const PIXEL_OFFSET_HALF: i32 = 4;
// GpUnit: UnitWorld=0, UnitDisplay=1, UnitPixel=2, UnitPoint=3 …（很容易寫錯成 3）
const UNIT_PIXEL: i32 = 2;
const STYLE_BOLD: i32 = 1;
const FILL_ALTERNATE: i32 = 0;
const FILL_WINDING: i32 = 1;
const CAP_ROUND: i32 = 2;
const JOIN_ROUND: i32 = 2;

type Gp = *mut core::ffi::c_void;

const DESIGN_W: i32 = 1920;
const DESIGN_H: i32 = 1080;
const HOTKEY_TOGGLE: i32 = 1;
const HOTKEY_CALIBRATE: i32 = 2;
const HOTKEY_UP: i32 = 3;
const HOTKEY_DOWN: i32 = 4;
const HOTKEY_SAVE: i32 = 5;
const HOTKEY_CANCEL: i32 = 6;
const TIMER_TICK: usize = 1;
const DEFAULT_TOGGLE: &str = "Ctrl+Alt+Shift+Y";
const DEFAULT_CALIBRATE: &str = "Ctrl+Alt+Shift+K";
// 假頻道名稱，顯示在「訂閱」提示框上方（等同 YouTube 的頻道名 tooltip）
const CHANNEL: &str = "PETER";

#[derive(Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Config {
    title: String,
    label: String,
    show_title: bool,
    bar_bottom: i32,
    toggle_hotkey: String,
    calibrate_hotkey: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            title: "《魔獸世界：永恆》開服不到兩週！暴雪緊急改規則？地城練等、陣營限制一次看！　#魔獸世界永恆 #魔獸世界".into(),
            label: "直播中".into(),
            show_title: true,
            // 貼齊畫面下緣（YouTube 全螢幕就是這樣），不會壓到快捷鍵列
            bar_bottom: 6,
            toggle_hotkey: DEFAULT_TOGGLE.into(),
            calibrate_hotkey: DEFAULT_CALIBRATE.into(),
        }
    }
}

fn config_path() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    let mut p = PathBuf::from(appdata);
    p.push("looks-like-youtube");
    p.push("config.json");
    Some(p)
}

fn load_config() -> Config {
    let Some(p) = config_path() else { return Config::default() };
    match std::fs::read_to_string(&p) {
        Ok(t) => serde_json::from_str(&t).unwrap_or_default(),
        Err(_) => Config::default(),
    }
}

fn save_config(cfg: &Config) {
    if let Some(p) = config_path() {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(t) = serde_json::to_string_pretty(cfg) {
            let _ = std::fs::write(p, t);
        }
    }
}

unsafe fn wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(Some(0)).collect()
}

// ---------- 熱鍵字串（沿用 Electron 的 "Control+Alt+Shift+Y" 格式） ----------

fn vk_from(name: &str) -> Option<u32> {
    let up = name.to_ascii_uppercase();
    let b = up.as_bytes();
    if b.len() == 1 {
        let c = b[0];
        if c.is_ascii_uppercase() || c.is_ascii_digit() {
            return Some(c as u32);
        }
    }
    if let Some(n) = up.strip_prefix('F') {
        if let Ok(n) = n.parse::<u32>() {
            if (1..=24).contains(&n) {
                return Some(0x70 + n - 1); // VK_F1..VK_F24
            }
        }
    }
    Some(match up.as_str() {
        "SPACE" => 0x20,
        "TAB" => 0x09,
        "BACKSPACE" => 0x08,
        "ENTER" | "RETURN" => 0x0D,
        "INSERT" => 0x2D,
        "DELETE" | "DEL" => 0x2E,
        "HOME" => 0x24,
        "END" => 0x23,
        "PAGEUP" => 0x21,
        "PAGEDOWN" => 0x22,
        "LEFT" => 0x25,
        "RIGHT" => 0x27,
        "UP" => 0x26,
        "DOWN" => 0x28,
        "NUMLOCK" => 0x90,
        "SCROLLLOCK" => 0x91,
        "PAUSE" => 0x13,
        "," => 0xBC,
        "." => 0xBE,
        "/" => 0xBF,
        ";" => 0xBA,
        "'" => 0xDE,
        "[" => 0xDB,
        "]" => 0xDD,
        "\\" => 0xDC,
        "-" => 0xBD,
        "=" => 0xBB,
        "`" => 0xC0,
        _ => return None,
    })
}

/// 解析 "Ctrl+Alt+Shift+Y" → (修飾鍵, 虛擬鍵碼)；修飾鍵不可為空。
fn parse_hotkey(s: &str) -> Option<(HOT_KEY_MODIFIERS, u32)> {
    let mut mods = 0u32;
    let mut vk = None;
    for part in s.split('+') {
        let p = part.trim();
        if p.is_empty() {
            continue;
        }
        match p.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => mods |= MOD_CONTROL.0,
            "alt" => mods |= MOD_ALT.0,
            "shift" => mods |= MOD_SHIFT.0,
            "win" | "super" | "cmd" | "meta" => mods |= MOD_WIN.0,
            other => vk = vk_from(other),
        }
    }
    let vk = vk?;
    if mods == 0 {
        return None;
    }
    Some((HOT_KEY_MODIFIERS(mods), vk))
}

struct App {
    hwnd: HWND,
    cfg: Config,
    armed: bool,
    calibrating: bool,
    cal_value: i32,
    cal_original: i32,
    width: i32,
    height: i32,
    origin_x: i32,
    origin_y: i32,
    cur: i32,
    family_name: String,
    // 全螢幕點陣圖很貴（14.7 MB），重用一張，只有尺寸變了才重建
    canvas: Option<Canvas>,
}

impl App {
    fn scale(&self) -> f32 {
        (self.width as f32 / DESIGN_W as f32).min(self.height as f32 / DESIGN_H as f32)
    }
    fn sx(&self, v: i32) -> f32 {
        v as f32 * self.scale()
    }
}

static APP: Mutex<Option<usize>> = Mutex::new(None);

fn app() -> &'static mut App {
    let g = APP.lock().unwrap();
    unsafe { &mut *(g.unwrap() as *mut App) }
}

// ---------- 繪製 ----------

struct Canvas {
    mem_dc: HDC,
    bmp: HBITMAP,
    old: HGDIOBJ,
    bits: *mut u32,
    w: i32,
    h: i32,
}

impl Canvas {
    unsafe fn new(w: i32, h: i32) -> Option<Self> {
        let screen = GetDC(None);
        let mem_dc = CreateCompatibleDC(Some(screen));
        ReleaseDC(None, screen);
        let mut bi = BITMAPINFO::default();
        bi.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h, // top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        };
        let mut bits: *mut core::ffi::c_void = ptr::null_mut();
        let bmp = CreateDIBSection(Some(mem_dc), &bi, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
        let old = SelectObject(mem_dc, HGDIOBJ(bmp.0));
        Some(Self { mem_dc, bmp, old, bits: bits as *mut u32, w, h })
    }
}

impl Drop for Canvas {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.mem_dc, self.old);
            let _ = DeleteObject(HGDIOBJ(self.bmp.0));
            let _ = DeleteDC(self.mem_dc);
        }
    }
}

unsafe fn fill_rect(g: Gp, color: u32, x: f32, y: f32, w: f32, h: f32) {
    let mut brush: Gp = ptr::null_mut();
    if GdipCreateSolidFill(color, &mut brush) != 0 {
        return;
    }
    GdipFillRectangleI(g, brush, x.round() as i32, y.round() as i32, w.round() as i32, h.round() as i32);
    let _ = GdipDeleteBrush(brush);
}

unsafe fn fill_round(g: Gp, color: u32, x: f32, y: f32, w: f32, h: f32, r: f32) {
    // 用兩條矩形疊成十字 + 四角圓，union 起來就是圓角矩形。
    // 比 AddPathArc 可靠：弧線會自動從目前點拉線，接錯就會出現交叉黑 X。
    let r = r.min(w / 2.0).min(h / 2.0);
    fill_rect(g, color, x + r, y, w - r * 2.0, h);
    fill_rect(g, color, x, y + r, w, h - r * 2.0);
    for (cx, cy) in [(x + r, y + r), (x + w - r, y + r), (x + r, y + h - r), (x + w - r, y + h - r)] {
        fill_ellipse(g, color, cx, cy, r);
    }
}

unsafe fn fill_ellipse(g: Gp, color: u32, cx: f32, cy: f32, r: f32) {
    let mut brush: Gp = ptr::null_mut();
    if GdipCreateSolidFill(color, &mut brush) != 0 {
        return;
    }
    GdipFillEllipse(g, brush, cx - r, cy - r, r * 2.0, r * 2.0);
    let _ = GdipDeleteBrush(brush);
}

unsafe fn fill_tri(g: Gp, color: u32, x: f32, y: f32, w: f32, h: f32) {
    // 用一串漸漸變窄的矩形硬拼出實心三角形，省掉 path 的麻煩
    let mut brush: Gp = ptr::null_mut();
    if GdipCreateSolidFill(color, &mut brush) != 0 {
        return;
    }
    let steps = 24;
    for i in 0..steps {
        let t = i as f32 / steps as f32;
        let hh = h / steps as f32;
        let ww = w * (1.0 - t);
        GdipFillRectangleI(g, brush, x.round() as i32, (y + t * h).round() as i32, ww.round() as i32, hh.ceil() as i32);
    }
    let _ = GdipDeleteBrush(brush);
}

// ---- 路徑輔助 ----

unsafe fn new_path() -> Gp {
    new_path_mode(FILL_WINDING)
}

/// FILL_ALTERNATE 搭配同心圖形可挖洞（畫圓環用）。
unsafe fn new_path_mode(mode: i32) -> Gp {
    let mut p: Gp = ptr::null_mut();
    if GdipCreatePath(mode, &mut p) != 0 {
        return ptr::null_mut();
    }
    p
}

unsafe fn path_rect(p: Gp, x: f32, y: f32, w: f32, h: f32) {
    GdipAddPathRectangle(p, x, y, w, h);
}

unsafe fn path_ellipse(p: Gp, x: f32, y: f32, w: f32, h: f32) {
    GdipAddPathEllipse(p, x, y, w, h);
}

unsafe fn path_round(p: Gp, x: f32, y: f32, w: f32, h: f32, r: f32) {
    // 兩矩形 + 四角圓；這裡在「單一路徑」內疊加，之後可以一次填滿或描邊
    let r = r.min(w / 2.0).min(h / 2.0);
    path_rect(p, x + r, y, w - r * 2.0, h);
    path_rect(p, x, y + r, w, h - r * 2.0);
    for (cx, cy) in [(x + r, y + r), (x + w - r, y + r), (x + r, y + h - r), (x + w - r, y + h - r)] {
        path_ellipse(p, cx - r, cy - r, r * 2.0, r * 2.0);
    }
}

unsafe fn path_line(p: Gp, x1: f32, y1: f32, x2: f32, y2: f32) {
    GdipAddPathLine(p, x1, y1, x2, y2);
}

unsafe fn path_close(p: Gp) {
    let _ = GdipClosePathFigure(p);
}

unsafe fn path_fill(g: Gp, p: Gp, color: u32) {
    if p.is_null() {
        return;
    }
    let mut brush: Gp = ptr::null_mut();
    if GdipCreateSolidFill(color, &mut brush) == 0 {
        GdipFillPath(g, brush, p);
        let _ = GdipDeleteBrush(brush);
    }
}

unsafe fn path_stroke(g: Gp, p: Gp, color: u32, width: f32) {
    if p.is_null() {
        return;
    }
    let mut pen: Gp = ptr::null_mut();
    if GdipCreatePen1(color, width, UNIT_PIXEL, &mut pen) != 0 {
        return;
    }
    GdipSetPenStartCap(pen, CAP_ROUND);
    GdipSetPenEndCap(pen, CAP_ROUND);
    GdipSetPenLineJoin(pen, JOIN_ROUND);
    GdipDrawPath(g, pen, p);
    let _ = GdipDeletePen(pen);
}

unsafe fn make_font(app: &App, size_px: f32, bold: bool) -> Gp {
    let name = wide(&app.family_name);
    let mut fam: Gp = ptr::null_mut();
    if GdipCreateFontFamilyFromName(name.as_ptr(), ptr::null_mut(), &mut fam) != 0 {
        return ptr::null_mut();
    }
    let mut font: Gp = ptr::null_mut();
    let style = if bold { STYLE_BOLD } else { 0 };
    let r = GdipCreateFont(fam, size_px, style, UNIT_PIXEL, &mut font);
    let _ = GdipDeleteFontFamily(fam);
    if r != 0 {
        ptr::null_mut()
    } else {
        font
    }
}

unsafe fn draw_text(
    g: Gp,
    font: Gp,
    color: u32,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    text: &str,
    shadow: Option<&[(f32, f32)]>,
) {
    if font.is_null() || text.is_empty() {
        return;
    }
    let mut brush: Gp = ptr::null_mut();
    if GdipCreateSolidFill(color, &mut brush) != 0 {
        return;
    }
    let utf16: Vec<u16> = text.encode_utf16().collect();
    let n = utf16.len() as i32;
    let rc = RectF { x, y, w, h };

    if let Some(offsets) = shadow {
        let mut sh: Gp = ptr::null_mut();
        if GdipCreateSolidFill(0xB000_0000, &mut sh) == 0 {
            for (dx, dy) in offsets {
                let rc2 = RectF { x: x + dx, y: y + dy, w, h };
                GdipDrawString(g, utf16.as_ptr(), n, font, &rc2, ptr::null_mut(), sh);
            }
            let _ = GdipDeleteBrush(sh);
        }
    }
    GdipDrawString(g, utf16.as_ptr(), n, font, &rc, ptr::null_mut(), brush);
    let _ = GdipDeleteBrush(brush);
}

/// 畫右側工具列的一個圖示。`x` / `y` 是圖示方框左上角，方框固定 20×20 設計像素。
/// 幾何全部沿用 YouTube DOM 的原始路徑座標（viewBox 24×24），
/// 只是換成 GDI+ 的填色/描邊，縮小到 20 並內縮 2 對齊視覺重心。
unsafe fn draw_icon(g: Gp, kind: u8, x0: f32, y0: f32, s: f32) {
    let stroke = (1.8 * s).max(1.0);
    let white = 0xFFFF_FFFF;
    // 24×24 viewBox 畫在 20 的框裡：整體 ×(20/24) 並內縮 2
    let k = 20.0 / 24.0;
    let x = |v: f32| x0 + s * (2.0 + v * k);
    let y = |v: f32| y0 + s * (2.0 + v * k);
    let len = |v: f32| v * s * k;

    let p = new_path();
    if p.is_null() {
        return;
    }

    match kind {
        // 讚（實心：袖口 + 手掌 + 拇指）
        0 => {
            path_rect(p, x(3.0), y(10.0), len(4.0), len(11.0));
            path_round(p, x(8.0), y(9.0), len(12.0), len(12.0), len(2.0));
            path_fill(g, p, white);
            let t = new_path();
            path_line(t, x(8.0), y(9.6), x(12.0), y(2.6));
            path_line(t, x(12.0), y(2.6), x(15.2), y(4.2));
            path_line(t, x(15.2), y(4.2), x(13.6), y(9.6));
            path_close(t);
            path_fill(g, t, white);
            let _ = GdipDeletePath(t);
        }
        // 爛（把讚左右鏡射，同樣實心；描邊在小尺寸下會糊成一團）
        1 => {
            let m = |v: f32| x(24.0 - v);
            path_rect(p, m(9.0), y(10.0), len(6.0), len(11.0));
            path_round(p, m(20.0), y(9.4), len(11.0), len(11.6), len(2.0));
            path_fill(g, p, white);
            let t = new_path();
            path_line(t, m(9.0), y(9.6), m(13.0), y(3.0));
            path_line(t, m(13.0), y(3.0), m(15.5), y(4.4));
            path_line(t, m(15.5), y(4.4), m(14.0), y(9.6));
            path_close(t);
            path_fill(g, t, white);
            let _ = GdipDeletePath(t);
        }
        // 分享（斜箭頭）
        2 => {
            path_line(p, x(6.0), y(20.0), x(20.0), y(4.0));
            path_stroke(g, p, white, stroke);
            let t = new_path();
            path_line(t, x(11.0), y(4.0), x(20.0), y(4.0));
            path_line(t, x(20.0), y(4.0), x(20.0), y(13.0));
            path_stroke(g, t, white, stroke);
            let _ = GdipDeletePath(t);
        }
        // 留言（後方大方框 + 前方疊上去的方框，同 YouTube 的雙層輪廓）
        3 => {
            let t = new_path();
            path_rect(t, x(4.0), y(8.0), len(12.0), len(12.0));
            path_stroke(g, t, white, stroke);
            let _ = GdipDeletePath(t);
            path_rect(p, x(8.0), y(4.0), len(12.0), len(12.0));
            path_stroke(g, p, white, stroke);
        }
        // 三點
        4 => {
            for cx in [5.0, 12.0, 19.0] {
                path_ellipse(p, x(cx) - len(1.8), y(10.2), len(3.6), len(3.6));
            }
            path_fill(g, p, white);
        }
        // 齒輪：一圈 12 齒的環（多邊形輪廓）+ alternate 挖掉中心孔，最後描邊
        5 => {
            let t = new_path_mode(FILL_ALTERNATE);
            let cx = x(12.0);
            let cy = y(12.0);
            let teeth = 12;
            let r_out = len(10.0);
            let r_in = len(7.0);
            let half = std::f32::consts::PI / teeth as f32 * 0.5;
            // 建齒輪外輪廓：每齒四個點（外外內內）交替，整圈連成一條折線
            let mut prev: Option<(f32, f32)> = None;
            for i in 0..teeth {
                let a = i as f32 * 2.0 * std::f32::consts::PI / teeth as f32;
                for (r, da) in [(r_out, -half), (r_out, half), (r_in, half), (r_in, -half)] {
                    let ang = a + da;
                    let px = cx + r * ang.cos();
                    let py = cy + r * ang.sin();
                    match prev {
                        Some((qx, qy)) => {
                            path_line(t, qx, qy, px, py);
                        }
                        None => {
                            let _ = GdipStartPathFigure(t);
                        }
                    }
                    prev = Some((px, py));
                }
            }
            if let Some((qx, qy)) = prev {
                // 收口回起點
                let a0 = -half;
                path_line(t, qx, qy, cx + r_out * a0.cos(), cy + r_out * a0.sin());
            }
            path_close(t);
            // 內孔（alternate 會挖空）
            path_ellipse(t, cx - len(3.4), cy - len(3.4), len(6.8), len(6.8));
            path_fill(g, t, white);
            path_stroke(g, t, white, (1.0 * s).max(0.8));
            let _ = GdipDeletePath(t);
        }
        // 子母畫面（外框，右下角再疊一個實心小方框；小框要壓在外框線上）
        6 => {
            path_round(p, x(3.0), y(5.0), len(18.0), len(14.0), len(2.0));
            path_stroke(g, p, white, stroke);
            let t = new_path();
            path_rect(t, x(11.0), y(10.5), len(7.5), len(6.5));
            path_fill(g, t, white);
            let _ = GdipDeletePath(t);
        }
        // 字幕（外框 + 兩條短線；線用填色矩形畫，避免描邊連線）
        7 => {
            path_round(p, x(3.0), y(5.0), len(18.0), len(14.0), len(2.0));
            path_stroke(g, p, white, stroke);
            let th = (1.8 * s).max(1.4);
            fill_rect(g, white, x(6.5), y(9.8), len(5.0), th);
            fill_rect(g, white, x(6.5), y(13.6), len(8.5), th);
        }
        // 全螢幕（四角括號；四段要各自成圖，否則線會跨角連成閃電）
        8 => {
            let corners = [
                (4.0, 4.0, 4.0, 10.0, 10.0, 4.0),   // 左上
                (20.0, 4.0, 20.0, 10.0, 14.0, 4.0), // 右上
                (4.0, 20.0, 4.0, 14.0, 10.0, 20.0), // 左下
                (20.0, 20.0, 20.0, 14.0, 14.0, 20.0), // 右下
            ];
            for (ax, ay, bx, by, cx, cy) in corners {
                let t = new_path();
                path_line(t, x(ax), y(ay), x(bx), y(by));
                path_line(t, x(ax), y(ay), x(cx), y(cy));
                path_stroke(g, t, white, stroke);
                let _ = GdipDeletePath(t);
            }
        }
        _ => {}
    }
    let _ = GdipDeletePath(p);
}

unsafe fn render(app: &mut App) {
    let (w, h) = (app.width, app.height);
    // 尺寸不合就重建；否則沿用，省掉每次 14.7 MB 的配置
    let need_new = match &app.canvas {
        Some(c) => c.w != w || c.h != h,
        None => true,
    };
    if need_new {
        app.canvas = Canvas::new(w, h);
    }
    // 取出來當區域變數，免得借用 app 綁死後續的 &mut 操作
    let Some(canvas) = app.canvas.take() else { return };

    // 未啟動且非校準時：整張透明，只送一次更新就結束
    if !app.armed && !app.calibrating {
        // 點陣圖是重複使用的，不清空就會把上一次的內容原樣送出去（變成關不掉）。
        // 注意 bits 是 *mut u32，筆數是像素數，不是位元組數。
        std::slice::from_raw_parts_mut(canvas.bits, (w * h) as usize).fill(0);
        present(app, &canvas);
        app.canvas = Some(canvas);
        return;
    }

    let mut gbm: Gp = ptr::null_mut();
    let ok = GdipCreateBitmapFromScan0(w, h, w * 4, PIXFMT_PARGB, canvas.bits, &mut gbm) == 0;
    let mut g: Gp = ptr::null_mut();
    let ok = ok && GdipGetImageGraphicsContext(gbm, &mut g) == 0;
    if !ok {
        if !gbm.is_null() {
            let _ = GdipDisposeImage(gbm);
        }
        app.canvas = Some(canvas);
        return;
    }
    GdipSetTextRenderingHint(g, HINT_ANTIALIAS_GRIDFIT);
    GdipSetSmoothingMode(g, SMOOTH_ANTIALIAS);
    GdipSetPixelOffsetMode(g, PIXEL_OFFSET_HALF);
    GdipGraphicsClear(g, 0x00000000);

    let s = app.scale();

    // 頂部淡漸層（讓白字在亮背景讀得到）
    if app.cfg.show_title {
        let bands = 22;
        for i in 0..bands {
            let t = i as f32 / bands as f32;
            let a = (0.58 * (1.0 - t).powf(1.6) * 255.0) as u32;
            if a == 0 {
                continue;
            }
            let color = (a << 24) | 0x0010_1010;
            fill_rect(g, color, 0.0, t * 56.0 * s, w as f32, (56.0 / bands as f32 * s).ceil());
        }
    }

    // 右上角「譯：關」小標記（左上與「有新版」都不畫）
    let chip = make_font(app, (14.0 * s).max(10.0), false);
    let cw = app.sx(84);
    let cx = w as f32 - app.sx(20) - cw;
    fill_round(g, 0xD228_2828, cx, app.sx(14), cw, app.sx(24), app.sx(3));
    draw_text(g, chip, 0xFFFF_FFFF, cx + app.sx(8), app.sx(17), cw - app.sx(16), app.sx(24), "譯：關", None);

    // 標題
    if app.cfg.show_title {
        let tf = make_font(app, (23.0 * s).max(12.0), true);
        draw_text(
            g,
            tf,
            0xFFFF_FFFF,
            app.sx(40),
            app.sx(9),
            w as f32 - app.sx(210),
            app.sx(34),
            &app.cfg.title.clone(),
            Some(&[(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 2.0)]),
        );
        let _ = GdipDeleteFont(tf);
    }

    // 底部控制列。工作列若在畫面上（視窗化／桌面），控制列要讓開，
    // 取「使用者設定」與「工作列高度」較大者；遊戲全螢幕時工作列不在，就照設定值。
    let bar_h = app.sx(46);
    let reserve = app.sx(app.cfg.bar_bottom).max(taskbar_inset() as f32);
    let bar_top = h as f32 - reserve - bar_h;
    let track_y = bar_top;
    // YouTube 的按鈕列貼齊控制列底部（46 高的條：上面 4 是進度條，下面 34 是按鈕列）
    let row_y = bar_top + app.sx(12);
    let icon_y = row_y + app.sx(7);

    // 控制列背景漸層（由下往上淡出）
    let bands = 26;
    for i in 0..bands {
        let t = i as f32 / bands as f32;
        let a = (0.80 * (1.0 - t).powf(1.5) * 255.0) as u32;
        if a > 2 {
            let color = (a << 24) | 0x0008_0808;
            fill_rect(g, color, 0.0, bar_top + t * bar_h, w as f32, (bar_h / bands as f32).ceil() + 1.0);
        }
    }

    // 進度條（直播：接近尾端、極慢往前爬）
    let track_w = w as f32;
    let track_th = (4.0 * s).max(2.0);
    fill_rect(g, 0x60FF_FFFF, 0.0, track_y, track_w, track_th);
    let fill_x = track_w * (app.cur as f32 / 100.0);
    fill_rect(g, 0xFFFF_0000, 0.0, track_y, fill_x, track_th);
    let knob_r = (7.0 * s).max(4.0);
    fill_ellipse(g, 0xFFFF_0000, fill_x, track_y + track_th / 2.0, knob_r);

    // 左側：播放 / 音量 / 直播標籤
    // 播放鍵：向右的實心三角（尖端朝右）
    {
        let (px, py, pw, ph) = (app.sx(16), icon_y + app.sx(3), app.sx(12), app.sx(15));
        let t = new_path();
        path_line(t, px, py, px, py + ph);
        path_line(t, px, py + ph, px + pw, py + ph / 2.0);
        path_close(t);
        path_fill(g, t, 0xFFFF_FFFF);
        let _ = GdipDeletePath(t);
    }
    // 音量：喇叭身 + 朝右三角
    {
        let (vx, vy) = (app.sx(42), icon_y + app.sx(6));
        fill_rect(g, 0xFFFF_FFFF, vx, vy + app.sx(3), app.sx(4), app.sx(8));
        let t = new_path();
        path_line(t, vx + app.sx(4), vy + app.sx(3), vx + app.sx(11), vy);
        path_line(t, vx + app.sx(11), vy, vx + app.sx(11), vy + app.sx(14));
        path_line(t, vx + app.sx(11), vy + app.sx(14), vx + app.sx(4), vy + app.sx(11));
        path_close(t);
        path_fill(g, t, 0xFFFF_FFFF);
        let _ = GdipDeletePath(t);
    }
    {
        let label = app.cfg.label.clone();
        let lf = make_font(app, (13.0 * s).max(9.0), false);
        let text_w = label.chars().count() as f32 * 13.0 * s + app.sx(30);
        fill_round(g, 0xE014_1414, app.sx(62), icon_y + app.sx(4), text_w, app.sx(22), app.sx(11));
        fill_ellipse(g, 0xFFFF_FFFF, app.sx(74), icon_y + app.sx(15), (3.2 * s).max(2.0));
        draw_text(g, lf, 0xFFFF_FFFF, app.sx(81), icon_y + app.sx(7), text_w, app.sx(22), &label, None);
        let _ = GdipDeleteFont(lf);
    }

    // 中央收合箭頭（朝下）
    let chev_y = bar_top - app.sx(10);
    fill_tri(g, 0xFFFF_FFFF, (w as f32 / 2.0) - app.sx(7), chev_y, app.sx(14), app.sx(-9));

    // 右側工具列，照 YouTube 的實際順序由左往右排：
    // 讚 爛 分享 留言 三點 [開關] 齒輪 子母畫面 字幕 全螢幕
    const ROW: [(u8, bool); 10] = [
        (0, false), (1, false), (2, false), (3, false), (4, false),
        (0, true), // true = 開關
        (5, false), (6, false), (7, false), (8, false),
    ];
    // 需要這麼多寬度；放不下就整體等比壓縮（4:3 或更窄的畫面會用到）
    let want: f32 = app.sx(32) * 9.0 + app.sx(44) + app.sx(9) * 9.0;
    let avail = w as f32 - app.sx(22) - app.sx(340);
    let f = (avail / want).clamp(0.55, 1.0);
    let cell = app.sx(32) * f;
    let gap = app.sx(9) * f;
    let toggle_w = app.sx(44) * f;
    let icon_size = app.sx(20) * f;
    let total: f32 = cell * 9.0 + toggle_w + gap * 9.0;
    let icon_s = s * f;
    let mut x = w as f32 - app.sx(22) - total;
    for (kind, is_toggle) in ROW {
        if is_toggle {
            fill_round(g, 0xFFE8_E8E8, x, icon_y + app.sx(3), toggle_w, app.sx(24), app.sx(12));
            fill_ellipse(g, 0xFF0B_57D0, x + toggle_w - app.sx(12) * f, icon_y + app.sx(15), (9.0 * icon_s / s).max(3.0));
            x += toggle_w + gap;
        } else {
            draw_icon(g, kind, x + (cell - icon_size) / 2.0, icon_y, icon_s);
            x += cell + gap;
        }
    }

    // 「訂閱」提示框（右上角，紅框 + 頻道名在上方，等同 YouTube 的 tooltip）
    {
        let sf = make_font(app, (14.0 * s).max(9.0), false);
        let nf = make_font(app, (11.0 * s).max(8.0), true);
        let bw = app.sx(44);
        let bh = app.sx(24);
        let bx = w as f32 - app.sx(18) - bw;
        let by = app.sx(52);
        draw_text(g, nf, 0xFFCC_0000, bx, by - app.sx(17), bw, app.sx(16), CHANNEL, None);
        fill_round(g, 0xFFCC_0000, bx, by, bw, bh, app.sx(3));
        draw_text(g, sf, 0xFFFF_FFFF, bx + app.sx(8), by + app.sx(3), bw, bh, "訂閱", None);
        let _ = GdipDeleteFont(sf);
        let _ = GdipDeleteFont(nf);
    }

    // 校準提示
    if app.calibrating {
        let hint_f = make_font(app, (22.0 * s).max(12.0), true);
        let small_f = make_font(app, (15.0 * s).max(10.0), false);
        let bw = app.sx(360);
        let bh = app.sx(110);
        let bx = (w as f32 - bw) / 2.0;
        let by = h as f32 * 0.38;
        fill_round(g, 0xF00F_0F0F, bx, by, bw, bh, app.sx(10));
        draw_text(g, hint_f, 0xFFFF_FFFF, bx + app.sx(24), by + app.sx(24), bw - app.sx(48), app.sx(34),
                  "↑ / ↓ 調整控制列位置", None);
        draw_text(g, small_f, 0xFFB0_B0B0, bx + app.sx(24), by + app.sx(66), bw - app.sx(48), app.sx(26),
                  "Enter 存檔　·　Esc 取消", None);
        let _ = GdipDeleteFont(hint_f);
        let _ = GdipDeleteFont(small_f);
        // 控制列外框提示
        fill_rect(g, 0xFFFF_5252, 0.0, bar_top - app.sx(1), w as f32, app.sx(1).max(1.0));
        fill_rect(g, 0xFFFF_5252, 0.0, bar_top + bar_h, w as f32, app.sx(1).max(1.0));
    }

    let _ = GdipDeleteFont(chip);
    GdipDeleteGraphics(g);
    let _ = GdipDisposeImage(gbm);
    present(app, &canvas);
    app.canvas = Some(canvas);
}

unsafe fn present(app: &App, canvas: &Canvas) {
    let screen = GetDC(None);
    let size = SIZE { cx: canvas.w, cy: canvas.h };
    let src = POINT { x: 0, y: 0 };
    let blend = BLENDFUNCTION {
        BlendOp: 0,
        BlendFlags: 0,
        SourceConstantAlpha: 255,
        AlphaFormat: 1, // AC_SRC_ALPHA
    };
    let _ = UpdateLayeredWindow(
        app.hwnd,
        Some(screen),
        Some(&POINT { x: app.origin_x, y: app.origin_y }),
        Some(&size),
        Some(canvas.mem_dc),
        Some(&src),
        COLORREF(0),
        Some(&blend),
        ULW_ALPHA,
    );
    ReleaseDC(None, screen);
}

fn redraw() {
    unsafe { render(app()) }
}

// ---------- 狀態操作 ----------

fn set_armed(on: bool) {
    let a = app();
    a.armed = on;
    redraw();
}

fn toggle() {
    let a = app();
    // 校準中按開關：先把位置存下來並結束校準，否則按鍵會被吃掉、看起來像關不掉
    if a.calibrating {
        stop_calibrate(true);
    }
    let next = !app().armed;
    set_armed(next);
}

fn register_cal_keys(a: &App) {
    unsafe {
        let _ = RegisterHotKey(Some(a.hwnd), HOTKEY_UP, HOT_KEY_MODIFIERS(0), VK_UP.0 as u32);
        let _ = RegisterHotKey(Some(a.hwnd), HOTKEY_DOWN, HOT_KEY_MODIFIERS(0), VK_DOWN.0 as u32);
        let _ = RegisterHotKey(Some(a.hwnd), HOTKEY_SAVE, HOT_KEY_MODIFIERS(0), VK_RETURN.0 as u32);
        let _ = RegisterHotKey(Some(a.hwnd), HOTKEY_CANCEL, HOT_KEY_MODIFIERS(0), VK_ESCAPE.0 as u32);
    }
}

fn unregister_cal_keys(a: &App) {
    unsafe {
        for id in [HOTKEY_UP, HOTKEY_DOWN, HOTKEY_SAVE, HOTKEY_CANCEL] {
            let _ = UnregisterHotKey(Some(a.hwnd), id);
        }
    }
}

fn start_calibrate() {
    let a = app();
    if a.calibrating {
        return;
    }
    a.calibrating = true;
    a.cal_original = a.cfg.bar_bottom;
    a.cal_value = a.cfg.bar_bottom;
    register_cal_keys(a);
    redraw();
}

fn stop_calibrate(save: bool) {
    let a = app();
    if !a.calibrating {
        return;
    }
    a.calibrating = false;
    unregister_cal_keys(a);
    if save {
        a.cfg.bar_bottom = a.cal_value;
        save_config(&a.cfg);
        a.armed = true;
    } else {
        a.cfg.bar_bottom = a.cal_original;
    }
    redraw();
}

/// 主螢幕工作區；沒有主螢幕時退回 0,0 全螢幕。
fn primary_rect() -> (i32, i32, i32, i32) {
    unsafe {
        let mon = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
        let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if GetMonitorInfoW(mon, &mut info).as_bool() {
            let r = info.rcMonitor;
            (r.left, r.top, r.right - r.left, r.bottom - r.top)
        } else {
            (0, 0, DESIGN_W * 2, DESIGN_H * 2)
        }
    }
}

/// 螢幕底部被工作列佔掉多少。遊戲視窗化時工作列會蓋在畫面上，
/// 控制列要自動讓開，否則會整條被壓在工具列底下。
fn taskbar_inset() -> i32 {
    unsafe {
        let mon = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
        let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if GetMonitorInfoW(mon, &mut info).as_bool() {
            let gap = info.rcMonitor.bottom - info.rcWork.bottom;
            gap.max(0)
        } else {
            0
        }
    }
}

fn resize_to_primary() {
    let (x, y, w, h) = primary_rect();
    let a = app();
    a.origin_x = x;
    a.origin_y = y;
    a.width = w;
    a.height = h;
    unsafe {
        let _ = SetWindowPos(a.hwnd, Some(HWND_TOPMOST), x, y, w, h, SWP_NOACTIVATE | SWP_SHOWWINDOW);
    }
    redraw();
}

// ---------- 視窗訊息 ----------

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_HOTKEY => {
            match wp.0 as i32 {
                HOTKEY_TOGGLE => toggle(),
                HOTKEY_CALIBRATE => {
                    let a = app();
                    if a.calibrating {
                        stop_calibrate(false)
                    } else {
                        start_calibrate()
                    }
                }
                HOTKEY_UP => {
                    let a = app();
                    a.cal_value = (a.cal_value + 2).min(900);
                    let v = a.cal_value;
                    a.cfg.bar_bottom = v;
                    redraw();
                }
                HOTKEY_DOWN => {
                    let a = app();
                    a.cal_value = (a.cal_value - 2).max(0);
                    let v = a.cal_value;
                    a.cfg.bar_bottom = v;
                    redraw();
                }
                HOTKEY_SAVE => stop_calibrate(true),
                HOTKEY_CANCEL => stop_calibrate(false),
                _ => {}
            }
            LRESULT(0)
        }
        WM_TIMER if wp.0 == TIMER_TICK => {
            // 解析度／螢幕切換時 WM_DISPLAYCHANGE 可能早於系統真正改完，
            // 所以在這裡再比對一次，尺寸不對就自己修正（順便當保險）。
            let (_, _, sw, sh) = primary_rect();
            {
                let a = app();
                if a.width != sw || a.height != sh {
                    resize_to_primary();
                    return LRESULT(0);
                }
            }
            let a = app();
            if a.armed && !a.calibrating {
                a.cur = 88 + ((a.cur - 88 + 4) % 8);
                redraw();
            }
            LRESULT(0)
        }
        WM_DISPLAYCHANGE => {
            resize_to_primary();
            LRESULT(0)
        }
        WM_DESTROY => {
            windows::Win32::UI::WindowsAndMessaging::PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

fn main() {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        let mut input = GdiplusStartupInput::default();
        input.version = 1;
        let mut token = 0usize;
        let gdiplus_ok = GdiplusStartup(&mut token, &input, ptr::null_mut()) == 0;

        let family = if gdiplus_ok {
            "Microsoft JhengHei UI".to_string()
        } else {
            "Segoe UI".to_string()
        };

        let hinst = GetModuleHandleW(None).unwrap();
        let class_name = wide("LooksLikeYouTube");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: hinst.into(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        if RegisterClassExW(&wc) == 0 {
            return;
        }

        let (ox, oy, width, height) = primary_rect();
        let title = wide("looks-like-youtube");

        let ex_style = WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW;
        let hwnd = match CreateWindowExW(
            ex_style,
            PCWSTR(class_name.as_ptr()),
            PCWSTR(title.as_ptr()),
            WS_POPUP | WS_VISIBLE,
            ox,
            oy,
            width,
            height,
            None,
            None,
            Some(hinst.into()),
            None,
        ) {
            Ok(h) => h,
            Err(_) => return,
        };

        let first_run = config_path().map(|p| !p.exists()).unwrap_or(false);
        let cfg = load_config();

        let boxed = Box::new(App {
            hwnd,
            cfg,
            armed: false,
            calibrating: false,
            cal_value: 0,
            cal_original: 0,
            width,
            height,
            origin_x: ox,
            origin_y: oy,
            cur: 90,
            family_name: family,
            canvas: None,
        });
        let ptr_val = Box::into_raw(boxed) as usize;
        *APP.lock().unwrap() = Some(ptr_val);

        // 覆蓋全螢幕、置頂，且永不啟用
        let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), ox, oy, width, height, SWP_NOACTIVATE | SWP_SHOWWINDOW);

        // 熱鍵照設定檔走；解析失敗就退回預設組合
        let a = app();
        let (tmod, tkey) = parse_hotkey(&a.cfg.toggle_hotkey)
            .or_else(|| parse_hotkey(DEFAULT_TOGGLE))
            .unwrap();
        let _ = RegisterHotKey(Some(hwnd), HOTKEY_TOGGLE, tmod, tkey);
        let (cmod, ckey) = parse_hotkey(&a.cfg.calibrate_hotkey)
            .or_else(|| parse_hotkey(DEFAULT_CALIBRATE))
            .unwrap();
        let _ = RegisterHotKey(Some(hwnd), HOTKEY_CALIBRATE, cmod, ckey);
        // 進度條每 2 秒才微動一次，跟 Electron 版一致；沒必要每秒重繪
        SetTimer(Some(hwnd), TIMER_TICK, 2000, None);

        if first_run {
            start_calibrate();
        } else {
            redraw();
        }
        let _ = a;

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        if gdiplus_ok {
            GdiplusShutdown(token);
        }
        let _ = CW_USEDEFAULT;
    }
}
