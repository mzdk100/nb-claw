//! LLM client manager

use {
    crate::{
        config::Config,
        llm::tools::{ToolCallResult, ToolRegistry},
        memory::Memory,
    },
    anyhow::{Context, Error, Result, anyhow},
    async_stream::try_stream,
    futures_util::StreamExt,
    llm_connector::{ChatRequest, LlmClient, Message, Role, Tool, ToolCall},
    serde_json::{from_str, json},
    std::{
        fmt::Display,
        sync::{RwLock, Weak},
        time::Instant,
    },
    tokio::time::{Duration, sleep},
    tracing::{info, warn},
    uuid::Uuid,
};

const RETRY_DELAY_SECS: u64 = 5;

/// Check if error indicates rate limiting
fn is_rate_limit_error<E: Display>(e: &E) -> bool {
    let err_str = e.to_string().to_lowercase();
    err_str.contains("rate limit")
        || err_str.contains("访问量过大")
        || err_str.contains("too many requests")
        || err_str.contains("429")
}

/// LLM Manager
///
/// Manages the LLM client and provides a high-level interface
/// for interacting with the AI assistant.
#[derive(Clone)]
pub struct LlmManager {
    client: LlmClient,
    tool_registry: Weak<ToolRegistry>,
    config: Config,
    memory: Weak<RwLock<Memory>>,
}

impl LlmManager {
    /// Create a new LLM manager from configuration
    pub fn new(
        config: &Config,
        tool_registry: Weak<ToolRegistry>,
        memory: Weak<RwLock<Memory>>,
    ) -> Result<Self> {
        // Build client using builder pattern - llm-connector provides professional defaults
        let client_builder = LlmClient::builder();

        // Configure based on provider
        let client_builder = match config.llm.provider.as_str() {
            "openai" => client_builder.openai(
                &config
                    .get_api_key()
                    .context("Failed to get API key from configuration")?,
            ),
            "anthropic" => client_builder.anthropic(
                &config
                    .get_api_key()
                    .context("Failed to get API key from configuration")?,
            ),
            "google" => client_builder.google(
                &config
                    .get_api_key()
                    .context("Failed to get API key from configuration")?,
            ),
            "zhipu" => client_builder.zhipu(
                &config
                    .get_api_key()
                    .context("Failed to get API key from configuration")?,
            ),
            "aliyun" => client_builder.aliyun(
                &config
                    .get_api_key()
                    .context("Failed to get API key from configuration")?,
            ),
            "deepseek" => client_builder.deepseek(
                &config
                    .get_api_key()
                    .context("Failed to get API key from configuration")?,
            ),
            "xiaomi" => client_builder.xiaomi(
                &config
                    .get_api_key()
                    .context("Failed to get API key from configuration")?,
            ),
            "volcengine" => client_builder.volcengine(
                &config
                    .get_api_key()
                    .context("Failed to get API key from configuration")?,
            ),
            "ollama" => {
                // Ollama typically doesn't need API key, use base_url if provided
                client_builder.ollama()
            }
            "tencent" => {
                let secret_id = config
                    .get_tencent_secret_id()
                    .context("Failed to get Tencent secret_id from configuration")?;
                let secret_key = config
                    .get_tencent_secret_key()
                    .context("Failed to get Tencent secret_key from configuration")?;
                client_builder.tencent(&secret_id, &secret_key)
            }
            "longcat" => client_builder.longcat_anthropic(
                &config
                    .get_api_key()
                    .context("Failed to get API key from configuration")?,
            ),
            "moonshot" => client_builder.moonshot(
                &config
                    .get_api_key()
                    .context("Failed to get API key from configuration")?,
            ),
            // Default to openai-compatible for unknown providers
            _ => {
                let api_key = config
                    .get_api_key()
                    .context("Failed to get API key from configuration")?;
                client_builder.openai_compatible(&api_key, &config.llm.provider)
            }
        };

        // Apply base_url if provided (overrides provider default)
        let client_builder = client_builder.base_url(&if let Some(url) = &config.llm.base_url
            && !url.is_empty()
        {
            url.to_owned()
        } else {
            match config.llm.provider.as_str() {
                "aliyun" => "https://dashscope.aliyuncs.com",
                "anthropic" => "https://api.anthropic.com",
                "deepseek" => "https://api.deepseek.com",
                "google" => "https://generativelanguage.googleapis.com/v1beta",
                "longcat" => "https://api.longcat.chat/anthropic",
                "moonshot" => "https://api.moonshot.cn/v1",
                "ollama" => "http://localhost:11434",
                "openai" => "https://api.openai.com/v1",
                "tencent" => "https://hunyuan.tencentcloudapi.com",
                "volcengine" => "https://ark.cn-beijing.volces.com/api/v3",
                "xiaomi" => "https://api.xiaomimimo.com/v1",
                "zhipu" => "https://open.bigmodel.cn",
                _ => panic!("Please set base_url in the configuration file"),
            }
            .into()
        });

        // Build the client
        let client = client_builder
            .build()
            .context("Failed to build LLM client")?;

        info!(
            "Created {} LLM client for model: {}",
            config.llm.provider, config.llm.model
        );

        Ok(Self {
            client,
            tool_registry,
            config: config.clone(),
            memory,
        })
    }

