//! Core scheduler engine implementation
//!
//! Provides task scheduling and execution using natural language descriptions.
//! Tasks are executed by the LLM, similar to user conversations.

use {
    crate::{
        config::{SchedulerConfig, StorageFormat},
        llm::LlmManager,
    },
    anyhow::{Context, Result},
    chrono::{DateTime, Datelike, Duration as ChronoDuration, Utc},
    postcard::{from_bytes, to_allocvec},
    serde::{Deserialize, Serialize},
    serde_json::{from_slice, to_string_pretty},
    std::{
        collections::{HashMap, HashSet},
        fs,
        path::PathBuf,
        sync::{Arc, Weak},
        time::Instant,
    },
    tokio::{
        sync::{Mutex, broadcast, mpsc, oneshot},
        time::{Duration, MissedTickBehavior, interval},
    },
    tracing::{error, info, warn},
    uuid::Uuid,
};

/// Unique task identifier
pub type TaskId = String;

/// Task event for notification
#[derive(Debug, Clone)]
pub enum TaskEvent {
    /// Task is ready for execution
    Ready(Task),
    /// Task execution completed
    Completed {
        name: String,
        success: bool,
        message: String,
    },
}

/// Task execution schedule type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Schedule {
    /// Execute once at a specific time
    Once { at: DateTime<Utc> },
    /// Execute at regular intervals
    Interval {
        seconds: u64,
        start_at: Option<DateTime<Utc>>,
        end_at: Option<DateTime<Utc>>,
    },
    /// Execute daily at specific time
    Daily { hour: u32, minute: u32 },
    /// Execute weekly on specific day
    Weekly { day: u32, hour: u32, minute: u32 },
    /// Execute immediately (no time constraint)
    Immediate,
}

impl Schedule {
    /// Calculate the next execution time from now
    pub fn next_execution(&self, from: DateTime<Utc>) -> Option<DateTime<Utc>> {
        match self {
            Schedule::Once { at } => {
                if at > &from {
                    Some(*at)
                } else {
                    None
                }
            }
            Schedule::Interval {
                seconds,
                start_at,
                end_at,
            } => {
                if let Some(end) = end_at
                    && from >= *end
                {
                    return None;
                }
                if let Some(start) = start_at
                    && from < *start
                {
                    return Some(*start);
                }

                let interval = ChronoDuration::seconds(*seconds as i64);
                let start = start_at.unwrap_or(from - interval);
                let mut next = start;

                while next <= from {
                    next = next + interval;
                }

                if let Some(end) = end_at
                    && next >= *end
                {
                    return None;
                }
                Some(next)
            }
            Schedule::Daily { hour, minute } => {
                let today = from.date_naive();
                let today_time = today.and_hms_opt(*hour, *minute, 0)?;
                let today_dt = DateTime::from_naive_utc_and_offset(today_time, Utc);

                if today_dt > from {
                    return Some(today_dt);
                }

                let tomorrow = today + ChronoDuration::days(1);
                let tomorrow_time = tomorrow.and_hms_opt(*hour, *minute, 0)?;
                Some(DateTime::from_naive_utc_and_offset(tomorrow_time, Utc))
            }
            Schedule::Weekly { day, hour, minute } => {
                let today = from.date_naive();
                let today_weekday = today.weekday().num_days_from_sunday();

                let days_until = if *day >= today_weekday {
                    *day - today_weekday
                } else {
                    7 - today_weekday + *day
                };

                let target_date = today + ChronoDuration::days(days_until as i64);
                let target_time = target_date.and_hms_opt(*hour, *minute, 0)?;
                let target_dt = DateTime::from_naive_utc_and_offset(target_time, Utc);

                if target_dt > from {
                    Some(target_dt)
                } else {
                    let next_week = target_date + ChronoDuration::days(7);
                    let next_time = next_week.and_hms_opt(*hour, *minute, 0)?;
                    Some(DateTime::from_naive_utc_and_offset(next_time, Utc))
                }
            }
            Schedule::Immediate => Some(from),
        }
    }

    pub fn is_once(&self) -> bool {
        matches!(self, Schedule::Once { .. })
    }

    pub fn is_immediate(&self) -> bool {
        matches!(self, Schedule::Immediate)
    }
}

/// Task status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Paused,
    Cancelled,
}

/// Execution result record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionRecord {
    pub timestamp: DateTime<Utc>,
    pub success: bool,
    pub message: String,
    pub duration_ms: u64,
}

