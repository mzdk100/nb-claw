# nb-claw

一个使用 Rust 实现的具有自主规划和执行能力的 AI 助手。

## 核心特性

- **聚焦易用性**：没有重量级的框架结构，用户配置友好
- **嵌入 Python 解释器**：模型可以通过执行 Python 脚本来控制设备
- **Shell 命令支持**：支持执行 shell 命令来与操作系统交互
- **工具函数**：
  - `run_py`：执行 Python 代码并返回结果
  - `py_mods`：列出可用的 Python 模块
  - `run_cmd`：执行 shell 命令
- **只支持内置模块**：Python 只支持内置模块，不支持任何第三方包
- **支持主流 LLM**：OpenAI、Anthropic、智谱、阿里云等
- **TOML 配置文件**：使用标准的 TOML 格式进行配置
- **硬编码工具说明**：工具使用说明已硬编码在程序中，用户不可修改
- **非标准工具调用支持**：支持解析非标准工具调用格式，增强模型兼容性
- **上下文长度优化**：避免不必要的上下文占用，系统仅仅注册三个工具，但却能实现强大的能力

## 快速开始

### 安装依赖

```bash
cargo build --release
```

### 配置

编辑 `config/config.toml` 文件：

```toml
[llm]
provider = "openai"  # openai, anthropic, zhipu, aliyun, ollama, deepseek
model = "gpt-4o-mini"
api_key = ""  # 或通过环境变量 OPENAI_API_KEY 设置
base_url = ""  # 可选，使用提供商标准 URL

[python]
sandbox = true
max_execution_time = 30
max_memory_mb = 512

[memory]
storage_path = "./data/memory"
max_conversations = 100

[system]
# 基础系统提示词（工具说明会自动添加到末尾）
system_prompt = """You are nb-claw, an autonomous AI assistant with the ability to execute Python code and shell commands.

Your goal is to help users by performing tasks, answering questions, and solving problems using your available tools. Be accurate, efficient, and clear in your responses.

When you need to perform computations or process data, use the appropriate tools. Always check the tool results and continue your work based on the output."""
max_context_length = 16000
```

### 运行

```bash
# 交互模式
nb-claw

# 测试模式
nb-claw --test

# 指定配置文件
nb-claw --config my-config.toml
```

## 项目结构

```
nb-claw/
├── config/
│   └── config.toml        # 主配置文件
├── src/
│   ├── main.rs               # 程序入口
│   ├── config.rs              # 配置管理
│   ├── llm/
│   │   ├── mod.rs
│   │   ├── client.rs        # LLM 客户端管理
│   │   └── tools.rs        # 工具定义
│   ├── python/
│   │   ├── mod.rs
│   │   └── interpreter.rs   # Python 解释器
│   └── memory.rs            # 记忆管理
├── Cargo.toml                 # Rust 项目配置
└── README.md                 # 本文件
```

## 设计理念

本项目采用创新的架构设计，通过嵌入 Python 解释器给模型最大的发挥空间。这与主流的架构都不同，我们追求创新性。

当模型需要在用户设备上执行一些操作时，可以使用 Python 脚本或 shell 命令的方法来控制。

### 工具说明

工具使用说明已经硬编码在程序中，包含以下三个工具：

1. **run_py** - 执行 Python 代码
   - 只能使用 Python 内置模块（无第三方包）
   - 多行代码支持
   - 将结果赋值给 `ret` 变量返回

2. **py_mods** - 列出 Python 模块
   - 列出所有可用的 Python 内置模块

3. **run_cmd** - 执行 shell 命令
   - Windows: 使用 Windows 命令（dir, type, del 等）
   - Unix/Linux/Mac: 使用 Unix 命令（ls, cat, rm 等）
   - 返回 stdout、stderr 和退出码

### 非标准工具调用格式

为了兼容无法返回标准工具调用格式的模型，系统支持解析以下非标准工具调用格式：

#### 格式 1：单个参数

```xml
<tool_call>tool_name<arg_key>param_name</arg_key><arg_value>param_value</arg_value>
```

示例：
```
我需要执行命令<tool_call>run_cmd<arg_key>command</arg_key><arg_value>LC_ALL=C date</arg_value>
```

#### 格式 2：多个参数

```xml
<tool_call>tool_name<args>
  <arg_key>param1</arg_key><arg_value>value1</arg_value>
  <arg_key>param2</arg_key><arg_value>value2</arg_value>
</args>
```

示例：
```
让我运行代码<tool_call>run_py<args><arg_key>code</arg_key><arg_value>ret = 2 + 2</arg_value></args>
```

#### 格式 3：无参数

```
让我查看模块<tool_call>py_mods
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
- **自定义内置模块**：我们会实现自己独特的一些内置模块，例如 `memory` 可以访问和修改模型的长期记忆内容，`uiauto` 可以查看和控制电脑等

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
