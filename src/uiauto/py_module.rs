//! Python bindings for UI Automation
//!
//! Object-oriented API design:
//! ```python
//! import uiauto
//!
//! window = uiauto.find_window("Notepad")
//! window.activate()
//!
//! edit = window.find_control(name="Edit", timeout_ms=5000)
//! edit.set_text("Hello, World!")
//!
//! button = window.find_control(name="OK", control_type="button")
//! button.click()
//! ```

use {
    super::{
        ControlInfo, ControlType, KeyModifiers, ScrollDirection, WindowInfo, create_automation,
    },
    crate::python::Module,
    pyo3::{exceptions::PyRuntimeError, prelude::*},
};

/// Python wrapper for Control - represents a UI control
#[pyclass(skip_from_py_object)]
#[derive(Clone)]
pub struct Control {
    inner: ControlInfo,
}

impl From<ControlInfo> for Control {
    fn from(info: ControlInfo) -> Self {
        Self { inner: info }
    }
}

#[pymethods]
impl Control {
    #[getter]
    fn id(&self) -> String {
        self.inner.id.clone()
    }

    #[getter]
    fn name(&self) -> String {
        self.inner.name.clone()
    }

    #[getter]
    fn control_type(&self) -> String {
        self.inner.control_type.to_name().to_string()
    }

    #[getter]
    fn automation_id(&self) -> Option<String> {
        self.inner.automation_id.clone()
    }

    #[getter]
    fn class_name(&self) -> Option<String> {
        self.inner.class_name.clone()
    }

    #[getter]
    fn is_enabled(&self) -> bool {
        self.inner.is_enabled
    }

    #[getter]
    fn is_visible(&self) -> bool {
        self.inner.is_visible
    }

    #[getter]
    fn is_focused(&self) -> bool {
        self.inner.is_focused
    }

    #[getter]
    fn bounds(&self) -> Option<(i32, i32, i32, i32)> {
        self.inner.bounds
    }

