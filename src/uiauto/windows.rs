//! Windows-specific UI Automation implementation
//!
//! Uses the `uiautomation` crate to provide UI automation capabilities on Windows.

use {
    super::*,
    std::time::Duration,
    uiautomation::{
        UIAutomation, UIElement,
        inputs::{Keyboard, Mouse},
        patterns::{
            UIInvokePattern, UIScrollPattern, UITextPattern, UITogglePattern, UIValuePattern,
            UIWindowPattern,
        },
        types::{ControlType as UIAControlType, Point, ScrollAmount, TreeScope, WindowVisualState},
        variants::Variant,
    },
};

/// Windows UI Automation backend
pub struct WindowsUIAutomation {
    automation: UIAutomation,
    keyboard: Keyboard,
}

impl WindowsUIAutomation {
    /// Create a new Windows UI Automation instance
    pub fn new() -> Result<Self, UIError> {
        let automation = UIAutomation::new().map_err(|e| UIError {
            message: format!("Failed to initialize UI Automation: {}", e),
            error_type: UIErrorType::OperationFailed,
        })?;

        Ok(Self {
            automation,
            keyboard: Keyboard::new(),
        })
    }

    /// Get the root desktop element
    fn get_root(&self) -> Result<UIElement, UIError> {
        self.automation.get_root_element().map_err(|e| UIError {
            message: format!("Failed to get root element: {}", e),
            error_type: UIErrorType::OperationFailed,
        })
    }

    /// Find all top-level windows
    pub fn list_windows(&self) -> Result<Vec<WindowInfo>, UIError> {
        let root = self.get_root()?;
        let condition = self
            .automation
            .create_property_condition(
                uiautomation::types::UIProperty::ControlType,
                Variant::from(UIAControlType::Window as i32),
                None,
            )
            .map_err(|e| UIError {
                message: format!("Failed to create condition: {}", e),
                error_type: UIErrorType::OperationFailed,
            })?;

        let windows = root
            .find_all(TreeScope::Children, &condition)
            .map_err(|e| UIError {
                message: format!("Failed to find windows: {}", e),
                error_type: UIErrorType::OperationFailed,
            })?;

        let mut result = Vec::new();
        for window in windows {
            if let Ok(info) = self.element_to_window_info(&window) {
                result.push(info);
            }
        }

        Ok(result)
    }

    /// Find a window by title (partial match)
    pub fn find_window(&self, title: &str) -> Result<WindowInfo, UIError> {
        let windows = self.list_windows()?;
        windows
            .into_iter()
            .find(|w| w.title.to_lowercase().contains(&title.to_lowercase()))
            .ok_or_else(|| UIError {
                message: format!("Window '{}' not found", title),
                error_type: UIErrorType::NotFound,
            })
    }

    /// Find a window by process ID
    pub fn find_window_by_pid(&self, pid: u32) -> Result<WindowInfo, UIError> {
        let windows = self.list_windows()?;
        windows
            .into_iter()
            .find(|w| w.process_id == pid)
            .ok_or_else(|| UIError {
                message: format!("Window with PID {} not found", pid),
                error_type: UIErrorType::NotFound,
            })
    }

