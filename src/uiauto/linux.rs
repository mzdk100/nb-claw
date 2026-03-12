//! Linux-specific UI Automation implementation via AT-SPI2
//!
//! Uses the `atspi` crate to communicate with the AT-SPI2 accessibility daemon.

use {
    super::{UIAutomation as UIAutomationTrait, *},
    atspi::{AccessibilityConnection, Role, State, connection::set_session_accessibility},
    enigo::{Enigo, Key, Keyboard, Mouse, MouseButton as EnigoMouseButton, MouseControllable},
    std::{cell::RefCell, thread::sleep, time::Duration},
    tokio::runtime::Runtime,
    tracing::{debug, warn},
    zbus::Connection,
};

//noinspection SpellCheckingInspection
/// Linux UI Automation backend using AT-SPI2
pub struct LinuxUIAutomation {
    connection: AccessibilityConnection,
    enigo: RefCell<Enigo>,
}

//noinspection SpellCheckingInspection
impl LinuxUIAutomation {
    /// Create a new Linux UI Automation instance
    pub fn new() -> Result<Self, UIError> {
        // Enable accessibility in the session
        let rt = Runtime::new().map_err(|e| UIError {
            message: format!("Failed to create tokio runtime: {}", e),
            error_type: UIErrorType::OperationFailed,
        })?;

        rt.block_on(async {
            // Try to enable accessibility
            let session_conn = Connection::session().await.map_err(|e| UIError {
                message: format!("Failed to connect to session bus: {}", e),
                error_type: UIErrorType::OperationFailed,
            })?;

            let _ = set_session_accessibility(&session_conn, true).await;
            debug!("Accessibility enabled in session");

            // Connect to AT-SPI
            let connection = AccessibilityConnection::new().await.map_err(|e| UIError {
                message: format!("Failed to connect to AT-SPI: {}", e),
                error_type: UIErrorType::OperationFailed,
            })?;

            let enigo = Enigo::new(&enigo::Settings::default()).map_err(|e| UIError {
                message: format!("Failed to initialize input system: {}", e),
                error_type: UIErrorType::OperationFailed,
            })?;

            Ok(Self {
                connection,
                enigo: RefCell::new(enigo),
            })
        })
    }

    /// Get the registry (root) accessible
    pub fn get_registry(
        &self,
    ) -> Result<atspi::proxy::accessible::AccessibleProxy<'static>, UIError> {
        let rt = tokio::runtime::Runtime::new().map_err(|e| UIError {
            message: format!("Failed to create runtime: {}", e),
            error_type: UIErrorType::OperationFailed,
        })?;