    /// Click this control
    fn click(&self) -> PyResult<()> {
        create_automation()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?
            .click(&self.inner.id)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    /// Double click this control
    fn double_click(&self) -> PyResult<()> {
        create_automation()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?
            .double_click(&self.inner.id)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    /// Right click this control
    fn right_click(&self) -> PyResult<()> {
        create_automation()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?
            .right_click(&self.inner.id)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    /// Get text from this control
    fn get_text(&self) -> PyResult<String> {
        create_automation()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?
            .get_text(&self.inner.id)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    /// Set text in this control
    fn set_text(&self, text: &str) -> PyResult<()> {
        create_automation()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?
            .set_text(&self.inner.id, text)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    /// Scroll this control
    #[pyo3(signature = (direction, amount=1))]
    fn scroll(&self, direction: &str, amount: i32) -> PyResult<()> {
        let dir = parse_scroll_direction(direction)?;
        create_automation()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?
            .scroll(&self.inner.id, dir, amount)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    /// Set focus to this control
    fn set_focus(&self) -> PyResult<()> {
        create_automation()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?
            .focus_control(&self.inner.id)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    fn __str__(&self) -> String {
        format!(
            "{}('{}')",
            self.inner.control_type.to_name(),
            self.inner.name
        )
    }

    fn __repr__(&self) -> String {
        self.__str__()
    }
}

/// Python wrapper for Window - represents a top-level window
#[pyclass(skip_from_py_object)]
#[derive(Clone)]
pub struct Window {
    inner: WindowInfo,
}

impl From<WindowInfo> for Window {
    fn from(info: WindowInfo) -> Self {
        Self { inner: info }
    }
}

#[pymethods]
impl Window {
    #[getter]
    fn id(&self) -> String {
        self.inner.id.clone()
    }

    #[getter]
    fn title(&self) -> String {
        self.inner.title.clone()
    }

    #[getter]
    fn process_id(&self) -> u32 {
        self.inner.process_id
    }

    #[getter]
    fn process_name(&self) -> String {
        self.inner.process_name.clone()
    }

    #[getter]
    fn is_active(&self) -> bool {
        self.inner.is_active
    }

    #[getter]
    fn is_minimized(&self) -> bool {
        self.inner.is_minimized
    }

    #[getter]
    fn is_maximized(&self) -> bool {
        self.inner.is_maximized
    }

    #[getter]
    fn bounds(&self) -> (i32, i32, i32, i32) {
        self.inner.bounds
    }

    /// Activate (bring to front) this window
    fn activate(&self) -> PyResult<()> {
        create_automation()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?
            .activate_window(&self.inner.id)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    /// Close this window
    fn close(&self) -> PyResult<()> {
        create_automation()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?
            .close_window(&self.inner.id)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    /// Minimize this window
    fn minimize(&self) -> PyResult<()> {
        create_automation()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?
            .minimize_window(&self.inner.id)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    /// Maximize this window
    fn maximize(&self) -> PyResult<()> {
        create_automation()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?
            .maximize_window(&self.inner.id)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    /// Restore this window
    fn restore(&self) -> PyResult<()> {
        create_automation()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?
            .restore_window(&self.inner.id)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    /// Find controls in this window
    ///
    /// If timeout_ms > 0, waits until at least one control is found.
    /// If timeout_ms == 0, returns immediately (may be empty list).
    #[pyo3(signature = (name=None, control_type=None, timeout_ms=0))]
    fn find_controls(
        &self,
        name: Option<&str>,
        control_type: Option<&str>,
        timeout_ms: u64,
    ) -> PyResult<Vec<Control>> {
        let ct = control_type.map(|s| ControlType::from_name(s));
        let automation = create_automation().map_err(|e| PyRuntimeError::new_err(e.to_string()))?;

        if timeout_ms > 0 {
            automation
                .wait_for_controls(&self.inner.id, name, ct, timeout_ms)
                .map(|controls| controls.into_iter().map(Control::from).collect())
                .map_err(|e| PyRuntimeError::new_err(e.to_string()))
        } else {
            automation
                .find_controls(&self.inner.id, name, ct)
                .map(|controls| controls.into_iter().map(Control::from).collect())
                .map_err(|e| PyRuntimeError::new_err(e.to_string()))
        }
    }

    /// Find a single control in this window
    ///
    /// If timeout_ms > 0, waits for the control to appear.
    /// If timeout_ms == 0, returns immediately or raises error if not found.
    #[pyo3(signature = (name=None, control_type=None, timeout_ms=0))]
    fn find_control(
        &self,
        name: Option<&str>,
        control_type: Option<&str>,
        timeout_ms: u64,
    ) -> PyResult<Control> {
        let ct = control_type.map(|s| ControlType::from_name(s));
        let automation = create_automation().map_err(|e| PyRuntimeError::new_err(e.to_string()))?;

        if timeout_ms > 0 {
            automation
                .wait_for_control(&self.inner.id, name, ct, timeout_ms)
                .map(Control::from)
                .map_err(|e| PyRuntimeError::new_err(e.to_string()))
        } else {
            automation
                .find_control(&self.inner.id, name, ct)
                .map(Control::from)
                .map_err(|e| PyRuntimeError::new_err(e.to_string()))
        }
    }

    fn __str__(&self) -> String {
        format!(
            "Window('{}' - {})",
            self.inner.title, self.inner.process_name
        )
    }

    fn __repr__(&self) -> String {
        self.__str__()
    }
}

/// Parse scroll direction from string
fn parse_scroll_direction(direction: &str) -> PyResult<ScrollDirection> {
    match direction.to_lowercase().as_str() {
        "up" => Ok(ScrollDirection::Up),
        "down" => Ok(ScrollDirection::Down),
        "left" => Ok(ScrollDirection::Left),
        "right" => Ok(ScrollDirection::Right),
        _ => Err(PyRuntimeError::new_err(format!(
            "Invalid scroll direction: {}. Use: up, down, left, right",
            direction
        ))),
    }
}

/// Main UI Automation module for Python
#[pyclass]
pub struct PyUIAutoManager;

#[pymethods]
impl PyUIAutoManager {
    /// List all top-level windows
    fn list_windows(&self) -> PyResult<Vec<Window>> {
        create_automation()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?
            .list_windows()
            .map(|windows| windows.into_iter().map(Window::from).collect())
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    /// Find a window by title (partial match)
    fn find_window(&self, title: &str) -> PyResult<Window> {
        create_automation()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?
            .find_window(title)
            .map(Window::from)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    /// Find a window by process ID
    fn find_window_by_pid(&self, pid: u32) -> PyResult<Window> {
        create_automation()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?
            .find_window_by_pid(pid)
            .map(Window::from)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    /// Wait for a window to appear
    #[pyo3(signature = (title, timeout_ms=5000))]
    fn wait_for_window(&self, title: &str, timeout_ms: u64) -> PyResult<Window> {
        create_automation()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?
            .wait_for_window(title, timeout_ms)
            .map(Window::from)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    /// Type text (keyboard input)
    fn type_text(&self, text: &str) -> PyResult<()> {
        create_automation()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?
            .type_text(text)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    /// Press a key with optional modifiers
    ///
    /// Key names: enter, tab, escape, space, backspace, delete, insert, home, end,
    /// pageup, pagedown, up, down, left, right, f1-f12, or single characters.
    #[pyo3(signature = (key, ctrl=false, alt=false, shift=false, win=false))]
    fn press_key(&self, key: &str, ctrl: bool, alt: bool, shift: bool, win: bool) -> PyResult<()> {
        let modifiers = KeyModifiers {
            ctrl,
            alt,
            shift,
            win,
        };
        create_automation()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?
            .press_key(key, modifiers)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    fn __str__(&self) -> String {
        "uiauto".to_string()
    }

    fn __repr__(&self) -> String {
        self.__str__()
    }
}

impl<'a> Module<'a> for Py<PyUIAutoManager> {
    fn get_name() -> &'static str {
        "uiauto"
    }
}

/// Create the UI Automation Python module
pub fn create_uiauto_module() -> PyResult<Py<PyUIAutoManager>> {
    Python::attach(|py| Py::new(py, PyUIAutoManager))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_module() {
        let result = create_uiauto_module();
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_uiauto_module() -> anyhow::Result<()> {
        use {
            crate::{config::PythonConfig, python::PythonInterpreter},
            tokio::time::{Duration, sleep},
        };

        sleep(Duration::from_secs(1)).await;
        // Register uiauto module globally first
        let uiauto_module = create_uiauto_module()?;
        PythonInterpreter::register_module_global(uiauto_module);

        let config = PythonConfig {
            sandbox: true,
            max_execution_time: 30,
            dangerous_modules: vec![],
            safe_modules: vec!["uiauto".to_string()],
        };
        let interpreter = PythonInterpreter::new(config)?;

        // Just test module import
        let res = interpreter
            .execute(
                r#"
import uiauto

# Test module is available
assert str(uiauto) == "uiauto"
        "#,
            )
            .await?;
        if !res.success {
            panic!(
                "Execution failed: stdout={:?}, stderr={:?}",
                res.stdout, res.stderr
            );
        }

        Ok(())
    }
}