    /// Find an element by its ID string
    fn find_element_by_id(&self, id: &str) -> Result<UIElement, UIError> {
        let root = self.get_root()?;

        // Try to find by automation ID
        if let Ok(condition) = self.automation.create_property_condition(
            uiautomation::types::UIProperty::AutomationId,
            Variant::from(id.to_string()),
            None,
        ) {
            if let Ok(element) = root.find_first(TreeScope::Descendants, &condition) {
                return Ok(element);
            }
        }

        // Try to find by name
        if let Ok(condition) = self.automation.create_property_condition(
            uiautomation::types::UIProperty::Name,
            Variant::from(id.to_string()),
            None,
        ) {
            if let Ok(element) = root.find_first(TreeScope::Descendants, &condition) {
                return Ok(element);
            }
        }

        // Try to parse as "name:type" format
        if let Some(pos) = id.find(':') {
            let name = &id[..pos];
            let type_str = &id[pos + 1..];
            let control_type = ControlType::from_name(type_str);

            let type_condition = self
                .automation
                .create_property_condition(
                    uiautomation::types::UIProperty::ControlType,
                    Variant::from(self.control_type_to_uia(control_type) as i32),
                    None,
                )
                .map_err(|e| UIError {
                    message: format!("Failed to create type condition: {}", e),
                    error_type: UIErrorType::OperationFailed,
                })?;

            let name_condition = self
                .automation
                .create_property_condition(
                    uiautomation::types::UIProperty::Name,
                    Variant::from(name.to_string()),
                    None,
                )
                .map_err(|e| UIError {
                    message: format!("Failed to create name condition: {}", e),
                    error_type: UIErrorType::OperationFailed,
                })?;

            let and_condition = self
                .automation
                .create_and_condition(type_condition, name_condition)
                .map_err(|e| UIError {
                    message: format!("Failed to create and condition: {}", e),
                    error_type: UIErrorType::OperationFailed,
                })?;

            return root.find_first(TreeScope::Descendants, &and_condition).map_err(|e| UIError {
                message: format!("Element '{}' not found: {}", id, e),
                error_type: UIErrorType::NotFound,
            });
        }

        Err(UIError {
            message: format!("Element '{}' not found", id),
            error_type: UIErrorType::NotFound,
        })
    }

    /// Activate (bring to front) a window
    pub fn activate_window(&self, window_id: &str) -> Result<(), UIError> {
        let element = self.find_element_by_id(window_id)?;
        element.set_focus().map_err(|e| UIError {
            message: format!("Failed to activate window: {}", e),
            error_type: UIErrorType::OperationFailed,
        })
    }

    /// Close a window
    pub fn close_window(&self, window_id: &str) -> Result<(), UIError> {
        let element = self.find_element_by_id(window_id)?;
        let pattern: UIWindowPattern = element.get_pattern().map_err(|e| UIError {
            message: format!("Failed to get window pattern: {}", e),
            error_type: UIErrorType::NotSupported,
        })?;
        pattern.close().map_err(|e| UIError {
            message: format!("Failed to close window: {}", e),
            error_type: UIErrorType::OperationFailed,
        })
    }

    /// Minimize a window
    pub fn minimize_window(&self, window_id: &str) -> Result<(), UIError> {
        let element = self.find_element_by_id(window_id)?;
        let pattern: UIWindowPattern = element.get_pattern().map_err(|e| UIError {
            message: format!("Failed to get window pattern: {}", e),
            error_type: UIErrorType::NotSupported,
        })?;
        pattern
            .set_window_visual_state(WindowVisualState::Minimized)
            .map_err(|e| UIError {
                message: format!("Failed to minimize window: {}", e),
                error_type: UIErrorType::OperationFailed,
            })
    }

    /// Maximize a window
    pub fn maximize_window(&self, window_id: &str) -> Result<(), UIError> {
        let element = self.find_element_by_id(window_id)?;
        let pattern: UIWindowPattern = element.get_pattern().map_err(|e| UIError {
            message: format!("Failed to get window pattern: {}", e),
            error_type: UIErrorType::NotSupported,
        })?;
        pattern
            .set_window_visual_state(WindowVisualState::Maximized)
            .map_err(|e| UIError {
                message: format!("Failed to maximize window: {}", e),
                error_type: UIErrorType::OperationFailed,
            })
    }

    /// Restore a window
    pub fn restore_window(&self, window_id: &str) -> Result<(), UIError> {
        let element = self.find_element_by_id(window_id)?;
        let pattern: UIWindowPattern = element.get_pattern().map_err(|e| UIError {
            message: format!("Failed to get window pattern: {}", e),
            error_type: UIErrorType::NotSupported,
        })?;
        pattern
            .set_window_visual_state(WindowVisualState::Normal)
            .map_err(|e| UIError {
                message: format!("Failed to restore window: {}", e),
                error_type: UIErrorType::OperationFailed,
            })
    }

