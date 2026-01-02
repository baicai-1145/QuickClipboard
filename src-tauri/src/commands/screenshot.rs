use tauri::Manager;

// 启动内置截图功能
#[tauri::command]
pub async fn start_builtin_screenshot(app: tauri::AppHandle) -> Result<(), String> {
    let app_clone = app.clone();
    tokio::task::spawn_blocking(move || {
        crate::windows::screenshot_window::auto_selection::clear_auto_selection_cache();
        crate::windows::screenshot_window::start_screenshot(&app_clone)
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}

// 捕获所有显示器截图
#[tauri::command]
pub fn capture_all_screenshots(app: tauri::AppHandle) -> Result<Vec<crate::services::screenshot::MonitorScreenshotInfo>, String> {
    crate::services::screenshot::capture_all_monitors_to_files(&app)
}

// 获取最近一次截屏结果
#[tauri::command]
pub fn get_last_screenshot_captures() -> Result<Vec<crate::services::screenshot::MonitorScreenshotInfo>, String> {
    crate::services::screenshot::get_last_captures()
}

// 取消当前截屏会话
#[tauri::command]
pub fn cancel_screenshot_session(app: tauri::AppHandle) -> Result<(), String> {
    crate::services::screenshot::clear_last_captures();
    crate::windows::screenshot_window::auto_selection::clear_auto_selection_cache();
    for (label, win) in app.webview_windows() {
        if label != "screenshot" && !label.starts_with("screenshot-") {
            continue;
        }
        let _ = win.set_size(tauri::Size::Logical(tauri::LogicalSize::new(1.0, 1.0)));
        let _ = win.hide();
        let _ = win.eval("window.location.reload()");
    }
    Ok(())
}

// 启用长截屏模式的鼠标穿透控制
#[tauri::command]
pub fn enable_long_screenshot_passthrough(
    app: tauri::AppHandle,
    physical_x: f64,
    physical_y: f64,
    physical_width: f64,
    physical_height: f64,
    physical_toolbar_x: f64,
    physical_toolbar_y: f64,
    physical_toolbar_width: f64,
    physical_toolbar_height: f64,
    selection_scale_factor: f64,
) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("screenshot") {
        crate::windows::screenshot_window::long_screenshot::enable_passthrough(
            window,
            physical_x, physical_y, physical_width, physical_height,
            physical_toolbar_x, physical_toolbar_y, physical_toolbar_width, physical_toolbar_height,
            selection_scale_factor
        );
        Ok(())
    } else {
        Err("Screenshot window not found".to_string())
    }
}

// 禁用长截屏模式的鼠标穿透控制
#[tauri::command]
pub fn disable_long_screenshot_passthrough() -> Result<(), String> {
    crate::windows::screenshot_window::long_screenshot::disable_passthrough();
    Ok(())
}

// 开始长截屏捕获
#[tauri::command]
pub fn start_long_screenshot_capture() -> Result<(), String> {
    crate::windows::screenshot_window::long_screenshot::start_capturing()
}

// 停止长截屏捕获
#[tauri::command]
pub fn stop_long_screenshot_capture() -> Result<(), String> {
    crate::windows::screenshot_window::long_screenshot::stop_capturing();
    Ok(())
}

// 更新长截屏预览面板位置
#[tauri::command]
pub fn update_long_screenshot_preview_panel(x: f64, y: f64, width: f64, height: f64) {
    crate::windows::screenshot_window::long_screenshot::update_preview_panel_rect(x, y, width, height);
}

// 更新长截屏工具栏位置
#[tauri::command]
pub fn update_long_screenshot_toolbar(x: f64, y: f64, width: f64, height: f64) {
    crate::windows::screenshot_window::long_screenshot::update_toolbar_rect(x, y, width, height);
}

// 保存长截屏
#[tauri::command]
pub async fn save_long_screenshot(path: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        crate::windows::screenshot_window::long_screenshot::save_long_screenshot(path)
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}

// 长截屏复制到剪贴板
#[tauri::command]
pub async fn copy_long_screenshot_to_clipboard() -> Result<(), String> {
    use clipboard_rs::{Clipboard, ClipboardContext};
    use sha2::{Sha256, Digest};
    
    tokio::task::spawn_blocking(move || {
        let data_dir = crate::services::get_data_directory()?;
        let images_dir = data_dir.join("clipboard_images");
        std::fs::create_dir_all(&images_dir)
            .map_err(|e| format!("创建目录失败: {}", e))?;
        
        let temp_path = images_dir.join("_temp_long_screenshot.png");
        crate::windows::screenshot_window::long_screenshot::save_long_screenshot(
            temp_path.to_string_lossy().to_string()
        )?;
        
        let png_data = std::fs::read(&temp_path)
            .map_err(|e| format!("读取图片失败: {}", e))?;
        let hash = format!("{:x}", Sha256::digest(&png_data));
        let filename = format!("{}.png", &hash[..16]);
        let final_path = images_dir.join(&filename);
        
        if final_path.exists() {
            let _ = std::fs::remove_file(&temp_path);
        } else {
            std::fs::rename(&temp_path, &final_path)
                .map_err(|e| format!("移动文件失败: {}", e))?;
        }
        
        let ctx = ClipboardContext::new()
            .map_err(|e| format!("创建剪贴板上下文失败: {}", e))?;
        ctx.set_files(vec![final_path.to_string_lossy().to_string()])
            .map_err(|e| format!("复制到剪贴板失败: {}", e))
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}

// OCR识别结果结构
#[derive(Debug, Clone, serde::Serialize)]
pub struct OcrWord {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, serde::Serialize)]
pub struct OcrLine {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub words: Vec<OcrWord>,
    pub word_gaps: Vec<f32>,
}

#[derive(Debug, serde::Serialize)]
pub struct OcrResult {
    pub text: String,
    pub lines: Vec<OcrLine>,
}

#[cfg(target_os = "macos")]
mod macos_vision_ocr {
    use super::{OcrLine, OcrResult, OcrWord};

    use image::GenericImageView;
    use objc2::rc::{autoreleasepool, Allocated, Retained};
    use objc2::runtime::{AnyObject, NSObject};
    use objc2::{extern_class, msg_send, sel, ClassType};
    use objc2_core_foundation::CGRect;
    use objc2_foundation::{ns_string, NSArray, NSDictionary, NSData, NSError, NSString, NSUInteger};

    // 确保链接到 Vision.framework，否则运行时可能无法找到相关类。
    #[link(name = "Vision", kind = "framework")]
    extern "C" {}

    extern_class!(
        #[unsafe(super(NSObject))]
        #[derive(PartialEq, Eq, Hash, Debug)]
        pub struct VNImageRequestHandler;
    );

    fn vision_language_hints(language: Option<&str>) -> Retained<NSArray<NSString>> {
        let mut langs: Vec<&'static NSString> = Vec::new();

        if let Some(lang) = language {
            let lower = lang.trim().to_lowercase();
            if lower.starts_with("zh") {
                langs.push(ns_string!("zh-Hans"));
            } else if lower.starts_with("en") {
                langs.push(ns_string!("en-US"));
            }
        }

        if langs.is_empty() {
            langs.push(ns_string!("zh-Hans"));
            langs.push(ns_string!("en-US"));
        }

        NSArray::from_slice(&langs)
    }

    pub fn recognize(image_data: &[u8], language: Option<&str>) -> Result<OcrResult, String> {
        let img = image::load_from_memory(image_data).map_err(|e| format!("读取图片失败: {}", e))?;
        let (img_width, img_height) = img.dimensions();
        if img_width == 0 || img_height == 0 {
            return Err("图片尺寸无效".to_string());
        }

        autoreleasepool(|_pool| {
            let data = NSData::with_bytes(image_data);
            let options = NSDictionary::<NSString, AnyObject>::new();

            let handler: Allocated<VNImageRequestHandler> =
                unsafe { msg_send![VNImageRequestHandler::class(), alloc] };
            let handler: Retained<VNImageRequestHandler> = unsafe {
                msg_send![handler, initWithData: &*data, orientation: 1u32, options: &*options]
            };

            let request: Retained<AnyObject> = unsafe { msg_send![objc2::class!(VNRecognizeTextRequest), new] };

            let langs = vision_language_hints(language);
            let _: () = unsafe { msg_send![&*request, setRecognitionLanguages: &*langs] };
            let _: () = unsafe { msg_send![&*request, setRecognitionLevel: 0isize] }; // Accurate
            let _: () = unsafe { msg_send![&*request, setUsesLanguageCorrection: true] };

            // macOS 13+ 才有 automaticallyDetectsLanguage；用 respondsToSelector 兼容旧系统。
            let supports_auto_detect: bool =
                unsafe { msg_send![&*request, respondsToSelector: sel!(setAutomaticallyDetectsLanguage:)] };
            if supports_auto_detect {
                let _: () = unsafe { msg_send![&*request, setAutomaticallyDetectsLanguage: true] };
            }

            let requests = NSArray::from_slice(&[&*request]);
            let mut error: *mut NSError = std::ptr::null_mut();
            let ok: bool = unsafe { msg_send![&handler, performRequests: &*requests, error: &mut error] };
            if !ok {
                if !error.is_null() {
                    // SAFETY: error 由 Objective-C 写入，且在当前 autoreleasepool 生命周期内有效。
                    let err = unsafe { &*error };
                    let desc = err.localizedDescription().to_string();
                    return Err(format!("Vision OCR 失败: {}", desc));
                }
                return Err("Vision OCR 失败".to_string());
            }

            let results: Option<Retained<NSArray<AnyObject>>> = unsafe { msg_send![&*request, results] };
            let Some(results) = results else {
                return Ok(OcrResult {
                    text: String::new(),
                    lines: Vec::new(),
                });
            };

            let mut collected: Vec<(f32, f32, OcrLine)> = Vec::new();
            let count = results.count() as usize;

            for idx in 0..count {
                let obs: Retained<AnyObject> = results.objectAtIndex(idx as NSUInteger);

                let candidates: Retained<NSArray<AnyObject>> =
                    unsafe { msg_send![&*obs, topCandidates: 1usize] };
                if candidates.count() == 0 {
                    continue;
                }
                let best: Retained<AnyObject> = candidates.objectAtIndex(0);
                let text_ns: Retained<NSString> = unsafe { msg_send![&*best, string] };
                let text = text_ns.to_string();
                if text.trim().is_empty() {
                    continue;
                }

                let bbox: CGRect = unsafe { msg_send![&*obs, boundingBox] };
                let x = (bbox.origin.x as f32) * (img_width as f32);
                let width = (bbox.size.width as f32) * (img_width as f32);
                let height = (bbox.size.height as f32) * (img_height as f32);
                let y = ((1.0 - (bbox.origin.y as f32) - (bbox.size.height as f32)) * (img_height as f32)).max(0.0);

                let line = OcrLine {
                    text: text.clone(),
                    x,
                    y,
                    width: width.max(0.0),
                    height: height.max(0.0),
                    words: vec![OcrWord {
                        text,
                        x,
                        y,
                        width: width.max(0.0),
                        height: height.max(0.0),
                    }],
                    word_gaps: Vec::new(),
                };

                collected.push((y, x, line));
            }

            collected.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal).then_with(|| {
                a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal)
            }));

            let ocr_lines: Vec<OcrLine> = collected.into_iter().map(|(_, _, l)| l).collect();
            let text = ocr_lines
                .iter()
                .map(|l| l.text.as_str())
                .collect::<Vec<_>>()
                .join("\n");

            Ok(OcrResult { text, lines: ocr_lines })
        })
    }
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn map_tesseract_language(language: Option<&str>) -> Option<String> {
    let raw = language?.trim();
    if raw.is_empty() {
        return None;
    }

    let lower = raw.to_lowercase();
    if lower.starts_with("zh") {
        return Some("chi_sim".to_string());
    }
    if lower.starts_with("en") {
        return Some("eng".to_string());
    }

    Some(raw.to_string())
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn parse_tesseract_tsv(tsv: &str) -> Result<OcrResult, String> {
    use std::collections::BTreeMap;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    struct LineKey {
        block_num: i32,
        par_num: i32,
        line_num: i32,
    }

    #[derive(Debug)]
    struct WordRow {
        word_num: i32,
        word: OcrWord,
    }

    let mut lines: BTreeMap<LineKey, Vec<WordRow>> = BTreeMap::new();

    for (idx, line) in tsv.lines().enumerate() {
        if idx == 0 {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 12 {
            continue;
        }

        let level: i32 = cols[0].parse().unwrap_or(0);
        if level != 5 {
            continue;
        }

        let block_num: i32 = cols[2].parse().unwrap_or(0);
        let par_num: i32 = cols[3].parse().unwrap_or(0);
        let line_num: i32 = cols[4].parse().unwrap_or(0);
        let word_num: i32 = cols[5].parse().unwrap_or(0);

        let left: f32 = cols[6].parse().unwrap_or(0.0);
        let top: f32 = cols[7].parse().unwrap_or(0.0);
        let width: f32 = cols[8].parse().unwrap_or(0.0);
        let height: f32 = cols[9].parse().unwrap_or(0.0);

        let text = cols[11].trim();
        if text.is_empty() {
            continue;
        }

        let key = LineKey {
            block_num,
            par_num,
            line_num,
        };

        lines.entry(key).or_default().push(WordRow {
            word_num,
            word: OcrWord {
                text: text.to_string(),
                x: left,
                y: top,
                width,
                height,
            },
        });
    }

    let mut ocr_lines: Vec<OcrLine> = Vec::new();
    for (_key, mut words) in lines {
        words.sort_by_key(|w| w.word_num);
        if words.is_empty() {
            continue;
        }

        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = 0.0f32;
        let mut max_y = 0.0f32;

        let mut word_gaps: Vec<f32> = Vec::new();
        let mut word_list: Vec<OcrWord> = Vec::new();

        for (i, row) in words.iter().enumerate() {
            let w = &row.word;
            min_x = min_x.min(w.x);
            min_y = min_y.min(w.y);
            max_x = max_x.max(w.x + w.width);
            max_y = max_y.max(w.y + w.height);

            if i + 1 < words.len() {
                let next = &words[i + 1].word;
                let gap = (next.x - (w.x + w.width)).max(0.0);
                word_gaps.push(gap);
            }

            word_list.push(w.clone());
        }

        let text = word_list
            .iter()
            .map(|w| w.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");

        ocr_lines.push(OcrLine {
            text,
            x: min_x,
            y: min_y,
            width: (max_x - min_x).max(0.0),
            height: (max_y - min_y).max(0.0),
            words: word_list,
            word_gaps,
        });
    }

    let text = ocr_lines
        .iter()
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    Ok(OcrResult { text, lines: ocr_lines })
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn recognize_with_tesseract_file(file_path: &str, language: Option<&str>) -> Result<OcrResult, String> {
    use std::process::Command;

    let mut cmd = Command::new("tesseract");
    cmd.arg(file_path).arg("stdout");

    if let Some(lang) = map_tesseract_language(language) {
        cmd.arg("-l").arg(lang);
    }

    cmd.arg("tsv");

    let output = cmd.output().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "未找到 tesseract 可执行文件；请先安装：brew install tesseract".to_string()
        } else {
            format!("调用 tesseract 失败: {}", e)
        }
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("tesseract 识别失败: {}", stderr.trim()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_tesseract_tsv(&stdout)
}

// OCR识别图片字节数组
#[tauri::command]
pub async fn recognize_image_ocr(image_data: Vec<u8>) -> Result<OcrResult, String> {
    tokio::task::spawn_blocking(move || {
        #[cfg(target_os = "windows")]
        {
            use qcocr::recognize_from_bytes;

            let result = recognize_from_bytes(&image_data, None)
                .map_err(|e| format!("OCR识别失败: {}", e))?;

            convert_ocr_result(result)
        }

        #[cfg(target_os = "macos")]
        {
            macos_vision_ocr::recognize(&image_data, None)
        }

        #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
        {
            let tmp = std::env::temp_dir()
                .join(format!("quickclipboard-ocr-{}.png", uuid::Uuid::new_v4()));
            std::fs::write(&tmp, &image_data).map_err(|e| format!("写入临时图片失败: {}", e))?;

            let result = recognize_with_tesseract_file(tmp.to_string_lossy().as_ref(), None);
            let _ = std::fs::remove_file(&tmp);
            result
        }
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}

// OCR识别图片文件
#[tauri::command]
pub async fn recognize_file_ocr(file_path: String, language: Option<String>) -> Result<OcrResult, String> {
    tokio::task::spawn_blocking(move || {
        #[cfg(target_os = "windows")]
        {
            use qcocr::recognize_from_file;

            let lang = language.as_deref();
            let result = recognize_from_file(&file_path, lang)
                .map_err(|e| format!("OCR识别失败: {}", e))?;

            convert_ocr_result(result)
        }

        #[cfg(target_os = "macos")]
        {
            let bytes = std::fs::read(&file_path).map_err(|e| format!("读取图片失败: {}", e))?;
            macos_vision_ocr::recognize(&bytes, language.as_deref())
        }

        #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
        {
            recognize_with_tesseract_file(&file_path, language.as_deref())
        }
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}

// 转换OCR结果为返回格式
#[cfg(target_os = "windows")]
fn convert_ocr_result(result: qcocr::OcrRecognitionResult) -> Result<OcrResult, String> {
    let lines = result.lines.iter().map(|line| {
        let words = line.words.iter().map(|word| OcrWord {
            text: word.text.clone(),
            x: word.bounds.x,
            y: word.bounds.y,
            width: word.bounds.width,
            height: word.bounds.height,
        }).collect();
        
        let word_gaps = line.compute_word_gaps();
        
        OcrLine {
            text: line.text.clone(),
            x: line.bounds.x,
            y: line.bounds.y,
            width: line.bounds.width,
            height: line.bounds.height,
            words,
            word_gaps,
        }
    }).collect();
    
    Ok(OcrResult {
        text: result.text,
        lines,
    })
}
