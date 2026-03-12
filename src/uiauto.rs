//! UI Automation module for non-visual UI control
//!
//! This module provides a platform-independent interface for UI automation,
//! allowing the AI to interact with GUI applications without visual understanding.
//!
//! # Platform Support
//! - Windows: Full support via UI Automation
//! - Linux: Full support via AT-SPI2
//! - macOS: Not yet implemented
//!
//! # Usage
//! ```python
//! import uiauto
//!
//! # Find and interact with a window
//! window = uiauto.find_window("Notepad")
//! window.activate()
//!
//! # Find a control with optional timeout
//! edit = window.find_control(name="Edit", timeout_ms=5000)
//! edit.set_text("Hello, World!")
//!
//! # Click a button
//! button = window.find_control(name="OK", control_type="button")
//! button.click()
//!
//! # Global keyboard input
//! uiauto.type_text("Hello")
//! uiauto.press_key("enter", ctrl=True)
//! ```

mod py_module;

#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
mod linux;

pub use py_module::create_uiauto_module;

use std::{
    error::Error,
    fmt::{Display, Formatter, Result as FmtResult},
};

/// Platform-independent error type for UI automation
#[derive(Debug, Clone)]
pub struct UIError {
    pub message: String,
    pub error_type: UIErrorType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum UIErrorType {
    /// Element not found
    NotFound,
    /// Operation not supported on this element
    NotSupported,
    /// Operation failed
    OperationFailed,
    /// Platform not supported (reserved for future platform expansion)
    PlatformNotSupported,
    /// Invalid argument
    InvalidArgument,
    /// Timeout waiting for element
    Timeout,
}

impl Display for UIError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self.error_type {
            UIErrorType::NotFound => write!(f, "Not found: {}", self.message),
            UIErrorType::NotSupported => write!(f, "Not supported: {}", self.message),
            UIErrorType::OperationFailed => write!(f, "Operation failed: {}", self.message),
            UIErrorType::PlatformNotSupported => {
                write!(f, "Platform not supported: {}", self.message)
            }
            UIErrorType::InvalidArgument => write!(f, "Invalid argument: {}", self.message),
            UIErrorType::Timeout => write!(f, "Timeout: {}", self.message),
        }
    }
}

impl Error for UIError {}

/// Platform-independent control type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ControlType {
    Button,
    Calendar,
    CheckBox,
    ComboBox,
    Edit,
    Hyperlink,
    Image,
    ListItem,
    List,
    Menu,
    MenuBar,
    MenuItem,
    ProgressBar,
    RadioButton,
    ScrollBar,
    Slider,
    Spinner,
    StatusBar,
    Tab,
    TabItem,
    Text,
    ToolBar,
    ToolTip,
    Tree,
    TreeItem,
    Window,
    Pane,
    Document,
    Group,
    Unknown,
}

impl ControlType {
    /// Convert from string name
    pub fn from_name(name: &str) -> Self {
        match name.to_lowercase().as_str() {
            "button" => ControlType::Button,
            "calendar" => ControlType::Calendar,
            "checkbox" | "check_box" => ControlType::CheckBox,
            "combobox" | "combo_box" => ControlType::ComboBox,
            "edit" | "text_field" | "textbox" => ControlType::Edit,
            "hyperlink" | "link" => ControlType::Hyperlink,
            "image" => ControlType::Image,
            "listitem" | "list_item" => ControlType::ListItem,
            "list" => ControlType::List,
            "menu" => ControlType::Menu,
            "menubar" | "menu_bar" => ControlType::MenuBar,
            "menuitem" | "menu_item" => ControlType::MenuItem,
            "progressbar" | "progress_bar" => ControlType::ProgressBar,
            "radiobutton" | "radio_button" => ControlType::RadioButton,
            "scrollbar" | "scroll_bar" => ControlType::ScrollBar,
            "slider" => ControlType::Slider,
            "spinner" => ControlType::Spinner,
            "statusbar" | "status_bar" => ControlType::StatusBar,
            "tab" => ControlType::Tab,
            "tabitem" | "tab_item" => ControlType::TabItem,
            "text" | "label" => ControlType::Text,
            "toolbar" | "tool_bar" => ControlType::ToolBar,
            "tooltip" | "tool_tip" => ControlType::ToolTip,
            "tree" => ControlType::Tree,
            "treeitem" | "tree_item" => ControlType::TreeItem,
            "window" => ControlType::Window,
            "pane" | "panel" => ControlType::Pane,
            "document" => ControlType::Document,
            "group" => ControlType::Group,
            _ => ControlType::Unknown,
        }
    }

    //noinspection SpellCheckingInspection
    /// Convert to string name
    pub fn to_name(&self) -> &'static str {
        match self {
            ControlType::Button => "button",
            ControlType::Calendar => "calendar",
            ControlType::CheckBox => "checkbox",
            ControlType::ComboBox => "combobox",
            ControlType::Edit => "edit",
            ControlType::Hyperlink => "hyperlink",
            ControlType::Image => "image",
            ControlType::ListItem => "listitem",
            ControlType::List => "list",
            ControlType::Menu => "menu",
            ControlType::MenuBar => "menubar",
            ControlType::MenuItem => "menuitem",
            ControlType::ProgressBar => "progressbar",
            ControlType::RadioButton => "radiobutton",
            ControlType::ScrollBar => "scrollbar",
            ControlType::Slider => "slider",
            ControlType::Spinner => "spinner",
            ControlType::StatusBar => "statusbar",
            ControlType::Tab => "tab",
            ControlType::TabItem => "tabitem",
            ControlType::Text => "text",
            ControlType::ToolBar => "toolbar",
            ControlType::ToolTip => "tooltip",
            ControlType::Tree => "tree",
            ControlType::TreeItem => "treeitem",
            ControlType::Window => "window",
            ControlType::Pane => "pane",
            ControlType::Document => "document",
            ControlType::Group => "group",
            ControlType::Unknown => "unknown",
        }
    }
}

