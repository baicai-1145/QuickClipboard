use tauri::{AppHandle, Emitter, Manager, Position, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use crate::utils::image_http_server::{PinEditData, set_pin_edit_data, clear_pin_edit_data, get_pin_edit_data};
use serde_json::json;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::thread;
use std::time::Duration;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};

#[cfg(target_os = "windows")]
use windows::Win32::Foundation::HWND;
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{SetWindowDisplayAffinity, WINDOW_DISPLAY_AFFINITY};

// 截屏模式
// 0: 普通模式
// 1: 快速保存模式（选区完成后直接复制到剪贴板）
// 2: 快速贴图模式（选区完成后直接贴图）
// 3: 快速OCR模式（选区完成后直接OCR识别并复制）
static SCREENSHOT_MODE: AtomicU8 = AtomicU8::new(0);

#[derive(Debug, Clone, Copy)]
struct PhysicalRect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

static PIN_EDIT_PASSTHROUGH_ACTIVE: AtomicBool = AtomicBool::new(false);
static PIN_EDIT_WINDOW: Lazy<Mutex<Option<WebviewWindow>>> = Lazy::new(|| Mutex::new(None));
static PIN_EDIT_RECTS: Lazy<Mutex<Vec<PhysicalRect>>> = Lazy::new(|| Mutex::new(Vec::new()));

fn create_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    let is_dev = cfg!(debug_assertions);
    let mut builder = WebviewWindowBuilder::new(
        app,
        "screenshot",
        WebviewUrl::App("windows/screenshot/index.html".into()),
    )
        .title("截屏窗口")
        .inner_size(1920.0, 1080.0)
        .position(0.0, 0.0)
        .decorations(false)
        .shadow(false);

    #[cfg(not(target_os = "macos"))]
    {
        builder = builder.transparent(true);
    }

    builder
        .always_on_top(!is_dev)
        .skip_taskbar(true)
        .visible(false)
        .resizable(false)
        .focused(false)
        .focusable(true)
        .visible_on_all_workspaces(true)
        .maximizable(false)
        .minimizable(false)
        .disable_drag_drop_handler()
        .build()
        .map_err(|e| format!("创建截屏窗口失败: {}", e))
}

fn get_or_create_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    app.get_webview_window("screenshot")
        .map(Ok)
        .unwrap_or_else(|| create_window(app))
}

#[cfg(target_os = "macos")]
fn get_macos_monitor_rects() -> Result<Vec<(i32, i32, u32, u32)>, String> {
    use xcap::Monitor;

    let monitors = Monitor::all().map_err(|e| format!("枚举显示器失败: {}", e))?;
    if monitors.is_empty() {
        return Err("未找到显示器".to_string());
    }

    let mut rects = Vec::with_capacity(monitors.len());
    for (idx, m) in monitors.into_iter().enumerate() {
        let x = m.x().map_err(|e| format!("获取显示器 X 坐标失败[{}]: {}", idx, e))?;
        let y = m.y().map_err(|e| format!("获取显示器 Y 坐标失败[{}]: {}", idx, e))?;
        let w = m.width().map_err(|e| format!("获取显示器宽度失败[{}]: {}", idx, e))?.max(1);
        let h = m.height().map_err(|e| format!("获取显示器高度失败[{}]: {}", idx, e))?.max(1);
        rects.push((x, y, w, h));
    }

    Ok(rects)
}

#[cfg(target_os = "macos")]
fn create_window_for_monitor(
    app: &AppHandle,
    label: &str,
    monitor_index: usize,
) -> Result<WebviewWindow, String> {
    let is_dev = cfg!(debug_assertions);
    let mut builder = WebviewWindowBuilder::new(
        app,
        label,
        WebviewUrl::App(format!("windows/screenshot/index.html?monitor={}", monitor_index).into()),
    )
        .title("截屏窗口")
        .inner_size(1920.0, 1080.0)
        .position(0.0, 0.0)
        .decorations(false)
        .shadow(false);

    #[cfg(not(target_os = "macos"))]
    {
        builder = builder.transparent(true);
    }

    builder
        .always_on_top(!is_dev)
        .skip_taskbar(true)
        .visible(false)
        .resizable(false)
        .focused(false)
        .focusable(true)
        .visible_on_all_workspaces(true)
        .maximizable(false)
        .minimizable(false)
        .disable_drag_drop_handler()
        .build()
        .map_err(|e| format!("创建截屏窗口失败({}): {}", label, e))
}