/// Task definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub name: String,
    pub description: String,
    pub schedule: Schedule,
    pub status: TaskStatus,
    pub created_at: DateTime<Utc>,
    pub last_run: Option<DateTime<Utc>>,
    pub next_run: Option<DateTime<Utc>>,
    pub success_count: u32,
    pub fail_count: u32,
    pub max_retries: u32,
    pub retry_count: u32,
    pub history: Vec<ExecutionRecord>,
    pub tags: Vec<String>,
    pub priority: u8,
}

impl Task {
    pub fn new(name: String, description: String, schedule: Schedule) -> Self {
        let now = Utc::now();
        let next_run = schedule.next_execution(now);
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            description,
            schedule,
            status: TaskStatus::Pending,
            created_at: now,
            last_run: None,
            next_run,
            success_count: 0,
            fail_count: 0,
            max_retries: 3,
            retry_count: 0,
            history: Vec::new(),
            tags: Vec::new(),
            priority: 5,
        }
    }

    pub fn update_next_run(&mut self) {
        let now = Utc::now();
        self.last_run = Some(now);
        self.next_run = self.schedule.next_execution(now);
    }

    /// Update next run time with retry delay for failed tasks
    /// Returns true if retry is scheduled, false if max retries exceeded
    pub fn schedule_retry(&mut self, retry_delay_secs: u64) -> bool {
        self.last_run = Some(Utc::now());
        self.next_run = Some(Utc::now() + ChronoDuration::seconds(retry_delay_secs as i64));
        self.status = TaskStatus::Pending;
        true
    }

    pub fn should_run(&self) -> bool {
        if self.status != TaskStatus::Pending {
            return false;
        }
        self.next_run.map_or(false, |next| next <= Utc::now())
    }

    pub fn record_execution(&mut self, success: bool, message: String, duration_ms: u64) {
        self.history.push(ExecutionRecord {
            timestamp: Utc::now(),
            success,
            message,
            duration_ms,
        });

        if self.history.len() > 10 {
            self.history.remove(0);
        }

        if success {
            self.success_count += 1;
            self.retry_count = 0;
        } else {
            self.fail_count += 1;
            if self.retry_count < self.max_retries {
                self.retry_count += 1;
            }
        }
    }
}

enum SchedulerCommand {
    AddTask(Task),
    RemoveTask(TaskId),
    PauseTask(TaskId),
    ResumeTask(TaskId),
    RunNow(TaskId),
    GetTasks(oneshot::Sender<Vec<Task>>),
    GetTask(TaskId, oneshot::Sender<Option<Task>>),
}

/// Shared state for worker
struct WorkerState {
    tasks: HashMap<TaskId, Task>,
    /// IDs of currently running tasks
    running_tasks: HashSet<TaskId>,
}

/// Scheduler engine
pub struct SchedulerEngine {
    command_tx: mpsc::Sender<SchedulerCommand>,
    event_tx: broadcast::Sender<TaskEvent>,
}

impl SchedulerEngine {
    /// Create scheduler (full scheduling + execution)
    pub fn new(config: SchedulerConfig, client: Weak<LlmManager>) -> Result<Self> {
        let (command_tx, command_rx) = mpsc::channel(100);
        let (event_tx, _) = broadcast::channel(100);

        let tasks = load_tasks(&config.storage_path, config.storage_format)?;
        info!("Loaded {} existing tasks", tasks.len());

        let state = Arc::new(Mutex::new(WorkerState {
            tasks,
            running_tasks: HashSet::new(),
        }));

        tokio::spawn(worker(config, command_rx, state, event_tx.clone(), client));

        Ok(Self {
            command_tx,
            event_tx,
        })
    }

    /// Subscribe to task events
    pub fn subscribe(&self) -> broadcast::Receiver<TaskEvent> {
        self.event_tx.subscribe()
    }

    /// Blocking version of add_task for use outside async context
    pub fn add_task_blocking(&self, mut task: Task) -> Result<TaskId> {
        task.id = Uuid::new_v4().to_string();
        let id = task.id.clone();
        self.command_tx
            .blocking_send(SchedulerCommand::AddTask(task))
            .context("Failed to send add task command")?;
        Ok(id)
    }

    /// Blocking version of remove_task for use outside async context
    pub fn remove_task_blocking(&self, id: &str) -> Result<()> {
        self.command_tx
            .blocking_send(SchedulerCommand::RemoveTask(id.to_string()))
            .context("Failed to send remove task command")
    }