    /// Get the provider name
    pub fn provider_name(&self) -> &str {
        self.client.provider_name()
    }

    /// Build system prompt from configuration + memory + Python instructions
    fn build_system_prompt(&self, user_prompt: &str) -> Result<String> {
        let base_prompt = &self.config.system.system_prompt;

        // Query memory for relevant context (if available)
        let memory_context = self.query_memory(user_prompt)?;

        // Add Python usage instructions
        const PYTHON_INSTRUCTIONS: &str = r#"

## Memory Management

You have access to a Python module called `memory` for managing long-term memory.

**IMPORTANT: The memory module must be imported first when running the `run_py` tool.**

Available functions:
- `memory.remember("content", importance=0.5)` - Store a new memory
- `memories = memory.recall("query", limit=5)` - Search relevant memories
- `memory.forget("query", limit=3)` - Decay importance; delete if < 0
- `stats = memory.stats()` - Get memory statistics

The `importance` parameter ranges from 0.0 to 1.0:
- 0.0-0.4: Short-term memory (cleared automatically)
- 0.5-0.7: Long-term memory (persistent)
- 0.8-1.0: Personal memory (high priority)

Example usage:
```python
import memory

# Store a memory
memory.remember("User's name is Alice", importance=0.8)

# Recall memories
memories = memory.recall("Name is", limit=5)
for m in memories:
    print(m)

# Decay importance of matching memories
# Just reduce the importance, no need to insist on complete deletion
forgot_count = memory.forget("About Alice's name", limit=3)

## UI Automation (cross-platform)

Importing builtin module `uiauto`, and use `help(uiauto)` to check the help.
```
"#;

        // Add VCS instructions if enabled
        const VCS_INSTRUCTIONS: &str = r#"
## Version Control System

Importing builtin module `vcs`, you can track file versions, create snapshots, and restore files. This is useful when the user needs you to restore a previously modified file. Use `help(vcs)` to check the help.
```
"#;

        // Add Scheduler instructions if enabled
        const SCHEDULER_INSTRUCTIONS: &str = r#"
## Task Scheduler (cross-platform)

Importing builtin module `scheduler`, you can create and manage scheduled tasks. Tasks are executed by the AI using natural language descriptions. Use `help(scheduler)` to check the help.

**IMPORTANT: Task descriptions are prompts passed to the AI when the task is ready to execute.**

Rules for task descriptions:
1. **NO time references** - Do NOT include "in 1 minute", "tomorrow", "later", etc. The scheduler already handles timing.
2. **CONCISE** - Just describe WHAT to do, not HOW. The execution model will figure out the details.
3. **Action-oriented** - Describe the action to perform when the task triggers.

Examples of GOOD task descriptions:
- "Remind user to drink water"
- "Check if backup folder has today's files"
- "Send a greeting to the user"

Examples of BAD task descriptions:
- "Remind user to drink water in one minute" (❌ includes time - causes infinite loop!)
- "Tomorrow morning, check the weather" (❌ includes time reference)
- "Use the notification API to send..." (❌ too detailed, not needed)

Keep it simple: just state the goal, no time constraints.
"#;

        const MORE_MODULES: &str = r#"
## Discover More Modules

Use the `py_mods` tool to list all available builtin modules. This is the best way to discover new capabilities:
For each module, use `help(module_name)` to see its documentation and available functions.

**Tip**: Run `py_mods` periodically to discover new modules that may have been added!
"#;
        // Combine all parts
        let mut full_prompt = base_prompt.to_owned();

        if !memory_context.is_empty() {
            full_prompt.push_str("\n## Relevant Context from Memory\n");
            full_prompt.push_str(&memory_context);
        }

        full_prompt.push_str(PYTHON_INSTRUCTIONS);

        // Add VCS instructions if enabled
        if self.config.vcs.enabled {
            full_prompt.push_str(VCS_INSTRUCTIONS);
        }

        // Add Scheduler instructions if enabled
        if self.config.scheduler.enabled {
            full_prompt.push_str(SCHEDULER_INSTRUCTIONS);
        }
        full_prompt.push_str(MORE_MODULES);

        Ok(full_prompt)
    }

    /// Query memory for relevant context
    fn query_memory(&self, user_prompt: &str) -> Result<String> {
        if let Some(memory) = self.memory.upgrade() {
            // Perform semantic search for relevant memories
            let memory = memory
                .read()
                .map_err(|e| anyhow::anyhow!("Failed to acquire read lock: {}", e))?;
            let results = memory.search(user_prompt, 5); // Get top 5 relevant memories

            if results.is_empty() {
                return Ok(String::new());
            }

            // Format results as context
            let mut context = String::new();
            for (i, result) in results.iter().enumerate() {
                context.push_str(&format!(
                    "{}. {} (relevance: {:.2})\n",
                    i + 1,
                    result.entry.content,
                    result.score
                ));
            }

            Ok(context)
        } else {
            // No memory engine available
            Ok(String::new())
        }
    }

    /// Send a streaming chat request with tool execution support
    pub async fn chat_stream<S>(
        &self,
        user_prompt: S,
    ) -> Result<impl StreamExt<Item = Result<String>>>
    where
        S: AsRef<str>,
    {
        let tools = self
            .tool_registry
            .upgrade()
            .ok_or(anyhow!("Tool registry unavailable"))?
            .get_all_tools()
            .iter()
            .map(|t| t.to_llm_tool())
            .collect::<Vec<_>>();

        // Build system prompt from configuration + memory + Python usage instructions
        let system_prompt = self.build_system_prompt(user_prompt.as_ref())?;
        // Initialize messages for this conversation
        let mut current_messages = vec![
            Message::system(&system_prompt),
            Message::user(user_prompt.as_ref()),
        ];

        // Create the inner stream that handles the entire conversation flow
        let stream = try_stream! {
            let mut marker_finished = false;
            let mut marker_tool_call_start = None;
            let mut marker_tool_call_end = false;

            while !marker_finished {
                // Send tool definitions only once at session start (save tokens)
                let request = ChatRequest {
                    model: self.config.llm.model.clone(),
                    messages: current_messages.clone(),
                    tools: Some(tools.clone()),
                    stream: Some(true),
                    enable_thinking: Some(self.config.system.thinking_mode),
                    ..Default::default()
                };
                let mut tasks = Vec::new();
                let mut temp_content = String::new();
                let mut full_content = String::new();
                let mut script = String::new();

                // Collect streaming response with retry logic
                let max_retries = self.config.llm.max_retries;
                let mut retry_count = 0u32;
                let mut stream_iter = loop {
                    match self.client.chat_stream(&request).await {
                        Ok(stream) => break stream,
                        Err(e) if is_rate_limit_error(&e) && retry_count < max_retries => {
                            retry_count += 1;
                            warn!("Rate limit hit (attempt {}/{}), retrying in {} seconds...",
                                  retry_count, max_retries, RETRY_DELAY_SECS);
                            sleep(Duration::from_secs(RETRY_DELAY_SECS)).await;
                            continue;
                        }
                        Err(e) => Err(e)?,
                    }
                };

                while let Some(chunk) = stream_iter.next().await {
                    let chunk = match chunk {
                        Ok(c) => c,
                        Err(e) if is_rate_limit_error(&e) && retry_count < max_retries => {
                            retry_count += 1;
                            warn!("Rate limit during stream (attempt {}/{}), retrying in {} seconds...",
                                  retry_count, max_retries, RETRY_DELAY_SECS);
                            sleep(Duration::from_secs(RETRY_DELAY_SECS)).await;
                            // Reconnect to stream
                            stream_iter = loop {
                                match self.client.chat_stream(&request).await {
                                    Ok(stream) => break stream,
                                    Err(e) if is_rate_limit_error(&e) && retry_count < max_retries => {
                                        retry_count += 1;
                                        warn!("Rate limit hit (attempt {}/{}), retrying in {} seconds...",
                                              retry_count, max_retries, RETRY_DELAY_SECS);
                                        sleep(Duration::from_secs(RETRY_DELAY_SECS)).await;
                                        continue;
                                    }
                                    Err(e) => Err(e)?,
                                }
                            };
                            continue;
                        }
                        Err(e) => Err(e)?,
                    };

                    let mut iter = chunk.choices.into_iter();
                    while let Some(choice) = iter.next() {
                        // In streaming mode, we need to accumulate content from delta
                        // Check for tool calls in the delta
                        if let Some(tool_calls) = choice.delta.tool_calls  {
                            let mut iter = tool_calls.into_iter();
                            while let Some(ToolCall {function, ..}) = iter.next() {
                                let Ok(args) = from_str(&function.arguments) else {
                                    continue;
                                };
                                let tool_registry = self.tool_registry.clone();
                                tasks.push((
                                    function.name.clone(),
                                    tokio::spawn(async move {
                                        Ok::<_, Error>(tool_registry.upgrade().ok_or(anyhow!("Tool registry unavailable"))?.execute_tool(&function.name, &args, Uuid::new_v4().into()).await)
                                    })
                                ));
                            }
                        }

                        // Collect content from delta
                        if let Some(c) = &choice.delta.content {
                            temp_content.push_str(&c);
                            if let Some((i, lang)) = &marker_tool_call_start && !marker_tool_call_end {
                                if let Some(j) = temp_content.rfind("```") && j > *i && (lang == "python" || lang == "shell") {
                                    let (tool_name, arg_name) = if lang == "python" {
                                        ("run_py", "code")
                                    } else {
                                        ("run_cmd", "command")
                                    };
                                    // Remove trailing backticks from script due to streaming token boundaries
                                    let code = script.trim_end_matches('`');
                                    let args = json![{arg_name: code}];
                                    script.clear();

                                    let tool_registry = self.tool_registry.clone();
                                    tasks.push((
                                        tool_name.into(),
                                        tokio::spawn(async move {
                                            Ok::<_, Error>(tool_registry.upgrade().ok_or(anyhow!("Tool registry unavailable"))?.execute_tool(tool_name, &args, Uuid::new_v4().into()).await)
                                        })
                                    ));

                                    marker_tool_call_end = true;
                                } else {
                                    script.push_str(c);
                                }
                            } else if marker_tool_call_end {
                                full_content.push_str(&temp_content);
                                temp_content = c.into();
                                marker_tool_call_start = None;
                                marker_tool_call_end = false;
                                yield c.into();
                            } else if let Some((i, lang, _, right)) = find_markdown_block_start(&temp_content) {
                                if !lang.is_empty() {
                                    script.push_str(right);
                                    marker_tool_call_start = Some((i, lang.to_owned()));
                                }
                            } else {
                                yield c.into();
                            }
                        }

                        if let Some(f) = &choice.finish_reason && (f == "stop" || f == "length") && tasks.is_empty() {
                            marker_finished = true;
                            break;
                        }
                    }
                }
                if !temp_content.is_empty() {
                    full_content.push_str(&temp_content);
                }

                let mut new_messages = current_messages.clone();
                // Add the cleaned message (without code blocks)
                new_messages.push(Message::assistant(full_content.clone()));

                for (name, res) in tasks.drain(..) {
                    let res = res.await??;
                    info!(
                        "Tool `{}`: success={}, result={}",
                        name, res.success, res.result
                    );

                    // Format tool result as plain text for small models
                    let formatted_result = format_tool_result_for_small_models(&res);

                    new_messages.push(Message::tool(formatted_result, res.call_id));
                }

                current_messages = new_messages;
            }

            // Process after conversation: memory consolidation + task analysis
            let fut = Self::process_after_chat(
                self.client.clone(),
                self.memory.clone(),
                self.tool_registry.clone(),
                self.config.llm.model.clone(),
                current_messages,
                tools,
                self.config.llm.max_retries
            );
            tokio::spawn(async move {
                if let Err(e) = fut.await {
                    warn!(?e, "Post-chat processing failed");
                }
            });
        };

        Ok(stream)
    }

    /// Execute a scheduled task (non-streaming with tool execution support)
    pub async fn chat(
        client: LlmClient,
        tool_registry: Weak<ToolRegistry>,
        model: String,
        messages: Vec<Message>,
        max_retries: u32,
        tools: Vec<Tool>,
    ) -> Result<(bool, String)> {
        // Execute with tool support (up to 10 rounds)
        for _ in 0..max_retries {
            let request = ChatRequest {
                model: model.clone(),
                messages: messages.clone(),
                tools: Some(tools.clone()),
                stream: Some(false),
                enable_thinking: Some(true),
                ..Default::default()
            };

            // Send combined request
            let Ok(response) = client.chat(&request).await else {
                sleep(Duration::from_secs(RETRY_DELAY_SECS)).await;
                continue;
            };

            let mut iter = response.choices.into_iter();
            while let Some(choice) = iter.next() {
                // Track tool execution success
                let mut all_tools_success = true;

                // Execute any tool calls
                if let Some(tool_calls) = &choice.message.tool_calls {
                    for tool_call in tool_calls {
                        let result = tool_registry
                            .upgrade()
                            .ok_or(anyhow!("Tool registry unavailable"))?
                            .execute_tool(
                                &tool_call.function.name,
                                &from_str(&tool_call.function.arguments)?,
                                Uuid::new_v4().into(),
                            )
                            .await;
                        info!("Tool calling: {:?}", result);
                        if !result.success {
                            all_tools_success = false;
                        }
                    }
                }

                // Handle Markdown code blocks for small models
                let mut content = choice.message.content_as_text();
                if content.is_empty() {
                    content = response.content.clone();
                }
                if !content.is_empty() {
                    if let Some((_, lang, _, code_start)) = find_markdown_block_start(&content)
                        && lang == "python"
                    {
                        let code = if let Some(end_pos) = code_start.find("```") {
                            &code_start[..end_pos]
                        } else {
                            code_start
                        };

                        if !code.trim().is_empty() {
                            let result = tool_registry
                                .upgrade()
                                .ok_or(anyhow!("Tool registry unavailable"))?
                                .execute_tool(
                                    "run_py",
                                    &json!({ "code": code.trim() }),
                                    Uuid::new_v4().into(),
                                )
                                .await;
                            info!("Tool calling (from markdown): {:?}", result);
                            if !result.success {
                                all_tools_success = false;
                            }
                        }
                    }

                    return Ok((all_tools_success, content));
                }
            }

            return Ok((false, Default::default()));
        }

        Ok((false, "Task execution exceeded maximum rounds".to_string()))
    }

    /// Process after conversation: memory consolidation and task analysis (single request)
    async fn process_after_chat(
        client: LlmClient,
        memory: Weak<RwLock<Memory>>,
        tool_registry: Weak<ToolRegistry>,
        model: String,
        history_messages: Vec<Message>,
        tools: Vec<Tool>,
        max_retries: u32,
    ) -> Result<()> {
        // Combined prompt for memory consolidation and task analysis
        const COMBINED_PROMPT: &str = r#"You are an assistant that processes conversations after they end. Your job has TWO parts:

## Part 1: Memory Consolidation
Record important information from the conversation using Python code:
- Record at least 2 short-term memories (importance 0.1-0.4)
- Record at least 1 long-term memory (importance 0.5-0.7)

What to record:
- Important facts about the user
- Technical decisions or preferences discovered
- Problems encountered and solutions
- Your genuine thoughts or reflections about the conversation

What NOT to record:
- DO NOT mention this processing request
- DO NOT record meta-information about "memory recording"

## Part 2: Task Analysis
Identify tasks that need follow-up or scheduling:
- Tasks that failed, timed out, or encountered errors
- Tasks that were interrupted before completion
- Tasks that need periodic follow-up (backups, health checks)
- User requests for something to happen at a specific time

Create tasks ONLY when clearly needed. Do NOT create tasks for successful operations or simple conversations.

## Output Format
Output a single Python code block that does BOTH tasks:

```python
import memory
import scheduler

# Part 1: Record memories (must)
memory.remember("something important", importance=0.3)
memory.remember("another thing", importance=0.6)

# Part 2: Create scheduled tasks (if needed)
# task_id = scheduler.once("Retry failed task", "print('retrying')", hours=1)
```

If no task is needed, just skip the scheduler part. If nothing worth remembering, express a thought instead.

## Important
- Use `run_py` tool to execute the code
- Keep it concise - one code block for everything
- Be selective about what to remember and schedule"#;

        // Filter tools to only include run_py
        let filtered_tools: Vec<_> = tools
            .iter()
            .filter(|t| t.function.name == "run_py")
            .cloned()
            .collect();

        let mut messages = vec![Message::system(COMBINED_PROMPT)];
        messages.extend(
            history_messages
                .into_iter()
                .filter(|m| m.role != Role::System),
        );
        messages.push(Message::user(
            "Process this conversation: record memories and create scheduled tasks if needed.",
        ));
        let (success, content) = Self::chat(
            client,
            tool_registry,
            model,
            messages,
            max_retries,
            filtered_tools,
        )
        .await?;
        if success && !content.is_empty() {
            info!("Post-chat response: {}", content);
        }

        // Save memories to disk
        if let Some(memory) = memory.upgrade() {
            if let Ok(lock) = memory.write() {
                lock.save_to_disk()?;
            }
        }

        Ok(())
    }

    /// Execute a task description (for scheduler)
    pub async fn execute_task(&self, description: &str) -> Result<(bool, String, u64)> {
        let start = Instant::now();

        // Build messages for task execution
        let system_prompt = self.build_system_prompt(description)?;
        let messages = vec![Message::system(&system_prompt), Message::user(description)];

        // Build tools using llm_connector types
        let tools = self
            .tool_registry
            .upgrade()
            .ok_or(anyhow!("Tool registry unavailable"))?
            .get_all_tools()
            .iter()
            .map(|t| t.to_llm_tool())
            .collect();

        let (success, message) = Self::chat(
            self.client.clone(),
            self.tool_registry.clone(),
            self.config.llm.model.clone(),
            messages,
            self.config.llm.max_retries,
            tools,
        )
        .await?;

        let duration_ms = start.elapsed().as_millis() as u64;
        Ok((success, message, duration_ms))
    }
}

