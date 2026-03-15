//! Python bindings for SchedulerEngine
//!
//! Provides a Python module `scheduler` that allows the AI assistant to:
//! - Create and schedule tasks using natural language descriptions
//! - Query and manage existing tasks
//! - Run tasks on demand
//!
//! # Usage Example
//! ```python
//! import scheduler
//!
//! # Create an immediate task (runs as soon as possible)
//! task_id = scheduler.task("Check backup", "Check if the backup folder exists and has recent files")
//!
//! # Create a one-time task
//! task_id = scheduler.once("Reminder", "Remind me to take a break", hours=2)
//!
//! # Create a daily task
//! task_id = scheduler.daily("Morning report", "Generate a summary of yesterday's activities", hour=9)
//!
//! # List all tasks
//! for task in scheduler.list():
//!     print(f"{task.name}: {task.status}")
//! ```

use {
    crate::{
        python::Module,
        scheduler::engine::{ExecutionRecord, Schedule, SchedulerEngine, Task, TaskStatus},
    },
    chrono::{DateTime, Duration, Utc},
    pyo3::{exceptions::PyRuntimeError, prelude::*},
    std::sync::Weak,
};

/// Task Scheduler Manager
///
/// Provides methods to create, query, and manage scheduled tasks.
/// Tasks are executed by the LLM using natural language descriptions.
#[pyclass(name = "Scheduler")]
pub struct PyScheduler {
    inner: Weak<SchedulerEngine>,
}

