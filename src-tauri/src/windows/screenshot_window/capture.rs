use serde::Serialize;
use tauri::AppHandle;
use once_cell::sync::Lazy;
use parking_lot::Mutex;

use xcap::Monitor;

#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
}

#[cfg(target_os = "macos")]
mod macos_capture {
    use core_graphics::display::CGDisplay;

    pub fn capture_display_bgra(display_id: u32) -> Result<(u32, u32, Vec<u8>), String> {
        let display = CGDisplay::new(display_id);
        let image = display
            .image()
            .ok_or_else(|| format!("CGDisplayCreateImage 返回空指针(display_id={})", display_id))?;

        let width = image.width() as usize;
        let height = image.height() as usize;
        let bytes_per_row = image.bytes_per_row() as usize;
        let data = image.data();
        let bytes = data.bytes();

        if width == 0 || height == 0 {
            return Err(format!(
                "抓屏得到空尺寸(display_id={}, width={}, height={})",
                display_id, width, height
            ));
        }

        let expected_row = width * 4;
        if bytes_per_row < expected_row {
            return Err(format!(
                "抓屏 bytes_per_row 异常(display_id={}, bytes_per_row={}, expected_row={})",
                display_id, bytes_per_row, expected_row
            ));
        }

        if bytes.len() < bytes_per_row.saturating_mul(height) {
            return Err(format!(
                "抓屏数据长度不足(display_id={}, data_len={}, need_at_least={})",
                display_id,
                bytes.len(),
                bytes_per_row.saturating_mul(height)
            ));
        }

        let mut out = vec![0u8; expected_row * height];
        for row in 0..height {
            let src_start = row * bytes_per_row;
            let dst_start = row * expected_row;
            out[dst_start..dst_start + expected_row]
                .copy_from_slice(&bytes[src_start..src_start + expected_row]);
        }

        // 经验上 CGDisplayCreateImage/CGWindowListCreateImage 的数据通常为 BGRA。
        // 这里强制 alpha=255，避免某些情况下“透明/黑屏”。
        for px in out.chunks_exact_mut(4) {
            px[3] = 255;
        }

        Ok((width as u32, height as u32, out))
    }
}

