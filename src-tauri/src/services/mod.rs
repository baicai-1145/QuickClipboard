pub mod clipboard;
pub mod database;
pub mod data_management;
pub mod notification;
pub mod settings;
pub mod system;
pub mod paste;
pub mod sound;
pub mod screenshot;
pub mod image_library;
pub mod low_memory;

pub use settings::{AppSettings, get_settings, update_settings, get_data_directory};
pub use notification::show_startup_notification;
pub use system::hotkey;
pub use sound::{SoundPlayer, AppSounds};

pub fn normalize_path_for_hash(path: &str) -> String {
    let normalized = path.replace("\\", "/");
    for prefix in ["clipboard_images/", "pin_images/"] {
        if let Some(idx) = normalized.find(prefix) {
            return normalized[idx..].to_string();
        }
    }
    normalized
}

// 解析存储的路径为实际绝对路径
pub fn resolve_stored_path(stored_path: &str) -> String {
    use std::path::{Path, PathBuf};

    let input = stored_path.trim();
    if input.is_empty() {
        return String::new();
    }

    let as_path = Path::new(input);
    if as_path.is_absolute() {
        return input.to_string();
    }

    let normalized = input.replace('\\', "/");

    let resolve_relative = |relative: &str| -> Option<String> {
        let data_dir = get_data_directory().ok()?;
        let components = relative
            .split('/')
            .filter(|s| !s.is_empty() && *s != ".");
        let mut joined = PathBuf::from(data_dir);
        for c in components {
            joined.push(c);
        }
        Some(joined.to_string_lossy().to_string())
    };

    for prefix in ["clipboard_images/", "pin_images/"] {
        if normalized.starts_with(prefix) {
            if let Some(p) = resolve_relative(&normalized) {
                return p;
            }
        }
    }

    for prefix in ["clipboard_images/", "pin_images/"] {
        if let Some(idx) = normalized.find(prefix) {
            let rel = &normalized[idx..];
            if let Some(p) = resolve_relative(rel) {
                if Path::new(&p).exists() {
                    return p;
                }
            }
        }
    }

    input.to_string()
}

pub fn is_portable_build() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().and_then(|s| s.to_str()).map(|s| s.to_ascii_lowercase()))
        .map(|name| name.contains("portable"))
        .unwrap_or(false)
}
