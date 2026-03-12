# nb-claw 配置指南

本文档详细介绍 nb-claw 的所有配置选项。

## 命令行工具

### 初始化配置文件

使用默认值快速创建配置文件：

```bash
# 在默认位置创建配置文件 (config/config.toml)
nb-claw --init-config

# 在指定位置创建配置文件
nb-claw --init-config my-config.toml
```

> **注意**：如果配置文件已存在，系统会询问是否覆盖，不会直接覆盖。

### 交互式配置向导

使用向导模式逐步配置：

```bash
nb-claw --config-wizard
```

向导会引导您：
1. 选择 LLM 提供商
2. 设置模型名称
3. 配置 API 密钥
4. 选择 Embedding 模型
5. 设置 HuggingFace 镜像（可选）
6. 保存配置

> **注意**：保存时如果文件已存在，会先询问是否覆盖。

### 其他命令行参数

| 参数 | 说明 |
|------|------|
| `-c, --config <PATH>` | 指定配置文件路径 |
| `-d, --debug` | 启用调试日志 |
| `--test` | 运行测试模式 |
| `--init-config [PATH]` | 初始化配置文件（覆盖前会询问） |
| `--config-wizard` | 运行交互式配置向导 |

---

## 配置文件位置

nb-claw 会在以下位置按顺序查找配置文件：

1. 命令行参数指定的路径（`--config`）
2. 当前目录下的 `config.toml`
3. `config/config.toml`

## 配置结构概览

```toml
[llm]      # LLM 提供商配置
[python]   # Python 解释器配置
[memory]   # 记忆系统配置
[memory.embedding]  # Embedding 模型配置
[system]   # 系统行为配置
```

---

## LLM 配置 `[llm]`

### 基础配置

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `provider` | string | ✅ | LLM 提供商名称 |
| `model` | string | ✅ | 模型名称 |
| `api_key` | string | ❌ | API 密钥（推荐使用环境变量） |
| `base_url` | string | ❌ | 自定义 API 端点 |
| `timeout_secs` | u64 | ❌ | 请求超时（秒），默认 60 |
| `max_retries` | u32 | ❌ | 最大重试次数，默认 3 |

### 支持的 Provider

| Provider | 环境变量 | 默认模型 | 默认 Base URL |
|----------|----------|----------|---------------|
| `openai` | `OPENAI_API_KEY` | gpt-5.4 | https://api.openai.com/v1 |
| `anthropic` | `ANTHROPIC_API_KEY` | claude-opus-4-6 | https://api.anthropic.com |
| `google` | `GOOGLE_API_KEY` | gemini-3.1-pro-preview | https://generativelanguage.googleapis.com/v1beta |
| `longcat` | `LONGCAT_API_KEY` | longcat-flash-thinking-2601 | https://api.longcat.chat/anthropic |
| `moonshot` | `MOONSHOT_API_KEY` | kimi-k2.5 | https://api.moonshot.cn/v1 |
| `zhipu` | `ZHIPU_API_KEY` | glm-5 | https://open.bigmodel.cn/api/paas/v4 |
| `aliyun` | `ALIYUN_API_KEY` | qwen3-coder-next | https://dashscope.aliyuncs.com |
| `ollama` | - | llama3.2 | http://localhost:11434 |
| `deepseek` | `DEEPSEEK_API_KEY` | deepseek-v3 | https://api.deepseek.com |
| `xiaomi` | `XIAOMI_API_KEY` | mimo-v2-flash | https://api.xiaomimimo.com/v1 |
| `volcengine` | `VOLCENGINE_API_KEY` | doubao-2.0 | https://ark.cn-beijing.volces.com/api/v3 |
| `tencent` | `TENCENT_SECRET_ID` + `TENCENT_SECRET_KEY` | hunyuan-image-3.0-instruct | https://hunyuan.tencentcloudapi.com |

### 配置示例