/// Control information returned by find operations
#[derive(Debug, Clone)]
pub struct ControlInfo {
    /// Unique identifier for this control
    pub id: String,
    /// Control name/label
    pub name: String,
    /// Control type
    pub control_type: ControlType,
    /// Automation ID (if available)
    pub automation_id: Option<String>,
    /// Class name (if available)
    pub class_name: Option<String>,
    /// Whether the control is enabled
    pub is_enabled: bool,
    /// Whether the control is visible
    pub is_visible: bool,
    /// Whether the control is focused
    pub is_focused: bool,
    /// Bounding rectangle (x, y, width, height)
    pub bounds: Option<(i32, i32, i32, i32)>,
}

/// Window information
#[derive(Debug, Clone)]
pub struct WindowInfo {
    /// Window handle/identifier
    pub id: String,
    /// Window title
    pub title: String,
    /// Process ID
    pub process_id: u32,
    /// Process name
    pub process_name: String,
    /// Whether the window is active
    pub is_active: bool,
    /// Whether the window is minimized
    pub is_minimized: bool,
    /// Whether the window is maximized
    pub is_maximized: bool,
    /// Window bounds (x, y, width, height)
    pub bounds: (i32, i32, i32, i32),
}

/// Keyboard modifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KeyModifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
}

/// Mouse button
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// Point for mouse operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

/// Scroll direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollDirection {
    Up,
    Down,
    Left,
    Right,
}

/// Platform-independent UI Automation trait
pub trait UIAutomation {
    /// List all top-level windows
    fn list_windows(&self) -> Result<Vec<WindowInfo>, UIError>;

    /// Find a window by title (partial match)
    fn find_window(&self, title: &str) -> Result<WindowInfo, UIError>;

    /// Find a window by process ID
    fn find_window_by_pid(&self, pid: u32) -> Result<WindowInfo, UIError>;

    /// Wait for a window to appear
    fn wait_for_window(&self, title: &str, timeout_ms: u64) -> Result<WindowInfo, UIError>;

    /// Activate (bring to front) a window
    fn activate_window(&self, window_id: &str) -> Result<(), UIError>;

    /// Close a window
    fn close_window(&self, window_id: &str) -> Result<(), UIError>;

    /// Minimize a window
    fn minimize_window(&self, window_id: &str) -> Result<(), UIError>;

    /// Maximize a window
    fn maximize_window(&self, window_id: &str) -> Result<(), UIError>;

    /// Restore a window
    fn restore_window(&self, window_id: &str) -> Result<(), UIError>;

    /// Find controls in a window
    fn find_controls(
        &self,
        window_id: &str,
        name: Option<&str>,
        control_type: Option<ControlType>,
    ) -> Result<Vec<ControlInfo>, UIError>;

    /// Find a single control
    fn find_control(
        &self,
        window_id: &str,
        name: Option<&str>,
        control_type: Option<ControlType>,
    ) -> Result<ControlInfo, UIError>;

    /// Wait for a control to appear
    fn wait_for_control(
        &self,
        window_id: &str,
        name: Option<&str>,
        control_type: Option<ControlType>,
        timeout_ms: u64,
    ) -> Result<ControlInfo, UIError>;

    /// Wait for controls to appear (at least one)
    fn wait_for_controls(
        &self,
        window_id: &str,
        name: Option<&str>,
        control_type: Option<ControlType>,
        timeout_ms: u64,
    ) -> Result<Vec<ControlInfo>, UIError>;

    /// Click on a control
    fn click(&self, control_id: &str) -> Result<(), UIError>;

    /// Double click on a control
    fn double_click(&self, control_id: &str) -> Result<(), UIError>;

    /// Right click on a control
    fn right_click(&self, control_id: &str) -> Result<(), UIError>;

    /// Get text from a control
    fn get_text(&self, control_id: &str) -> Result<String, UIError>;

    /// Set text in a control
    fn set_text(&self, control_id: &str, text: &str) -> Result<(), UIError>;

    /// Type text (keyboard input)
    fn type_text(&self, text: &str) -> Result<(), UIError>;

    /// Press a key with optional modifiers
    fn press_key(&self, key: &str, modifiers: KeyModifiers) -> Result<(), UIError>;

    /// Scroll in a control
    fn scroll(
        &self,
        control_id: &str,
        direction: ScrollDirection,
        amount: i32,
    ) -> Result<(), UIError>;

    /// Set focus to a control
    fn focus_control(&self, control_id: &str) -> Result<(), UIError>;
}

/// Create a platform-specific UI Automation instance
#[cfg(windows)]
pub fn create_automation() -> Result<Box<dyn UIAutomation>, UIError> {
    Ok(Box::new(windows::WindowsUIAutomation::new()?))
}

/// Create a platform-specific UI Automation instance
#[cfg(target_os = "linux")]
pub fn create_automation() -> Result<Box<dyn UIAutomation>, UIError> {
    Ok(Box::new(linux::LinuxUIAutomation::new()?))
}