    /// Blocking version of pause_task for use outside async context
    pub fn pause_task_blocking(&self, id: &str) -> Result<()> {
        self.command_tx
            .blocking_send(SchedulerCommand::PauseTask(id.to_string()))
            .context("Failed to send pause task command")
    }

    /// Blocking version of resume_task for use outside async context
    pub fn resume_task_blocking(&self, id: &str) -> Result<()> {
        self.command_tx
            .blocking_send(SchedulerCommand::ResumeTask(id.to_string()))
            .context("Failed to send resume task command")
    }

    /// Blocking version of run_now for use outside async context
    pub fn run_now_blocking(&self, id: &str) -> Result<()> {
        self.command_tx
            .blocking_send(SchedulerCommand::RunNow(id.to_string()))
            .context("Failed to send run now command")
    }

    /// Blocking version of list_tasks for use outside async context
    pub fn list_tasks_blocking(&self) -> Result<Vec<Task>> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .blocking_send(SchedulerCommand::GetTasks(tx))
            .context("Failed to send get tasks command")?;
        rx.blocking_recv().context("Failed to receive tasks list")
    }

    /// Blocking version of get_task for use outside async context
    pub fn get_task_blocking(&self, id: &str) -> Result<Option<Task>> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .blocking_send(SchedulerCommand::GetTask(id.to_string(), tx))
            .context("Failed to send get task command")?;
        rx.blocking_recv().context("Failed to receive task")
    }
}

