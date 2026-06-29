<div align="center">

# ✨ Ark 蒸馏智能体 · Workflow

[![Tauri](https://img.shields.io/badge/Tauri-2.0-FFC107?logo=tauri&logoColor=white)](https://tauri.app/)
[![Rust](https://img.shields.io/badge/Rust-1.77+-DEA584?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![TypeScript](https://img.shields.io/badge/TypeScript-5.5-3178C6?logo=typescript&logoColor=white)](https://www.typescriptlang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)]()

一个基于 **Tauri** 的桌面智能体应用，集成 **火山方舟（Ark）** AI 模型，提供可自由配置的 **Workflow（YAML/JSON DAG）**，内置完整的「认知蒸馏」闭环。

[快速开始](#快速开始) ·
[核心特性](#核心特性) ·
[架构说明](#架构说明) ·
[Workflow 配置](#workflow-速览)

</div>

---

## 🎯 核心特性

| 特性 | 描述 |
|------|------|
| 🔌 **Ark 原生接入** | 完整支持火山方舟 OpenAI 兼容接口，`coding-plan` 端点直接接入，API 密钥本地安全存储 |
| 🧩 **可配置 Workflow** | YAML/JSON 定义 DAG 工作流，支持 `llm` / `synthesize` / `validate` / `generate_skill` 等节点 |
| 🔄 **智能闭环** | `validate` 节点验证不通过时自动回退重试，受 `max` 次数保护，防止死循环 |
| ⚡ **并行执行** | 拓扑分层自动并发，支持最大并发数配置，构建期自动做环检测 |
| 🧠 **女娲蒸馏** | 内置 6 路并行研究 → 复核 → 合成 → 验证 → 生成 → 测试 → 双评审 完整蒸馏闭环 |
| 💬 **蒸馏后对话** | 生成的 Skill 直接进入对话状态，可与蒸馏后的智能体多轮交流 |

## 🖼️ 应用预览

> 📸 **截图占位** - 运行应用后截图替换
>
> - **设置页**：Ark 配置、模型选择、API Key 输入、连接测试
> - **运行页**：Workflow 执行可视化，节点状态实时更新，流式输出
> - **编辑器**：YAML 工作流编辑，实时语法校验，DAG 验证
> - **对话页**：蒸馏后 Skill 对话，多智能体切换

## 🏗️ 架构说明

```
ark-nuwa-desktop/
├── 📁 src-tauri/                  # Rust 后端内核
│   ├── src/
│   │   ├── llm.rs                # LLM 客户端抽象层 (可 Mock)
│   │   ├── ark.rs                # 火山方舟 SSE 流式客户端
│   │   ├── mock.rs               # 测试用 Mock 客户端
│   │   ├── distill.rs            # 女娲蒸馏工作流内嵌
│   │   ├── config.rs             # 配置管理与密钥脱敏
│   │   ├── session.rs            # 对话会话状态管理
│   │   ├── commands.rs           # Tauri IPC 命令层
│   │   └── workflow/
│   │       ├── model.rs          # Workflow 数据模型与解析
│   │       ├── context.rs        # 变量上下文与模板插值
│   │       └── engine.rs         # DAG 引擎：环检测 + 拓扑 + 并行 + 闭环
│   ├── Cargo.toml
│   └── tauri.conf.json
│
├── 📁 src/                       # TypeScript 前端 (Vite)
│   ├── views/                    # 四视图：设置 / 编辑器 / 运行 / 对话
│   ├── components/               # UI 组件
│   └── main.ts
│
├── 📁 workflows/                 # 工作流配置示例
│   └── nuwa-distill.yaml        # 女娲蒸馏完整 Workflow
│
├── 📁 scripts/                   # 构建脚本
│   └── gen-icon.mjs             # 应用图标生成器
│
└── package.json
```

## 🚀 快速开始

### 前置环境

- **Rust 工具链** (1.77+)：`rustup install stable`
- **MSVC Build Tools + Windows SDK** (Tauri 编译依赖)
- **Node.js 18+** + **pnpm**：`npm install -g pnpm`

### 开发运行

```bash
# 1. 克隆项目
git clone https://github.com/reymondmeking-dot/ark-nuwa-desktop.git
cd ark-nuwa-desktop

# 2. 安装前端依赖
pnpm install

# 3. 开发模式启动
pnpm tauri dev

# 4. 生产构建
pnpm tauri build
```

### 后端测试

完整内核测试（零网络依赖，使用 MockClient）：

```bash
cd src-tauri
cargo test
```

✅ 测试覆盖：
- DAG 拓扑分层、环检测、非法依赖检测
- 同层并行执行与上下游数据传递
- 模板变量插值（变量/节点输出/缺失键报错）
- 闭环重试：验证失败 → 回退 → 通过 / 超限失败
- 端到端完整蒸馏流程验证
- Ark SSE 流解析

## 📝 Workflow 速览

```yaml
name: 我的认知蒸馏工作流
vars: { person: "段永平" }
max_concurrency: 6

nodes:
  # 并行研究节点
  - { id: research_1, type: llm, prompt: "从投资角度研究 {{person}}", output: angle_1 }
  - { id: research_2, type: llm, prompt: "从管理角度研究 {{person}}", output: angle_2 }

  # 合成节点
  - id: synthesize
    type: synthesize
    depends_on: [research_1, research_2]
    prompt: |
      基于以下研究成果：
      - {{angle_1}}
      - {{angle_2}}
      合成统一的认知框架
    output: framework

  # 验证门控（闭环）
  - id: validation_gate
    type: validate
    depends_on: [synthesize]
    criteria:
      - 框架具有跨领域普适性
      - 核心原则可预测行为
      - 与其他认知体系有明确区分度
    on_fail:
      goto: synthesize    # 验证不通过，回退到合成节点重试
      max: 2              # 最多重试 2 次

  # 生成 Skill
  - id: generate
    type: generate_skill
    depends_on: [validation_gate]
    output: skill
```

## 🧪 端到端验证步骤

1. ✅ **启动**：`pnpm tauri dev` 启动应用
2. ✅ **配置**：设置页填写 Ark Base URL、Model、API Key，点击「测试连接」
3. ✅ **运行**：运行页选择内置女娲蒸馏工作流，输入人物，观察执行
4. ✅ **对话**：蒸馏完成后切换到对话页，与蒸馏智能体多轮交流
5. ✅ **自定义**：编辑器页编写自定义 YAML Workflow，验证后运行

## 📁 项目结构

```
.
├── ⚙️ 配置文件
│   ├── package.json          # 前端依赖与脚本
│   ├── tsconfig.json         # TypeScript 配置
│   ├── vite.config.ts        # Vite 构建配置
│   └── src-tauri/
│       ├── Cargo.toml        # Rust 依赖
│       └── tauri.conf.json   # Tauri 应用配置
│
├── 🎨 资源文件
│   └── src-tauri/icons/      # 应用图标集 (ico/png)
│
└── 📝 文档
    └── README.md             # 你正在看的文件 😉
```

## 🤝 贡献指南

欢迎提交 Issue 和 Pull Request！

1. Fork 本仓库
2. 创建特性分支：`git checkout -b feature/awesome-feature`
3. 提交更改：`git commit -am 'Add awesome feature'`
4. 推送到分支：`git push origin feature/awesome-feature`
5. 提交 Pull Request

## 📄 许可证

[MIT License](LICENSE) © 2026

---

<div align="center">

**蒸馏方法学参考**：[alchaincyf/nuwa-skill](https://github.com/alchaincyf/nuwa-skill)

**Made with** ❤️ **using** 🦀 **Rust +** ⚡ **Tauri +** 💙 **TypeScript**

</div>
