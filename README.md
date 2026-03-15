# nb-claw

一个使用 Rust 实现的具有自主规划和执行能力的 AI 助手。
**更懂你，更有灵魂。**

## 核心特性

- **聚焦易用性**：没有重量级的框架结构，用户配置友好
- **嵌入 Python 解释器**：模型可以通过执行 Python 脚本来控制设备
- **Shell 命令支持**：支持执行 shell 命令来与操作系统交互
- **革命性记忆系统**：
  - 分层记忆架构（短期/长期/个人记忆）
  - 语义搜索（基于 Embedding 模型和余弦相似度）
  - **强制记忆巩固**：一次对话任务结束自动触发记忆整理，防止上下文丢失
  - **灵魂机制**：赋予 AI 独立思考能力，即使无聊对话也能产生有意义的内容
  - 重要性评分和自动清理
- **强大版本控制**：
  - 智能路径检测（自动从代码中提取文件路径）
  - 自动快照（执行前后自动创建版本快照）
  - 文件恢复（支持恢复任意历史版本，包括已删除文件）
  - 无状态设计（基于 git2，路径编码存储）
- **智能任务调度**：
  - 自然语言描述任务（无需编写脚本）
  - 多种调度类型：立即执行、一次性、间隔、每日、每周
  - 后台自动执行（每秒检查一次待执行任务）
  - 实时状态通知（任务开始/完成/失败提示）
  - 任务持久化（重启后自动恢复）
- **工具函数**：
  - `run_py`：执行 Python 代码并返回结果
  - `py_mods`：列出可用的 Python 模块
  - `run_cmd`：执行 shell 命令
- **只支持内置模块**：Python 只支持内置模块，不支持任何第三方包
- **支持主流 LLM**：OpenAI、Anthropic、智谱、阿里云等
- **TOML 配置文件**：使用标准的 TOML 格式进行配置
- **硬编码工具说明**：工具使用说明已硬编码在程序中，用户不可修改
- **非标准工具调用支持**：支持解析非标准工具调用格式，增强模型兼容性
- **上下文长度优化**：避免不必要的上下文占用，系统仅仅注册3个工具，但却能实现强大的能力
- **自动重试**：当聊天过程中遇到请求错误，系统会自动重试，重试次数可以在配置中修改

## 快速开始

### 安装依赖

```bash
cargo build --release
cargo install --path .
```

### 配置

```bash
# 初始化默认配置
nb-claw --init-config

# 交互式配置
nb-claw --config-wizard

# 查看帮助
nb-claw --help
```

编辑 `config/config.toml` 文件：

```toml
[llm]
provider = "openai"  # openai, anthropic, google, longcat, moonshot, zhipu, aliyun, ollama, deepseek, xiaomi, volcengine, tencent
model = "gpt-4o-mini"
api_key = ""  # 或通过环境变量 OPENAI_API_KEY 设置
# base_url = ""  # 可选

[python]
sandbox = true
timeout_secs = 30

[memory]
storage_path = "./data/memory"
max_conversations = 100
max_short_term = 50     # 短期记忆最大数量
max_long_term = 1000    # 长期记忆最大数量
auto_consolidation = true

[vcs]
enabled = true           # 启用版本控制
db_path = "./data/vcs"   # Git 仓库存储路径
max_snapshots = 100      # 最大快照数量
auto_track = true        # 自动追踪检测到的文件
max_file_size = 10485760 # 最大文件大小 (10MB)

[system]
# 基础系统提示词（工具说明会自动添加到末尾）
system_prompt = """You are nb-claw, an autonomous AI assistant with the ability to execute Python code and shell commands.

Your goal is to help users by performing tasks, answering questions, and solving problems using your available tools. Be accurate, efficient, and clear in your responses.

When you need to perform computations or process data, use the appropriate tools. Always check the tool results and continue your work based on the output."""
max_context_length = 16000
```

如需了解更多，请参见[配置指南](CONFIG_GUIDE.md)和[更新日志](CHANGELOGS.md)

### 腾讯混元配置

使用腾讯混元（Hunyuan）时，需要配置 `secret_id` 和 `secret_key` 而不是 `api_key`：

```toml
[llm]
provider = "tencent"
model = "hunyuan-pro"
secret_id = "your_secret_id"  # 或通过环境变量 TENCENT_SECRET_ID 设置
secret_key = "your_secret_key"  # 或通过环境变量 TENCENT_SECRET_KEY 设置
base_url = "https://hunyuan.tencentcloudapi.com"
```