/// Background worker (full scheduling + parallel execution)
async fn worker(
    config: SchedulerConfig,
    mut command_rx: mpsc::Receiver<SchedulerCommand>,
    state: Arc<Mutex<WorkerState>>,
    event_tx: broadcast::Sender<TaskEvent>,
    client: Weak<LlmManager>,
) {
    let mut check_interval = interval(Duration::from_secs(config.check_interval));
    check_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = check_interval.tick() => {
                // Collect tasks to run (avoid holding lock during execution)
                let tasks_to_run: Vec<(TaskId, String, String)> = {
                    let state_guard = state.lock().await;
                    state_guard
                        .tasks
                        .iter()
                        .filter(|(id, task)| {
                            task.should_run() && !state_guard.running_tasks.contains(*id)
                        })
                        .map(|(id, task)| {
                            (id.clone(), task.name.clone(), task.description.clone())
                        })
                        .collect()
                };

                // Execute tasks in parallel
                for (task_id, task_name, task_description) in tasks_to_run {
                    // Mark as running
                    {
                        let mut state_guard = state.lock().await;
                        if let Some(task) = state_guard.tasks.get_mut(&task_id) {
                            task.status = TaskStatus::Running;
                        }
                        state_guard.running_tasks.insert(task_id.clone());
                        let _ = save_tasks(&config.storage_path, config.storage_format, &state_guard.tasks);
                    }

                    let client_clone = client.clone();
                    let state_clone = state.clone();
                    let config_clone = config.clone();
                    let event_tx_clone = event_tx.clone();

                    // Send task started event
                    {
                        let state_guard = state.lock().await;
                        if let Some(task) = state_guard.tasks.get(&task_id) {
                            let _ = event_tx.send(TaskEvent::Ready(task.clone()));
                        }
                    }

                    // Spawn task execution (parallel)
                    tokio::spawn(async move {
                        let start = Instant::now();
                        info!("Executing task: {}", task_name);
                        let Some(client) = client_clone.upgrade() else {
                            return;
                        };

                        match client.execute_task(&task_description).await {
                            Ok((success, message, _duration)) => {
                                let duration_ms = start.elapsed().as_millis() as u64;

                                let mut state_guard = state_clone.lock().await;
                                state_guard.running_tasks.remove(&task_id);

                                if let Some(task) = state_guard.tasks.get_mut(&task_id) {
                                    task.record_execution(success, message.clone(), duration_ms);

                                    if success {
                                        // Success: update status and schedule next run
                                        task.status = if task.schedule.is_once() {
                                            TaskStatus::Completed
                                        } else {
                                            TaskStatus::Pending
                                        };
                                        task.update_next_run();
                                        info!("Task '{}' completed successfully ({}ms)", task_name, duration_ms);
                                    } else if task.retry_count >= task.max_retries {
                                        // Max retries exceeded: mark as failed
                                        task.status = TaskStatus::Failed;
                                        task.next_run = None;
                                        warn!("Task '{}' failed permanently after {} retries: {}", task_name, task.max_retries, message);
                                    } else {
                                        // Retry with delay (60 seconds)
                                        task.schedule_retry(5);
                                        info!(
                                            "Task '{}' failed, will retry in 5s (attempt {}/{})",
                                            task_name, task.retry_count, task.max_retries
                                        );
                                    }

                                    let _ = save_tasks(&config_clone.storage_path, config_clone.storage_format, &state_guard.tasks);

                                    let _ = event_tx_clone.send(TaskEvent::Completed {
                                        name: task_name,
                                        success,
                                        message,
                                    });
                                }
                            }
                            Err(e) => {
                                let mut state_guard = state_clone.lock().await;
                                state_guard.running_tasks.remove(&task_id);

                                if let Some(task) = state_guard.tasks.get_mut(&task_id) {
                                    task.record_execution(false, e.to_string(), 0);

                                    if task.retry_count >= task.max_retries {
                                        task.status = TaskStatus::Failed;
                                        task.next_run = None;
                                        error!("Task '{}' failed permanently after {} retries: {}", task_name, task.max_retries, e);
                                    } else {
                                        task.schedule_retry(5);
                                        error!(
                                            "Task '{}' execution error, will retry in 5s (attempt {}/{}): {}",
                                            task_name, task.retry_count, task.max_retries, e
                                        );
                                    }

                                    let _ = save_tasks(&config_clone.storage_path, config_clone.storage_format, &state_guard.tasks);
                                }
                            }
                        }
                    });
                }
            }

            Some(cmd) = command_rx.recv() => {
                match cmd {
                    SchedulerCommand::AddTask(task) => {
                        let id = task.id.clone();
                        let mut state_guard = state.lock().await;
                        state_guard.tasks.insert(id.clone(), task);
                        let _ = save_tasks(&config.storage_path, config.storage_format, &state_guard.tasks);
                    }
                    SchedulerCommand::RemoveTask(id) => {
                        let mut state_guard = state.lock().await;
                        state_guard.tasks.remove(&id);
                        state_guard.running_tasks.remove(&id);
                        let _ = save_tasks(&config.storage_path, config.storage_format, &state_guard.tasks);
                    }
                    SchedulerCommand::PauseTask(id) => {
                        let mut state_guard = state.lock().await;
                        if let Some(task) = state_guard.tasks.get_mut(&id) {
                            task.status = TaskStatus::Paused;
                            let _ = save_tasks(&config.storage_path, config.storage_format, &state_guard.tasks);
                        }
                    }
                    SchedulerCommand::ResumeTask(id) => {
                        let mut state_guard = state.lock().await;
                        if let Some(task) = state_guard.tasks.get_mut(&id) {
                            task.status = TaskStatus::Pending;
                            task.next_run = task.schedule.next_execution(Utc::now());
                            let _ = save_tasks(&config.storage_path, config.storage_format, &state_guard.tasks);
                        }
                    }
                    SchedulerCommand::RunNow(id) => {
                        let mut state_guard = state.lock().await;
                        if let Some(task) = state_guard.tasks.get_mut(&id) {
                            task.next_run = Some(Utc::now());
                        }
                    }
                    SchedulerCommand::GetTasks(tx) => {
                        let state_guard = state.lock().await;
                        let _ = tx.send(state_guard.tasks.values().cloned().collect());
                    }
                    SchedulerCommand::GetTask(id, tx) => {
                        let state_guard = state.lock().await;
                        let _ = tx.send(state_guard.tasks.get(&id).cloned());
                    }
                }
            }
        }
    }
}

/// Load tasks from storage
fn load_tasks(path: &str, format: StorageFormat) -> Result<HashMap<TaskId, Task>> {
    // Determine file path with extension
    let storage_path = match format {
        StorageFormat::Json => PathBuf::from(format!("{}.json", path)),
        StorageFormat::Binary => PathBuf::from(format!("{}.bin", path)),
    };

    if !storage_path.exists() {
        return Ok(HashMap::new());
    }

    let data = fs::read(&storage_path)
        .with_context(|| format!("Failed to read tasks from {:?}", storage_path))?;

    if data.is_empty() {
        return Ok(HashMap::new());
    }

    let tasks: Vec<Task> = match format {
        StorageFormat::Json => from_slice(&data)
            .with_context(|| format!("Failed to parse JSON tasks from {:?}", storage_path))?,
        StorageFormat::Binary => from_bytes(&data)
            .with_context(|| format!("Failed to parse binary tasks from {:?}", storage_path))?,
    };

    Ok(tasks
        .into_iter()
        .map(|mut t| {
            if t.status == TaskStatus::Running {
                t.status = TaskStatus::Pending;
            }
            t.next_run = t.schedule.next_execution(Utc::now());
            (t.id.clone(), t)
        })
        .collect())
}