    /// Find controls in a window
    pub fn find_controls(
        &self,
        window_id: &str,
        name: Option<&str>,
        control_type: Option<ControlType>,
    ) -> Result<Vec<ControlInfo>, UIError> {
        let window = self.find_element_by_id(window_id)?;
        let mut result = Vec::new();

        // Get all descendants
        let condition = self.automation.create_true_condition().map_err(|e| UIError {
            message: format!("Failed to create condition: {}", e),
            error_type: UIErrorType::OperationFailed,
        })?;

        let elements = window
            .find_all(TreeScope::Descendants, &condition)
            .map_err(|e| UIError {
                message: format!("Failed to find controls: {}", e),
                error_type: UIErrorType::OperationFailed,
            })?;

        for element in elements {
            if let Ok(info) = self.element_to_control_info(&element) {
                // Filter by name
                if let Some(n) = name {
                    if !info.name.to_lowercase().contains(&n.to_lowercase()) {
                        continue;
                    }
                }
                // Filter by control type
                if let Some(ct) = control_type {
                    if info.control_type != ct {
                        continue;
                    }
                }
                result.push(info);
            }
        }

        Ok(result)
    }

    /// Find a single control
    pub fn find_control(
        &self,
        window_id: &str,
        name: Option<&str>,
        control_type: Option<ControlType>,
    ) -> Result<ControlInfo, UIError> {
        let controls = self.find_controls(window_id, name, control_type)?;
        controls.into_iter().next().ok_or_else(|| UIError {
            message: "Control not found".to_string(),
            error_type: UIErrorType::NotFound,
        })
    }

    /// Click on a control
    pub fn click(&self, control_id: &str) -> Result<(), UIError> {
        let element = self.find_element_by_id(control_id)?;

        // Try invoke pattern first
        if let Ok(pattern) = element.get_pattern::<UIInvokePattern>() {
            if pattern.invoke().is_ok() {
                return Ok(());
            }
        }

        // Try toggle pattern
        if let Ok(pattern) = element.get_pattern::<UITogglePattern>() {
            if pattern.toggle().is_ok() {
                return Ok(());
            }
        }

        // Fall back to click on bounding rect center
        let rect = element.get_bounding_rectangle().map_err(|e| UIError {
            message: format!("Failed to get bounding rectangle: {}", e),
            error_type: UIErrorType::OperationFailed,
        })?;

        let center = Point::new(
            (rect.get_left() + rect.get_right()) / 2,
            (rect.get_top() + rect.get_bottom()) / 2,
        );

        // Use mouse click via input simulation
        let mouse = Mouse::new();
        mouse.click(&center).map_err(|e| UIError {
            message: format!("Failed to click: {}", e),
            error_type: UIErrorType::OperationFailed,
        })
    }

    /// Double click on a control
    pub fn double_click(&self, control_id: &str) -> Result<(), UIError> {
        let element = self.find_element_by_id(control_id)?;
        let rect = element.get_bounding_rectangle().map_err(|e| UIError {
            message: format!("Failed to get bounding rectangle: {}", e),
            error_type: UIErrorType::OperationFailed,
        })?;

        let center = Point::new(
            (rect.get_left() + rect.get_right()) / 2,
            (rect.get_top() + rect.get_bottom()) / 2,
        );

        let mouse = Mouse::new();
        mouse.double_click(&center).map_err(|e| UIError {
            message: format!("Failed to double click: {}", e),
            error_type: UIErrorType::OperationFailed,
        })
    }

    /// Right click on a control
    pub fn right_click(&self, control_id: &str) -> Result<(), UIError> {
        let element = self.find_element_by_id(control_id)?;
        let rect = element.get_bounding_rectangle().map_err(|e| UIError {
            message: format!("Failed to get bounding rectangle: {}", e),
            error_type: UIErrorType::OperationFailed,
        })?;

        let center = Point::new(
            (rect.get_left() + rect.get_right()) / 2,
            (rect.get_top() + rect.get_bottom()) / 2,
        );

        let mouse = Mouse::new();
        mouse.right_click(&center).map_err(|e| UIError {
            message: format!("Failed to right click: {}", e),
            error_type: UIErrorType::OperationFailed,
        })
    }