        rt.block_on(async {
            atspi::proxy::accessible::AccessibleProxy::new(self.connection.connection())
                .await
                .map_err(|e| UIError {
                    message: format!("Failed to get registry: {}", e),
                    error_type: UIErrorType::OperationFailed,
                })
        })
    }

    /// List all top-level windows (applications)
    fn list_windows_impl(&self) -> Result<Vec<WindowInfo>, UIError> {
        let rt = tokio::runtime::Runtime::new().map_err(|e| UIError {
            message: format!("Failed to create runtime: {}", e),
            error_type: UIErrorType::OperationFailed,
        })?;

        rt.block_on(async {
            let registry = self.get_registry()?;
            let children = registry.get_children().await.map_err(|e| UIError {
                message: format!("Failed to get children: {}", e),
                error_type: UIErrorType::OperationFailed,
            })?;

            let mut result = Vec::new();
            for child_ref in children {
                if let Ok(info) = self.object_ref_to_window_info(&child_ref).await {
                    result.push(info);
                }
            }

            Ok(result)
        })
    }

    /// Find a window by title (partial match)
    fn find_window_impl(&self, title: &str) -> Result<WindowInfo, UIError> {
        let windows = self.list_windows_impl()?;
        windows
            .into_iter()
            .find(|w| w.title.to_lowercase().contains(&title.to_lowercase()))
            .ok_or_else(|| UIError {
                message: format!("Window '{}' not found", title),
                error_type: UIErrorType::NotFound,
            })
    }

    /// Find a window by process ID
    fn find_window_by_pid_impl(&self, pid: u32) -> Result<WindowInfo, UIError> {
        let windows = self.list_windows_impl()?;
        windows
            .into_iter()
            .find(|w| w.process_id == pid)
            .ok_or_else(|| UIError {
                message: format!("Window with PID {} not found", pid),
                error_type: UIErrorType::NotFound,
            })
    }

    /// Activate (bring to front) a window
    fn activate_window_impl(&self, _window_id: &str) -> Result<(), UIError> {
        // AT-SPI doesn't have a direct "activate window" method
        // We can try to grab focus on the window's component
        warn!("activate_window is limited on Linux - may not work for all applications");
        Ok(())
    }

    /// Close a window
    fn close_window_impl(&self, window_id: &str) -> Result<(), UIError> {
        let rt = tokio::runtime::Runtime::new().map_err(|e| UIError {
            message: format!("Failed to create runtime: {}", e),
            error_type: UIErrorType::OperationFailed,
        })?;

        rt.block_on(async {
            let proxy = self.get_accessible_proxy(window_id).await?;

            // Try to find and invoke "close" action
            if let Ok(action) = self.get_action_proxy(&proxy).await {
                let actions = action.get_actions().await.unwrap_or_default();
                for (i, action_info) in actions.iter().enumerate() {
                    if action_info.name.to_lowercase().contains("close") {
                        action.do_action(i as i32).await.ok();
                        return Ok(());
                    }
                }
            }

            Err(UIError {
                message: "Close action not available".to_string(),
                error_type: UIErrorType::NotSupported,
            })
        })
    }

    /// Minimize a window (limited support on Linux)
    fn minimize_window_impl(&self, _window_id: &str) -> Result<(), UIError> {
        warn!("minimize_window is limited on Linux");
        Ok(())
    }

    /// Maximize a window (limited support on Linux)
    fn maximize_window_impl(&self, _window_id: &str) -> Result<(), UIError> {
        warn!("maximize_window is limited on Linux");
        Ok(())
    }

    /// Restore a window (limited support on Linux)
    fn restore_window_impl(&self, _window_id: &str) -> Result<(), UIError> {
        warn!("restore_window is limited on Linux");
        Ok(())
    }

    /// Find controls in a window
    fn find_controls_impl(
        &self,
        window_id: &str,
        name: Option<&str>,
        control_type: Option<ControlType>,
    ) -> Result<Vec<ControlInfo>, UIError> {
        let rt = tokio::runtime::Runtime::new().map_err(|e| UIError {
            message: format!("Failed to create runtime: {}", e),
            error_type: UIErrorType::OperationFailed,
        })?;

        rt.block_on(async {
            let proxy = self.get_accessible_proxy(window_id).await?;
            let mut result = Vec::new();

            self.collect_controls_recursive(&proxy, name, control_type, &mut result)
                .await?;

            Ok(result)
        })
    }

    /// Find a single control
    fn find_control_impl(
        &self,
        window_id: &str,
        name: Option<&str>,
        control_type: Option<ControlType>,
    ) -> Result<ControlInfo, UIError> {
        let controls = self.find_controls_impl(window_id, name, control_type)?;
        controls.into_iter().next().ok_or_else(|| UIError {
            message: "Control not found".to_string(),
            error_type: UIErrorType::NotFound,
        })
    }

    /// Wait for a window to appear
    fn wait_for_window_impl(&self, title: &str, timeout_ms: u64) -> Result<WindowInfo, UIError> {
        let start = std::time::Instant::now();
        let timeout = Duration::from_millis(timeout_ms);

        loop {
            if let Ok(window) = self.find_window_impl(title) {
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
    fn wait_for_control_impl(
        &self,
        window_id: &str,
        name: Option<&str>,
        control_type: Option<ControlType>,
        timeout_ms: u64,
    ) -> Result<ControlInfo, UIError> {
        let start = std::time::Instant::now();
        let timeout = Duration::from_millis(timeout_ms);

        loop {
            if let Ok(control) = self.find_control_impl(window_id, name, control_type) {
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
    fn wait_for_controls_impl(
        &self,
        window_id: &str,
        name: Option<&str>,
        control_type: Option<ControlType>,
        timeout_ms: u64,
    ) -> Result<Vec<ControlInfo>, UIError> {
        let start = std::time::Instant::now();
        let timeout = Duration::from_millis(timeout_ms);

        loop {
            let controls = self.find_controls_impl(window_id, name, control_type)?;
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

    /// Click on a control
    fn click_impl(&self, control_id: &str) -> Result<(), UIError> {
        let rt = tokio::runtime::Runtime::new().map_err(|e| UIError {
            message: format!("Failed to create runtime: {}", e),
            error_type: UIErrorType::OperationFailed,
        })?;

        rt.block_on(async {
            let proxy = self.get_accessible_proxy(control_id).await?;

            // Try Action interface first
            if let Ok(action) = self.get_action_proxy(&proxy).await {
                if action.nactions().await.unwrap_or(0) > 0 {
                    // First action is usually the default "click"
                    action.do_action(0).await.ok();
                    return Ok(());
                }
            }

            // Fall back to mouse click at component position
            if let Ok(component) = self.get_component_proxy(&proxy).await {
                if let Ok((x, y, width, height)) =
                    component.get_extents(atspi::CoordType::Screen).await
                {
                    let center_x = x + width / 2;
                    let center_y = y + height / 2;

                    let mut enigo = self.enigo.borrow_mut();
                    enigo.mouse_move_to(center_x, center_y);
                    enigo.mouse_click(EnigoMouseButton::Left);

                    return Ok(());
                }
            }

            Err(UIError {
                message: "Click not supported on this element".to_string(),
                error_type: UIErrorType::NotSupported,
            })
        })
    }

    /// Double click on a control
    fn double_click_impl(&self, control_id: &str) -> Result<(), UIError> {
        let rt = tokio::runtime::Runtime::new().map_err(|e| UIError {
            message: format!("Failed to create runtime: {}", e),
            error_type: UIErrorType::OperationFailed,
        })?;

        rt.block_on(async {
            let proxy = self.get_accessible_proxy(control_id).await?;

            if let Ok(component) = self.get_component_proxy(&proxy).await {
                if let Ok((x, y, width, height)) =
                    component.get_extents(atspi::CoordType::Screen).await
                {
                    let center_x = x + width / 2;
                    let center_y = y + height / 2;

                    let mut enigo = self.enigo.borrow_mut();
                    enigo.mouse_move_to(center_x, center_y);
                    enigo.mouse_click(EnigoMouseButton::Left);
                    std::thread::sleep(Duration::from_millis(50));
                    enigo.mouse_click(EnigoMouseButton::Left);

                    return Ok(());
                }
            }

            Err(UIError {
                message: "Double click not supported".to_string(),
                error_type: UIErrorType::NotSupported,
            })
        })
    }

    /// Right click on a control
    fn right_click_impl(&self, control_id: &str) -> Result<(), UIError> {
        let rt = tokio::runtime::Runtime::new().map_err(|e| UIError {
            message: format!("Failed to create runtime: {}", e),
            error_type: UIErrorType::OperationFailed,
        })?;

        rt.block_on(async {
            let proxy = self.get_accessible_proxy(control_id).await?;

            if let Ok(component) = self.get_component_proxy(&proxy).await {
                if let Ok((x, y, width, height)) =
                    component.get_extents(atspi::CoordType::Screen).await
                {
                    let center_x = x + width / 2;
                    let center_y = y + height / 2;

                    let mut enigo = self.enigo.borrow_mut();
                    enigo.mouse_move_to(center_x, center_y);
                    enigo.mouse_click(EnigoMouseButton::Right);

                    return Ok(());
                }
            }

            Err(UIError {
                message: "Right click not supported".to_string(),
                error_type: UIErrorType::NotSupported,
            })
        })
    }

    /// Get text from a control
    fn get_text_impl(&self, control_id: &str) -> Result<String, UIError> {
        let rt = tokio::runtime::Runtime::new().map_err(|e| UIError {
            message: format!("Failed to create runtime: {}", e),
            error_type: UIErrorType::OperationFailed,
        })?;

        rt.block_on(async {
            let proxy = self.get_accessible_proxy(control_id).await?;

            // Try Text interface
            if let Ok(text) = self.get_text_proxy(&proxy).await {
                if let Ok(count) = text.character_count().await {
                    if count > 0 {
                        if let Ok(text_content) = text.get_text(0, count).await {
                            return Ok(text_content);
                        }
                    }
                }
            }

            // Try Value interface
            if let Ok(value) = self.get_value_proxy(&proxy).await {
                if let Ok(text) = value.text().await {
                    if !text.is_empty() {
                        return Ok(text);
                    }
                }
            }

            // Fall back to name
            proxy.name().await.map_err(|e| UIError {
                message: format!("Failed to get text: {}", e),
                error_type: UIErrorType::OperationFailed,
            })
        })
    }

    /// Set text in a control
    fn set_text_impl(&self, control_id: &str, text: &str) -> Result<(), UIError> {
        let rt = tokio::runtime::Runtime::new().map_err(|e| UIError {
            message: format!("Failed to create runtime: {}", e),
            error_type: UIErrorType::OperationFailed,
        })?;

        rt.block_on(async {
            let proxy = self.get_accessible_proxy(control_id).await?;

            // Try EditableText interface
            if let Ok(editable) = self.get_editable_text_proxy(&proxy).await {
                if editable.set_text_contents(text).await.is_ok() {
                    return Ok(());
                }
            }

            // Try Value interface
            if let Ok(value) = self.get_value_proxy(&proxy).await {
                if value.set_current_value(text.len() as f64).await.is_ok() {
                    // This won't work for text, try keyboard input
                }
            }

            // Fall back to keyboard input
            // First, focus the element
            if let Ok(component) = self.get_component_proxy(&proxy).await {
                let _ = component.grab_focus().await;
            }

            // Select all and type
            std::thread::sleep(Duration::from_millis(50));

            {
                let mut enigo = self.enigo.borrow_mut();
                enigo.key(Key::Control, enigo::Direction::Press);
                enigo.key(Key::Unicode('a'), enigo::Direction::Click);
                enigo.key(Key::Control, enigo::Direction::Release);

                std::thread::sleep(Duration::from_millis(50));

                enigo.text(text);
            }

            Ok(())
        })
    }

    /// Type text (keyboard input)
    fn type_text_impl(&self, text: &str) -> Result<(), UIError> {
        self.enigo.borrow_mut().text(text);
        Ok(())
    }

    /// Press a key with optional modifiers
    fn press_key_impl(&self, key: &str, modifiers: KeyModifiers) -> Result<(), UIError> {
        let enigo_key = self.parse_key(key)?;

        let mut enigo = self.enigo.borrow_mut();

        if modifiers.ctrl {
            enigo.key(Key::Control, enigo::Direction::Press);
        }
        if modifiers.alt {
            enigo.key(Key::Alt, enigo::Direction::Press);
        }
        if modifiers.shift {
            enigo.key(Key::Shift, enigo::Direction::Press);
        }
        if modifiers.win {
            enigo.key(Key::Meta, enigo::Direction::Press);
        }

        enigo.key(enigo_key, enigo::Direction::Click);

        if modifiers.win {
            enigo.key(Key::Meta, enigo::Direction::Release);
        }
        if modifiers.shift {
            enigo.key(Key::Shift, enigo::Direction::Release);
        }
        if modifiers.alt {
            enigo.key(Key::Alt, enigo::Direction::Release);
        }
        if modifiers.ctrl {
            enigo.key(Key::Control, enigo::Direction::Release);
        }

        Ok(())
    }

    /// Scroll in a control
    fn scroll_impl(
        &self,
        control_id: &str,
        direction: ScrollDirection,
        _amount: i32,
    ) -> Result<(), UIError> {
        let rt = tokio::runtime::Runtime::new().map_err(|e| UIError {
            message: format!("Failed to create runtime: {}", e),
            error_type: UIErrorType::OperationFailed,
        })?;

        rt.block_on(async {
            let proxy = self.get_accessible_proxy(control_id).await?;

            // AT-SPI doesn't have a direct scroll method, use keyboard
            if let Ok(component) = self.get_component_proxy(&proxy).await {
                let _ = component.grab_focus().await;
            }

            sleep(Duration::from_millis(50));

            {
                let mut enigo = self.enigo.borrow_mut();
                match direction {
                    ScrollDirection::Up => {
                        enigo.key(Key::PageUp, enigo::Direction::Click);
                    }
                    ScrollDirection::Down => {
                        enigo.key(Key::PageDown, enigo::Direction::Click);
                    }
                    ScrollDirection::Left => {
                        enigo.key(Key::Control, enigo::Direction::Press);
                        enigo.key(Key::PageUp, enigo::Direction::Click);
                        enigo.key(Key::Control, enigo::Direction::Release);
                    }
                    ScrollDirection::Right => {
                        enigo.key(Key::Control, enigo::Direction::Press);
                        enigo.key(Key::PageDown, enigo::Direction::Click);
                        enigo.key(Key::Control, enigo::Direction::Release);
                    }
                }
            }

            Ok(())
        })
    }

    /// Set focus to a control
    fn focus_control_impl(&self, control_id: &str) -> Result<(), UIError> {
        let rt = tokio::runtime::Runtime::new().map_err(|e| UIError {
            message: format!("Failed to create runtime: {}", e),
            error_type: UIErrorType::OperationFailed,
        })?;

        rt.block_on(async {
            let proxy = self.get_accessible_proxy(control_id).await?;

            if let Ok(component) = self.get_component_proxy(&proxy).await {
                component.grab_focus().await.map_err(|e| UIError {
                    message: format!("Failed to focus: {}", e),
                    error_type: UIErrorType::OperationFailed,
                })?;
            }

            Ok(())
        })
    }
}

//noinspection SpellCheckingInspection
impl LinuxUIAutomation {
    // --- Helper methods ---

    async fn get_accessible_proxy(
        &self,
        id: &str,
    ) -> Result<atspi::proxy::accessible::AccessibleProxy<'static>, UIError> {
        // Parse id format: "bus_name:path" or just search
        let (bus_name, path) = if let Some(pos) = id.find(':') {
            let bus = &id[..pos];
            let path = &id[pos + 1..];
            (bus.to_string(), format!("/{}", path.replace(':', "/")))
        } else {
            // Default registry
            (
                "org.a11y.atspi.Registry".to_string(),
                "/org/a11y/atspi/accessible/root".to_string(),
            )
        };

        atspi::proxy::accessible::AccessibleProxy::builder(self.connection.connection())
            .destination(bus_name)?
            .path(path)?
            .build()
            .await
            .map_err(|e| UIError {
                message: format!("Failed to create proxy: {}", e),
                error_type: UIErrorType::NotFound,
            })
    }

    async fn get_action_proxy(
        &self,
        accessible: &atspi::proxy::accessible::AccessibleProxy<'_>,
    ) -> Result<atspi::proxy::action::ActionProxy<'_>, UIError> {
        atspi::proxy::action::ActionProxy::builder(self.connection.connection())
            .destination(accessible.inner().destination().clone())?
            .path(accessible.inner().path().clone())?
            .build()
            .await
            .map_err(|e| UIError {
                message: format!("Failed to get action proxy: {}", e),
                error_type: UIErrorType::NotSupported,
            })
    }

    async fn get_component_proxy(
        &self,
        accessible: &atspi::proxy::accessible::AccessibleProxy<'_>,
    ) -> Result<atspi::proxy::component::ComponentProxy<'_>, UIError> {
        atspi::proxy::component::ComponentProxy::builder(self.connection.connection())
            .destination(accessible.inner().destination().clone())?
            .path(accessible.inner().path().clone())?
            .build()
            .await
            .map_err(|e| UIError {
                message: format!("Failed to get component proxy: {}", e),
                error_type: UIErrorType::NotSupported,
            })
    }

    async fn get_text_proxy(
        &self,
        accessible: &atspi::proxy::accessible::AccessibleProxy<'_>,
    ) -> Result<atspi::proxy::text::TextProxy<'_>, UIError> {
        atspi::proxy::text::TextProxy::builder(self.connection.connection())
            .destination(accessible.inner().destination().clone())?
            .path(accessible.inner().path().clone())?
            .build()
            .await
            .map_err(|e| UIError {
                message: format!("Failed to get text proxy: {}", e),
                error_type: UIErrorType::NotSupported,
            })
    }

    async fn get_editable_text_proxy(
        &self,
        accessible: &atspi::proxy::accessible::AccessibleProxy<'_>,
    ) -> Result<atspi::proxy::editable_text::EditableTextProxy<'_>, UIError> {
        atspi::proxy::editable_text::EditableTextProxy::builder(self.connection.connection())
            .destination(accessible.inner().destination().clone())?
            .path(accessible.inner().path().clone())?
            .build()
            .await
            .map_err(|e| UIError {
                message: format!("Failed to get editable text proxy: {}", e),
                error_type: UIErrorType::NotSupported,
            })
    }

    async fn get_value_proxy(
        &self,
        accessible: &atspi::proxy::accessible::AccessibleProxy<'_>,
    ) -> Result<atspi::proxy::value::ValueProxy<'_>, UIError> {
        atspi::proxy::value::ValueProxy::builder(self.connection.connection())
            .destination(accessible.inner().destination().clone())?
            .path(accessible.inner().path().clone())?
            .build()
            .await
            .map_err(|e| UIError {
                message: format!("Failed to get value proxy: {}", e),
                error_type: UIErrorType::NotSupported,
            })
    }

    async fn collect_controls_recursive(
        &self,
        proxy: &atspi::proxy::accessible::AccessibleProxy<'_>,
        name_filter: Option<&str>,
        type_filter: Option<ControlType>,
        result: &mut Vec<ControlInfo>,
    ) -> Result<(), UIError> {
        // Get info for this element
        if let Ok(info) = self.accessible_to_control_info(proxy).await {
            let name_match = name_filter.map_or(true, |n| {
                info.name.to_lowercase().contains(&n.to_lowercase())
            });
            let type_match = type_filter.map_or(true, |t| info.control_type == t);

            if name_match && type_match {
                result.push(info);
            }
        }

        // Recurse into children
        let children = proxy.get_children().await.unwrap_or_default();
        for child_ref in children {
            if let Ok(child_proxy) = self.get_accessible_proxy_from_ref(&child_ref).await {
                Box::pin(self.collect_controls_recursive(
                    &child_proxy,
                    name_filter,
                    type_filter,
                    result,
                ))
                .await?;
            }
        }

        Ok(())
    }

    async fn get_accessible_proxy_from_ref(
        &self,
        obj_ref: &atspi::ObjectRefOwned,
    ) -> Result<atspi::proxy::accessible::AccessibleProxy<'static>, UIError> {
        atspi::proxy::accessible::AccessibleProxy::builder(self.connection.connection())
            .destination(obj_ref.name.clone())?
            .path(obj_ref.path.clone())?
            .build()
            .await
            .map_err(|e| UIError {
                message: format!("Failed to create proxy from ref: {}", e),
                error_type: UIErrorType::OperationFailed,
            })
    }

    async fn object_ref_to_window_info(
        &self,
        obj_ref: &atspi::ObjectRefOwned,
    ) -> Result<WindowInfo, UIError> {
        let proxy = self.get_accessible_proxy_from_ref(obj_ref).await?;
        self.accessible_to_window_info(&proxy).await
    }

    async fn accessible_to_window_info(
        &self,
        proxy: &atspi::proxy::accessible::AccessibleProxy<'_>,
    ) -> Result<WindowInfo, UIError> {
        let name = proxy.name().await.unwrap_or_default();
        let _role = proxy.get_role().await.unwrap_or(Role::Invalid);

        // Get process info from application
        let app = proxy.get_application().await.ok();
        let (process_id, process_name) = if let Some(app_ref) = app {
            let app_proxy = self.get_accessible_proxy_from_ref(&app_ref).await.ok();
            if let Some(app) = app_proxy {
                let pid = app.inner().destination().to_string().parse().unwrap_or(0);
                (pid, format!("pid:{}", pid))
            } else {
                (0, "unknown".to_string())
            }
        } else {
            (0, "unknown".to_string())
        };

        // Get bounds
        let bounds = if let Ok(component) = self.get_component_proxy(proxy).await {
            component
                .get_extents(atspi::CoordType::Screen)
                .await
                .map(|(x, y, w, h)| (x, y, w, h))
                .unwrap_or((0, 0, 0, 0))
        } else {
            (0, 0, 0, 0)
        };

        // Get state
        let (is_active, is_minimized, is_maximized) = if let Ok(states) = proxy.get_state().await {
            (
                states.contains(State::Active),
                states.contains(State::Iconified),
                false, // AT-SPI doesn't have a direct "maximized" state
            )
        } else {
            (false, false, false)
        };

        // Generate ID
        let bus = proxy.inner().destination().to_string();
        let path = proxy.inner().path().to_string();
        let id = format!("{}:{}", bus, path.replace('/', ":"));

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

    async fn accessible_to_control_info(
        &self,
        proxy: &atspi::proxy::accessible::AccessibleProxy<'_>,
    ) -> Result<ControlInfo, UIError> {
        let name = proxy.name().await.unwrap_or_default();
        let _role = proxy.get_role().await.unwrap_or(Role::Invalid);
        let control_type = self.role_to_control_type(role);

        let automation_id = proxy.accessible_id().await.ok();
        let states = proxy.get_state().await.unwrap_or_default();

        let is_enabled = states.contains(State::Enabled);
        let is_visible = states.contains(State::Visible);
        let is_focused = states.contains(State::Focused);

        let bounds = if let Ok(component) = self.get_component_proxy(proxy).await {
            component
                .get_extents(atspi::CoordType::Screen)
                .await
                .ok()
                .map(|(x, y, w, h)| (x, y, w, h))
        } else {
            None
        };

        // Generate ID
        let bus = proxy.inner().destination().to_string();
        let path = proxy.inner().path().to_string();
        let id = format!("{}:{}", bus, path.replace('/', ":"));

        Ok(ControlInfo {
            id,
            name,
            control_type,
            automation_id,
            class_name: None,
            is_enabled,
            is_visible,
            is_focused,
            bounds,
        })
    }

    fn role_to_control_type(&self, role: Role) -> ControlType {
        match role {
            Role::PushButton | Role::ToggleButton => ControlType::Button,
            Role::CheckBox => ControlType::CheckBox,
            Role::ComboBox => ControlType::ComboBox,
            Role::Entry | Role::PasswordText => ControlType::Edit,
            Role::Label => ControlType::Text,
            Role::List => ControlType::List,
            Role::ListItem => ControlType::ListItem,
            Role::Menu => ControlType::Menu,
            Role::MenuBar => ControlType::MenuBar,
            Role::MenuItem => ControlType::MenuItem,
            Role::ProgressBar => ControlType::ProgressBar,
            Role::RadioButton => ControlType::RadioButton,
            Role::ScrollBar => ControlType::ScrollBar,
            Role::Slider => ControlType::Slider,
            Role::SpinButton => ControlType::Spinner,
            Role::StatusBar => ControlType::StatusBar,
            Role::PageTab => ControlType::Tab,
            Role::PageTabList => ControlType::TabItem,
            Role::Table => ControlType::List,
            Role::TableCell => ControlType::ListItem,
            Role::Text | Role::Paragraph => ControlType::Text,
            Role::ToolBar => ControlType::ToolBar,
            Role::ToolTip => ControlType::ToolTip,
            Role::Tree => ControlType::Tree,
            Role::TreeItem => ControlType::TreeItem,
            Role::Frame | Role::Dialog | Role::Window => ControlType::Window,
            Role::Panel => ControlType::Pane,
            Role::DocumentFrame => ControlType::Document,
            Role::Grouping => ControlType::Group,
            Role::Image => ControlType::Image,
            Role::Link => ControlType::Hyperlink,
            Role::Calendar => ControlType::Calendar,
            _ => ControlType::Unknown,
        }
    }

    fn parse_key(&self, key: &str) -> Result<Key, UIError> {
        let key_lower = key.to_lowercase();
        match key_lower.as_str() {
            "enter" | "return" => Ok(Key::Return),
            "tab" => Ok(Key::Tab),
            "escape" | "esc" => Ok(Key::Escape),
            "space" => Ok(Key::Space),
            "backspace" | "back" => Ok(Key::Backspace),
            "delete" | "del" => Ok(Key::Delete),
            "insert" | "ins" => Ok(Key::Insert),
            "home" => Ok(Key::Home),
            "end" => Ok(Key::End),
            "pageup" | "page_up" => Ok(Key::PageUp),
            "pagedown" | "page_down" => Ok(Key::PageDown),
            "up" | "arrow_up" => Ok(Key::UpArrow),
            "down" | "arrow_down" => Ok(Key::DownArrow),
            "left" | "arrow_left" => Ok(Key::LeftArrow),
            "right" | "arrow_right" => Ok(Key::RightArrow),
            "f1" => Ok(Key::F1),
            "f2" => Ok(Key::F2),
            "f3" => Ok(Key::F3),
            "f4" => Ok(Key::F4),
            "f5" => Ok(Key::F5),
            "f6" => Ok(Key::F6),
            "f7" => Ok(Key::F7),
            "f8" => Ok(Key::F8),
            "f9" => Ok(Key::F9),
            "f10" => Ok(Key::F10),
            "f11" => Ok(Key::F11),
            "f12" => Ok(Key::F12),
            _ if key.len() == 1 => Ok(Key::Unicode(key.chars().next().unwrap())),
            _ => Err(UIError {
                message: format!("Unknown key: {}", key),
                error_type: UIErrorType::InvalidArgument,
            }),
        }
    }
}

impl Default for LinuxUIAutomation {
    fn default() -> Self {
        Self::new().expect("Failed to initialize Linux UI Automation")
    }
}

impl UIAutomationTrait for LinuxUIAutomation {
    fn list_windows(&self) -> Result<Vec<WindowInfo>, UIError> {
        self.list_windows_impl()
    }

    fn find_window(&self, title: &str) -> Result<WindowInfo, UIError> {
        self.find_window_impl(title)
    }

    fn find_window_by_pid(&self, pid: u32) -> Result<WindowInfo, UIError> {
        self.find_window_by_pid_impl(pid)
    }

    fn wait_for_window(&self, title: &str, timeout_ms: u64) -> Result<WindowInfo, UIError> {
        self.wait_for_window_impl(title, timeout_ms)
    }

    fn activate_window(&self, window_id: &str) -> Result<(), UIError> {
        self.activate_window_impl(window_id)
    }

    fn close_window(&self, window_id: &str) -> Result<(), UIError> {
        self.close_window_impl(window_id)
    }

    fn minimize_window(&self, window_id: &str) -> Result<(), UIError> {
        self.minimize_window_impl(window_id)
    }

    fn maximize_window(&self, window_id: &str) -> Result<(), UIError> {
        self.maximize_window_impl(window_id)
    }

    fn restore_window(&self, window_id: &str) -> Result<(), UIError> {
        self.restore_window_impl(window_id)
    }

    fn find_controls(
        &self,
        window_id: &str,
        name: Option<&str>,
        control_type: Option<ControlType>,
    ) -> Result<Vec<ControlInfo>, UIError> {
        self.find_controls_impl(window_id, name, control_type)
    }

    fn find_control(
        &self,
        window_id: &str,
        name: Option<&str>,
        control_type: Option<ControlType>,
    ) -> Result<ControlInfo, UIError> {
        self.find_control_impl(window_id, name, control_type)
    }

    fn wait_for_control(
        &self,
        window_id: &str,
        name: Option<&str>,
        control_type: Option<ControlType>,
        timeout_ms: u64,
    ) -> Result<ControlInfo, UIError> {
        self.wait_for_control_impl(window_id, name, control_type, timeout_ms)
    }

    fn wait_for_controls(
        &self,
        window_id: &str,
        name: Option<&str>,
        control_type: Option<ControlType>,
        timeout_ms: u64,
    ) -> Result<Vec<ControlInfo>, UIError> {
        self.wait_for_controls_impl(window_id, name, control_type, timeout_ms)
    }

    fn click(&self, control_id: &str) -> Result<(), UIError> {
        self.click_impl(control_id)
    }

    fn double_click(&self, control_id: &str) -> Result<(), UIError> {
        self.double_click_impl(control_id)
    }

    fn right_click(&self, control_id: &str) -> Result<(), UIError> {
        self.right_click_impl(control_id)
    }

    fn get_text(&self, control_id: &str) -> Result<String, UIError> {
        self.get_text_impl(control_id)
    }

    fn set_text(&self, control_id: &str, text: &str) -> Result<(), UIError> {
        self.set_text_impl(control_id, text)
    }

    fn type_text(&self, text: &str) -> Result<(), UIError> {
        self.type_text_impl(text)
    }

    fn press_key(&self, key: &str, modifiers: KeyModifiers) -> Result<(), UIError> {
        self.press_key_impl(key, modifiers)
    }

    fn scroll(
        &self,
        control_id: &str,
        direction: ScrollDirection,
        amount: i32,
    ) -> Result<(), UIError> {
        self.scroll_impl(control_id, direction, amount)
    }

    fn focus_control(&self, control_id: &str) -> Result<(), UIError> {
        self.focus_control_impl(control_id)
    }
}
