# 更新日志

## [2026-03-12] 版本控制系统 (VCS)

### ✨ 新特性

#### 全新版本控制系统
nb-claw 现在具备强大的文件版本追踪能力，基于 git2 库实现：

- **智能路径检测**：自动从 Python 代码和 CMD 命令中提取文件路径
  - 支持变量赋值：`file_path = r'D:\data.txt'`
  - 支持 `os.path.join()` 函数调用
  - 支持 pathlib `/` 操作符拼接
  - 支持字符串 `+` 连接
- **自动快照**：执行 Python/CMD 前自动追踪文件，执行后自动创建快照
- **上下文感知提交**：提取代码中路径出现的上下文作为提交消息（前后各5行）
- **文件恢复**：支持恢复任意历史版本的文件，即使文件已被删除
- **无状态设计**：路径使用 base64 编码存储在 git tree 中，无需外部索引文件

#### Python API
模型可通过内置 `vcs` 模块操作版本控制：

```python
import vcs

# 创建快照
vcs.snapshot("保存配置更改", ["config.json", "data.db"])

# 列出快照
for snap in vcs.list():
    print(f"[{snap.short_id}] {snap.message}")

# 恢复已删除的文件
vcs.restore("D:\\work\\document.txt")
```

### 🐛 Bug 修复

- **UTF-8 边界截断崩溃**：修复多字节字符截断时在字符中间切割导致的 panic

### 📁 文件变更

- **新增** `src/vcs.rs` - VCS 模块入口
- **新增** `src/vcs/engine.rs` - Git 版本控制引擎
- **新增** `src/vcs/path_extractor.rs` - 智能路径提取器
- **新增** `src/vcs/py_module.rs` - Python API 绑定
- **修改** `src/config.rs` - 添加 VCS 配置项
- **修改** `src/llm/tools.rs` - 集成自动文件追踪

## [2026-03-12] UI 自动化模块平台无关重构

### 🔄 重构

#### UI 自动化核心架构升级
- **Trait 抽象层**：新增 `UIAutomation` trait 定义统一的 UI 自动化接口（18 个方法）
- **平台工厂函数**：通过 `create_automation()` 返回平台特定实现，Python 模块导出实现平台无关
- **条件编译**：使用 `#[cfg(windows)]` 和 `#[cfg(target_os = "linux")]` 实现跨平台支持
- **动态分发**：使用 `Box<dyn UIAutomation>` 实现运行时多态

### 📁 文件变更

- **修改** `src/uiauto.rs` - Trait 定义和平台工厂函数
- **修改** `src/uiauto/manager.rs` - 平台无关的 Python 绑定
- **重构** `src/uiauto/windows.rs` - 实现 `UIAutomation` trait
- **新增** `src/uiauto/linux.rs` - 实现 `UIAutomation` trait

## [2026-03-11] 记忆系统大升级 & 多项 Bug 修复

### ✨ 新特性

#### 记忆系统核心升级
- **强制记忆巩固流程**：每次对话结束时自动触发记忆整理，确保重要信息不丢失
- **灵魂机制**：当对话无重要内容可记录时，模型会主动分享有趣的想法、哲学思考或推荐，赋予 AI 独特的个性
- **防污染设计**：截断过长的CMD和Python输出
- **上下文隔离**：记忆请求触发消息被排除在记忆来源之外，仅聚焦真实对话内容

### 🐛 Bug 修复

- **UTF-8 边界截断崩溃**：修复变量值和输出截断时在多字节字符中间切割导致的 panic
- **Windows 命令输出乱码**：优先使用 UTF-8 解码，回退到 GBK，解决 Python 输出中文乱码问题
- **工具调用格式误判**：只处理 `python` 和 `shell` 代码块，避免其他代码块被错误识别为工具调用

### 🔧 改进

- **Python 解释器增强**：正确支持 `global` 和 `nonlocal` 关键字
- **变量输出截断**：变量值超过 128 字符时截断，显示原始长度
- **命令输出截断**：stdout/stderr 超过 4000 字符时保留末尾（最新输出更重要）
- **代码质量**：添加多个测试用例验证 Python 解释器功能