pub fn has_screen_capture_permission() -> bool {
    #[cfg(target_os = "macos")]
    unsafe {
        CGPreflightScreenCaptureAccess()
    }

    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

pub fn request_screen_capture_permission() -> bool {
    #[cfg(target_os = "macos")]
    unsafe {
        CGRequestScreenCaptureAccess()
    }

    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

fn ensure_screen_capture_permission() -> Result<(), String> {
    if has_screen_capture_permission() {
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        // 触发系统弹窗（若系统允许）。大多数情况下授权需要“重启应用”后才生效。
        let _ = request_screen_capture_permission();
    }

    Err("截图需要 macOS 的“屏幕录制”权限。请到【系统设置 → 隐私与安全性 → 屏幕录制】开启 QuickClipboard，然后完全退出并重新打开应用。".to_string())
}

// 单个显示器截图的信息
#[derive(Serialize, Clone)]
pub struct MonitorScreenshotInfo {
    pub file_path: String,
    pub physical_x: i32,
    pub physical_y: i32,
    pub physical_width: u32,
    pub physical_height: u32,
    pub logical_x: i32,
    pub logical_y: i32,
    pub logical_width: u32,
    pub logical_height: u32,
    pub scale_factor: f64,
}

// 最近一次截屏结果缓存
static LAST_CAPTURES: Lazy<Mutex<Option<Vec<MonitorScreenshotInfo>>>> =
    Lazy::new(|| Mutex::new(None));

// 清除最近一次截屏结果
pub fn clear_last_captures() {
    let mut guard = LAST_CAPTURES.lock();
    *guard = None;
}

// 捕获所有显示器的截图
pub fn capture_all_monitors_to_files(app: &AppHandle) -> Result<Vec<MonitorScreenshotInfo>, String> {
    ensure_screen_capture_permission()?;

    let xcap_monitors = Monitor::all().map_err(|e| format!("枚举显示器失败: {}", e))?;
    if xcap_monitors.is_empty() {
        return Err("未找到显示器".to_string());
    }

    let mut results: Vec<(MonitorScreenshotInfo, Vec<u8>)> = Vec::with_capacity(xcap_monitors.len());

    for (index, monitor) in xcap_monitors.into_iter().enumerate() {
        let monitor_id = monitor
            .id()
            .map_err(|e| format!("获取显示器 ID 失败[{}]: {}", index, e))?;
        let logical_x = monitor
            .x()
            .map_err(|e| format!("获取显示器 X 坐标失败: {}", e))?;
        let logical_y = monitor
            .y()
            .map_err(|e| format!("获取显示器 Y 坐标失败: {}", e))?;
        let logical_width = monitor
            .width()
            .map_err(|e| format!("获取显示器宽度失败: {}", e))?
            .max(1);
        let logical_height = monitor
            .height()
            .map_err(|e| format!("获取显示器高度失败: {}", e))?
            .max(1);

        // 逐个显示器串行截取（macOS 下更容易稳定）。
        #[cfg(target_os = "macos")]
        let (width, height, bgra) = macos_capture::capture_display_bgra(monitor_id)
            .map_err(|e| format!("截取屏幕失败[{}](display_id={}): {}", index, monitor_id, e))?;

        #[cfg(not(target_os = "macos"))]
        let (width, height, bgra) = {
            let img = monitor
                .capture_image()
                .map_err(|e| format!("截取屏幕失败[{}]: {}", index, e))?;
            let (w, h) = img.dimensions();
            let raw = img.as_raw();

            // RGBA -> BGRA（BMP 32bpp）
            let mut out = raw.clone();
            for px in out.chunks_exact_mut(4) {
                px.swap(0, 2);
                px[3] = 255;
            }
            (w, h, out)
        };

        let pixel_data_size = width * height * 4;
        let file_size = 14 + 40 + pixel_data_size;

        let mut buf = vec![0u8; file_size as usize];

        buf[0..2].copy_from_slice(b"BM");
        buf[2..6].copy_from_slice(&(file_size as u32).to_le_bytes());
        buf[10..14].copy_from_slice(&54u32.to_le_bytes());

        buf[14..18].copy_from_slice(&40u32.to_le_bytes());
        buf[18..22].copy_from_slice(&(width as i32).to_le_bytes());
        buf[22..26].copy_from_slice(&(-(height as i32)).to_le_bytes());
        buf[26..28].copy_from_slice(&1u16.to_le_bytes());
        buf[28..30].copy_from_slice(&32u16.to_le_bytes());
        buf[34..38].copy_from_slice(&(pixel_data_size as u32).to_le_bytes());

        buf[54..].copy_from_slice(&bgra);

        // macOS：logical_* 是“点”，physical_* 是“像素”。优先使用 xcap 给出的 scale_factor。
        let mut scale_factor = monitor.scale_factor().ok().map(|v| v as f64).unwrap_or(0.0);
        if scale_factor <= 0.0 || !scale_factor.is_finite() {
            let scale_x = width as f64 / logical_width as f64;
            let scale_y = height as f64 / logical_height as f64;
            scale_factor = (scale_x + scale_y) / 2.0;
        }
        if !scale_factor.is_finite() || scale_factor <= 0.0 {
            scale_factor = 1.0;
        }

        let physical_x = (logical_x as f64 * scale_factor).round() as i32;
        let physical_y = (logical_y as f64 * scale_factor).round() as i32;

        let info = MonitorScreenshotInfo {
            file_path: String::new(),
            physical_x,
            physical_y,
            physical_width: width,
            physical_height: height,
            logical_x,
            logical_y,
            logical_width,
            logical_height,
            scale_factor,
        };

        results.push((info, buf));
    }

    // 将所有 BMP 数据写入 HTTP 图像缓存，并获取服务器端口
    let images: Vec<Vec<u8>> = results.iter().map(|(_, buf)| buf.clone()).collect();
    let port = crate::utils::image_http_server::set_images(images)?;

    let infos: Vec<MonitorScreenshotInfo> = results
        .into_iter()
        .enumerate()
        .map(|(index, (mut info, _buf))| {
            info.file_path = format!("http://127.0.0.1:{}/screen/{}.bmp", port, index);
            info
        })
        .collect();

    Ok(infos)
}

// 截取所有显示器并将结果写入全局缓存
pub fn capture_and_store_last(app: &AppHandle) -> Result<(), String> {
    let captures = capture_all_monitors_to_files(app)?;
    let mut guard = LAST_CAPTURES.lock();
    *guard = Some(captures);
    Ok(())
}

// 获取最近一次截屏结果
pub fn get_last_captures() -> Result<Vec<MonitorScreenshotInfo>, String> {
    let guard = LAST_CAPTURES.lock();
    guard
        .clone()
        .ok_or_else(|| "尚未有可用截屏，请先触发截屏".to_string())
}