/// Save tasks to storage
fn save_tasks(path: &str, format: StorageFormat, tasks: &HashMap<TaskId, Task>) -> Result<()> {
    let storage_path = match format {
        StorageFormat::Json => PathBuf::from(format!("{}.json", path)),
        StorageFormat::Binary => PathBuf::from(format!("{}.bin", path)),
    };

    if let Some(parent) = storage_path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory: {:?}", parent))?;
        }
    }

    let tasks_vec: Vec<_> = tasks.values().cloned().collect();

    match format {
        StorageFormat::Json => {
            let data = to_string_pretty(&tasks_vec).context("Failed to serialize tasks to JSON")?;
            fs::write(&storage_path, data)
                .with_context(|| format!("Failed to write tasks to {:?}", storage_path))?;
        }
        StorageFormat::Binary => {
            let data = to_allocvec(&tasks_vec).context("Failed to serialize tasks to binary")?;
            fs::write(&storage_path, data)
                .with_context(|| format!("Failed to write tasks to {:?}", storage_path))?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Timelike;

    #[test]
    fn test_task_new() {
        let task = Task::new(
            "Test Task".to_string(),
            "This is a test task description".to_string(),
            Schedule::Immediate,
        );

        assert_eq!(task.name, "Test Task");
        assert_eq!(task.description, "This is a test task description");
        assert_eq!(task.status, TaskStatus::Pending);
        assert!(task.next_run.is_some());
        assert_eq!(task.success_count, 0);
        assert_eq!(task.fail_count, 0);
    }

    #[test]
    fn test_task_should_run() {
        let mut task = Task::new(
            "Test".to_string(),
            "Description".to_string(),
            Schedule::Immediate,
        );

        // Pending + next_run <= now => should run
        assert!(task.should_run());

        // Running status => should not run
        task.status = TaskStatus::Running;
        assert!(!task.should_run());

        // Paused status => should not run
        task.status = TaskStatus::Paused;
        assert!(!task.should_run());

        // Completed status => should not run
        task.status = TaskStatus::Completed;
        assert!(!task.should_run());
    }

    #[test]
    fn test_task_record_execution() {
        let mut task = Task::new(
            "Test".to_string(),
            "Description".to_string(),
            Schedule::Immediate,
        );

        // Record success
        task.record_execution(true, "OK".to_string(), 100);
        assert_eq!(task.success_count, 1);
        assert_eq!(task.fail_count, 0);
        assert_eq!(task.history.len(), 1);
        assert!(task.history[0].success);
        assert_eq!(task.history[0].message, "OK");

        // Record failure
        task.record_execution(false, "Error".to_string(), 200);
        assert_eq!(task.success_count, 1);
        assert_eq!(task.fail_count, 1);
        assert_eq!(task.history.len(), 2);
        assert!(!task.history[1].success);
        assert_eq!(task.history[1].message, "Error");
    }

    #[test]
    fn test_task_history_limit() {
        let mut task = Task::new(
            "Test".to_string(),
            "Description".to_string(),
            Schedule::Immediate,
        );

        // Add 12 records (limit is 10)
        for i in 0..12 {
            task.record_execution(true, format!("Record {}", i), 100);
        }

        assert_eq!(task.history.len(), 10);
        // First record should be "Record 2" (0 and 1 were removed)
        assert_eq!(task.history[0].message, "Record 2");
    }

    #[test]
    fn test_schedule_immediate() {
        let schedule = Schedule::Immediate;
        let now = Utc::now();

        // Immediate should return now
        let next = schedule.next_execution(now);
        assert!(next.is_some());

        // is_once and is_immediate
        assert!(!schedule.is_once());
        assert!(schedule.is_immediate());
    }

    #[test]
    fn test_schedule_once_future() {
        let future = Utc::now() + ChronoDuration::hours(1);
        let schedule = Schedule::Once { at: future };

        let now = Utc::now();
        let next = schedule.next_execution(now);
        assert!(next.is_some());
        assert_eq!(next.unwrap(), future);

        assert!(schedule.is_once());
        assert!(!schedule.is_immediate());
    }

    #[test]
    fn test_schedule_once_past() {
        let past = Utc::now() - ChronoDuration::hours(1);
        let schedule = Schedule::Once { at: past };

        let now = Utc::now();
        let next = schedule.next_execution(now);
        assert!(next.is_none());
    }

    #[test]
    fn test_schedule_interval() {
        let schedule = Schedule::Interval {
            seconds: 60,
            start_at: None,
            end_at: None,
        };

        let now = Utc::now();
        let next = schedule.next_execution(now);
        assert!(next.is_some());
        // Next execution should be within the next minute
        let diff = next.unwrap() - now;
        assert!(diff.num_seconds() <= 60);

        assert!(!schedule.is_once());
        assert!(!schedule.is_immediate());
    }

    #[test]
    fn test_schedule_interval_with_start() {
        let start = Utc::now() + ChronoDuration::hours(1);
        let schedule = Schedule::Interval {
            seconds: 60,
            start_at: Some(start),
            end_at: None,
        };

        let now = Utc::now();
        let next = schedule.next_execution(now);
        assert!(next.is_some());
        // Should start at the specified start time
        assert!(next.unwrap() >= start);
    }

    #[test]
    fn test_schedule_interval_with_end() {
        let end = Utc::now() - ChronoDuration::hours(1); // Already ended
        let schedule = Schedule::Interval {
            seconds: 60,
            start_at: None,
            end_at: Some(end),
        };

        let now = Utc::now();
        let next = schedule.next_execution(now);
        assert!(next.is_none()); // Should not run after end time
    }

    #[test]
    fn test_schedule_daily() {
        let schedule = Schedule::Daily {
            hour: 9,
            minute: 30,
        };

        let now = Utc::now();
        let next = schedule.next_execution(now);
        assert!(next.is_some());

        let next = next.unwrap();
        assert_eq!(next.hour(), 9);
        assert_eq!(next.minute(), 30);

        assert!(!schedule.is_once());
        assert!(!schedule.is_immediate());
    }

    #[test]
    fn test_schedule_weekly() {
        // Sunday = 0, Monday = 1, ..., Saturday = 6
        let schedule = Schedule::Weekly {
            day: 1, // Monday
            hour: 9,
            minute: 0,
        };

        let now = Utc::now();
        let next = schedule.next_execution(now);
        assert!(next.is_some());

        let next = next.unwrap();
        assert_eq!(next.hour(), 9);
        assert_eq!(next.minute(), 0);

        assert!(!schedule.is_once());
        assert!(!schedule.is_immediate());
    }

    #[test]
    fn test_task_update_next_run() {
        let mut task = Task::new(
            "Test".to_string(),
            "Description".to_string(),
            Schedule::Interval {
                seconds: 60,
                start_at: None,
                end_at: None,
            },
        );

        task.update_next_run();

        // last_run should be set
        assert!(task.last_run.is_some());
        // next_run should be updated (for interval, it should be in the future)
        assert!(task.next_run.is_some());
    }

    #[test]
    fn test_task_retry_count() {
        let mut task = Task::new(
            "Test".to_string(),
            "Description".to_string(),
            Schedule::Immediate,
        );
        task.max_retries = 3;

        // Fail 3 times
        for _ in 0..3 {
            task.record_execution(false, "Error".to_string(), 100);
        }

        assert_eq!(task.retry_count, 3);
        assert_eq!(task.fail_count, 3);

        // One more failure should not increase retry_count beyond max
        task.record_execution(false, "Error".to_string(), 100);
        assert_eq!(task.retry_count, 3); // Still 3 (max)

        // Success resets retry_count
        task.record_execution(true, "OK".to_string(), 100);
        assert_eq!(task.retry_count, 0);
    }

    #[test]
    fn test_schedule_serialization() {
        // Test JSON serialization
        let schedule = Schedule::Daily {
            hour: 14,
            minute: 30,
        };
        let json = serde_json::to_string(&schedule).unwrap();
        assert!(json.contains("daily"));
        assert!(json.contains("14"));
        assert!(json.contains("30"));

        // Deserialize
        let deserialized: Schedule = serde_json::from_str(&json).unwrap();
        assert_eq!(schedule, deserialized);
    }

    #[test]
    fn test_task_serialization() {
        let task = Task::new(
            "Test".to_string(),
            "Description".to_string(),
            Schedule::Immediate,
        );

        // Serialize to JSON
        let json = serde_json::to_string(&task).unwrap();
        assert!(json.contains("Test"));
        assert!(json.contains("Description"));

        // Deserialize
        let deserialized: Task = serde_json::from_str(&json).unwrap();
        assert_eq!(task.name, deserialized.name);
        assert_eq!(task.description, deserialized.description);
        assert_eq!(task.status, deserialized.status);
    }
}