    /// Get text from a control
    pub fn get_text(&self, control_id: &str) -> Result<String, UIError> {
        let element = self.find_element_by_id(control_id)?;

        // Try value pattern
        if let Ok(pattern) = element.get_pattern::<UIValuePattern>() {
            if let Ok(value) = pattern.get_value() {
                if !value.is_empty() {
                    return Ok(value);
                }
            }
        }

        // Try text pattern
        if let Ok(_pattern) = element.get_pattern::<UITextPattern>() {
            // Text pattern requires getting text range
            // For simplicity, fall back to name
        }

        // Fall back to name property
        element.get_name().map_err(|e| UIError {
            message: format!("Failed to get text: {}", e),
            error_type: UIErrorType::OperationFailed,
        })
    }

    /// Set text in a control
    pub fn set_text(&self, control_id: &str, text: &str) -> Result<(), UIError> {
        let element = self.find_element_by_id(control_id)?;

        // Try value pattern first
        if let Ok(pattern) = element.get_pattern::<UIValuePattern>() {
            if pattern.set_value(text).is_ok() {
                return Ok(());
            }
        }

        // Fall back to typing
        element.set_focus().map_err(|e| UIError {
            message: format!("Failed to focus element: {}", e),
            error_type: UIErrorType::OperationFailed,
        })?;

        // Select all and type new text
        self.keyboard
            .send_keys("{ctrl}a")
            .map_err(|e| UIError {
                message: format!("Failed to select all: {}", e),
                error_type: UIErrorType::OperationFailed,
            })?;

        std::thread::sleep(Duration::from_millis(50));

        self.keyboard.send_text(text).map_err(|e| UIError {
            message: format!("Failed to type text: {}", e),
            error_type: UIErrorType::OperationFailed,
        })?;

        Ok(())
    }

    /// Type text (keyboard input)
    pub fn type_text(&self, text: &str) -> Result<(), UIError> {
        self.keyboard.send_text(text).map_err(|e| UIError {
            message: format!("Failed to type text: {}", e),
            error_type: UIErrorType::OperationFailed,
        })
    }

    /// Press a key with optional modifiers
    pub fn press_key(&self, key: &str, modifiers: KeyModifiers) -> Result<(), UIError> {
        // Build the key string with modifiers
        let mut key_str = String::new();

        if modifiers.ctrl {
            key_str.push_str("{ctrl}");
        }
        if modifiers.alt {
            key_str.push_str("{alt}");
        }
        if modifiers.shift {
            key_str.push_str("{shift}");
        }
        if modifiers.win {
            key_str.push_str("{win}");
        }

        // Add the key
        let key_name = self.parse_key(key)?;
        key_str.push_str(&format!("{{{}}}", key_name));

        self.keyboard.send_keys(&key_str).map_err(|e| UIError {
            message: format!("Failed to press key: {}", e),
            error_type: UIErrorType::OperationFailed,
        })
    }

    /// Parse a key name to UIA key code
    fn parse_key(&self, key: &str) -> Result<String, UIError> {
        let key_lower = key.to_lowercase();
        match key_lower.as_str() {
            "enter" | "return" => Ok("ENTER".to_string()),
            "tab" => Ok("TAB".to_string()),
            "escape" | "esc" => Ok("ESC".to_string()),
            "space" => Ok("SPACE".to_string()),
            "backspace" | "back" => Ok("BACK".to_string()),
            "delete" | "del" => Ok("DELETE".to_string()),
            "insert" | "ins" => Ok("INSERT".to_string()),
            "home" => Ok("HOME".to_string()),
            "end" => Ok("END".to_string()),
            "pageup" | "page_up" => Ok("PGUP".to_string()),
            "pagedown" | "page_down" => Ok("PGDN".to_string()),
            "up" | "arrow_up" => Ok("UP".to_string()),
            "down" | "arrow_down" => Ok("DOWN".to_string()),
            "left" | "arrow_left" => Ok("LEFT".to_string()),
            "right" | "arrow_right" => Ok("RIGHT".to_string()),
            "f1" => Ok("F1".to_string()),
            "f2" => Ok("F2".to_string()),
            "f3" => Ok("F3".to_string()),
            "f4" => Ok("F4".to_string()),
            "f5" => Ok("F5".to_string()),
            "f6" => Ok("F6".to_string()),
            "f7" => Ok("F7".to_string()),
            "f8" => Ok("F8".to_string()),
            "f9" => Ok("F9".to_string()),
            "f10" => Ok("F10".to_string()),
            "f11" => Ok("F11".to_string()),
            "f12" => Ok("F12".to_string()),
            _ if key.len() == 1 => Ok(key.to_uppercase()),
            _ => Err(UIError {
                message: format!("Unknown key: {}", key),
                error_type: UIErrorType::InvalidArgument,
            }),
        }
    }