#[cfg(target_os = "macos")]
fn ensure_macos_screenshot_windows(app: &AppHandle) -> Result<Vec<WebviewWindow>, String> {
    use tauri::{LogicalPosition, LogicalSize, Size};

    let rects = get_macos_monitor_rects()?;
    let mut windows = Vec::with_capacity(rects.len());

    for (idx, (x, y, w, h)) in rects.iter().copied().enumerate() {
        let label = if idx == 0 {
            "screenshot".to_string()
        } else {
            format!("screenshot-{}", idx)
        };

        let window = if let Some(win) = app.get_webview_window(&label) {
            // 确保 query 参数正确（旧窗口可能没有 monitor 参数）
            let _ = win.eval(&format!(
                "(function(){{try{{const p=new URLSearchParams(location.search);if(p.get('monitor')!=='{idx}'){{location.search='?monitor={idx}';}}}}catch(e){{}}}})();"
            ));
            win
        } else {
            create_window_for_monitor(app, &label, idx)?
        };

        // macOS：用 logical(点) 尺寸/坐标，避免 Retina 下缩放错位。
        let _ = window.set_size(Size::Logical(LogicalSize::new(w as f64, h as f64)));
        let _ = window.set_position(Position::Logical(LogicalPosition::new(x as f64, y as f64)));

        windows.push(window);
    }

    // 如果显示器数量变少，隐藏多余的 screenshot-* 窗口
    let expected = windows.len();
    for (label, win) in app.webview_windows() {
        if label == "screenshot" {
            continue;
        }
        if let Some(rest) = label.strip_prefix("screenshot-") {
            if let Ok(idx) = rest.parse::<usize>() {
                if idx >= expected {
                    let _ = win.hide();
                }
            }
        }
    }

    Ok(windows)
}

fn resize_window_to_virtual_screen(window: &WebviewWindow) {
    let (x, y, width, height) = crate::screen::ScreenUtils::get_virtual_screen_size_by_app(window.app_handle())
        .unwrap_or((0, 0, 1920, 1080));

    let _ = window.set_size(tauri::Size::Physical(tauri::PhysicalSize::new(width as u32, height as u32)));
    let _ = window.set_position(Position::Physical(PhysicalPosition::new(x, y)));
}

// 设置窗口是否从屏幕捕获中排除
#[cfg(target_os = "windows")]
fn set_window_exclude_from_capture(window: &WebviewWindow, exclude: bool) {
    if let Ok(hwnd) = window.hwnd() {
        let affinity = WINDOW_DISPLAY_AFFINITY(if exclude { 0x11 } else { 0x00 });
        unsafe { let _ = SetWindowDisplayAffinity(HWND(hwnd.0), affinity); }
    }
}

#[cfg(not(target_os = "windows"))]
fn set_window_exclude_from_capture(_window: &WebviewWindow, _exclude: bool) {}