```toml
# OpenAI (GPT-5.4, 1M context)
[llm]
provider = "openai"
model = "gpt-5.4"

# Anthropic (Claude Opus 4.6, 1M context)
[llm]
provider = "anthropic"
model = "claude-opus-4-6-20260201"

# 智谱 AI (GLM-5, Coding & Agent SOTA)
[llm]
provider = "zhipu"
model = "glm-5"

# 月之暗面 (Kimi K2.5, 开源多模态 MoE)
[llm]
provider = "moonshot"
model = "kimi-k2.5"

# Ollama（本地部署，无需 API Key）
[llm]
provider = "ollama"
model = "llama3.2"
base_url = "http://localhost:11434"  # 可选

# 腾讯混元（需要 secret_id 和 secret_key）
[llm]
provider = "tencent"
model = "hunyuan-pro"
secret_id = "your-secret-id"
secret_key = "your-secret-key"
```

### API Key 最佳实践

**推荐使用环境变量：**

```bash
# Linux/Mac
export OPENAI_API_KEY="sk-..."

# Windows CMD
set OPENAI_API_KEY=sk-...

# Windows PowerShell
$env:OPENAI_API_KEY="sk-..."
```

配置文件中无需指定 `api_key`：

```toml
[llm]
provider = "openai"
model = "gpt-4o"
# api_key 从环境变量自动读取
```

---

## Python 配置 `[python]`

控制 Python 沙箱环境的行为。

| 参数 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `sandbox` | bool | `true` | 启用沙箱模式，限制危险操作 |
| `timeout_secs` | u64 | `30` | 代码执行超时（秒） |
| `dangerous_modules` | [string] | 见下表 | 沙箱模式下禁止导入的模块 |
| `safe_modules` | [string] | 见下表 | 沙箱模式下允许导入的模块 |

### 默认的危险模块

```toml
dangerous_modules = [
    "os",           # 系统操作
    "subprocess",   # 子进程
    "shutil",       # 文件操作
    "socket",       # 网络套接字
    "ftplib",       # FTP
    "telnetlib",    # Telnet
    "pickle",       # 序列化（安全风险）
    "marshal",      # 序列化
    "importlib",    # 动态导入
]
```

### 默认的安全模块

```toml
safe_modules = [
    "math",         # 数学函数
    "json",         # JSON 处理
    "re",           # 正则表达式
    "datetime",     # 日期时间
    "collections",  # 集合类型
    "itertools",    # 迭代器
    "statistics",   # 统计函数
    "decimal",      # 高精度数学
    "fractions",    # 分数
    "random",       # 随机数
    "string",       # 字符串常量
    "textwrap",     # 文本包装
    "urllib",       # URL 处理
    "memory",       # 记忆模块
    "typing",       # 类型提示
]
```

### 配置示例

```toml
[python]
# 启用沙箱模式（推荐）
sandbox = true
# 代码执行超时
timeout_secs = 30
# 自定义安全模块列表
safe_modules = ["math", "json", "re", "datetime", "collections", "memory"]
```

**关闭沙箱模式（不推荐）：**

```toml
[python]
sandbox = false
timeout_secs = 60
```

---

## Memory 配置 `[memory]`

控制记忆系统的存储和行为。

| 参数 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `storage_path` | string | `./data/memory` | 记忆存储路径 |
| `max_conversations` | usize | `100` | 最大对话数量 |
| `max_short_term` | usize | `50` | 短期记忆上限 |
| `max_long_term` | usize | `1000` | 长期记忆上限 |
| `auto_consolidation` | bool | `true` | 自动记忆整合 |
| `storage_format` | string | `binary` | 存储格式：`json` 或 `binary` |

### 存储格式

| 格式 | 说明 |
|------|------|
| `binary` | 二进制格式（默认），使用 postcard 序列化。紧凑、快速、更安全 |
| `json` | JSON 格式，人类可读，便于调试 |

**二进制格式的优势：**
- 更紧凑：通常比 JSON 小 30-50%
- 更快速：序列化/反序列化效率更高
- 更安全：无法直接编辑，防止意外修改

**格式自动迁移：**

切换格式时，系统会自动迁移现有数据：
- 从 JSON 切换到 Binary：读取 `.json` 文件后保存为 `.bin`，删除旧文件
- 从 Binary 切换到 JSON：读取 `.bin` 文件后保存为 `.json`，删除旧文件

### 配置示例