    /// Scroll in a control
    pub fn scroll(
        &self,
        control_id: &str,
        direction: ScrollDirection,
        _amount: i32,
    ) -> Result<(), UIError> {
        let element = self.find_element_by_id(control_id)?;
        let pattern: UIScrollPattern = element.get_pattern().map_err(|e| UIError {
            message: format!("Failed to get scroll pattern: {}", e),
            error_type: UIErrorType::NotSupported,
        })?;

        let no_scroll = ScrollAmount::NoAmount;
        let large_scroll = ScrollAmount::LargeIncrement;

        match direction {
            ScrollDirection::Up => pattern.scroll(no_scroll, large_scroll),
            ScrollDirection::Down => pattern.scroll(no_scroll, large_scroll),
            ScrollDirection::Left => pattern.scroll(large_scroll, no_scroll),
            ScrollDirection::Right => pattern.scroll(large_scroll, no_scroll),
        }
        .map_err(|e| UIError {
            message: format!("Failed to scroll: {}", e),
            error_type: UIErrorType::OperationFailed,
        })
    }

    /// Set focus to a control
    pub fn focus_control(&self, control_id: &str) -> Result<(), UIError> {
        let element = self.find_element_by_id(control_id)?;
        element.set_focus().map_err(|e| UIError {
            message: format!("Failed to focus control: {}", e),
            error_type: UIErrorType::OperationFailed,
        })
    }

    /// Get control info by ID
    #[allow(dead_code)]
    pub fn get_control_info(&self, control_id: &str) -> Result<ControlInfo, UIError> {
        let element = self.find_element_by_id(control_id)?;
        self.element_to_control_info(&element)
    }

    /// Get window info by ID
    #[allow(dead_code)]
    pub fn get_window_info(&self, window_id: &str) -> Result<WindowInfo, UIError> {
        let element = self.find_element_by_id(window_id)?;
        self.element_to_window_info(&element)
    }

