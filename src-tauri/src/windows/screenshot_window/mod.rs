mod manager;
pub mod capture;
pub mod element_rect;
pub mod ui_automation_types;

#[cfg_attr(not(target_os = "windows"), path = "ui_elements_stub.rs")]
pub mod ui_elements;

#[cfg_attr(not(target_os = "windows"), path = "auto_selection_stub.rs")]
pub mod auto_selection;
pub mod long_screenshot;
mod image_stitcher;

pub use manager::*;