fn start_screenshot_with_mode(app: &AppHandle, mode: u8) -> Result<(), String> {
    let settings = crate::get_settings();
    if !settings.screenshot_enabled {
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        if !crate::windows::screenshot_window::capture::has_screen_capture_permission() {
            // 尝试触发系统授权弹窗（若系统允许）。多数情况下授权需要重启应用才会生效。
            let _ = crate::windows::screenshot_window::capture::request_screen_capture_permission();

            let _ = app
                .dialog()
                .message("截图需要 macOS 的“屏幕录制”权限。\n\n请到【系统设置 → 隐私与安全性 → 屏幕录制】开启 QuickClipboard，然后完全退出并重新打开应用。\n\n提示：建议把 QuickClipboard.app 放到 /Applications 后再授权。")
                .buttons(MessageDialogButtons::Ok)
                .blocking_show();

            return Err("缺少屏幕录制权限".to_string());
        }
    }
    #[cfg(target_os = "macos")]
    let windows = ensure_macos_screenshot_windows(app)?;

    #[cfg(not(target_os = "macos"))]
    let windows = vec![get_or_create_window(app)?];

    if windows.iter().any(|w| w.is_visible().unwrap_or(false)) {
        if let Some(w) = windows.first() {
            let _ = w.set_focus();
        }
        return Ok(());
    }
    SCREENSHOT_MODE.store(mode, Ordering::SeqCst);

    let app_clone = app.clone();
    thread::spawn(move || {
        // 先抓取屏幕，再显示遮罩窗口：
        // 否则会把遮罩窗口本身捕获进去，导致“只剩 Dock/桌面、应用全黑”的效果。
        let capture_result = crate::services::screenshot::capture_and_store_last(&app_clone);
        
        let app_for_main = app_clone.clone();
        let _ = app_clone.run_on_main_thread(move || {
            let screenshot_windows: Vec<WebviewWindow> = app_for_main
                .webview_windows()
                .into_iter()
                .filter(|(label, _)| label == "screenshot" || label.starts_with("screenshot-"))
                .map(|(_label, window)| window)
                .collect();

            if screenshot_windows.is_empty() {
                return;
            }

            if capture_result.is_ok() {
                let is_dev = cfg!(debug_assertions);
                for (idx, window) in screenshot_windows.iter().enumerate() {
                    #[cfg(not(target_os = "macos"))]
                    {
                        resize_window_to_virtual_screen(window);
                    }

                    let _ = window.set_always_on_top(!is_dev);
                    let _ = window.show();
                    if idx == 0 {
                        let _ = window.set_focus();
                    }

                    // 捕获已完成，这里恢复默认捕获策略（Windows 特有，macOS 下无害）
                    set_window_exclude_from_capture(window, false);
                }

                for window in &screenshot_windows {
                    let _ = window.emit("screenshot:new-session", json!({ "screenshotMode": mode }));
                }
                
                if let Err(e) = crate::windows::screenshot_window::auto_selection::start_auto_selection(app_for_main.clone()) {
                    eprintln!("无法启动自动选区: {}", e);
                }
            } else if let Err(ref e) = capture_result {
                eprintln!("截屏失败: {}", e);
                for window in &screenshot_windows {
                    let _ = window.hide();
                }
                let _ = app_for_main
                    .dialog()
                    .message(format!("截屏失败：{}\n\n如果你在 macOS 上使用，请确认已开启【系统设置 → 隐私与安全性 → 屏幕录制】权限，并重启应用。", e))
                    .buttons(MessageDialogButtons::Ok)
                    .blocking_show();
            }
        });
    });

    Ok(())
}

pub fn start_screenshot(app: &AppHandle) -> Result<(), String> {
    start_screenshot_with_mode(app, 0)
}

pub fn start_screenshot_quick_save(app: &AppHandle) -> Result<(), String> {
    start_screenshot_with_mode(app, 1)
}

pub fn start_screenshot_quick_pin(app: &AppHandle) -> Result<(), String> {
    start_screenshot_with_mode(app, 2)
}

pub fn start_screenshot_quick_ocr(app: &AppHandle) -> Result<(), String> {
    start_screenshot_with_mode(app, 3)
}

// 获取当前截屏模式
#[tauri::command]
pub fn get_screenshot_mode() -> u8 {
    SCREENSHOT_MODE.load(Ordering::SeqCst)
}

// 重置截屏模式
#[tauri::command]
pub fn reset_screenshot_mode() {
    SCREENSHOT_MODE.store(0, Ordering::SeqCst);
}

// 启动贴图编辑模式
#[allow(clippy::too_many_arguments)]
pub fn start_pin_edit_mode(
    app: &AppHandle,
    image_path: String,
    x: i32, y: i32,
    width: u32, height: u32,
    logical_width: u32, logical_height: u32,
    scale_factor: f64,
    window_label: String,
    window_x: i32, window_y: i32,
    window_width: f64, window_height: f64,
    original_image_path: Option<String>,
    edit_data_json: Option<String>,
) -> Result<(), String> {
    SCREENSHOT_MODE.store(0, Ordering::SeqCst);
    let window = get_or_create_window(app)?;
    let edit_data = PinEditData {
        image_path,
        x, y,
        width, height,
        logical_width, logical_height,
        scale_factor,
        window_label,
        window_x, window_y,
        window_width, window_height,
        original_image_path,
        edit_data: edit_data_json,
    };
    set_pin_edit_data(edit_data)?;

    let _ = window.emit("screenshot:pin-edit-mode", ());
    resize_window_to_virtual_screen(&window);
    let _ = window.show();
    let _ = window.set_focus();

    Ok(())
}