    /// Wait for a window to appear
    pub fn wait_for_window(&self, title: &str, timeout_ms: u64) -> Result<WindowInfo, UIError> {
        let start = std::time::Instant::now();
        let timeout = Duration::from_millis(timeout_ms);

        loop {
            if let Ok(window) = self.find_window(title) {
                return Ok(window);
            }

            if start.elapsed() >= timeout {
                return Err(UIError {
                    message: format!("Timeout waiting for window '{}'", title),
                    error_type: UIErrorType::Timeout,
                });
            }

            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// Wait for a control to appear
    pub fn wait_for_control(
        &self,
        window_id: &str,
        name: Option<&str>,
        control_type: Option<ControlType>,
        timeout_ms: u64,
    ) -> Result<ControlInfo, UIError> {
        let start = std::time::Instant::now();
        let timeout = Duration::from_millis(timeout_ms);

        loop {
            if let Ok(control) = self.find_control(window_id, name, control_type) {
                return Ok(control);
            }

            if start.elapsed() >= timeout {
                return Err(UIError {
                    message: "Timeout waiting for control".to_string(),
                    error_type: UIErrorType::Timeout,
                });
            }

            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// Wait for controls to appear (at least one)
    pub fn wait_for_controls(
        &self,
        window_id: &str,
        name: Option<&str>,
        control_type: Option<ControlType>,
        timeout_ms: u64,
    ) -> Result<Vec<ControlInfo>, UIError> {
        let start = std::time::Instant::now();
        let timeout = Duration::from_millis(timeout_ms);

        loop {
            let controls = self.find_controls(window_id, name, control_type)?;
            if !controls.is_empty() {
                return Ok(controls);
            }

            if start.elapsed() >= timeout {
                return Err(UIError {
                    message: "Timeout waiting for controls".to_string(),
                    error_type: UIErrorType::Timeout,
                });
            }

            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// Convert UIElement to WindowInfo
    fn element_to_window_info(&self, element: &UIElement) -> Result<WindowInfo, UIError> {
        let name = element.get_name().unwrap_or_default();
        let process_id = element.get_process_id().unwrap_or(0);

        // Get process name
        let process_name = get_process_name(process_id);

        // Check window state
        let (is_minimized, is_maximized) =
            if let Ok(pattern) = element.get_pattern::<UIWindowPattern>() {
                let state = pattern.get_window_visual_state().unwrap_or(WindowVisualState::Normal);
                (
                    state == WindowVisualState::Minimized,
                    state == WindowVisualState::Maximized,
                )
            } else {
                (false, false)
            };

        // Check if active (has focus)
        let is_active = element.has_keyboard_focus().unwrap_or(false);

        // Get bounds
        let bounds = if let Ok(rect) = element.get_bounding_rectangle() {
            (
                rect.get_left(),
                rect.get_top(),
                rect.get_right() - rect.get_left(),
                rect.get_bottom() - rect.get_top(),
            )
        } else {
            (0, 0, 0, 0)
        };

        // Generate unique ID
        let id = generate_element_id(element, &name);

        Ok(WindowInfo {
            id,
            title: name,
            process_id,
            process_name,
            is_active,
            is_minimized,
            is_maximized,
            bounds,
        })
    }

    /// Convert UIElement to ControlInfo
    fn element_to_control_info(&self, element: &UIElement) -> Result<ControlInfo, UIError> {
        let name = element.get_name().unwrap_or_default();
        let control_type = self.uia_control_type_to_local(
            element.get_control_type().unwrap_or(UIAControlType::Custom),
        );
        let automation_id = element.get_automation_id().ok();
        let class_name = element.get_classname().ok();
        let is_enabled = element.is_enabled().unwrap_or(false);
        let is_visible = !element.is_offscreen().unwrap_or(true);
        let is_focused = element.has_keyboard_focus().unwrap_or(false);

        let bounds = if let Ok(rect) = element.get_bounding_rectangle() {
            Some((
                rect.get_left(),
                rect.get_top(),
                rect.get_right() - rect.get_left(),
                rect.get_bottom() - rect.get_top(),
            ))
        } else {
            None
        };

        let id = generate_element_id(element, &name);

        Ok(ControlInfo {
            id,
            name,
            control_type,
            automation_id,
            class_name,
            is_enabled,
            is_visible,
            is_focused,
            bounds,
        })
    }

    /// Convert local ControlType to UIA ControlType
    fn control_type_to_uia(&self, ct: ControlType) -> UIAControlType {
        match ct {
            ControlType::Button => UIAControlType::Button,
            ControlType::Calendar => UIAControlType::Calendar,
            ControlType::CheckBox => UIAControlType::CheckBox,
            ControlType::ComboBox => UIAControlType::ComboBox,
            ControlType::Edit => UIAControlType::Edit,
            ControlType::Hyperlink => UIAControlType::Hyperlink,
            ControlType::Image => UIAControlType::Image,
            ControlType::ListItem => UIAControlType::ListItem,
            ControlType::List => UIAControlType::List,
            ControlType::Menu => UIAControlType::Menu,
            ControlType::MenuBar => UIAControlType::MenuBar,
            ControlType::MenuItem => UIAControlType::MenuItem,
            ControlType::ProgressBar => UIAControlType::ProgressBar,
            ControlType::RadioButton => UIAControlType::RadioButton,
            ControlType::ScrollBar => UIAControlType::ScrollBar,
            ControlType::Slider => UIAControlType::Slider,
            ControlType::Spinner => UIAControlType::Spinner,
            ControlType::StatusBar => UIAControlType::StatusBar,
            ControlType::Tab => UIAControlType::Tab,
            ControlType::TabItem => UIAControlType::TabItem,
            ControlType::Text => UIAControlType::Text,
            ControlType::ToolBar => UIAControlType::ToolBar,
            ControlType::ToolTip => UIAControlType::ToolTip,
            ControlType::Tree => UIAControlType::Tree,
            ControlType::TreeItem => UIAControlType::TreeItem,
            ControlType::Window => UIAControlType::Window,
            ControlType::Pane => UIAControlType::Pane,
            ControlType::Document => UIAControlType::Document,
            ControlType::Group => UIAControlType::Group,
            ControlType::Unknown => UIAControlType::Custom,
        }
    }

    /// Convert UIA ControlType to local ControlType
    fn uia_control_type_to_local(&self, ct: UIAControlType) -> ControlType {
        match ct {
            UIAControlType::Button => ControlType::Button,
            UIAControlType::Calendar => ControlType::Calendar,
            UIAControlType::CheckBox => ControlType::CheckBox,
            UIAControlType::ComboBox => ControlType::ComboBox,
            UIAControlType::Edit => ControlType::Edit,
            UIAControlType::Hyperlink => ControlType::Hyperlink,
            UIAControlType::Image => ControlType::Image,
            UIAControlType::ListItem => ControlType::ListItem,
            UIAControlType::List => ControlType::List,
            UIAControlType::Menu => ControlType::Menu,
            UIAControlType::MenuBar => ControlType::MenuBar,
            UIAControlType::MenuItem => ControlType::MenuItem,
            UIAControlType::ProgressBar => ControlType::ProgressBar,
            UIAControlType::RadioButton => ControlType::RadioButton,
            UIAControlType::ScrollBar => ControlType::ScrollBar,
            UIAControlType::Slider => ControlType::Slider,
            UIAControlType::Spinner => ControlType::Spinner,
            UIAControlType::StatusBar => ControlType::StatusBar,
            UIAControlType::Tab => ControlType::Tab,
            UIAControlType::TabItem => ControlType::TabItem,
            UIAControlType::Text => ControlType::Text,
            UIAControlType::ToolBar => ControlType::ToolBar,
            UIAControlType::ToolTip => ControlType::ToolTip,
            UIAControlType::Tree => ControlType::Tree,
            UIAControlType::TreeItem => ControlType::TreeItem,
            UIAControlType::Window => ControlType::Window,
            UIAControlType::Pane => ControlType::Pane,
            UIAControlType::Document => ControlType::Document,
            UIAControlType::Group => ControlType::Group,
            _ => ControlType::Unknown,
        }
    }
}

/// Generate a unique element ID
fn generate_element_id(element: &UIElement, name: &str) -> String {
    // Use a combination of properties to create a stable ID
    let control_type = element
        .get_control_type()
        .map(|ct| format!("{:?}", ct))
        .unwrap_or_default();

    let automation_id = element.get_automation_id().unwrap_or_default();

    if !automation_id.is_empty() {
        format!("{}:{}", automation_id, control_type.to_lowercase())
    } else if !name.is_empty() {
        format!("{}:{}", name, control_type.to_lowercase())
    } else {
        format!("element:{}", control_type.to_lowercase())
    }
}

/// Get process name from PID
fn get_process_name(pid: u32) -> String {
    use std::process::Command;

    #[cfg(windows)]
    {
        if let Ok(output) = Command::new("tasklist")
            .args(["/FI", &format!("PID eq {}", pid), "/FO", "CSV", "/NH"])
            .output()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Parse CSV output: "notepad.exe","1234","Console","1","5,000 K"
            if let Some(line) = stdout.lines().next() {
                let parts: Vec<&str> = line.split(',').collect();
                if !parts.is_empty() {
                    return parts[0].trim_matches('"').to_string();
                }
            }
        }
    }

    format!("pid:{}", pid)
}

impl Default for WindowsUIAutomation {
    fn default() -> Self {
        Self::new().expect("Failed to initialize UI Automation")
    }
}