fn find_markdown_block_start(
    content: &str,
) -> Option<(
    /*byte pos*/ usize,
    /*lang*/ &str,
    /*left splited*/ &str,
    /*right splited*/ &str,
)> {
    fn mat<'a>(content: &'a str, start_word: &str, out: &mut &'a str) -> bool {
        let s = content.trim_start().to_ascii_lowercase();
        if s.len() < start_word.len() {
            return false;
        }

        if !s.starts_with(start_word) {
            return false;
        }

        let (_, q) = s.split_at(start_word.len());
        if q.is_empty() {
            let j = content.len() - s.len() + start_word.len();
            let (_, q) = content.split_at(j);
            *out = q;
            true
        } else if q.starts_with([' ', '\n']) {
            let j = content.len() - s.len() + start_word.len() + 1;
            let (_, q) = content.split_at(j);
            *out = q;
            true
        } else {
            false
        }
    }

    if let Some(i) = content.find("```") {
        let (left, right) = content.split_at(i);
        if right.len() < 4 {
            return Some((i, "", left, ""));
        }

        let (_, q) = content.split_at(i + 3);
        let mut j = "";
        if mat(q, "py", &mut j) || mat(q, "python", &mut j) {
            return Some((i, "python", left, j));
        } else if mat(q, "bash", &mut j)
            || mat(q, "fish", &mut j)
            || mat(q, "zsh", &mut j)
            || mat(q, "sh", &mut j)
            || mat(q, "cmd", &mut j)
            || mat(q, "powershell", &mut j)
            || mat(q, "shell", &mut j)
        {
            return Some((i, "shell", left, j));
        }
    }

    None
}