// 获取贴图编辑数据的命令
#[tauri::command]
pub fn get_pin_edit_mode_data() -> Result<Option<PinEditData>, String> {
    Ok(get_pin_edit_data())
}

// 清除贴图编辑数据
#[tauri::command]
pub fn clear_pin_edit_mode() {
    disable_pin_edit_passthrough();
    clear_pin_edit_data();
}

// 更新贴图图片并恢复显示
#[tauri::command]
pub fn confirm_pin_edit(
    app: AppHandle,
    new_file_path: String,
    edit_data_json: Option<String>,
) -> Result<(), String> {
    if let Some(data) = get_pin_edit_data() {
        let old_file_path = data.image_path.clone();
        let is_old_same_as_original = data.original_image_path.as_ref() == Some(&old_file_path);
        if old_file_path != new_file_path && !is_old_same_as_original {
            let _ = std::fs::remove_file(&old_file_path);
        }

        let original_image_path = data.original_image_path.clone();

        if let Some(window) = app.get_webview_window(&data.window_label) {
            crate::windows::pin_image_window::update_pin_image_data(
                &data.window_label,
                new_file_path.clone(),
                original_image_path,
                edit_data_json,
            );
            let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize::new(
                data.window_width,
                data.window_height,
            )));
            let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(
                data.window_x,
                data.window_y,
            )));
            let _ = app.emit_to(
                &data.window_label,
                "pin-image:refresh",
                json!({ "file_path": new_file_path }),
            );
            let _ = window.show();
        }
    }
    Ok(())
}

// 恢复显示原贴图窗口
#[tauri::command]
pub fn cancel_pin_edit(app: AppHandle) -> Result<(), String> {
    if let Some(data) = get_pin_edit_data() {
        if let Some(window) = app.get_webview_window(&data.window_label) {
            let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize::new(data.window_width, data.window_height)));
            let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(data.window_x, data.window_y)));
            let _ = window.show();
        }
    }
    Ok(())
}

#[tauri::command]
pub fn enable_pin_edit_passthrough(
    app: AppHandle,
    rects: Vec<[f64; 4]>, 
) -> Result<(), String> {
    let window = app.get_webview_window("screenshot")
        .ok_or("未找到截屏窗口")?;
    
    *PIN_EDIT_WINDOW.lock() = Some(window);
    *PIN_EDIT_RECTS.lock() = rects.iter()
        .map(|r| PhysicalRect { x: r[0], y: r[1], width: r[2], height: r[3] })
        .collect();
    
    if !PIN_EDIT_PASSTHROUGH_ACTIVE.load(Ordering::Relaxed) {
        PIN_EDIT_PASSTHROUGH_ACTIVE.store(true, Ordering::Relaxed);
        thread::spawn(|| pin_edit_passthrough_loop());
    }
    
    Ok(())
}

#[tauri::command]
pub fn disable_pin_edit_passthrough() {
    PIN_EDIT_PASSTHROUGH_ACTIVE.store(false, Ordering::Relaxed);
    
    if let Some(window) = PIN_EDIT_WINDOW.lock().as_ref() {
        let _ = window.set_ignore_cursor_events(false);
    }
    
    *PIN_EDIT_WINDOW.lock() = None;
    *PIN_EDIT_RECTS.lock() = Vec::new();
}

#[tauri::command]
pub fn update_pin_edit_passthrough_rects(rects: Vec<[f64; 4]>) {
    *PIN_EDIT_RECTS.lock() = rects.iter()
        .map(|r| PhysicalRect { x: r[0], y: r[1], width: r[2], height: r[3] })
        .collect();
}

fn pin_edit_passthrough_loop() {
    while PIN_EDIT_PASSTHROUGH_ACTIVE.load(Ordering::Relaxed) {
        if let Some(window) = PIN_EDIT_WINDOW.lock().as_ref() {
            let (cursor_x, cursor_y) = crate::mouse::get_cursor_position();
            let x = cursor_x as f64;
            let y = cursor_y as f64;

            let is_in_rect = PIN_EDIT_RECTS.lock().iter().any(|rect| {
                x >= rect.x && x <= rect.x + rect.width &&
                y >= rect.y && y <= rect.y + rect.height
            });

            let _ = window.set_ignore_cursor_events(!is_in_rect);
        }
        
        thread::sleep(Duration::from_millis(16));
    }
}