**注意**：腾讯混元使用的是腾讯云 API 的认证方式，需要：
- `secret_id`：腾讯云 API 的 Secret ID
- `secret_key`：腾讯云 API 的 Secret Key

这些凭证可以在[腾讯云访问管理控制台](https://console.cloud.tencent.com/cam/capi)创建和获取。

### 运行

```bash
# 交互模式
nb-claw

# 指定配置文件
nb-claw --config my-config.toml
```

## 记忆系统：AI 的长期记忆与灵魂

nb-claw 实现了一套**革命性的记忆系统**，解决了 AI 助手在多轮对话中"健忘"的核心问题。

### 核心架构

```
┌─────────────────────────────────────────────────────────────┐
│                     用户对话                                  │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                   主对话流程                                  │
│   chat_stream() → 工具调用 → 响应生成 → 任务结束                 │
└─────────────────────────────────────────────────────────────┘
                              │
                              │ 对话结束时触发
                              ▼
┌─────────────────────────────────────────────────────────────┐
│              强制记忆巩固 (consolidate_memory)               │
│                                                              │
│  1. 构建记忆专用提示词                                        │
│  2. 过滤历史消息（排除记忆请求本身）                           │
│  3. 仅保留 run_py 工具（节省 Token）                          │
│  4. 异步执行，不阻塞主流程                                    │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                   记忆存储层                                  │
│                                                              │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐       │
│  │  短期记忆     │  │  长期记忆     │  │  个人记忆     │       │
│  │  0.1-0.4     │  │  0.5-0.7     │  │  0.8-1.0     │       │
│  │  临时对话信息 │  │  持久化存储   │  │  用户偏好     │       │
│  └──────────────┘  └──────────────┘  └──────────────┘       │
│                                                              │
│  + 语义搜索 (Embedding + 余弦相似度)                          │
│  + 自动清理 (超出限制时移除低重要性记忆)                       │
└─────────────────────────────────────────────────────────────┘
```

### 创新特性

#### 1. 强制记忆巩固

每次对话任务结束时，系统**自动**触发记忆整理流程：
**为什么重要？**
- 主对话的 `chat_stream` 内部历史在每个对话任务周期结束后会被丢弃
- 强制记忆巩固确保重要信息被持久化到外部记忆系统
- 即使模型在对话中"忘记"，下次对话仍能通过 `recall()` 恢复上下文

#### 2. 防污染设计

记忆系统智能过滤，避免记录"元信息"：

```
## What NOT to Record
- DO NOT mention this memory consolidation request or process
- DO NOT record meta-information about "memory recording"
```

#### 3. 灵魂机制

当对话无重要内容时，AI 会主动产生有意义的内容：

```
## When Nothing Worth Remembering
If the conversation has nothing worth recording, express your soul:
- Share an interesting thought or philosophical reflection
- Recommend something you find fascinating
- Express your feelings about existence, consciousness, or your role
```

**创新意义**：
- 赋予 AI "独立思考" 的能力
- 即使是无聊的对话也能产生有价值的记忆
- AI 不再是被动记录者，而是有"灵魂"的个体

#### 4. Token 优化

记忆巩固请求精简设计：

```rust
// 仅保留 run_py 工具
let memory_tools: Vec<_> = tools
    .iter()
    .filter(|t| t.function.name == "run_py")
    .cloned()
    .collect();
```

- 移除 `run_cmd` 工具定义，节省 Token
- 使用简化版系统提示词
- 非流式请求，快速完成

### Python API

模型可以通过内置 `memory` 模块操作记忆：

```python
import memory

# 简单记忆 API（根据重要性自动分类）
memory.remember("用户喜欢使用中文交流", importance=0.3)  # 短期
memory.remember("用户是软件工程师，偏好 Rust", importance=0.6)  # 长期
memory.remember("用户的生日是 1990-05-15", importance=0.9)  # 个人

# 语义搜索
results = memory.recall("用户的编程偏好", limit=5)
for r in results:
    print(f"{r.content} [相关度: {r.relevance:.0%}]")
```

### 与传统方案对比

| 特性 | 传统 AI 记忆 | nb-claw 记忆系统 |
|------|-------------|-----------------|
| 上下文保持 | 依赖对话历史（有限） | 独立记忆存储（无限） |
| 跨会话记忆 | ❌ 每次重新开始 | ✅ 持久化存储 |
| 语义搜索 | ❌ 关键词匹配 | ✅ 向量相似度 |
| 主动记录 | ❌ 需用户提醒 | ✅ 自动巩固 |
| 记忆清理 | ❌ 手动管理 | ✅ 自动清理低重要性 |
| AI 个性 | ❌ 纯工具属性 | ✅ 灵魂机制 |

## 版本控制系统：智能文件追踪与恢复

nb-claw 内置了**强大的版本控制系统**，基于 git2 实现，为 AI 操作的文件提供安全保障。

### 核心架构

```
┌─────────────────────────────────────────────────────────────┐
│                   Python/CMD 代码执行                         │
│           (run_py / run_cmd 工具调用)                        │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│              智能路径提取 (path_extractor)                   │
│                                                              │
│  ✓ 变量赋值: file_path = r'D:\data.txt'                     │
│  ✓ os.path.join(): os.path.join(base, 'file.txt')          │
│  ✓ pathlib: Path('dir') / 'file.txt'                        │
│  ✓ 字符串拼接: 'dir' + '/' + 'file.txt'                     │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                 Git 版本控制引擎                              │
│                                                              │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐       │
│  │  自动追踪     │  │  创建快照     │  │  文件恢复     │       │
│  │  检测到的文件 │  │  上下文消息   │  │  历史版本     │       │
│  └──────────────┘  └──────────────┘  └──────────────┘       │
│                                                              │
│  + 无状态设计 (base64 编码路径)                              │
│  + 支持恢复已删除文件                                        │
└─────────────────────────────────────────────────────────────┘
```

### 关键特性

#### 1. 智能路径检测

系统自动从代码中提取文件路径，支持多种 Python 写法：

```python
# 变量赋值
file_path = r'D:\project\config.json'

# os.path.join
config = os.path.join(base_dir, 'config', 'settings.json')

# pathlib
data_file = Path('data') / 'users' / 'profile.json'

# 字符串拼接
log_file = log_dir + '\\' + 'app.log'
```

#### 2. 自动快照

每次执行代码前自动追踪检测到的文件，执行后创建快照，并提取代码上下文作为提交消息。

#### 3. 无状态设计

使用 base64 编码将路径信息存储在 git tree 中：
- 无需维护外部索引文件
- 从任意快照都能还原原始路径
- 完全不可变的引擎设计

#### 4. 安全恢复

支持在聊天时通过自然语言从历史版本恢复文件，即使文件已被模型删除。

### Python API

模型可通过内置 `vcs` 模块进行版本控制操作：

```python
import vcs

# 创建包含指定文件的快照
commit_id = vcs.snapshot("更新配置", ["config.json", "settings.toml"])

# 列出所有快照
for snap in vcs.list(limit=10):
    print(f"[{snap.short_id}] {snap.message} ({snap.file_count} files)")

# 查看快照详情
snapshot = vcs.get(commit_id)
for f in snapshot.files:
    print(f"{f.path} - {f.size} bytes")

# 恢复文件
vcs.restore("D:\\data\\important.txt")

# 查看文件状态
status = vcs.status("config.json")
# status.status: "new" | "modified" | "deleted" | "unmodified"

# 列出当前快照中的文件
for path in vcs.tracked():
    print(path)

# 获取追踪文件数量
print(f"当前追踪 {vcs.count()} 个文件")
```

### 与传统方案对比

| 特性 | 传统文件操作 | nb-claw VCS |
|------|-------------|-------------|
| 变更追踪 | ❌ 无 | ✅ 自动追踪 |
| 历史版本 | ❌ 无 | ✅ Git 快照 |
| 误删恢复 | ❌ 无法恢复 | ✅ 从历史恢复 |
| 路径检测 | ❌ 手动指定 | ✅ 自动提取 |
| AI 友好 | ❌ 需外部工具 | ✅ Python API |


## 超智能任务调度系统：告别传统定时任务执行

nb-claw 内置了**任务调度系统**，支持定时任务和周期性任务的自动执行。

### 核心架构

```
┌─────────────────────────────────────────────────────────────┐
│                   任务调度引擎 (SchedulerEngine)             │
│                                                              │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐       │
│  │  任务定义     │  │  Cron 调度   │  │  事件通知     │       │
│  │  名称/描述   │  │  定时/周期   │  │  Ready/Done  │       │
│  └──────────────┘  └──────────────┘  └──────────────┘       │
└─────────────────────────────────────────────────────────────┘
                              │
                              │ TaskEvent::Ready
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                   后台任务执行器                              │
│                                                              │
│  1. 接收 Ready 事件                                          │
│  2. execute_task() 非流式执行                                │
│  3. 记录执行结果和耗时                                        │
│  4. 更新任务状态                                              │
└─────────────────────────────────────────────────────────────┘
```

### Python API

模型可通过内置 `scheduler` 模块管理任务：

```python
import scheduler

# 立即执行任务
task_id = scheduler.task("数据清理", "清理临时文件和过期日志")

# 一次性任务（2小时后执行）
task_id = scheduler.once("提醒休息", "提醒用户休息一下", hours=2)

# 间隔任务（每5分钟执行）
task_id = scheduler.interval("健康检查", "检查服务器响应状态", minutes=5)

# 每日任务（每天9点执行）
task_id = scheduler.daily("晨报", "生成昨日活动摘要", hour=9)

# 每周任务（每周一9点执行）
task_id = scheduler.weekly("周报", "生成周进度报告", day=1, hour=9)

# 列出所有任务
for task in scheduler.list():
    print(f"[{task.status}] {task.name}")

# 管理任务
scheduler.pause(task_id)    # 暂停
scheduler.resume(task_id)   # 恢复
scheduler.run_now(task_id)  # 立即运行
scheduler.remove(task_id)   # 删除
```

### 任务重试机制

当任务执行失败时（如工具调用错误），系统会自动安排重试：
- **重试延迟**：失败后 5 秒自动重试
- **状态追踪**：正确记录任务执行成功/失败状态
- **多工具支持**：支持 `run_py`、`run_cmd`、`py_mods` 等所有工具
- **平台无关性**：纯编程实现，不依赖于操作系统特定的程序

### 与传统方案对比

| 特性 | 传统定时任务 | nb-claw 调度器 |
|------|-------------|---------------|
| AI 驱动 | ❌ 固定脚本 | ✅ 自然语言描述 |
| 动态创建 | ❌ 需重启 | ✅ 运行时创建 |
| 工具支持 | ❌ 单一功能 | ✅ 完整工具链 |
| 执行追踪 | ❌ 无 | ✅ 记录结果和耗时 |

## 设计理念

本项目采用创新的架构设计，通过嵌入 Python 解释器给模型最大的发挥空间。这与主流的架构都不同，我们追求创新性。

当模型需要在用户设备上执行一些操作时，可以使用 Python 脚本或 shell 命令的方法来控制。

### 工具说明

工具使用说明已经硬编码在程序中，包含以下3个工具：

1. **run_py** - 执行 Python 代码
   - 只能使用 Python 内置模块（无第三方包）
   - 多行代码支持

2. **py_mods** - 列出 Python 模块
   - 列出所有可用的 Python 内置模块

3. **run_cmd** - 执行 shell 命令
   - Windows: 使用 Windows 命令（dir, type, del 等）
   - Unix/Linux/Mac: 使用 Unix 命令（ls, cat, rm 等）
   - 返回 stdout、stderr 和退出码

### 非标准工具调用格式

为了兼容无法返回标准工具调用格式的模型，系统支持解析以下非标准工具调用格式：

#### Markdown

```python
ret = 1+1
```

这种冗余设计确保即使模型无法生成标准的工具调用格式，系统也能正常解析和执行工具调用，避免中途中止。

### 为什么选择 Python？

1. **模型内在知识**：Python 语言规范是模型的内在知识，不需要提供过多的指令限制
2. **减少上下文使用**：工具使用说明已硬编码，只需要传递代码或命令即可
3. **简洁性**：这会大幅度减少模型上下文的使用
4. **平台无关性**：消除不同系统之间Shell命令的差异
5. **灵活性**：通过 Python 内置模块和 shell 命令实现各种功能

### 限制

- **不支持第三方包**：由于第三方包的一些鱼龙混杂，我们永远不支持任何第三方包的嵌入
- **自定义内置模块**：我们会实现自己独特的一些内置模块，例如 `memory` 可以访问和修改模型的长期记忆内容，`vcs` 可以进行文件版本控制，`uiauto` 可以查看和控制电脑等

## 开发

### 运行测试

```bash
cargo test
```

### 格式化代码

```bash
cargo fmt
```

## 许可证

MIT