/// Format tool result as plain text for small models that don't understand structured formats
/// This makes it easier for small models to read and understand tool execution results
fn format_tool_result_for_small_models(tool_result: &ToolCallResult) -> String {
    let status = if tool_result.success {
        "SUCCESS"
    } else {
        "FAILED"
    };

    match tool_result.tool_name.as_str() {
        "run_py" => {
            format!(
                "[Python Execution {}]\nOutput:\n{}\n",
                status, tool_result.result
            )
        }
        "run_cmd" => {
            format!(
                "[Command Execution {}]\nOutput:\n{}\n",
                status, tool_result.result
            )
        }
        "py_mods" => {
            format!("[Available Modules]\n{}\n", tool_result.result)
        }
        _ => {
            format!(
                "[Tool {} Result]\n{}\n",
                tool_result.tool_name, tool_result.result
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_markdown_block_start() {
        assert_eq!(
            find_markdown_block_start("```py\nprint('hello')\n```"),
            Some((0, "python", "", "print('hello')\n```"))
        );
        assert_eq!(
            find_markdown_block_start("```python\nprint('hello')\n```"),
            Some((0, "python", "", "print('hello')\n```"))
        );
        assert_eq!(
            find_markdown_block_start("```bash\necho 'hello'\n```"),
            Some((0, "shell", "", "echo 'hello'\n```"))
        );
        assert_eq!(
            find_markdown_block_start("```sh\necho 'hello'\n```"),
            Some((0, "shell", "", "echo 'hello'\n```"))
        );
        assert_eq!(
            find_markdown_block_start("```cmd\necho 'hello'\n```"),
            Some((0, "shell", "", "echo 'hello'\n```"))
        );
        assert_eq!(
            find_markdown_block_start("```java\necho 'hello'\n```"),
            None
        );
        assert_eq!(find_markdown_block_start("```pyt\necho 'hello'\n```"), None);
        assert_eq!(
            find_markdown_block_start("test\n```python"),
            Some((5, "python", "test\n", ""))
        );
    }
}