```toml
[memory]
storage_path = "./data/memory"
max_conversations = 100
max_short_term = 50
max_long_term = 1000
auto_consolidation = true
# 使用二进制格式（推荐）
storage_format = "binary"
```

**调试时使用 JSON 格式：**

```toml
[memory]
storage_format = "json"  # 便于手动查看和编辑
```

---

## Embedding 配置 `[memory.embedding]`

配置语义搜索使用的 Embedding 模型。

| 参数 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `enabled` | bool | `true` | 是否启用 Embedding 模型 |
| `model` | string | `BAAI/bge-m3` | Embedding 模型名称 |
| `hf_endpoint` | string | - | HuggingFace 镜像地址 |

### 可用模型

#### BGE 系列（英语）

| 模型 | 维度 | 特点 |
|------|------|------|
| `Xenova/bge-small-en-v1.5` | 384 | 速度快 |
| `Xenova/bge-base-en-v1.5` | 768 | 平衡性能 |
| `Xenova/bge-large-en-v1.5` | 1024 | 最佳效果 |
| `Qdrant/bge-small-en-v1.5-onnx-Q` | 384 | 量化版，更小 |
| `Qdrant/bge-base-en-v1.5-onnx-Q` | 768 | 量化版 |
| `Qdrant/bge-large-en-v1.5-onnx-Q` | 1024 | 量化版 |

#### BGE 中文系列

| 模型 | 维度 | 特点 |
|------|------|------|
| `Xenova/bge-small-zh-v1.5` | 512 | 中文小型模型 |
| `Xenova/bge-large-zh-v1.5` | 1024 | 中文大型模型 |

#### BGE M3 多语言

| 模型 | 维度 | 特点 |
|------|------|------|
| `BAAI/bge-m3` | 1024 | 支持 100+ 语言，8192 context |

#### Multilingual E5 系列

| 模型 | 维度 | 特点 |
|------|------|------|
| `intfloat/multilingual-e5-small` | 384 | 多语言小型模型 |
| `intfloat/multilingual-e5-base` | 768 | 多语言中型模型 |
| `Qdrant/multilingual-e5-large-onnx` | 1024 | 多语言大型模型 |

#### Snowflake Arctic 系列

| 模型 | 维度 | 特点 |
|------|------|------|
| `snowflake/snowflake-arctic-embed-xs` | 384 | 超小模型 |
| `snowflake/snowflake-arctic-embed-s` | 384 | 小型模型 |
| `Snowflake/snowflake-arctic-embed-m` | 768 | 中型模型 |
| `snowflake/snowflake-arctic-embed-m-long` | 768 | 中型，2048 context |
| `snowflake/snowflake-arctic-embed-l` | 1024 | 大型模型 |

#### MiniLM 系列

| 模型 | 维度 | 特点 |
|------|------|------|
| `Qdrant/all-MiniLM-L6-v2-onnx` | 384 | 轻量级，快速 |
| `Xenova/all-MiniLM-L6-v2` | 384 | 量化版 |
| `Xenova/all-MiniLM-L12-v2` | 384 | 稍大版本 |

#### 其他模型

| 模型 | 维度 | 特点 |
|------|------|------|
| `Alibaba-NLP/gte-base-en-v1.5` | 768 | 阿里 GTE |
| `Alibaba-NLP/gte-large-en-v1.5` | 1024 | 阿里 GTE 大型 |
| `mixedbread-ai/mxbai-embed-large-v1` | 1024 | MixedBread |
| `nomic-ai/nomic-embed-text-v1` | 768 | 8192 context |
| `nomic-ai/nomic-embed-text-v1.5` | 768 | 8192 context |
| `jinaai/jina-embeddings-v2-base-code` | 768 | 代码优化 |
| `jinaai/jina-embeddings-v2-base-en` | 768 | 通用英语 |
| `onnx-community/embeddinggemma-300m-ONNX` | 768 | Google Gemma |

### 配置示例

```toml
[memory.embedding]
# 启用 Embedding 模型
enabled = true
# 使用多语言模型（推荐中文用户）
model = "BAAI/bge-m3"
# 使用 HuggingFace 镜像（中国大陆用户）
hf_endpoint = "https://hf-mirror.com"
```

### 镜像配置

**方法一：配置文件**