#[pymethods]
impl PyScheduler {
    /// Create a task with natural language description (runs immediately by default).
    ///
    /// # Arguments
    /// * `name` - Task name
    /// * `description` - Natural language description of what the task should do
    ///
    /// # Returns
    /// Task ID string
    ///
    /// # Example
    /// ```python
    /// task_id = scheduler.task("Backup check", "Check if D:/backup has files from today")
    /// task_id = scheduler.task("Weather alert", "Check the weather forecast and notify if rain is expected")
    /// ```
    fn task(&self, name: String, description: String) -> PyResult<String> {
        let engine = self
            .inner
            .upgrade()
            .ok_or_else(|| PyRuntimeError::new_err("Scheduler engine has been dropped"))?;

        let schedule = Schedule::Immediate;
        let task = Task::new(name, description, schedule);

        engine
            .add_task_blocking(task)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    /// Create a one-time task that executes once at a future time.
    ///
    /// # Arguments
    /// * `name` - Task name
    /// * `description` - Natural language description of what the task should do
    /// * `hours` - Hours from now (default: 0)
    /// * `minutes` - Minutes from now (default: 0)
    /// * `seconds` - Seconds from now (default: 0)
    ///
    /// # Returns
    /// Task ID string
    ///
    /// # Example
    /// ```python
    /// # Run in 1 hour
    /// task_id = scheduler.once("Reminder", "Remind the user to take a break", hours=1)
    ///
    /// # Run in 30 minutes
    /// task_id = scheduler.once("Quick check", "Check if the download is complete", minutes=30)
    /// ```
    #[pyo3(signature = (name, description, hours=0, minutes=0, seconds=0))]
    fn once(
        &self,
        name: String,
        description: String,
        hours: u64,
        minutes: u64,
        seconds: u64,
    ) -> PyResult<String> {
        let engine = self
            .inner
            .upgrade()
            .ok_or_else(|| PyRuntimeError::new_err("Scheduler engine has been dropped"))?;

        let at = Utc::now()
            + Duration::hours(hours as i64)
            + Duration::minutes(minutes as i64)
            + Duration::seconds(seconds as i64);

        let schedule = Schedule::Once { at };
        let task = Task::new(name, description, schedule);

        engine
            .add_task_blocking(task)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    /// Create an interval task that runs repeatedly.
    ///
    /// # Arguments
    /// * `name` - Task name
    /// * `description` - Natural language description of what the task should do
    /// * `seconds` - Interval in seconds (default: 60)
    /// * `minutes` - Interval in minutes (default: 0)
    /// * `hours` - Interval in hours (default: 0)
    ///
    /// # Returns
    /// Task ID string
    ///
    /// # Example
    /// ```python
    /// # Run every 5 minutes
    /// task_id = scheduler.interval("Health check", "Check if the server is responding", minutes=5)
    ///
    /// # Run every hour
    /// task_id = scheduler.interval("Hourly log", "Summarize recent activities", hours=1)
    /// ```
    #[pyo3(signature = (name, description, seconds=60, minutes=0, hours=0))]
    fn interval(
        &self,
        name: String,
        description: String,
        seconds: u64,
        minutes: u64,
        hours: u64,
    ) -> PyResult<String> {
        let engine = self
            .inner
            .upgrade()
            .ok_or_else(|| PyRuntimeError::new_err("Scheduler engine has been dropped"))?;

        let total_seconds = seconds + minutes * 60 + hours * 3600;
        let schedule = Schedule::Interval {
            seconds: total_seconds,
            start_at: None,
            end_at: None,
        };
        let task = Task::new(name, description, schedule);

        engine
            .add_task_blocking(task)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    /// Create a daily task that runs at a specific time each day.
    ///
    /// # Arguments
    /// * `name` - Task name
    /// * `description` - Natural language description of what the task should do
    /// * `hour` - Hour (0-23, default: 9)
    /// * `minute` - Minute (0-59, default: 0)
    ///
    /// # Returns
    /// Task ID string
    ///
    /// # Example
    /// ```python
    /// # Run every day at 9:00 AM
    /// task_id = scheduler.daily("Morning greeting", "Greet the user and summarize today's schedule", hour=9)
    ///
    /// # Run every day at 11:30 PM
    /// task_id = scheduler.daily("Nightly report", "Generate a summary of today's activities", hour=23, minute=30)
    /// ```
    #[pyo3(signature = (name, description, hour=9, minute=0))]
    fn daily(&self, name: String, description: String, hour: u32, minute: u32) -> PyResult<String> {
        let engine = self
            .inner
            .upgrade()
            .ok_or_else(|| PyRuntimeError::new_err("Scheduler engine has been dropped"))?;

        let schedule = Schedule::Daily { hour, minute };
        let task = Task::new(name, description, schedule);

        engine
            .add_task_blocking(task)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    /// Create a weekly task that runs on a specific day each week.
    ///
    /// # Arguments
    /// * `name` - Task name
    /// * `description` - Natural language description of what the task should do
    /// * `day` - Day of week (0=Sunday, 1=Monday, ..., 6=Saturday, default: 1)
    /// * `hour` - Hour (0-23, default: 9)
    /// * `minute` - Minute (0-59, default: 0)
    ///
    /// # Returns
    /// Task ID string
    ///
    /// # Example
    /// ```python
    /// # Run every Monday at 9:00 AM
    /// task_id = scheduler.weekly("Weekly report", "Generate weekly progress report", day=1, hour=9)
    ///
    /// # Run every Sunday at midnight
    /// task_id = scheduler.weekly("Weekly cleanup", "Clean up temporary files and old logs", day=0, hour=0, minute=0)
    /// ```
    #[pyo3(signature = (name, description, day=1, hour=9, minute=0))]
    fn weekly(
        &self,
        name: String,
        description: String,
        day: u32,
        hour: u32,
        minute: u32,
    ) -> PyResult<String> {
        let engine = self
            .inner
            .upgrade()
            .ok_or_else(|| PyRuntimeError::new_err("Scheduler engine has been dropped"))?;

        let schedule = Schedule::Weekly { day, hour, minute };
        let task = Task::new(name, description, schedule);

        engine
            .add_task_blocking(task)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    /// Create a task at a specific datetime.
    ///
    /// # Arguments
    /// * `name` - Task name
    /// * `description` - Natural language description of what the task should do
    /// * `at` - ISO 8601 datetime string (e.g., "2024-12-25T10:00:00Z")
    ///
    /// # Returns
    /// Task ID string
    ///
    /// # Example
    /// ```python
    /// task_id = scheduler.at("New Year greeting", "Wish the user a happy new year", "2024-01-01T00:00:00Z")
    /// ```
    fn at(&self, name: String, description: String, at: String) -> PyResult<String> {
        let engine = self
            .inner
            .upgrade()
            .ok_or_else(|| PyRuntimeError::new_err("Scheduler engine has been dropped"))?;

        let datetime = DateTime::parse_from_rfc3339(&at)
            .map_err(|e| PyRuntimeError::new_err(format!("Invalid datetime format: {}", e)))?
            .with_timezone(&Utc);

        let schedule = Schedule::Once { at: datetime };
        let task = Task::new(name, description, schedule);

        engine
            .add_task_blocking(task)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    /// List all scheduled tasks.
    ///
    /// # Arguments
    /// * `status` - Filter by status (optional): "pending", "running", "completed", "failed", "paused"
    ///
    /// # Returns
    /// List of PyTaskInfo objects
    ///
    /// # Example
    /// ```python
    /// for task in scheduler.list():
    ///     print(f"[{task.status}] {task.name} - {task.description[:50]}")
    /// ```
    #[pyo3(signature = (status=None))]
    fn list(&self, status: Option<&str>) -> PyResult<Vec<PyTaskInfo>> {
        let engine = self
            .inner
            .upgrade()
            .ok_or_else(|| PyRuntimeError::new_err("Scheduler engine has been dropped"))?;

        let tasks = engine
            .list_tasks_blocking()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;

        let filter_status = status.and_then(|s| match s.to_lowercase().as_str() {
            "pending" => Some(TaskStatus::Pending),
            "running" => Some(TaskStatus::Running),
            "completed" => Some(TaskStatus::Completed),
            "failed" => Some(TaskStatus::Failed),
            "paused" => Some(TaskStatus::Paused),
            "cancelled" => Some(TaskStatus::Cancelled),
            _ => None,
        });

        let result: Vec<_> = tasks
            .iter()
            .filter(|t| filter_status.map_or(true, |s| t.status == s))
            .map(PyTaskInfo::from)
            .collect();

        Ok(result)
    }

    /// Get detailed information about a specific task.
    ///
    /// # Arguments
    /// * `task_id` - Task ID
    ///
    /// # Returns
    /// PyTask object if found, None otherwise
    fn get(&self, task_id: String) -> PyResult<Option<PyTask>> {
        let engine = self
            .inner
            .upgrade()
            .ok_or_else(|| PyRuntimeError::new_err("Scheduler engine has been dropped"))?;

        let task = engine
            .get_task_blocking(&task_id)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;

        Ok(task.map(PyTask::from))
    }

    /// Remove a scheduled task.
    ///
    /// # Arguments
    /// * `task_id` - Task ID
    fn remove(&self, task_id: String) -> PyResult<bool> {
        let engine = self
            .inner
            .upgrade()
            .ok_or_else(|| PyRuntimeError::new_err("Scheduler engine has been dropped"))?;

        engine
            .remove_task_blocking(&task_id)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;

        Ok(true)
    }

    /// Pause a scheduled task.
    fn pause(&self, task_id: String) -> PyResult<()> {
        let engine = self
            .inner
            .upgrade()
            .ok_or_else(|| PyRuntimeError::new_err("Scheduler engine has been dropped"))?;

        engine
            .pause_task_blocking(&task_id)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    /// Resume a paused task.
    fn resume(&self, task_id: String) -> PyResult<()> {
        let engine = self
            .inner
            .upgrade()
            .ok_or_else(|| PyRuntimeError::new_err("Scheduler engine has been dropped"))?;

        engine
            .resume_task_blocking(&task_id)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    /// Run a task immediately.
    fn run_now(&self, task_id: String) -> PyResult<()> {
        let engine = self
            .inner
            .upgrade()
            .ok_or_else(|| PyRuntimeError::new_err("Scheduler engine has been dropped"))?;

        engine
            .run_now_blocking(&task_id)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    /// Get count of tasks.
    #[pyo3(signature = (status=None))]
    fn count(&self, status: Option<&str>) -> PyResult<usize> {
        let engine = self
            .inner
            .upgrade()
            .ok_or_else(|| PyRuntimeError::new_err("Scheduler engine has been dropped"))?;

        let tasks = engine
            .list_tasks_blocking()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;

        let filter_status = status.and_then(|s| match s.to_lowercase().as_str() {
            "pending" => Some(TaskStatus::Pending),
            "running" => Some(TaskStatus::Running),
            "completed" => Some(TaskStatus::Completed),
            "failed" => Some(TaskStatus::Failed),
            "paused" => Some(TaskStatus::Paused),
            _ => None,
        });

        let count = tasks
            .iter()
            .filter(|t| filter_status.map_or(true, |s| t.status == s))
            .count();

        Ok(count)
    }

    fn __str__(&self) -> String {
        "Scheduler(定时任务管理器)".to_string()
    }

    fn __repr__(&self) -> String {
        self.__str__()
    }
}

/// Lightweight task information for listing.
#[pyclass(skip_from_py_object)]
#[derive(Clone)]
pub struct PyTaskInfo {
    #[pyo3(get)]
    pub id: String,
    #[pyo3(get)]
    pub name: String,
    #[pyo3(get)]
    pub description: String,
    #[pyo3(get)]
    pub status: String,
    #[pyo3(get)]
    pub next_run: Option<String>,
    #[pyo3(get)]
    pub last_run: Option<String>,
    #[pyo3(get)]
    pub success_count: u32,
    #[pyo3(get)]
    pub fail_count: u32,
    #[pyo3(get)]
    pub tags: Vec<String>,
}

impl From<&Task> for PyTaskInfo {
    fn from(task: &Task) -> Self {
        Self {
            id: task.id.clone(),
            name: task.name.clone(),
            description: task.description.clone(),
            status: match task.status {
                TaskStatus::Pending => "pending",
                TaskStatus::Running => "running",
                TaskStatus::Completed => "completed",
                TaskStatus::Failed => "failed",
                TaskStatus::Paused => "paused",
                TaskStatus::Cancelled => "cancelled",
            }
            .to_string(),
            next_run: task.next_run.map(|t| t.to_rfc3339()),
            last_run: task.last_run.map(|t| t.to_rfc3339()),
            success_count: task.success_count,
            fail_count: task.fail_count,
            tags: task.tags.clone(),
        }
    }
}

#[pymethods]
impl PyTaskInfo {
    fn __str__(&self) -> String {
        format!(
            "[{}] {} - {}",
            self.status,
            self.name,
            self.description.chars().take(50).collect::<String>()
        )
    }

    fn __repr__(&self) -> String {
        self.__str__()
    }
}

/// Detailed task information including history.
#[pyclass(skip_from_py_object)]
#[derive(Clone)]
pub struct PyTask {
    #[pyo3(get)]
    pub id: String,
    #[pyo3(get)]
    pub name: String,
    #[pyo3(get)]
    pub description: String,
    #[pyo3(get)]
    pub status: String,
    #[pyo3(get)]
    pub next_run: Option<String>,
    #[pyo3(get)]
    pub last_run: Option<String>,
    #[pyo3(get)]
    pub success_count: u32,
    #[pyo3(get)]
    pub fail_count: u32,
    #[pyo3(get)]
    pub max_retries: u32,
    #[pyo3(get)]
    pub priority: u8,
    #[pyo3(get)]
    pub tags: Vec<String>,
    #[pyo3(get)]
    pub history: Vec<PyExecutionRecord>,
}

impl From<Task> for PyTask {
    fn from(task: Task) -> Self {
        Self {
            id: task.id.clone(),
            name: task.name.clone(),
            description: task.description.clone(),
            status: match task.status {
                TaskStatus::Pending => "pending",
                TaskStatus::Running => "running",
                TaskStatus::Completed => "completed",
                TaskStatus::Failed => "failed",
                TaskStatus::Paused => "paused",
                TaskStatus::Cancelled => "cancelled",
            }
            .to_string(),
            next_run: task.next_run.map(|t| t.to_rfc3339()),
            last_run: task.last_run.map(|t| t.to_rfc3339()),
            success_count: task.success_count,
            fail_count: task.fail_count,
            max_retries: task.max_retries,
            priority: task.priority,
            tags: task.tags,
            history: task
                .history
                .into_iter()
                .map(PyExecutionRecord::from)
                .collect(),
        }
    }
}

#[pymethods]
impl PyTask {
    fn __str__(&self) -> String {
        format!(
            "Task({}) [{}] - {} executions",
            self.name,
            self.status,
            self.success_count + self.fail_count
        )
    }

    fn __repr__(&self) -> String {
        self.__str__()
    }
}

/// Execution record for a task run.
#[pyclass(skip_from_py_object)]
#[derive(Clone)]
pub struct PyExecutionRecord {
    #[pyo3(get)]
    pub timestamp: String,
    #[pyo3(get)]
    pub success: bool,
    #[pyo3(get)]
    pub message: String,
    #[pyo3(get)]
    pub duration_ms: u64,
}

impl From<ExecutionRecord> for PyExecutionRecord {
    fn from(record: ExecutionRecord) -> Self {
        Self {
            timestamp: record.timestamp.to_rfc3339(),
            success: record.success,
            message: record.message,
            duration_ms: record.duration_ms,
        }
    }
}

#[pymethods]
impl PyExecutionRecord {
    fn __str__(&self) -> String {
        let status = if self.success { "OK" } else { "FAIL" };
        format!("[{}] {} - {}ms", status, self.timestamp, self.duration_ms)
    }

    fn __repr__(&self) -> String {
        self.__str__()
    }
}

impl<'a> Module<'a> for Py<PyScheduler> {
    fn get_name() -> &'static str {
        "scheduler"
    }
}

/// Create a Python module for scheduler access
pub fn create_scheduler_module(scheduler: Weak<SchedulerEngine>) -> PyResult<Py<PyScheduler>> {
    Python::attach(|py| {
        let py_scheduler = PyScheduler { inner: scheduler };
        Py::new(py, py_scheduler)
    })
}