```toml
[memory.embedding]
hf_endpoint = "https://hf-mirror.com"
```

**方法二：环境变量**

```bash
export HF_ENDPOINT="https://hf-mirror.com"
```

---

## System 配置 `[system]`

控制系统行为和提示词。

| 参数 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `system_prompt` | string | 见示例 | AI 助手的系统提示词 |
| `max_context_length` | usize | `16000` | 最大上下文长度 |
| `thinking_mode` | bool | `false` | 显示 LLM 思考过程 |

### 配置示例

```toml
[system]
system_prompt = """You are nb-claw, an autonomous AI assistant.

You have access to Python code execution and memory management.
Always be helpful, accurate, and concise."""

max_context_length = 32000
thinking_mode = false
```

---

## 完整配置示例

### 最简配置

```toml
[llm]
provider = "openai"
model = "gpt-4o"
# API key 通过环境变量设置：OPENAI_API_KEY

[system]
system_prompt = "You are a helpful assistant."
```

### 开发环境配置

```toml
[llm]
provider = "ollama"
model = "qwen3.5:9b"

[python]
sandbox = true
timeout_secs = 30

[memory]
storage_path = "./data/memory"
auto_consolidation = true
storage_format = "binary"

[memory.embedding]
enabled = true
model = "Xenova/bge-small-en-v1.5"

[system]
system_prompt = """You are nb-claw, an AI assistant with Python execution capability.
Use the memory module to store and recall information."""
max_context_length = 16000
```

### 生产环境配置

```toml
[llm]
provider = "anthropic"
model = "claude-sonnet-4-20250514"
timeout_secs = 120
max_retries = 5

[python]
sandbox = true
timeout_secs = 60
dangerous_modules = ["os", "subprocess", "shutil", "socket", "pickle", "importlib"]
safe_modules = ["math", "json", "re", "datetime", "collections", "itertools", "memory", "typing"]

[memory]
storage_path = "/var/lib/nb-claw/memory"
max_conversations = 500
max_short_term = 100
max_long_term = 5000
auto_consolidation = true
storage_format = "binary"

[memory.embedding]
enabled = true
model = "intfloat/multilingual-e5-base"
hf_endpoint = "https://huggingface.co"

[system]
system_prompt = """You are nb-claw, an enterprise AI assistant.

Guidelines:
- Be professional and accurate
- Use memory to maintain context
- Execute Python code safely within the sandbox"""
max_context_length = 64000
thinking_mode = false
```

---

## 常见问题

### Q: 配置文件找不到？

确保配置文件路径正确：
- 默认查找 `./config.toml` 或 `./config/config.toml`
- 使用 `--config` 参数指定路径

### Q: API Key 如何管理？

推荐使用环境变量：
```bash
export OPENAI_API_KEY="sk-..."
export ANTHROPIC_API_KEY="sk-ant-..."
```

### Q: Embedding 模型下载失败？

中国大陆用户配置镜像：
```toml
[memory.embedding]
hf_endpoint = "https://hf-mirror.com"
```

或设置环境变量：
```bash
export HF_ENDPOINT="https://hf-mirror.com"
```
如果还是遇到下载失败，您也可以选择使用一些代理工具。

### Q: 如何禁用语义搜索？

```toml
[memory.embedding]
enabled = false
```

### Q: 记忆存储格式如何选择？

- **二进制格式（默认）**：推荐生产使用，更紧凑、更快速、更安全
- **JSON 格式**：调试时使用，可以直接查看和编辑记忆内容

切换格式时系统会自动迁移数据。

### Q: Python 代码执行超时？

增加超时时间：
```toml
[python]
timeout_secs = 60
```

### Q: 如何添加自定义安全模块？

```toml
[python]
safe_modules = ["math", "json", "re", "datetime", "my_custom_module"]
```

### Q: 如何快速开始使用？

```bash
# 方式一：使用向导配置
nb-claw --config-wizard

# 方式二：使用默认配置
nb-claw --init-config
# 然后编辑 config/config.toml 设置 API key
```

---

## 验证配置

运行以下命令验证配置是否正确：

```bash
# 使用测试模式验证配置
nb-claw --test

# 启用调试日志查看详细信息
nb-claw --debug
```
