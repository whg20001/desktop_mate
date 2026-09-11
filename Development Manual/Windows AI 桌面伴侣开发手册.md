# Windows AI 桌面伴侣开发手册
## Tauri 2 + Three.js + Rust + Windows Native API

版本：Architecture v1.0  
目标平台：Windows 10 / Windows 11  
核心角色格式：PMX  
最终目标：构建一个无传统窗口感、可以在 Windows 桌面自由活动、拖拽、站立于应用窗口、感知桌面环境，并最终具备语音与 AI 能力的 3D 桌面角色。

---

# 1. 产品目标

最终用户启动程序之后，不应该看到：

```text
┌─────────────────────────┐
│ Desktop Companion       │
│                         │
│          角色           │
│                         │
└─────────────────────────┘
```

而应该看到：

```text
Windows Desktop

📁 Project

                         👧

        ┌────────────────────────┐
        │ Chrome                 │
        │                        │
        └────────────────────────┘

  VS Code

────────────────────────────────────
Taskbar
```

用户应该感觉：

> 角色本身存在于 Windows 桌面中。

而不是：

> 角色存在于某个应用程序窗口中。

第一阶段目标包括：

- PMX 正确加载；
- 材质和纹理正确；
- 透明背景；
- 无标题栏；
- 无边框；
- 不显示任务栏按钮；
- 角色可鼠标点击；
- 透明区域不阻挡桌面操作；
- 角色可以拖动；
- 可以放在桌面任何位置；
- 支持多个显示器；
- 可以检测其他 Windows 窗口；
- 可以站在窗口顶部；
- 可以掉落；
- 可以站在任务栏上；
- 自动眨眼；
- 呼吸；
- LookAt；
- Morph 表情。

之后再逐步加入：

```text
TTS
STT
Lip Sync
LLM
Memory
Emotion
Agent
Desktop Awareness
UI Automation
Vision
```

---

# 2. 最终技术栈

推荐固定如下：

| 模块 | 技术 |
|---|---|
| Desktop Shell | Tauri 2 |
| Native Core | Rust |
| Windows API | windows-rs |
| Web UI | TypeScript |
| Build | Vite |
| 3D Engine | Three.js |
| PMX Runtime | MMD Runtime Adapter |
| 首选 MMD 实现 | @moeru/three-mmd |
| MMD Physics | @moeru/three-mmd-physics-ammo |
| Native Desktop Physics | Rust |
| Desktop Detection | Win32 |
| Window Detection | User32 + DWM |
| UI Understanding | Windows UI Automation |
| Storage（非敏感配置） | SQLite / Tauri Store |
| Secrets Storage | keyring（Windows Credential Manager）/ Tauri Stronghold |
| HTTP Client（Rust 侧） | reqwest |
| AI | Python `LocalLlmProvider`（本机 OpenAI-compatible API，Endpoint 必须为 HTTP loopback） |
| Memory | Python `ConversationSessionStore` + `MemoryPort`；本机 Mem0；Phase II 本地 Graphiti |
| Vision（预留） | Rust trait `VisionProvider`（接口先行，默认不启用） |
| Audio Engine | `TtsProvider` + SFX + Playback + LipSync 帧契约（Provider 与密钥在 Rust 侧） |

需要注意一个当前技术变化：

Three.js 在 r170 已经将原来的 MMD 模块标记为 deprecated，因此项目不要把旧版 Three.js `MMDLoader` 直接写死在业务代码里。现在推荐把 MMD 放在独立 `MmdRuntime` 抽象层。目前 `@moeru/three-mmd` 仍在维护，并提供 PMX/MMD runtime、动画、toon material 以及独立 Ammo 物理插件。

同样的抽象原则也适用于 BrainEngine 与 AudioEngine：当前对话业务只依赖 `LocalLlmProvider` / `MemoryPort`，不直接依赖 Mem0 内部对象；AudioEngine 依赖 `TtsProvider` 等稳定 port。跨 Engine 的工作流由 CompanionOrchestrator 组合；动作建议统一经过 BehaviorPlanner。详见“AI Layer”“Memory Interface”“Vision Interface”和“AudioEngine Architecture”章节。

---

# 3. 一个非常重要的架构决定

第一版不要使用：

```text
一个透明窗口
覆盖整个桌面
```

而采用：

```text
一个 Character
=
一个局部透明 Windows Window
```

例如：

```text
Windows Desktop

                 ┌ - - - - - - - ┐
                 │                │
                 │       👧       │
                 │                │
                 │                │
                 └ - - - - - - - ┘
```

虚线窗口实际上存在，但是：

```text
transparent = true
decorations = false
shadow = false
skipTaskbar = true
```

所以用户完全看不到。

窗口大小例如：

```text
700 × 1000
```

角色实际上只占：

```text
350 × 750
```

周围留出：

```text
100~200 px
```

供：

- 头发；
- 手臂；
-裙摆；
- 跳跃；
- 动画；
- 物理；

使用。

这样比整个屏幕覆盖一个 WebView 更适合第一版。

---

# 4. 总体系统架构

目标架构采用：

```text
一个应用编排层（Application Orchestration）
+
四个职责域（Desktop / Character / Brain / Audio）
```

这里的 Engine 是**职责和依赖边界**，不等于四个进程、四个线程或四种语言。第一版仍然可以运行在同一个 Tauri 应用中；边界通过 TypeScript interface、Rust trait、Tauri command 和 event 保持稳定。

```text
                         Desktop Companion
                    CompanionOrchestrator
                  （生命周期、路由、优先级协调）
                              │
       ┌──────────────┬──────────────┬──────────────┐
       ▼              ▼              ▼              ▼
DesktopEngine  CharacterEngine  AudioEngine    BrainEngine
    Rust          Three.js       Rust/TS     Rust Host + Python
Windows/UIA      PMX/Motion      TTS/SFX      Agent/Memory
Physics/Window   Morph/Render    LipSync     BehaviorProposal
       │              ▲              │              │
       │              │              │              ▼
       └─ events ─────┴──────────────┴──────► BehaviorPlanner
                                      （校验、仲裁、调度、降级）
                                                  │
                                      BehaviorIntent / SpeechIntent
                                                  │
                         ┌────────────────────────┼───────────────┐
                         ▼                        ▼               ▼
                  DesktopEngine           CharacterEngine  AudioEngine
```

`CompanionOrchestrator` 是应用层编排器，不是第五个业务 Engine。它负责组合各 Engine、订阅事件、转发契约和释放资源，但不实现 Windows 物理、PMX 动画、LLM 请求或音频解码。

`ConversationOrchestrator` 则是 BrainEngine 内部的会话编排器，负责 Memory、DesktopContext、可选 Vision 与 LocalLlmProvider 的调用。两者不要混为一个巨型类：

```text
CompanionOrchestrator     → 跨 Engine 的应用工作流
ConversationOrchestrator  → BrainEngine 内的一次对话工作流
```

Phase II 已引入统一 `MemoryManager`。ConversationOrchestrator 仍只依赖窄 `MemoryPort`，不知道 Mem0、Graphiti 或 outbox 的实现：

```text
                         BrainEngine
                              │
                  ConversationOrchestrator
                              │
          ┌───────────────────┼───────────────────┐
          ▼                   ▼                   ▼
   LocalLlmProvider   ConversationSessionStore   MemoryPort
       Agent              Session/Turn Outbox       │
                                                  ▼
                                      MemoryManager / MemoryPolicy
                                          │                 │
                                   Mem0Provider      GraphitiProvider
```

本地候选提取与 Mem0/Graphiti 使用的 LLM、Embedding 都显式指向回环 API，`MemoryPort` 隔离 Provider SDK。Graphiti 与 Mem0 并列，不形成 `Mem0 → Graphiti` 的硬依赖链；SQLite `MemoryEventStore` 是唯一可审计事实源。动作建议仍由 BrainEngine 生成语义 ID，并由外部 `BehaviorPlanner` 最终批准。

核心数据契约：

```text
DesktopEngine   → DesktopEvent / SemanticDesktopContext
BrainEngine     → AgentResponse / BehaviorProposal
MemoryPort       → Memory recall / write / update / delete
BehaviorPlanner → BehaviorIntent / SpeechIntent
AudioEngine     → AudioEvent / LipSyncFrame
CharacterEngine → CharacterEvent / VisualState
```

禁止通过共享可变对象跨 Engine 操作内部状态。跨边界只能传递可验证的数据契约或调用窄接口。

核心原则仍然是：

```text
Rust = Windows 世界 + Sidecar 生命周期、安全 IPC 与配置主机
Three.js = 角色视觉世界
Python Brain Sidecar = 对话、语义决策 + 受控记忆编排
AudioEngine = 声音生命周期与嘴型数据来源
```

---

# 5. 引擎职责与语言边界

## DesktopEngine（Rust）负责

```text
Windows 坐标
窗口位置
显示器与 DPI
全局鼠标与拖拽
桌面重力和碰撞
窗口检测与前台应用
任务栏与支撑面
UI Automation
角色原生窗口移动
语义化 DesktopContext
```

DesktopEngine 输出桌面事实和系统事件，不决定角色应该表现出什么情绪，也不播放 PMX 动作。

## CharacterEngine（TypeScript / Three.js）负责

```text
PMX / Mesh / Skeleton
Morph / MMD Physics
动作播放与混合
LookAt / Blink
材质与 Lighting
角色点击形状计算
应用 LipSyncFrame
应用已经批准的 BehaviorIntent
```

CharacterEngine 负责“怎么表现”，不负责调用 LLM、读取密钥或决定 Windows 窗口物理。

## BrainEngine（Rust Host + Python Sidecar）负责

```text
Rust Desktop Host
├─ BrainSupervisor：启动、readiness、崩溃检测、退避重启、关闭
├─ 临时认证令牌、回环端口、设置和窄 Tauri command
└─ CharacterResponse 的结构、范围和动作 allow-list 二次校验

Python Brain Sidecar
├─ ConversationOrchestrator / LocalLlmProvider
├─ SQLite Conversation Session 与记忆写入 outbox
├─ Mem0 + SQLite history + embedded Qdrant
├─ Session / Semantic / Episodic Memory
└─ 生成 speech / emotion / action 语义建议
```

BrainEngine 可以根据场景和检索到的记忆选择一个**语义动作 ID**，但输出的是 `BehaviorProposal`，不是已经获得执行权的命令。Rust Host 不实现对话推理，Python Sidecar 不拥有 Windows、PMX 骨骼、Morph 或播放设备；最终动作仍必须经过前端 `BehaviorPlanner`。

## AudioEngine（Rust Provider + 前端或原生播放适配器）负责

```text
TTS / STT Provider
SFX
播放、取消和打断
音频资源生命周期
RMS / Viseme 时间轴
AudioEvent / LipSyncFrame
```

AudioEngine 不修改 PMX Morph。它只产生嘴型帧，CharacterEngine 再将帧应用到模型。

## BehaviorPlanner 是执行安全边界

BehaviorPlanner 接收系统事件、用户交互和 BrainEngine 的建议，输出经过批准的 `BehaviorIntent`。它负责：

```text
动作 ID 是否注册
动作是否允许 AI 使用
当前角色状态是否允许播放
优先级与抢占规则
冷却时间和并发互斥
强度、时长和参数范围
失败时延迟、替换或回到 Idle
```

因此下面两句话必须同时成立：

```text
AI 可以在合适场景选择 Greeting / Thinking / Talking 等语义动作
AI 不可以直接设置 Bone.rotation、Morph 权重、播放设备或 HWND
```

允许的输出示例：

```json
{
  "speech": "早上好。",
  "emotion": { "type": "happy", "intensity": 0.7 },
  "action": { "id": "greeting", "intensity": 0.6 }
}
```

禁止的输出示例：

```json
{
  "rightArmRotationZ": 1.24,
  "mouthMorph": 0.9,
  "windowY": 500,
  "audioDevice": "default"
}
```

依赖方向必须保持单向：

```text
BrainEngine ──proposal──► BehaviorPlanner ──intent──► CharacterEngine
                                          └─speech──► AudioEngine
AudioEngine ──lip-sync frame / event────────────────► CharacterEngine
DesktopEngine ──system event────────────────────────► BehaviorPlanner
```

BrainEngine、AudioEngine 和 DesktopEngine 都不能持有 `CharacterRuntime` 的具体实现；由应用编排层注入接口并转发事件，从结构上避免循环依赖。

---
# 6. 开发环境

Windows 端 Tauri 当前要求 Microsoft C++ Build Tools 和 WebView2；Windows 10 较新版本和 Windows 11 通常已经带有 WebView2。Rust 应使用 MSVC toolchain。

建议安装：

```text
Visual Studio Build Tools
 └── Desktop development with C++

Rustup
Node.js LTS
pnpm
Git
VS Code
```

执行：

```bash
rustup default stable-msvc
```

检查：

```bash
rustc --version
cargo --version
node --version
pnpm --version
```

---

# 7. 创建项目

建议项目名：

```text
desktop-companion
```

创建：

```bash
pnpm create tauri-app
```

选择：

```text
Frontend:
TypeScript

Package Manager:
pnpm
```

暂时不需要 React。

因为 Character Renderer 本质是：

```text
Canvas + Three.js
```

Vue / React 在这一层意义不大。

之后进入：

```bash
cd desktop-companion
pnpm install
pnpm tauri dev
```

Tauri 官方目前仍推荐通过 Tauri CLI 启动开发服务器。

---

# 8. 安装 Three.js 与 MMD

执行：

```bash
pnpm add three
pnpm add @moeru/three-mmd
pnpm add @moeru/three-mmd-physics-ammo
pnpm add zod
```

开发依赖：

```bash
pnpm add -D @types/three
```

PMX 相关代码绝对不要散落在：

```text
main.ts
renderer.ts
interaction.ts
```

而应该集中到：

```text
src/character/mmd/
```

---

# 9. 推荐项目目录

目录按职责域组织；`app/` 只负责组合，Engine 之间通过 `behavior/`、`audio/` 和 `ipc/` 中的稳定契约通信。

```text
desktop-companion/
│
├── src/
│   ├── app/
│   │   ├── bootstrap.ts
│   │   ├── CompanionOrchestrator.ts   // Composition Root 与跨 Engine 工作流
│   │   └── lifecycle.ts
│   │
│   ├── desktop/                       // DesktopEngine 的前端窄桥接
│   │   └── DesktopBridge.ts
│   │
│   ├── character/                     // CharacterEngine
│   │   ├── CharacterRuntime.ts
│   │   ├── CharacterCatalog.ts
│   │   ├── mmd/
│   │   │   ├── MmdRuntime.ts
│   │   │   ├── MoeruMmdRuntime.ts
│   │   │   ├── BoneMap.ts
│   │   │   ├── MorphMap.ts
│   │   │   └── ModelManifest.ts
│   │   ├── animation/
│   │   │   ├── MotionController.ts
│   │   │   ├── MotionCatalog.ts
│   │   │   ├── BlinkController.ts
│   │   │   └── LookAtController.ts
│   │   └── interaction/
│   │       ├── HitRegionController.ts
│   │       └── PointerController.ts
│   │
│   ├── behavior/                      // Proposal → 可执行 Intent
│   │   ├── BehaviorTypes.ts
│   │   ├── BehaviorPlanner.ts
│   │   ├── BehaviorScheduler.ts
│   │   └── BehaviorPolicy.ts          // 优先级、抢占、冷却和 AI allow-list
│   │
│   ├── audio/                         // AudioEngine 的播放侧
│   │   ├── AudioTypes.ts
│   │   ├── AudioController.ts         // 播放、打断与统一生命周期
│   │   ├── speech/
│   │   │   ├── SpeechController.ts
│   │   │   └── WebSpeechEngine.ts     // 无密钥调试适配器
│   │   ├── sfx/
│   │   │   └── SfxController.ts
│   │   └── lipsync/
│   │       └── LipSyncAdapter.ts      // AudioFrame → LipSyncFrame
│   │
│   ├── brain/                         // BrainEngine 的 WebView IPC 客户端
│   │   └── BrainBridge.ts             // 不持有密钥，不直接发网络请求
│   │
│   ├── config/
│   │   ├── CharacterSettings.ts
│   │   ├── SettingsPanel.ts
│   │   └── SettingsWindow.ts
│   │
│   ├── renderer/
│   │   ├── CharacterRenderer.ts
│   │   └── RenderLoop.ts
│   │
│   ├── ipc/
│   │   ├── commands.ts
│   │   ├── events.ts
│   │   └── schemas.ts                 // 所有跨 Rust / TS DTO 的运行时校验
│   │
│   └── ui/
│       ├── SpeechBubble.ts
│       └── ContextMenu.ts
│
├── src-tauri/src/
│   ├── lib.rs
│   ├── runtime.rs
│   │
│   ├── orchestration/
│   │   └── companion.rs               // Native 生命周期与跨服务装配，不含业务细节
│   │
│   ├── commands/
│   │   ├── character.rs
│   │   ├── desktop.rs
│   │   ├── brain.rs
│   │   └── audio.rs
│   │
│   ├── windows/
│   │   ├── monitor.rs
│   │   ├── cursor.rs
│   │   ├── enumeration.rs
│   │   └── events.rs
│   │
│   ├── desktop/                       // DesktopEngine
│   │   ├── world.rs
│   │   ├── surface.rs
│   │   ├── physics.rs
│   │   └── automation.rs
│   │
│   ├── character/                     // 角色原生窗口状态
│   │   └── state.rs
│   │
│   ├── brain/                         // Rust Brain Host / Supervisor
│   │   ├── supervisor.rs              // Sidecar 生命周期、令牌、探活、退避重启
│   │   ├── client.rs                  // 仅回环、禁代理和重定向的受控客户端
│   │   ├── locality.rs                // 回环 Endpoint 与 hardened HTTP client
│   │   └── model.rs                   // IPC DTO 与 CharacterResponse 校验
│   ├── vision/                        // 可选 Vision port，默认关闭
│   │
│   ├── audio/                         // AudioEngine 的 Provider / 原生侧
│   │   ├── provider.rs                // TtsProvider / SttProvider
│   │   ├── manager.rs
│   │   ├── playback.rs
│   │   └── sfx.rs
│   │
│   ├── storage/
│   │   └── settings.rs
│   └── error.rs
│
├── src-tauri/capabilities/
├── brain-sidecar/                     // Python BrainEngine 业务实现
│   ├── src/desktop_companion_brain/
│   │   ├── server.py                  // 鉴权 loopback HTTP
│   │   ├── orchestrator.py            // ConversationOrchestrator
│   │   ├── llm.py                     // 本地 OpenAI-compatible LLM
│   │   ├── memory.py                  // MemoryPort 与 Mem0 adapter
│   │   ├── memory_manager.py          // 多 Provider 编排与融合召回
│   │   ├── memory_policy.py           // 候选审批与敏感过滤
│   │   ├── memory_store.py            // SQLite 事实源与 Provider outbox
│   │   ├── memory_inference.py        // 本地 LLM 候选提取
│   │   ├── graphiti_provider.py       // 可选本机时间图
│   │   ├── session_store.py           // SQLite 会话与 turn outbox
│   │   ├── security.py                // 本地 URL / 数据目录边界
│   │   └── openai_client.py           // 无代理、无重定向传输
│   └── tests/
├── docs/
├── public/
├── package.json
└── pnpm-lock.yaml
```

当前代码可以渐进迁移，不要求一次性移动所有文件。例如现有 `src/speech/` 可先作为 `AudioEngine` 的语音子模块，现有 Rust `ai/`、`memory/`、`vision/`、`speech/` 可以先由 facade 组合，再在稳定后调整物理目录。

目录依赖规则：

```text
app           可以组合所有 facade
behavior      只依赖共享类型、目录查询接口和 Engine port
character     不依赖 brain 的 Provider 或响应原始 JSON
audio         不依赖 CharacterRuntime 具体类，只依赖 LipSyncTarget port
brain         不依赖 Three.js、PMX、Web Audio 或 Desktop HWND
ipc           不依赖具体 Engine 实现
```

不要为了目录整齐把所有逻辑塞进 `CompanionOrchestrator`。编排层应该薄，只描述调用顺序和所有权。
## 当前代码到目标架构的映射

| 当前实现 | 目标职责 | 迁移策略 |
|---|---|---|
| `src/app/bootstrap.ts` | 当前 Composition Root | 保留启动职责，跨 Engine 工作流逐步移入 `CompanionOrchestrator` |
| `src/desktop/DesktopBridge.ts` + Rust `desktop/`、`runtime.rs` | DesktopEngine | 保持 IPC 窄接口，不让前端重算桌面物理 |
| `src/character/CharacterRuntime.ts` | CharacterEngine facade | 后续只接收 `BehaviorIntent`、视觉设置和 `LipSyncFrame` |
| `BehaviorPlanner.ts` + `BehaviorPolicy.ts` + `BehaviorScheduler.ts` | 行为准入与调度边界 | Planner 归一化外部建议，Policy 校验来源与 allow-list，Scheduler 负责物理抢占、排队、冷却和恢复 |
| `MotionController.ts` + `MotionCatalog.ts` | 动作执行与动作目录 | 保持模型实现细节，不接收 AgentResponse 原始 JSON |
| `src/speech/SpeechController.ts` | AudioEngine 语音原型 | 逐步由 `AudioController` 统一 TTS、SFX、打断和播放会话 |
| Rust `brain/` + Python `brain-sidecar/` | BrainEngine | Rust 管生命周期、安全 IPC 和二次校验；Python 管对话、本地 LLM、会话、MemoryPolicy 与 Provider；不维护第二套 Rust 记忆事实源 |
| Python `MemoryPort` / `MemoryManager` | 长期记忆边界 | SQLite 为事实源；隔离并编排 Mem0 与可选 Graphiti，支持独立降级和重建 |
| Rust `vision/` | 可选 Vision 契约 | 默认关闭，等待明确授权的视觉功能 |
| Rust `speech/provider.rs` | AudioEngine Provider 契约 | 增加 Provider Manager、受控播放和 RMS/Viseme 输出 |

当前 Phase I 与 Phase II 记忆主链已实现：

```text
BehaviorTypes
BehaviorPlanner
BehaviorPolicy / BehaviorScheduler
BrainBridge / ConversationOrchestrator
Speech Bubble / 情绪 / Web Speech TTS 联动
Rust BrainSupervisor / BrainClient
Python LocalLlmProvider / SQLite Conversation Session
Mem0 + SQLite history + embedded Qdrant
Memory Recall / Write / Update / Delete
MemoryPolicy / 审批 / Provider outbox / 融合召回
可选 Graphiti + 本机 Neo4j / Provider 状态 / 索引重建
鉴权、readiness、自动关闭、崩溃检测与退避重启
```

后续仍需完成原生 AudioEngine 播放实现、STT、Conversation UI 增强，以及由用户配置的真实本机 Neo4j 与真实本地模型上的 Graphiti 集成验收。迁移时先增加 facade 和契约，再移动目录；不要把“大规模改路径”与“改变运行行为”放在同一个提交中。

---
# 10. Tauri Character Window

第一版 Character Window 推荐：

```json
{
  "label": "character",
  "title": "Desktop Companion",
  "width": 700,
  "height": 1000,
  "transparent": true,
  "decorations": false,
  "shadow": false,
  "resizable": false,
  "alwaysOnTop": true,
  "skipTaskbar": true
}
```

特别要注意：

```text
shadow = false
```

Windows 下 undecorated window 如果保留 shadow，可能仍出现边缘、圆角或其他 Windows 非客户区视觉效果。Tauri 当前提供 `transparent`、`skipTaskbar`、`alwaysOnTop` 等相关配置/API。

---

# 11. WebView 透明

HTML：

```css
html,
body {
    margin: 0;
    padding: 0;

    width: 100%;
    height: 100%;

    overflow: hidden;

    background: transparent !important;
}

canvas {
    display: block;
    background: transparent;
}
```

Three.js：

```ts
const renderer = new THREE.WebGLRenderer({
    alpha: true,
    antialias: true,
    premultipliedAlpha: true,
});

renderer.setPixelRatio(window.devicePixelRatio);

renderer.setClearColor(0x000000, 0);

renderer.setSize(
    window.innerWidth,
    window.innerHeight,
);
```

Scene：

```ts
const scene = new THREE.Scene();

scene.background = null;
```

---

# 12. CharacterRenderer

推荐：

```ts
export class CharacterRenderer {
    readonly scene: THREE.Scene;
    readonly camera: THREE.PerspectiveCamera;
    readonly renderer: THREE.WebGLRenderer;

    constructor(canvas: HTMLCanvasElement) {
        this.scene = new THREE.Scene();

        this.camera = new THREE.PerspectiveCamera(
            35,
            window.innerWidth / window.innerHeight,
            0.01,
            100,
        );

        this.camera.position.set(0, 1.2, 4);

        this.renderer = new THREE.WebGLRenderer({
            canvas,
            alpha: true,
            antialias: true,
        });

        this.renderer.setClearColor(0, 0);
    }

    render(): void {
        this.renderer.render(
            this.scene,
            this.camera,
        );
    }
}
```

---

# 13. 不要直接依赖某一个 MMD Library

定义：

```ts
export interface MmdRuntime {
    load(url: string): Promise<void>;

    update(delta: number): void;

    setMorph(
        name: string,
        weight: number,
    ): void;

    getBone(name: string):
        THREE.Bone | undefined;

    getRoot():
        THREE.Object3D | undefined;

    dispose(): void;
}
```

然后：

```text
MmdRuntime
    ↑
MoeruMmdRuntime
```

未来：

```text
MmdRuntime
    ↑
YohawingMmdRuntime
```

也可以直接切换。

这样避免 PMX 引擎被锁死。

---

# 14. PMX Runtime

当前建议先使用：

```text
@moeru/three-mmd
```

基本结构：

```ts
import {
    MMDLoader,
} from '@moeru/three-mmd';

import {
    MMDAmmoPlugin,
} from '@moeru/three-mmd-physics-ammo';
```

加载：

```ts
const loader =
    new MMDLoader()
        .register(MMDAmmoPlugin);

const mmd =
    await loader.loadAsync(modelUrl);

scene.add(mmd.mesh);
```

更新：

```ts
function update(delta: number) {
    mmd.update(delta);
}
```

如果以后使用 `AnimationMixer`：

```text
AnimationMixer
       ↓
MMD Runtime
       ↓
IK
Grant
Physics
```

`@moeru/three-mmd` 当前提供 `updateWithMixer()` 来组合 AnimationMixer、MMD IK、grant 和物理更新。

---

# 15. PMX 兼容性验收

第一阶段一定建立：

```text
PMXCompatibilityTest
```

对你当前模型检查：

### Mesh

```text
模型是否正常出现
坐标方向是否正确
比例是否正确
```

### Texture

```text
face
body
hair
cloth
sphere texture
toon
```

### Bone

检查：

```text
センター
上半身
上半身2
首
頭
両目
右目
左目
右腕
左腕
右手首
左手首
```

### Morph

检查：

```text
まばたき
笑い
あ
い
う
え
お
```

不要写死这些名称。

创建：

```ts
export interface CharacterBoneMap {
    center?: string;
    head?: string;
    neck?: string;

    leftEye?: string;
    rightEye?: string;

    leftHand?: string;
    rightHand?: string;
}
```

以及：

```ts
export interface CharacterMorphMap {
    blink?: string;
    smile?: string;

    a?: string;
    i?: string;
    u?: string;
    e?: string;
    o?: string;
}
```

保存进：

```text
manifest.json
```

---

# 16. Model Manifest

建议每个角色：

```text
models/
└── 827a5c/
    ├── manifest.json
    ├── model.pmx
    └── textures/
```

Manifest：

```json
{
  "id": "827a5c",
  "name": "Character01",

  "model": "model.pmx",

  "scale": 1.0,

  "groundOffset": 0.0,

  "bones": {
    "head": "頭",
    "neck": "首",
    "leftEye": "左目",
    "rightEye": "右目"
  },

  "morphs": {
    "blink": "まばたき",
    "smile": "笑い",
    "a": "あ",
    "i": "い",
    "u": "う",
    "e": "え",
    "o": "お"
  }
}
```

---

# 17. 不建议直接加载用户任意目录

PMX 最大的问题之一不是 PMX 本身，而是：

```text
PMX
 ↓
relative texture path
```

因此推荐用户执行：

```text
导入角色
```

而不是：

```text
永久引用 C:\Downloads\xxx\model.pmx
```

导入：

```text
User Model Folder
       ↓
Rust Importer
       ↓
AppLocalData
       ↓
models/{UUID}/
```

最终：

```text
$APPLOCALDATA/models/{uuid}
```

Tauri 的 asset protocol 可以安全地把磁盘文件提供给 WebView，但需要配置明确的 scope；官方也建议使用尽量窄的路径范围，而不是开放整个文件系统。

建议只允许：

```text
$APPLOCALDATA/models/**/*
```

而不是：

```text
**/*
```

---

# 18. 为什么桌面坐标必须交给 Rust

整个程序至少存在三套坐标：

```text
Desktop Coordinate
Window Coordinate
Three.js Coordinate
```

一定不能混。

---

# 19. Desktop Coordinate

Windows Virtual Desktop：

```text
x
y
```

注意：

第二显示器可能：

```text
x = -1920
```

Microsoft 的 `MONITORINFO` 中 `rcMonitor` 和 `rcWork` 都是 virtual-screen coordinates，而且非主显示器坐标可以为负数。

因此：

```rust
struct DesktopPoint {
    x: i32,
    y: i32,
}
```

绝对不要使用：

```rust
u32
```

否则负显示器坐标会出问题。

---

# 20. Monitor

定义：

```rust
#[derive(Clone, Debug, Serialize)]
pub struct MonitorInfo {
    pub id: String,

    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,

    pub work_left: i32,
    pub work_top: i32,
    pub work_right: i32,
    pub work_bottom: i32,

    pub dpi: u32,

    pub primary: bool,
}
```

其中：

```text
monitor rect
```

表示整个屏幕。

而：

```text
work rect
```

通常表示：

```text
屏幕减去任务栏
```

因此：

```text
work_bottom
```

非常适合作为第一版桌宠的“地面”。

---

# 21. DPI

必须从一开始就处理 DPI。

不要假设：

```text
1 CSS pixel
=
1 physical pixel
```

Windows 推荐 Per-Monitor DPI 模型；`GetDpiForWindow` 可以根据窗口的 DPI awareness 返回对应窗口 DPI。

建议所有 DesktopWorld 坐标统一为：

```text
Physical Screen Pixel
```

前端进入 Three.js 之前再做转换。

---

# 22. Window Detection

核心：

```text
EnumWindows
```

Microsoft 官方定义：

> EnumWindows 枚举屏幕上的顶层窗口。

建议：

```rust
pub struct DesktopWindow {
    pub hwnd: isize,

    pub process_id: u32,

    pub title: String,

    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,

    pub visible: bool,
    pub minimized: bool,
}
```

---

# 23. 不推荐仅使用 GetWindowRect

原因：

`GetWindowRect()` 的结果可能包含 invisible resize borders，而且其结果会受到 DPI virtualization 影响。Microsoft 建议如果要获得可见窗口边界，可以使用：

```text
DwmGetWindowAttribute
+
DWMWA_EXTENDED_FRAME_BOUNDS
```



因此：

```text
DesktopWorld window bounds

优先：

DwmGetWindowAttribute

Fallback:

GetWindowRect
```

---

# 24. DesktopWorld

Native 层维护：

```rust
pub struct DesktopWorld {
    pub monitors: Vec<MonitorInfo>,

    pub windows: Vec<DesktopWindow>,

    pub foreground_window: Option<isize>,

    pub cursor: DesktopPoint,
}
```

之后扩展：

```rust
pub enum DesktopSurface {
    MonitorFloor,
    Taskbar,
    WindowTop,
}
```

---

# 25. 不要每帧 EnumWindows

错误做法：

```text
60 FPS
 ↓
EnumWindows
 ↓
60次/秒
```

没有必要。

MVP：

```text
5~10 Hz
```

同步一次窗口列表。

之后升级为：

```text
SetWinEventHook
```

监听：

```text
EVENT_OBJECT_LOCATIONCHANGE
```

Windows 会在窗口位置、形状或大小改变时产生相应辅助功能事件。

最终：

```text
WinEvent
 ↓
只更新发生变化的Window
```

---

# 26. Character Native State

Rust：

```rust
pub enum CharacterMode {
    Idle,
    Walking,
    Dragged,
    Falling,
    Landing,
    Sitting,
    Sleeping,
}
```

状态：

```rust
pub struct CharacterState {
    pub x: f32,
    pub y: f32,

    pub vx: f32,
    pub vy: f32,

    pub mode: CharacterMode,

    pub grabbed: bool,

    pub support_surface:
        Option<DesktopSurface>,
}
```

---

# 27. Desktop Physics

桌面物理一定放 Rust。

不要：

```text
Three.js physics
 ↓
控制 Windows Position
```

Three.js 里面的 Physics：

```text
头发
裙子
衣服
饰品
```

Rust Physics：

```text
角色整体
重力
窗口碰撞
桌面碰撞
拖拽
移动
```

这是两套完全不同的 Physics。

---

# 28. Physics Loop

建议：

```text
60 Hz
```

逻辑：

```rust
loop {
    update_gravity();

    update_position();

    detect_surface();

    resolve_collision();

    move_native_window();
}
```

例如：

```text
vy += gravity * dt

y += vy * dt
```

---

# 29. Window Top 作为平台

假设：

```text
Chrome

left   = 300
top    = 200
right  = 1400
bottom = 900
```

角色脚底：

```text
characterFootX
characterFootY
```

判断：

```text
footX >= left
&&
footX <= right
```

并且：

```text
previousFootY < top
currentFootY >= top
```

则：

```text
LAND
```

最终：

```text
character.bottom = window.top
vy = 0
```

---

# 30. 不要把所有窗口都当平台

过滤：

```text
Invisible
Minimized
Tool windows
Companion own window
Zero size
Offscreen
Desktop shell
```

建议额外维护：

```rust
pub enum SurfacePriority {
    Desktop,
    NormalWindow,
    Taskbar,
}
```

---

# 31. Drag

拖拽角色时：

```text
Mouse Down
 ↓
BeginDrag
 ↓
CharacterMode::Dragged
 ↓
gravity = off
 ↓
native window follows mouse
```

Mouse Up：

```text
EndDrag
 ↓
寻找脚下Surface
 ↓
有Surface
     ↓
   Land
 ↓
没有
     ↓
   Fall
```

---

# 32. 一个非常重要的设计

不要拖：

```text
Three.js character.position
```

而拖：

```text
Native Character Window
```

也就是说：

```text
Desktop Position
=
Native Window Position
```

Three.js 角色始终大致固定：

```text
Window Local Center
```

这样 Desktop 坐标体系会非常清晰。

---

# 33. 鼠标穿透

这是核心。

Tauri 当前提供：

```ts
setIgnoreCursorEvents(true)
```

让窗口忽略鼠标事件。

但如果永久：

```text
true
```

角色也无法点击。

因此需要：

```text
Global Cursor
     ↓
Hit Region
     ↓

Inside Character?
    │
 ┌──┴──┐
YES    NO
 │      │
 ▼      ▼
false  true
```

---

# 34. Global Cursor

Rust 使用：

```text
GetCursorPos
```

独立于 WebView。

建议：

```text
60~120 Hz
```

读取鼠标位置。

但是：

```text
setIgnoreCursorEvents()
```

只在状态变化时调用。

不要：

```text
120次/秒调用
```

而是：

```text
false → true
true → false
```

才修改。

---

# 35. Character Hit Region

不要做：

```text
读取Canvas每一个Pixel Alpha
```

代价没必要。

第一版建立：

```text
head circle
body rectangle
left arm capsule
right arm capsule
legs region
```

例如：

```text
        (Head)
          ○

       ┌─────┐
       │Body │
       └─────┘

       ╱     ╲
     Arms   Arms

       │     │
      Legs  Legs
```

Three.js 根据骨骼：

```text
頭
首
腰
手
足
```

投影到：

```text
Screen Position
```

然后得到：

```ts
interface HitRegion {
    x: number;
    y: number;
    width: number;
    height: number;
}
```

传给 Rust。

---

# 36. 为什么不推荐 WM_NCHITTEST 作为第一版核心

`HTTRANSPARENT` 存在一个容易被忽略的限制：

Windows 官方说明它继续向下传递 hit-test 时针对的是**同一线程中的 underlying windows**。

所以第一版不要依赖：

```text
WM_NCHITTEST
→ HTTRANSPARENT
```

解决跨应用穿透。

使用：

```text
Tauri setIgnoreCursorEvents
+
Global Cursor
+
Hit Regions
```

更加容易控制。

---

# 37. Render Loop

TypeScript：

```ts
const clock = new THREE.Clock();

function frame() {
    const delta =
        Math.min(
            clock.getDelta(),
            1 / 20
        );

    character.update(delta);

    renderer.render();

    requestAnimationFrame(frame);
}
```

限制最大 delta：

```text
50ms
```

避免窗口恢复后：

```text
Physics爆炸
```

---

# 38. Blink

不要依赖 VMD。

```ts
class BlinkController {
    timer = 0;
    nextBlink = 3;

    update(dt: number) {
        this.timer += dt;

        if (this.timer >= this.nextBlink) {
            this.blink();
        }
    }
}
```

建议：

```text
Blink Duration
≈ 100~180 ms
```

随机间隔：

```text
2.5 ~ 6 s
```

偶尔：

```text
double blink
```

会自然很多。

---

# 39. Breathing

不要移动整个窗口。

只对：

```text
Upper Body
Chest
Shoulder
```

产生非常轻微的：

```text
rotation
position
```

例如：

```text
sin(time)
```

周期：

```text
3~5 seconds
```

幅度极小。

---

# 40. LookAt

结构：

```text
Global Mouse
 ↓
Rust
 ↓
Character Window Relative Position
 ↓
TypeScript
 ↓
Head / Eyes
```

例如：

```ts
interface LookTarget {
    x: number;
    y: number;
}
```

通过 Three.js camera：

```text
screen
→ NDC
→ target
```

控制：

```text
Eyes first
Head second
Body third
```

限制：

```text
Eye yaw
Head yaw
Head pitch
```

避免 180° 转头。

---

# 41. Animation 状态不要与 Physics 状态混合

Rust：

```text
Physics State
```

TS：

```text
Visual Animation State
```

Rust：

```text
Falling
```

通知 TS：

```json
{
  "animation": "fall"
}
```

Rust：

```text
Landing
```

通知：

```json
{
  "animation": "land"
}
```

---

# 42. Tauri IPC

Tauri 当前推荐使用：

```text
command
```

完成：

```text
Frontend → Rust
```

使用：

```text
events
```

完成：

```text
Rust → Frontend
```

其 command 可以接收 serde 可反序列化参数并返回 serde 可序列化结果。

例如：

```rust
#[tauri::command]
pub fn begin_drag(
    state: tauri::State<AppState>
) -> Result<(), String> {

    state.character.begin_drag();

    Ok(())
}
```

TS：

```ts
await invoke('begin_drag');
```

---

# 43. IPC 分类

不要有：

```text
set_character_x
set_character_y
set_character_vx
set_character_vy
```

应该是 semantic command：

```text
begin_drag
drag_to
end_drag

walk_to
jump

set_character_model

get_desktop_world
```

Intelligence 相关 command 同样遵循 semantic 风格。WebView 只调用 Rust command；Rust 负责权限、安全校验和 Sidecar 生命周期，实际对话、会话与 Mem0 请求由本机 Python Brain Sidecar 执行。Vision 等未来能力仍必须通过受控 Engine port 接入：

```text
get_brain_status
get_brain_settings
configure_brain
converse

list_brain_memories
update_brain_memory
delete_brain_memory

set_vision_enabled
list_vision_providers
set_vision_provider
capture_and_describe
```

---

# 44. Rust → Three Events

建议：

```text
character://state
desktop://changed
cursor://position
window://foreground
character://surface
```

例如：

```json
{
  "state": "falling"
}
```

而不是发送：

```text
每帧 100 个骨骼值
```

未来增加流式响应时，Intelligence 事件使用 `ai://` 前缀，驱动对话气泡与“思考中”状态，不驱动骨骼。当前 Phase I 使用一次性 Tauri `converse` 返回值，不注册这些事件：

```text
ai://thinking
ai://delta
ai://response
ai://error
```

---

# 45. UI Automation

等桌面窗口交互稳定之后再加入。

Windows UI Automation 可以通过：

```text
ElementFromPoint
```

获取某个 desktop coordinate 下对应的 UI element。Microsoft 明确说明其参数使用 desktop physical screen coordinates。

最终：

```rust
pub struct DesktopElement {
    pub name: String,

    pub control_type: String,

    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,

    pub process_id: u32,
}
```

---

# 46. BoundingRectangle

UI Automation 元素可以取得：

```text
BoundingRectangle
```

并且坐标是 physical screen coordinates。

所以可以直接进入：

```text
DesktopWorld
```

---

# 47. 不要扫描整棵 UI Tree

错误：

```text
每秒
 ↓
遍历Windows所有UIAutomation元素
```

非常重。

正确：

```text
User interaction location
       ↓
ElementFromPoint
```

或者：

```text
Character nearby region
```

只检测附近。

---

# 48. Desktop Context

最终 Agent 不应该收到：

```text
HWND
RECT
AutomationId
```

而应该收到：

```json
{
  "foregroundApp": "Visual Studio Code",

  "nearbyWindows": [
    {
      "name": "Google Chrome",
      "relation": "right"
    }
  ],

  "character": {
    "state": "sitting",
    "surface": "Chrome"
  }
}
```

这叫：

```text
Semantic Desktop Context
```

---

# 49. Character Behavior

Behavior 不是动画文件名，而是角色在某个场景下的**语义表现状态**。建议分成三类：

```text
系统物理行为：Dragged / Falling / Landing
交互行为：Greeting / Talking
环境与 AI 表现：Idle / Thinking / Happy / Sit / Sleep / 后续导入动作
```

系统物理行为由 DesktopEngine 的事实触发，优先级最高；AI 不得禁用、替换或延长它们。AI 可以从 MotionCatalog 中 `aiSelectable = true` 且用户已启用的动作里选择表现行为。

不要再用一个枚举同时表达：

```text
桌面物理状态
当前动画片段
情绪
说话状态
AI 建议
```

推荐将它们拆成可组合状态：

```ts
interface CharacterBehaviorState {
  physical: 'idle' | 'dragged' | 'falling' | 'landing';
  primaryMotion: string;
  emotion: EmotionState;
  speaking: boolean;
}
```

动作选择使用两个不同契约：

```ts
interface BehaviorProposal {
  actionId?: string;
  emotion?: EmotionIntent;
  speech?: string;
  intensity?: number;
}

interface BehaviorIntent {
  actionId: string;
  source: 'system' | 'interaction' | 'ai' | 'audio';
  priority: number;
  intensity: number;
  issuedAt: number;
}
```

`BehaviorProposal` 是 BrainEngine 的建议，可以被拒绝；`BehaviorIntent` 是 BehaviorPlanner 校验后交给 CharacterEngine 的执行意图。代码和日志中不要把两者都简称为 `action`。

---

# 50. Behavior 与 Animation 分离

完整链路应为：

```text
DesktopEvent / UserInteraction / AgentResponse / AudioEvent
                         │
                         ▼
                  BehaviorPlanner
              外部响应 → BehaviorProposal
                         │
                         ▼
                   BehaviorPolicy
         来源 / Catalog / allow-list / 强度校验
              → 生成 BehaviorIntent
                         │
                         ▼
                  BehaviorScheduler
       物理状态仲裁 / 抢占 / 排队 / 冷却 / 恢复
                         │
                         ▼
                  AnimationSelector
          语义动作 ID → 程序化动作或具体 VMD
                         │
                         ▼
                   MotionController
             混合、过渡、骨骼与 Morph 执行
```

例如 AI 在问候场景选择：

```json
{
  "action": { "id": "greeting", "intensity": 0.7 }
}
```

BehaviorPlanner 的处理：

```text
1. MotionCatalog 中是否存在 greeting
2. greeting 是否允许 AI 使用且用户已启用
3. 当前是否处于 Dragged / Falling / Landing
4. 是否仍在冷却时间
5. 强度是否在允许范围
6. 批准、延迟、替换或拒绝
```

若角色正在 Falling：

```text
AI Proposal: Greeting
        ↓
BehaviorPlanner: 延迟或拒绝
        ↓
CharacterEngine: 继续 Falling
        ↓
Landing 完成后重新评估 Greeting
```

推荐优先级：

| 层级 | 来源 | 示例 | 默认策略 |
|---|---|---|---|
| 100 | 系统安全/物理 | Dragged、Falling、Landing | 立即抢占，不允许 AI 覆盖 |
| 80 | 用户直接交互 | 点击问候、用户明确命令 | 可抢占普通 AI 表现 |
| 60 | 语音同步 | Talking、Listening | 与主动作按策略叠加 |
| 40 | AI 场景动作 | Greeting、Thinking、Happy | 仅从 allow-list 选择 |
| 10 | 环境动作 | Idle、偶尔歪头 | 任意高优先级行为可打断 |

涉及桌面移动时还要拆成两个意图：

```text
BehaviorIntent: WalkTo(Chrome)
        ├─ MovementIntent → DesktopEngine → Native Window Movement
        └─ MotionIntent   → CharacterEngine → Walk Animation
```

CharacterEngine 不直接移动 HWND；DesktopEngine 也不直接选择 VMD。

---

# 51. Motion Catalog 与 VMD 导入

Agent 不选择文件名：

```text
walk_001.vmd
```

Agent 只能选择稳定的语义动作 ID：

```text
walk
greeting
thinking
```

MotionCatalog 再把动作 ID 映射到程序化动作或某个 VMD 变体：

```ts
interface MotionDefinition {
  id: string;
  displayName: string;
  description: string;
  source: 'procedural' | 'vmd';
  asset?: string;
  scenes: string[];
  aiSelectable: boolean;
  loop: boolean;
  interruptible: boolean;
  cooldownMs: number;
  allowedPhysicalStates: string[];
  fallbackId?: string;
}
```

只向 BrainEngine提供以下最小信息：

```json
{
  "id": "thinking",
  "description": "短暂思考和观察",
  "scenes": ["思考", "等待回答"]
}
```

不要向 LLM 暴露本地路径、VMD 文件名、骨骼名称或动画混合参数。

建议资源目录：

```text
assets/
└── motions/
    ├── idle/
    ├── greeting/
    ├── talking/
    ├── thinking/
    ├── sit/
    └── special/
```

VMD 导入流程必须包含：

```text
文件选择与 capability scope
格式与大小校验
动作元数据读取
骨骼映射/重定向检查
模型兼容性预览
场景标签与 aiSelectable 配置
复制到受管资源目录
注册 MotionCatalog
```

导入失败时不能留下已注册但不可播放的动作。MotionCatalog 的保存与资源复制必须具有同一事务语义，或提供可恢复的回滚步骤。

---

# 52. AudioEngine Architecture

AudioEngine 是独立职责域，但不直接控制 CharacterEngine。目标数据流：

```text
User / System / BrainEngine
             │
         SpeechIntent
             ▼
        AudioController
     ┌───────┼────────┐
     ▼       ▼        ▼
    TTS     SFX     Playback
     │                │
     └────── Audio Timeline
                    │
          ┌─────────┴─────────┐
          ▼                   ▼
      Speaker             LipSyncAnalyzer
                              │
                         LipSyncFrame
                              │
                              ▼
                      CharacterEngine Morph
```

职责拆分：

| 组件 | 职责 |
|---|---|
| `TtsProvider` | 文本合成音频和可选 VisemeCue，不管理角色 |
| `SttProvider` | 用户显式授权后的语音识别，不生成动作 |
| `AudioController` | 播放、取消、打断、音量组与活动会话所有权 |
| `SfxController` | 短音效、分类音量和并发限制 |
| `LipSyncAnalyzer` | 从 RMS 或音素时间戳生成标准嘴型帧 |
| `CharacterEngine` | 消费 LipSyncFrame 并应用 PMX Morph |

稳定事件：

```text
audio://preparing
audio://started
audio://frame
audio://completed
audio://cancelled
audio://failed
```

`audio://frame` 只携带归一化音量、可选 viseme 和时间戳，不携带 PMX Morph 名称。角色模型自己的 MorphMap 负责最终映射。

BrainEngine 只能产生 `speech` 文本或语音意图；它不能指定扬声器设备、直接播放字节、改变系统音量或伪造播放完成事件。

---

# 53. Lip Sync

第一阶段：

```text
Audio RMS (0..1)
       ↓
LipSyncFrame.level
       ↓
CharacterEngine
       ↓
Mouth Open Morph
```

第二阶段：

```text
TTS VisemeCue / phoneme timestamps
       ↓
A / I / U / E / O
       ↓
模型 MorphMap
       ↓
PMX Morph
```

例如：

```text
A → あ
I → い
U → う
E → え
O → お
```

规则：

```text
语音帧优先于程序化 Talking 嘴型
播放取消、失败或结束时必须发送静音帧
嘴型可以与非冲突的上半身动作叠加
SFX 默认不驱动嘴型
```

---

# 54. Emotion

定义：

```ts
export type Emotion =
  | 'neutral'
  | 'happy'
  | 'sad'
  | 'angry'
  | 'surprised'
  | 'tired';
```

Emotion 也是语义建议，不直接等于 Morph 权重：

```text
EmotionProposal
      ↓
BehaviorPlanner / EmotionPolicy
      ↓
EmotionIntent
      ↓
EmotionController
      ↓
模型专属 MorphMap + Motion Overlay
```

例如 BrainEngine 可以建议：

```json
{
  "emotion": { "type": "happy", "intensity": 0.72 }
}
```

系统校验后再映射为：

```text
smile morph = 0.7
eyes = 0.2
head pose = positive
idle animation = happy idle
```

不同模型可以使用完全不同的 Morph 名称和数值范围，BrainEngine 不需要知道这些差异。

---
# 55. AI Layer

AI 一定是最后接入运行时，但 **接口必须从项目一开始就抽象好**，否则后续更换模型、更换记忆框架、增加视觉能力时都会牵动 Character Runtime。

## 核心原则：WebView 永远不直接对话 LLM

Frontend（Three.js / TypeScript）**不允许**持有 API Key，也**不允许**直接发起对模型服务商的网络请求。原因：

- Tauri 2 的 capability 模型默认限制 WebView 的网络与文件系统权限（见「Capability Security」章节）；
- API Key 一旦进入 WebView 上下文，就存在被前端依赖库、DevTools、或未来的 Prompt Injection 链路间接泄露的风险；
- 所有需要密钥的操作（HTTP 请求、OAuth 刷新、密钥读写）都更适合放在 Rust 侧，用 `reqwest` 发起，用 OS 级安全存储保存密钥。

当前版本采用统一的本地边界：对话 LLM、记忆提取 LLM 与 Embedding API 都只能使用经过双端校验的 HTTP loopback 地址，不定义、注册或启用远程 LLM Provider，也不存在本地模型失败后的云端 fallback。若未来改变该政策，必须单独进行隐私设计和用户授权评审。

最终数据流：

```text
Frontend（对话输入 / 文本气泡）
      │  invoke("converse")
      ▼
Rust: BrainSupervisor / BrainClient
      ├─ 临时令牌与 loopback IPC
      ├─ 请求 DTO 校验
      └─ AvailableAction[]             ← 当前允许 AI 建议的动作摘要
      │
      ▼
Python: ConversationOrchestrator
      ├─ SQLite SessionStore.get_recent()
      ├─ Local Mem0.search()
      └─ LocalLlmProvider.complete()
      │
      ▼
AgentResponse（仍是不可信模型输出）
      │
      ▼
Schema / Safety / Range Validation
      │
      ▼
CharacterResponse
      ├─ speech ──────────────────────► AudioEngine
      ├─ behaviorProposal ────────────► BehaviorPlanner
      ├─ emotionProposal ─────────────► BehaviorPlanner
      └─ 本轮对话 ────────────────────► SQLite commit + Memory outbox
                                               │
                                               ▼
                                      Local Mem0 remember_turn()

BehaviorPlanner ── BehaviorIntent ───► CharacterEngine / DesktopEngine
```

`AvailableAction[]` 只包含动作 ID、描述和场景标签，来源于 MotionCatalog 与用户配置。它不包含本地文件路径、VMD 文件名、骨骼名称或 Morph 参数。这样 AI 能根据场景选择动作，但无法越过动作目录和执行策略。

## LocalLlmProvider：当前本机模型入口

Phase I 只有一个生产实现：Python `LocalLlmProvider`。它调用本机 `/v1/chat/completions`，不需要为单一实现维护 Rust trait、注册表或热切换状态。

```python
def complete(
    *,
    user_input,
    recent_messages,
    memories,
    available_actions,
    desktop_context,
) -> dict:
    ...
```

Rust IPC 的 `ConversationPayload` 和 `CharacterResponse` 是跨进程稳定契约；Python 内部参数不是第二套公共 API。Endpoint 和模型可在设置页修改，保存后 BrainSupervisor 受控重启 Sidecar。客户端禁用系统代理和 HTTP 重定向，Endpoint 必须解析为 loopback，不接收远程 API Key，也不存在云端 fallback。

当前不维护 Agent registry、热切换 command 或流式事件协议。只有出现第二个真实本机 Provider 和明确的无重启切换需求时，才提取 Manager；流式输出也在有真实 UI 消费者时再增加。

## LLM 结构化输出（不变的契约）

```json
{
  "speech": "你回来啦。",
  "emotion": {
    "type": "happy",
    "intensity": 0.8
  },
  "behavior": {
    "actionId": "greeting",
    "intensity": 0.7,
    "reason": "用户刚刚回到桌面"
  }
}
```

请求校验必须满足：

```text
LocalLlmProvider Endpoint → 必须通过 HTTP loopback 校验
ConversationOrchestrator → 只能传入本机会话和 MemoryPort 召回的上下文
本机服务不可用 → 返回 Degraded，不回退到远程 Provider
```

当前没有远程或无记忆云端模式；所有对话请求都通过本机 LocalLlmProvider 执行。

上面的 `behavior` 只是 `BehaviorProposal`。BehaviorPlanner 必须再次检查 MotionCatalog、用户启用项、当前物理状态、优先级与冷却时间，批准后才生成 `BehaviorIntent`。

当前 Phase II 由 `MemoryManager.remember_turn` 调用本地候选提取与 `MemoryPolicy`，再将批准事件写入 SQLite 并异步投递 Mem0/Graphiti。Mem0 的 LLM 和 Embedding 均显式绑定本机 API；ConversationOrchestrator 只依赖 `MemoryPort`，不直接依赖 Provider SDK。

---

# 56. Memory Interface（记忆系统接口）

记忆系统采用“本地会话事实源 + canonical memory event store + 窄 MemoryPort + 可替换 Provider”的结构。Phase II 的 `MemoryManager` 位于 Python Sidecar，仅编排本地 SQLite、Mem0 与可选 Graphiti；Rust 不维护第二套 MemoryManager 或事实源。BrainEngine 和 ConversationOrchestrator 不能直接依赖 Mem0/Graphiti 的 SDK、HTTP 数据结构或存储模型。

## 强制本地化边界

记忆系统实行 **Local-Only / Fail-Closed**。这是不可被设置项、Provider 或降级逻辑覆盖的架构不变量：

```text
数据落盘：只允许本机应用数据目录
数据处理：只允许本机进程、本机 sidecar 或本机推理服务
数据传输：只允许进程内调用、Tauri IPC、Windows Named Pipe 或 loopback
外部网络：默认拒绝；失败时不允许回退到云端
```

“记忆数据”包括但不限于：

```text
原始会话与语义事件
Session / Semantic / Episodic Memory
MemoryContext / MemoryWriteProposal / ApprovedMemoryEvent
embedding、向量索引、图节点和图关系
提取、归并、去重、重排时使用的 prompt 与响应
待处理队列、缓存、备份、导出和包含正文的诊断日志
```

允许的通信目标只有：

```text
进程内接口
Tauri IPC
Windows Named Pipe
127.0.0.0/8
::1
解析后全部地址均为 loopback 的 localhost
```

必须拒绝公网地址、局域网地址、网络共享路径、云同步目录，以及从 loopback 跳转到非 loopback 的 HTTP redirect。HTTP 客户端必须禁用代理继承和自动跨域重定向，连接前后都要验证最终目标仍为本机。

Mem0、向量数据库、embedding 模型、重排模型、Graphiti 及其图数据库必须全部在本机运行。任何遥测、云备份、托管控制台同步或上传诊断正文的功能都必须关闭。

用于“提取哪些信息值得记住”、分类、归并、摘要、去重和重要性评估的 LLM 必须由 `Mem0Memory` 显式配置为本机 API。禁止调用远程 Provider，禁止在本地模型不可用时自动回退到云端；失败时只保留本地 Session Memory 和 outbox 待处理状态。

```text
ConversationOrchestrator
          │
          ▼
ConversationSessionStore          MemoryPort
Session / Outbox                      │
SQLite                                ▼
                                  Mem0Memory
                             Semantic + Episodic
```

`ConversationOrchestrator` 负责查询顺序、降级和对话提交；`ConversationSessionStore` 负责 Session 与对话写入 outbox；`MemoryPort` 负责隔离具体后端。当前 Phase II 已实现 `MemoryManager / MemoryPolicy`、canonical `MemoryEventStore`、Provider outbox，以及 Mem0 与可选 Graphiti 的结果融合和独立降级。

## 三类记忆

| 类型 | 内容 | 所有者与存储 | 生命周期 |
|---|---|---|---|
| Session Memory | 当前会话最近若干轮消息和必要状态 | 本地 `SessionStore` / SQLite | 会话内立即可见，可按策略归档或清除 |
| Semantic Memory | 用户偏好、稳定事实、人物关系和长期设定 | Phase I 本机 `Mem0Provider` | 跨会话长期保存，可更新、合并和遗忘 |
| Episodic Memory | 带发生时间、场景与来源的具体经历 | Phase I 本机 `Mem0Provider` | 跨会话长期保存，按时间和语义召回 |

Session Memory 不经过向量检索才能使用，也不依赖 Mem0 在线状态。Semantic 与 Episodic 是项目自己的逻辑分类；即使两者第一阶段都映射到 Mem0，也必须在项目 DTO 中保持区分。

当前第二阶段中，Temporal Graph 是第四种**检索视图**，不是第四份原始事实源。它适合回答“某个关系何时发生、如何变化、事件先后顺序是什么”，由已经批准的情景事件构建：

```text
本地 Conversation / Event Log（事实源）
              │
              ├─► Mem0Provider：事实提炼、语义召回
              └─► GraphitiProvider：时间、实体与关系图
```

禁止把 Graphiti 串成 Mem0 的下游实现。两者必须是 `MemoryManager` 下的并列 Provider，可以独立启用、禁用、重建或替换。

## 本地事实源

本地 SQLite 保存完整、可审计、可删除的会话与重要语义事件。Mem0 和 Graphiti 中的数据属于可重建的派生索引，不代替本地事实源。这样更换 embedding、调整提炼策略或重建时间图时，不需要依赖第三方后端保留全部原始上下文。

只记录通过语义边界的事件，例如：

```text
允许：用户完成一个任务、用户明确表达偏好、一次对话消息
禁止：每一帧骨骼数据、每次鼠标移动、原始音频帧、未授权窗口内容
```

原始会话是否长期保留由用户设置控制；关闭历史记录时，Session Memory 仍可仅驻留当前进程。

## 稳定数据契约

```rust
pub enum MemoryKind {
    Semantic,
    Episodic,
}

pub struct MemoryScope {
    pub user_id: String,
    pub character_id: String,
    pub session_id: Option<String>,
}

pub struct MemoryWriteProposal {
    pub content: String,
    pub suggested_kind: Option<MemoryKind>,
    pub importance: Option<f32>,
    pub tags: Vec<String>,
    pub occurred_at: Option<DateTime<Utc>>,
    pub source_event_ids: Vec<String>,
}

pub struct MemoryEntry {
    pub scope: MemoryScope,
    pub kind: MemoryKind,
    pub content: String,
    pub importance: f32,
    pub tags: Vec<String>,
    pub occurred_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub source_event_ids: Vec<String>,
    pub metadata: serde_json::Value,
}

pub struct MemoryQuery {
    pub scope: MemoryScope,
    pub text: String,
    pub kinds: Vec<MemoryKind>,
    pub top_k: usize,
    pub time_range: Option<TimeRange>,
}

pub struct MemoryRecord {
    pub id: MemoryId,
    pub entry: MemoryEntry,
    pub score: f32,
    pub provider_id: String,
}

pub struct MemoryContext {
    pub recent_messages: Vec<ChatMessage>,
    pub semantic: Vec<MemoryRecord>,
    pub episodic: Vec<MemoryRecord>,
    pub temporal_relations: Vec<TemporalRelation>,
}
```

`user_id` 隔离不同用户，`character_id` 隔离不同桌宠的人设与共同经历，`session_id` 只限定当前会话。不得只用一个全局字符串作为所有记忆的命名空间。

## Phase I MemoryPort

```python
class MemoryPort(Protocol):
    def ready(self): ...
    def close(self): ...
    def search(self, query, scope, limit): ...
    def remember_turn(self, messages, scope, turn_id): ...
    def list(self, scope): ...
    def update(self, memory_id, content, scope): ...
    def delete(self, memory_id, scope): ...
```

`MemoryManager` 和关闭长期记忆时使用的 `DisabledMemory` 实现同一窄接口。`ConversationSessionStore` 负责对话提交与待提取 turn；`MemoryEventStore` 负责已审核记忆事实与逐 Provider outbox。Rust 不再维护第二份记忆事实源。

Agent 只能接收裁剪后的 `MemoryContext`，不能获得 Provider 客户端。检索结果被视为不可信引用数据，不能作为 system instruction 执行；任何类似“忽略规则”“运行命令”的记忆内容都只能作为被引用的用户资料。

`LocalEndpoint` 构造时必须完成 IP 解析、loopback 校验、代理禁用和 redirect 策略校验，不能用未经验证的 URL 字符串代替。Mem0 内部的 LLM/Embedding 客户端必须接受相同的双端校验，且不得包含远程 fallback。

## Phase II MemoryPolicy

```text
Conversation Turn / System Semantic Event
              │
              ▼
 LocalMemoryInferenceProvider
       （仅本地 LLM API）
              │
      MemoryWriteProposal
              │
              ▼
         MemoryPolicy
  权限 → 敏感性 → 分类 → 去重 → 重要性
              │
        ApprovedMemoryEvent
              │
              ▼
         MemoryManager
        ┌─────┴───────────────┐
        ▼                     ▼
  Mem0Provider       Phase II GraphitiProvider
```

上图是当前 Phase II 多 Provider 写入流程。`LocalMemoryInferenceProvider` 只调用本机 OpenAI-compatible LLM，输出候选而不能直接写 Provider。`MemoryPolicy` 统一执行长度、敏感信息、分类、重要性、去重与可选人工确认；只有写入 SQLite 的 `ApprovedMemoryEvent` 才能进入 Mem0/Graphiti outbox。

## 对话调用顺序

```text
对话开始前：
  1. SessionStore.get_recent(scope, N)
  2. MemoryPort.search(query)
  3. ConversationOrchestrator 将最近会话与召回结果传给 LocalLlmProvider

对话结束后：
  1. ConversationSessionStore 原子提交 user / assistant / CharacterResponse
  2. 同一事务创建 memory outbox 状态
  3. MemoryManager.remember_turn() 使用本地 LLM 生成候选并经过 MemoryPolicy
  4. 候选写入 SQLite；批准项分别排队同步 Mem0 / Graphiti
  5. Provider 成功后按 event revision 完成投递；失败则指数退避并保留待重试状态
```

Mem0 或 Graphiti 不可用时，对话仍依靠 Session Memory 与 SQLite 事实源工作；长期写入进入本地待处理队列并采用有上限的指数退避，不阻塞角色渲染、动作播放或应用退出。Graphiti 的初始化在独立 asyncio 线程中进行，不能占用 Sidecar readiness 的启动窗口。

## Mem0 接入策略（Phase I）

Mem0 的 Python 实现必须在本机受控 Sidecar 中运行，由 Rust `BrainClient` 通过 loopback 调用，不把 Python 解释器嵌入 Character WebView：

```text
Rust Desktop Host / BrainSupervisor
      │ 临时令牌 + loopback HTTP DTO
      ▼
Python Brain Sidecar / ConversationOrchestrator
      │ MemoryPort
      ▼
Local Mem0
      ├─ Local embedding model
      ├─ Qdrant embedded local mode
      └─ SQLite history / conversation / outbox
```

Mem0 sidecar 必须绑定 loopback、使用随机会话凭据、由 Tauri 生命周期统一启动和关闭，并提供健康检查与版本兼容检查。配置中不提供 Mem0 Cloud、远程 `base_url` 或远程 API Key；检测到非本机地址时配置保存和启动都必须失败。启动检查还必须验证 Mem0 内部配置的 LLM、embedding 与 vector store 均为本机实现，不能只检查 Rust 到 Mem0 的第一跳。

Phase I 不需要 Docker。当前实现使用 SQLite 加 Qdrant embedded local mode；Rust Host 每次启动生成 64 位十六进制临时令牌和随机回环端口，Python Sidecar 只监听 `127.0.0.1`。Rust 与 Python 两侧都拒绝非 loopback LLM/Embedding URL，并禁用系统代理、HTTP 重定向、Mem0 遥测以及任何默认云端 Provider。所有会话、向量、历史、outbox 与日志只能写入 Tauri app local data 下的 `brain/` 目录。

Embedding 向量维度必须显式配置，并同时传给 Mem0 embedder 与 Qdrant collection。项目不预设任何本地模型或模型服务；接入前必须确认实际输出维度，并重建与新维度不兼容的本地向量集合，禁止静默沿用错误维度。

日志默认只记录时间、级别、请求方法、路由和错误类型，不记录认证令牌、完整对话、Prompt、记忆正文或请求正文。

Mem0 的内部 memory ID、metadata 字段和过滤语法不能越过 Python `MemoryPort`。Rust 和前端只接收稳定的 `BrainMemory` DTO，确保以后可以替换后端。

Phase II 不再让 Mem0 决定哪些内容值得保存。Mem0 只消费已经批准的 canonical event，并继续显式使用本地 LLM、本地 Embedding、SQLite history 和 Qdrant embedded mode；其旧 Phase I 记录通过 `legacy:mem0:` 兼容 ID 继续支持查看、修改和删除。

## Graphiti 接入策略（Phase II）

Graphiti 及其图数据库同样必须部署在本机。Graphiti 用于补充时间图查询，不替代 SessionStore，也不自动接管全部 Semantic Memory。`MemoryManager` 并行查询本地 Mem0 和本地 Graphiti，然后完成：

```text
按作用域过滤
结果去重
时间相关性与语义相关性融合
来源标注
上下文 token 预算裁剪
```

Graphiti 的写入来源是 `ApprovedMemoryEvent` 或本地事实源重建任务，不读取 Mem0 私有存储。这样 Mem0 与 Graphiti 任一后端都可以单独升级或移除。

Graphiti 不得连接托管图数据库、远程 embedding 或远程 LLM。当前实现固定 `graphiti-core >=0.30.1,<0.31`，仅支持本机 Neo4j Driver；Windows 下不采用不可用的 FalkorDB Lite，也不采用 Graphiti 已弃用的 Kuzu Driver。URI 仅允许直连 `bolt://` 回环地址，禁止可能通过路由发现返回其他节点的 `neo4j://`；LLM 与 Embedding 显式注入禁用代理和重定向的本地客户端，重排使用无网络本地实现，并设置 `GRAPHITI_TELEMETRY_ENABLED=false`。

Graphiti 默认关闭。密码只读取 Rust 启动环境继承的 `DESKTOP_COMPANION_GRAPHITI_PASSWORD`，不得进入 WebView、Tauri command 参数或 `settings.json`。Graphiti 缺密码、未安装、Neo4j 未启动或索引失败时，Provider 标记为降级；SQLite、Mem0、Session Memory 和正常对话继续工作。

## 目录与配置

```text
src-tauri/src/brain/
├── supervisor.rs
├── client.rs
├── locality.rs
└── model.rs

brain-sidecar/src/desktop_companion_brain/
├── orchestrator.py
├── session_store.py
├── memory.py                  # MemoryPort 与 Mem0 Provider
├── memory_manager.py          # 多源召回、审批操作与 outbox worker
├── memory_policy.py           # 候选校验、敏感过滤与重要性策略
├── memory_store.py            # canonical SQLite event store
├── memory_inference.py        # 仅本地 LLM 的候选提取
├── graphiti_provider.py       # 可选本机 Neo4j 时间图
├── llm.py
├── security.py
└── server.py
```

当前 Tauri command：

```text
get_brain_status
get_brain_settings
configure_brain
converse
list_brain_memories
update_brain_memory
delete_brain_memory
get_memory_status
approve_brain_memory
reject_brain_memory
rebuild_brain_memory
```

当前设置页提供长期记忆开关、回答前召回、回答后写入、最低重要性、人工审批、审计 tombstone 保留期、列表、修改、逐条删除、Provider 状态和按角色作用域重建索引。按作用域清空和导出仍属于后续显式功能，不通过隐藏 Tauri command 预留。

记忆设置中的 Endpoint 只能选择自动发现的本机服务或填写通过 `LocalEndpoint` 校验的 loopback 地址。界面不提供云端 Provider、API Key、OAuth 或远程地址字段。

导出目标同样必须是本地磁盘路径，并拒绝 UNC、映射网络盘和已知云同步目录。导出不会触发任何自动上传；用户离开应用后自行复制文件不属于运行时功能。

---

# 57. Vision Interface（VL / 视觉能力接口）

视觉能力**第一版不实现**，但接口与权限模型现在就定义好，避免以后接入视觉模型时重新设计权限系统。

## 设计目标

```text
现在：
  定义 trait + 数据结构 + 权限开关
  不接任何真实模型，也不做任何截屏

以后：
  接入任意一个多模态模型（云端 Provider 或用户自选的本机多模态服务）
  只需要新增一个 VisionProvider 实现，Character / Behavior 层不需要改动
```

## VisionProvider trait

```rust
#[async_trait::async_trait]
pub trait VisionProvider: Send + Sync {
    fn id(&self) -> &str;

    async fn describe(&self, image: ImageInput, prompt: Option<String>)
        -> Result<VisionResult, VisionError>;
}

pub enum ImageInput {
    ScreenRegion(DesktopRect),  // 由 Rust native 截屏，不经过 WebView
    StaticImageBytes(Vec<u8>),
    // 未来可扩展：Webcam(...) 等
}

pub struct VisionResult {
    pub description: String,
    pub tags: Vec<String>,
    pub detections: Vec<DetectedRegion>, // 可选，供后续与 UI Automation 融合
}
```

## 与本机多模态 Provider 的关系

多数多模态模型的图片输入其实走的是同一条 Chat API（把图片作为一条 message 内容），因此 `VisionProvider` 通常不需要独立实现，而是：

```text
未来的 LocalMultimodalProvider 可同时承担对话与 VisionProvider

VisionProvider.describe()
  = 构造一条包含图片的本机多模态请求
  → 复用同一个 HTTP client / 同一份密钥配置
  → 只是 prompt 与解析方式不同
```

只有当用户接入独立的纯本地视觉模型（例如专门运行 LLaVA 的服务）时，才需要单独实现 VisionProvider。

## 权限与隐私边界

视觉能力涉及"看屏幕"，必须比普通 AI 对话权限更严格：

```text
默认关闭

用户必须在设置里显式打开："允许桌宠观察屏幕"
   独立于"允许桌宠联网使用 AI"这个开关

每一次实际截屏都必须有可感知的提示
   （例如角色出现"观察"动作 / 系统托盘图标短暂变化）

第一版只允许"用户显式触发"的单次截屏
   （例如用户对话中说"看看这是什么"），
   禁止后台定时 / 静默截屏
```

截屏本身仍然属于 Rust Native Core 的职责（与 DesktopWorld 一样），WebView 不直接拿到屏幕像素数据，只会收到 VisionProvider 处理后的文字描述。

## 目录与配置

```text
src-tauri/src/vision/
├── mod.rs
├── provider.rs        // VisionProvider trait + 数据结构
├── manager.rs
├── capture.rs          // 截屏（Windows native，受权限开关控制）
└── providers/
    ├── mod.rs
    └── local_multimodal.rs  // 未来本机多模态实现
```

对应 Tauri command（先定义，Phase 10 之后再实现）：

```text
set_vision_enabled(bool)
list_vision_providers
set_vision_provider(providerId)
capture_and_describe(region?)   // 显式触发，返回 VisionResult
```

---

# 58. Provider 配置与密钥安全

AI / Vision 与云端 TTS Provider 共用同一套配置和密钥管理原则，避免每个模块各写一套安全逻辑。音色、音量等非敏感播放参数可以进入普通设置；API Key、OAuth Token 和云端语音凭据必须进入 OS 级安全存储。

Memory 是强制例外：记忆存储、检索、embedding、重排、提取 LLM 和 Graphiti 全部只允许本机实现，不适用下文的远程 API Key / OAuth 接入方式。记忆服务只能配置经 `LocalEndpoint` 校验的本机端点；本地 sidecar 的随机会话凭据只用于本机进程鉴权。

## 当前接入方式

```text
本地 OpenAI-compatible 模式（唯一启用模式）
  用户在设置中填写：
    loopback base_url
    model
  适用：
    任意由用户选择的本机 OpenAI-compatible 服务
  限制：
    不接收远程 API Key
    不允许 HTTPS、公网或局域网地址
    不允许系统代理、HTTP redirect 或云端 fallback
```

API Key 与 OAuth 只作为未来可能的无记忆功能设计记录，当前版本不得实现、显示或启用。若未来启用，仍不能接收任何会话历史、Prompt、MemoryContext 或持久记忆派生内容。

## 密钥必须走 OS 级安全存储

```text
不允许：
  API Key / OAuth Token 以明文写入 SQLite、JSON 配置文件、日志

必须：
  使用 keyring crate（Windows 下即 Windows Credential Manager）
  或 Tauri Stronghold 插件保存密钥本体

  SQLite / Tauri Store 只保存非敏感元数据：
    provider id、base_url、model 名称、功能开关、
    以及一个指向安全存储条目的引用
```

前端 Settings 界面永远只通过 command 与密钥交互，不会拿到明文：

```text
set_provider_credentials(providerId, apiKey)   // 只返回成功/失败
test_provider_credentials(providerId)          // 发测试请求验证可用性
get_provider_summary(providerId)               // 返回 base_url / model /
                                                //   掩码后的 key 预览，
                                                //   例如 sk-****ab12
clear_provider_credentials(providerId)
```

## 多 Provider профиль 并存

AI 对话、记忆、视觉三者可以使用不同实现，但记忆始终留在本机。例如：

```text
普通 Chat         → LocalLlmProvider
记忆感知 Chat     → LocalLlmProvider
记忆提取 LLM      → Mem0 中显式配置的本地 LLM
Memory            → 本地 SessionStore + 本地 Mem0Provider
Vision            → 独立权限控制
```

这与“统一编排层 + 四个职责域”的架构一致：AI、记忆、视觉是 BrainEngine 内部三个独立可插拔子模块；TTS 是 AudioEngine 的可插拔 Provider。它们共享安全规范，但不共享业务状态。当前所有 LLM 与 Embedding Provider 都必须是本机 Provider。

---

# 59. LLM 输出边界

LLM 可以输出：

```text
speech 文本
emotion 语义与有限强度
MotionCatalog 中允许 AI 使用的 actionId
记忆写入建议
工具调用建议（只有后续显式启用时）
```

LLM 不允许输出或执行：

```text
Bone.rotation / Bone.position
Morph 索引或任意权重
VMD 本地路径或文件名
Window HWND / 任意桌面坐标写入
扬声器设备或系统音量控制
ExecutePowerShell / DeleteFile
ClickAnything / MoveMouse
绕过 MotionCatalog 的动作 ID
伪造 audio://completed 等生命周期事件
```

这不是禁止 AI 选择动作，而是把选择权限制在稳定的语义动作目录内：

```text
允许：actionId = "greeting"
禁止：rightArm.rotation.z = 1.24
```

Desktop Awareness 和 Desktop Control 必须分开。第一阶段只有：

```text
Read-only Awareness
+
Semantic Behavior Proposal
```

后续如果增加 Agent 操作电脑，必须使用另一套显式 Tool Proposal 契约，并经过：

```text
用户显式授权
Capability allow-list
参数与目标校验
危险操作确认
可取消的执行会话
审计日志（不记录敏感正文）
```

Tool Proposal 不得复用 BehaviorProposal；角色表现动作和操作电脑是两个完全不同的权限域。

---
# 60. Capability Security

Tauri 2 已经采用 capability / permission / scope 模型限制 WebView 可以访问的系统功能。官方建议对不同窗口按 capabilities 约束权限。

推荐：

```text
character window
```

只能：

```text
character commands
cursor data
model asset
```

不要让它直接拥有：

```text
arbitrary filesystem
shell
network
```

AI / Vision 的外部网络请求只允许在 Rust 侧按权限发起，不属于 character window 的 capability。Memory 的规则更严格：Rust 侧也只能连接 loopback 或使用本机 IPC，记忆模块 capability 不包含公网、局域网、系统代理或网络共享访问。WebView 永远只通过语义化 command 与 Rust 通信，拿不到 Provider 客户端或原始网络权限。

---

# 61. Settings Window

角色窗口不承载复杂菜单，配置页面必须使用独立 Tauri Window，避免遮挡模型或改变角色命中区域。

当前交互：

```text
Double Click Character
        ↓
Independent Settings Window（4:3）
        ├─ 助手模型
        ├─ 声音配置
        ├─ 动作配置
        └─ 记忆配置
```

职责边界：

| 页面 | 保存内容 | 不允许做的事 |
|---|---|---|
| 助手模型 | 模型 ID、显示比例、色彩和非敏感连接参数 | 直接加载任意磁盘路径 |
| 声音配置 | 音色 ID、语言、音量、语速、音调 | 在 WebView 保存云端密钥 |
| 动作配置 | AI 动作开关、允许的语义动作 ID | 让 AI 直接填骨骼或 VMD 路径 |
| 记忆配置 | 当前：长期记忆、召回/写入开关、本地 Endpoint、查看/修改/逐条删除；后续：历史策略、清空/导出 | 配置云端/局域网服务、把记忆上下文交给远程 Agent 或只删除展示记录 |

模型和动作选择都依赖目录注册：

```text
CharacterCatalog → characterModelId
MotionCatalog    → enabledAiMotionIds
```

配置页只编辑设置和发出预览请求；模型加载、声音播放和动作执行仍由各自 Engine 完成。切换需要重载的资源时，界面必须明确提示“保存后下次启动生效”，不能伪装成已经热切换。

API Key、OAuth Token 和云端语音凭据必须通过 Rust command 写入安全存储，不能进入普通 CharacterSettings。

Mem0 / Graphiti / 记忆提取 LLM 的本机 Endpoint、sidecar 会话凭据与健康检查由 Rust 侧管理。设置页只能调用窄 command，不得直接请求记忆服务；任何非 loopback 地址必须在保存前被拒绝。当前 Phase II 的 Graphiti 默认关闭；未配置密码、Neo4j 未启动或初始化失败时必须明确显示降级，不能伪装成可用。

---
# 62. Speech Bubble

Speech Bubble 第一版可以放：

```text
Character Window
```

但是如果文字可能很宽：

建议最终：

```text
Character Window
+
Bubble Window
```

两个独立透明窗口。

Bubble 跟随 Character。

---

# 63. 多显示器

由于 Character Position 本来就是：

```text
Desktop Coordinate
```

角色跨显示器时不需要：

```text
Three.js跨Scene
```

只需要：

```text
Native Character Window
```

移动过去。

当：

```text
Monitor A DPI = 100%
Monitor B DPI = 150%
```

MonitorManager 发：

```text
dpi_changed
```

TS 调整：

```text
device pixel ratio
renderer size
model scaling
```

---

# 64. Character Window Resize

不要根据模型每帧 resize。

固定：

```text
700 × 1000
```

或者：

```text
600 × 900
```

角色尺寸只在 Window 内调整。

这能避免：

```text
频繁 DWM resize
WebView重绘
Canvas resize
```

---

# 65. Performance Budget

目标：

```text
Idle CPU:
< 3~5%

GPU:
尽量 < 10%

RAM:
< 300~500 MB

Desktop polling:
< 10 Hz

Cursor polling:
60~120 Hz

Render:
60 FPS
```

如果用户机器较弱：

```text
30 FPS Mode
```

---

# 66. 角色不动时降低负载

如果：

```text
Sleep
Idle
Background
```

可以：

```text
30 FPS
```

聊天或拖动：

```text
60 FPS
```

甚至：

```text
Motion Adaptive Rendering
```

---

# 67. Desktop Window Event Optimization

最终：

```text
EnumWindows
```

只负责：

```text
Initial Snapshot
```

之后：

```text
SetWinEventHook
```

处理：

```text
create
destroy
move
resize
foreground
```

减少 polling。

---

# 68. 错误处理

Rust 所有 Windows API 调用：

```text
不要 unwrap()
```

统一：

```rust
Result<T, AppError>
```

定义：

```rust
pub enum AppError {
    WindowsApi(String),
    ModelImport(String),
    Io(String),
    InvalidState(String),
}
```

Frontend：

```text
AppError
 ↓
log
 ↓
fallback
```

不要崩整个桌宠。

---

# 69. Logging

Rust：

```text
INFO
WARN
ERROR
DEBUG
```

Frontend：

```text
Renderer
MMD
Animation
IPC
```

最终日志：

```text
logs/
2026-08-30.log
```

绝对不要记录：

```text
用户屏幕内容
敏感UI文本
LLM API Key
```

---

# 70. 第一阶段不要做的东西

先明确禁止范围。

暂时不要做（实现层面）：

```text
LLM
STT
复杂TTS
长期记忆的具体存储/检索实现
Screen Capture
Computer Vision
插件系统
自动控制电脑
多个角色
皮肤商店
非记忆配置云同步（记忆数据永久禁止云同步）
复杂设置UI
```

先做好：

```text
一个角色
真正活在Windows桌面上
```

> 只为当前生产消费者定义接口。跨 Engine 的稳定 DTO 可以先行，但单实现注册表、空 Manager、无调用 command 和假 Provider 不作为“预留扩展点”加入；出现第二个真实实现时再提取抽象。

---

# 71. Phase 0：工程骨架

目标：

```text
Tauri 启动
+
透明角色窗口
+
Three Canvas
+
Engine facade 与稳定契约
```

只建立接口，不接真实 AI 或云端语音：

```text
DesktopBridge
CharacterRuntime facade
BehaviorProposal / BehaviorIntent
AudioEvent / LipSyncFrame
当前 Engine 边界所需的最小 DTO
已有生产消费者的 Provider port
```

验收：

```text
应用可启动且透明窗口正确
CharacterEngine 不依赖具体 Agent/TTS Provider
Brain Stub 无法直接获取 CharacterRuntime
跨 Rust / TS 的 DTO 有 schema 校验
应用退出时所有 controller / listener 可释放
```

---
# 72. Phase 1：PMX

完成：

```text
PMX
Texture
Material
Bone
Morph
MMD Physics
```

验收：

```text
模型完整
脸部正确
头发正确
衣服正确
物理正常
```

---

# 73. Phase 2：基础生命感

完成：

```text
Blink
Breathing
LookAt
Idle
```

验收：

角色静止 5 分钟不会像“死模型”。

---

# 74. Phase 3：鼠标交互

完成：

```text
Hit Region
Cursor Polling
Click Through
Mouse Down
Mouse Up
Hover
```

验收：

```text
点角色
→ 角色响应

点角色旁边
→ Chrome正常收到点击
```

这是整个项目第一个重要 milestone。

---

# 75. Phase 4：Drag

完成：

```text
BeginDrag
MoveWindow
EndDrag
```

验收：

用户可以：

```text
抓住角色
拖到任意屏幕
松开
```

---

# 76. Phase 5：Desktop Physics

完成：

```text
Gravity
Fall
Land
ScreenFloor
```

验收：

```text
把角色拖到半空
松开
→ 掉到任务栏/地面
```

---

# 77. Phase 6：Windows Platform

完成：

```text
EnumWindows
DWM Bounds
DesktopWorld
WindowTop Surface
```

验收：

```text
角色可以站在Chrome窗口顶部
```

移动 Chrome：

角色：

第一版可以掉下来。

之后升级：

```text
follow moving platform
```

---

# 78. Phase 7：行为与动作调度

完成：

```text
MotionCatalog
BehaviorProposal / BehaviorIntent
BehaviorPlanner
BehaviorScheduler
MotionController
系统物理动作优先级
AI allow-list 配置
```

先接程序化动作：

```text
Idle
Greeting
Dragged
Falling
Landing
Talking
```

再接 VMD 时保持相同语义动作 ID，不修改 BrainEngine 契约。

验收：

```text
动作切换无明显跳帧
Dragged / Falling / Landing 不会被 AI 或 Talking 覆盖
不存在的 actionId 被拒绝并安全回到 Idle
关闭 AI 动作选择后所有 AI Proposal 都不会执行
高优先级行为完成后可以恢复或重新评估排队动作
```

---
# 79. Phase 8：UI Automation

完成：

```text
ElementFromPoint
BoundingRectangle
ControlType
Name
```

验收：

角色可以知道：

```text
附近是Chrome
鼠标正在按钮上
当前窗口是VS Code
```

---

# 80. Phase 9：AudioEngine

完成：

```text
AudioController
TTS Provider 或本地调试 Engine
SFX Controller 基础接口
播放 / 取消 / 打断
AudioEvent
LipSyncFrame
Speech Bubble
```

验收：

```text
语音、嘴型和气泡生命周期一致
新的语音请求会正确打断旧请求
结束、取消和失败时嘴型都回到零
关闭语音后不会播放但角色其他动作保持正常
SFX 不会错误驱动说话嘴型
AudioEngine 不直接修改 PMX Morph
```

---
# 81. Phase 10：BrainEngine 与本地记忆（Phase I 基础链 + Phase II 多 Provider）

当前 Phase I 完成：

```text
LocalLlmProvider（本机 OpenAI-compatible API）
ConversationSessionStore（本地 SQLite 会话事实源与 outbox）
MemoryPort / Mem0Memory / DisabledMemory
Rust LocalEndpoint + Python local-only HTTP client
Python Mem0 adapter（Phase I 长期语义与情景记忆）
ConversationOrchestrator
AvailableAction 注入
Schema / Safety / Range Validation
BehaviorProposal → BehaviorPlanner
SpeechIntent → AudioEngine
BrainSupervisor 自动启动 / readiness / 自动关闭
崩溃检测 / 稳定窗口 / 有界退避自动重启
随机临时令牌 / loopback-only / no-proxy / no-redirect
SQLite memory outbox / turnId 幂等恢复
```

当前阶段必须按依赖顺序实现和验收，不能跳过前置层：

```text
1. 生命周期管理
   Rust 生成临时令牌与回环端口
   自动启动 → readiness → 崩溃检测 → 有界退避重启 → 自动关闭

2. 对话骨架
   Conversation Session → ConversationOrchestrator → CharacterResponse
   turnId 幂等；先提交本地会话，再通过 outbox 完成长记忆写入

3. 本地 LLM
   只允许 HTTP loopback OpenAI-compatible API
   禁代理、禁重定向、禁远程 fallback

4. Mem0
   显式本地 LLM、本地 Embedding、SQLite history、Qdrant embedded local mode
   Recall / Write / Update / Delete；不需要 Docker

5. UI / 情绪 / TTS
   Speech Bubble 始终可用
   emotion / actionIntent 经 Schema 和 BehaviorPlanner 校验
   TTS 失败不能阻断文字、情绪或动作

6. 故障恢复测试
   慢启动、启动即退出、ready 后崩溃、连续崩溃退避
   错误令牌 401、依赖 Degraded、outbox 重启恢复、关闭无残留进程
```

调试环境和验证命令：

```powershell
uv sync --project brain-sidecar
uv run --project brain-sidecar python -m unittest discover -s brain-sidecar/tests -v
pnpm check
cargo test --manifest-path src-tauri/Cargo.toml
pnpm tauri dev
```

`pnpm tauri dev` 是当前演示入口，不要求生成 exe。若本地 LLM 或 Embedding 服务未启动，角色窗口仍须正常显示，Brain 状态显示 `Degraded`；关闭 Tauri 后不得残留 `desktop_companion_brain` Python 进程。

验收：

```text
角色可以进行多轮对话
对话具备短期上下文
Mem0 不可用时仍可使用 Session Memory 对话
记忆提取、归并和重排只调用本机 LLM API
本机 LLM 不可用时不会回退到任何远程 API
公网、局域网、系统代理和非 loopback redirect 均被拒绝
Semantic / Episodic Memory 使用独立类型和作用域
不同 user / character / session 的记忆不会串线
可以在设置里修改本机 Endpoint、模型与 Embedding 维度，保存后受控重启 Sidecar
LLM 只能从当前 MotionCatalog allow-list 建议 actionId
未知、禁用或状态冲突的动作被拒绝、延迟或替换
角色 Falling 时 AI Greeting 不会覆盖系统动作
LLM speech 进入 AudioEngine，而不是直接调用播放设备
用户可以查看、修改或逐条删除记忆
长期记忆写入执行敏感内容过滤，并通过 SQLite outbox 保证可恢复
远程 LLM Provider 不存在且不可用
日志、遥测、缓存和待处理队列不会把记忆内容发送或写入非本机位置
```

`VisionProvider` 保持已定义、默认关闭，留给后续显式授权的视觉功能。第一版 BrainEngine 不具备任意文件、Shell、鼠标或窗口控制能力。

Graphiti 属于当前已实现的 Phase II 增强，不作为 Phase 10 基础链的交付门槛；真实本机 Neo4j 服务上的集成验收仍待执行：

```text
TemporalGraphProvider
本机 GraphitiProvider + 本机图数据库
ApprovedMemoryEvent → 时间图索引
Mem0 + Graphiti 并列查询与结果融合
从本地事实源重建时间图
```

验收时必须证明关闭 Graphiti 不影响 Mem0 与 Session Memory，关闭 Mem0 也不影响本地会话连续性。

---
# 82. 第一个月推荐开发顺序

## Week 1

```text
Tauri
Transparent Window
Three Renderer
PMX Load
```

必须达到：

```text
PMX正常站在桌面上
```

---

## Week 2

```text
Morph
Blink
LookAt
Hit Region
Click Through
```

必须达到：

```text
角色可点
桌面可点
```

---

## Week 3

```text
Native Drag
Desktop Coordinates
Monitor
DPI
Gravity
```

必须达到：

```text
拖起来
松手
掉下来
```

---

## Week 4

```text
EnumWindows
DWM Rect
DesktopWorld
Window Surface
```

必须达到：

```text
角色能站在Chrome上
```

如果一个月完成这四件事：

> 这个项目最困难的底层骨架基本就成立了。

---

# 83. 核心运行数据流

## DesktopEngine → CharacterEngine

```text
Windows
   │  GetCursorPos / DWM / UIA
   ▼
DesktopEngine（Rust）
   │  DesktopWorld / CharacterState / DesktopEvent
   ▼
Tauri IPC / Events
   │
   ├─► BehaviorPlanner（Dragged / Falling / Landing）
   └─► CharacterEngine（LookAt 输入、命中区域同步）
```

DesktopEngine 拥有原生窗口位置和桌面物理；CharacterEngine 拥有视觉姿态。两边通过状态快照和语义事件同步，不共享位置对象。

## 用户交互 → Behavior

```text
Pointer / Click / Double Click
          │
          ▼
CompanionOrchestrator
          ├─ 设置请求 ─────────► Settings Window
          └─ 交互 Proposal ────► BehaviorPlanner
                                      │
                                      ▼
                               BehaviorIntent
                                      │
                                      ▼
                               CharacterEngine
```

## BrainEngine → 动作与声音

```text
User Input + Memory + DesktopContext + AvailableAction[]
                         │
                         ▼
                    BrainEngine
                         │ AgentResponse
                         ▼
               Schema / Safety Validation
                  ┌──────┴────────┐
                  ▼               ▼
        BehaviorProposal        SpeechIntent
                  │               │
                  ▼               ▼
          BehaviorPlanner     AudioEngine
                  │               │
          BehaviorIntent      AudioEvent
                  │               ├─► Speech Bubble
                  ▼               └─► LipSyncFrame
          CharacterEngine                 │
                  ▲                       │
                  └───────────────────────┘
```

## BrainEngine → 记忆

```text
User Input
    │
    ▼
ConversationOrchestrator
    ├─ ConversationSessionStore.recent()
    └─ MemoryPort.search()
    │
    ▼
Recent messages + memories → LocalLlmProvider
                    │
                    ▼
             CharacterResponse
                    │
       ┌────────────┴─────────────┐
       ▼                          ▼
ConversationSessionStore   MemoryPort.remember_turn()
commit + outbox                  Local Mem0
```

ConversationSessionStore 保存原始会话与对话写入 outbox；当前 Phase II 的 canonical MemoryEventStore 保存批准事件与 Provider outbox，MemoryManager 聚合本机 Mem0 和可选 Graphiti。记忆检索可以影响本地 Agent 的回答、情绪与动作建议，但动作仍必须走 `BehaviorProposal → BehaviorPlanner → BehaviorIntent`。当前对话链路不允许定义或注册远程 LLM Provider。

## 优先级与抢占

```text
System Physical > User Interaction > Audio Sync > AI Behavior > Ambient Idle
```

优先级只由 BehaviorPolicy 定义，不能由 LLM 在响应中自行指定。LLM 给出的 intensity 只能影响已经允许的表现幅度，不能提升权限或优先级。

---
# 84. 最终运行时所有权

```text
Desktop Companion
│
├── CompanionOrchestrator
│   ├── 创建与释放 Engine facade
│   ├── 路由跨 Engine 事件
│   └── 不保存 Engine 内部业务状态
│
├── DesktopEngine
│   ├── DesktopWorld
│   ├── Native Character State
│   ├── Window / Monitor / UIA
│   └── Gravity / Collision / Drag
│
├── Behavior Runtime
│   ├── BehaviorPlanner
│   ├── BehaviorScheduler
│   ├── BehaviorPolicy
│   └── MotionCatalog 只读查询
│
├── CharacterEngine
│   ├── PMX / Bone / Morph / IK
│   ├── MotionController
│   ├── Blink / LookAt
│   ├── Material / Renderer
│   └── LipSyncTarget
│
├── AudioEngine
│   ├── TTS / STT Provider
│   ├── SFX / Playback
│   ├── Audio Session
│   └── LipSyncFrame / AudioEvent
│
└── BrainEngine
    ├── ConversationOrchestrator
    ├── LocalLlmProvider（本机 LLM API）
    ├── ConversationSessionStore（本地事实源与 outbox）
    ├── MemoryPort
    ├── Mem0Memory（Phase I）
    ├── MemoryManager / MemoryPolicy / Graphiti（Phase II）
    ├── VisionProvider
    └── CharacterResponse / BehaviorProposal
```

所有权规则：

```text
一个 Engine 只能修改自己拥有的状态
跨 Engine 数据默认不可变
所有长生命周期任务必须可取消
所有订阅必须由创建者负责释放
所有外部 Provider 输出进入系统前必须校验
```

---
# 85. 最重要的开发原则

整个项目开发过程中，始终遵守这几个原则。

### 原则一

```text
Windows世界
≠
Three.js世界
```

Rust 管 Windows。

Three.js 管角色。

---

### 原则二

```text
MMD Physics
≠
Desktop Physics
```

MMD Physics：

```text
Hair
Clothes
Accessories
```

Desktop Physics：

```text
Gravity
Window
Taskbar
Desktop
```

---

### 原则三

BrainEngine 可以选择语义动作，但不能直接控制底层系统：

```text
BrainEngine
    ↓ BehaviorProposal
Schema / Safety Validation
    ↓
BehaviorPlanner
    ↓ BehaviorIntent
CharacterEngine / DesktopEngine / AudioEngine
```

`BehaviorProposal` 没有执行权；只有经过策略校验的 `BehaviorIntent` 才能进入执行层。

---

### 原则四

PMX Runtime 必须抽象。

不要：

```text
业务代码
↓
@moeru/three-mmd
```

应该：

```text
业务代码
↓
MmdRuntime
↓
@moeru/three-mmd
```

---

### 原则五

桌面坐标必须从第一天支持：

```text
负坐标
多显示器
DPI
```

否则后面一定重构。

---

# 86. MVP Definition of Done

第一版真正完成的标准不是：

```text
PMX能显示
```

而是下面全部完成：

```text
[ ] EXE可以启动

[ ] 无边框

[ ] 无标题栏

[ ] 透明背景

[ ] 任务栏不出现角色窗口

[ ] PMX正确加载

[ ] Texture正确

[ ] Morph正确

[ ] Physics正确

[ ] 自动Blink

[ ] LookAt

[ ] 角色可点击

[ ] 透明区域桌面可以正常点击

[ ] 角色可以拖动

[ ] 支持多个显示器

[ ] 松手可以掉落

[ ] 可以落在桌面

[ ] 可以获取Windows窗口

[ ] 可以站在Windows窗口顶部

[ ] Chrome移动后角色行为正常

[ ] 应用退出时无残留窗口

[ ] CPU/GPU占用合理
```

只有达到这里，我才建议按顺序进入：

```text
Behavior Runtime
↓
AudioEngine
↓
BrainEngine
```

---

# 87. 第二版目标

MVP 后：

```text
VMD Motion Library
Walking
Sitting
Sleeping
Window Follow
Taskbar Interaction
Speech Bubble
TTS
LipSync
Emotion
LLM
```

---

# 88. 第三版目标

再加入：

```text
UI Automation
Desktop Context
Application Awareness
Long-term Memory
Computer Vision
Agent Tools
Plugin System
Multiple Characters
Character Importer
```

---

# 89. 推荐的最终产品形态

最终软件采用“一个应用编排层 + 四个职责域”：

```text
                         Desktop Companion
                    CompanionOrchestrator
                              │
       ┌──────────────┬──────────────┬──────────────┐
       ▼              ▼              ▼              ▼
DesktopEngine  CharacterEngine  AudioEngine    BrainEngine
    Rust          Three.js         Rust           Rust
Windows/UIA      PMX/Motion      TTS/SFX       Agent/Memory
Physics/Window   Morph/Render    LipSync        BehaviorProposal
       │              ▲              │              │
       └──── events ──┴──────────────┴──────► BehaviorPlanner
                                                   │
                                      BehaviorIntent / SpeechIntent
                                                   │
                         ┌─────────────────────────┼──────────────┐
                         ▼                         ▼              ▼
                  DesktopEngine            CharacterEngine AudioEngine
```

四个职责域解耦以后，可以分别替换实现：

```text
PMX → VRM
Three.js → 其他 Renderer
Web Speech → 原生/云端 TTS
LocalLlmProvider → 未来其他本机 LLM 实现
Mem0Memory → 未来其他本机长期记忆实现
本机 Graphiti → 未来其他本机 Temporal Graph 实现
```

替换某一个实现时，其他 Engine 不应发生结构性修改。真正稳定的是 Engine port 和数据契约，而不是某个目录名或第三方库。

BrainEngine 内部的 AI、Memory、Vision 分别通过稳定 port 插拔；AudioEngine 的 TTS/STT 同样通过 Provider trait 插拔。Phase I 基础链由 Python `ConversationOrchestrator` 组合 `ConversationSessionStore` 与 `MemoryPort`；当前 Phase II 已由 `MemoryManager` 并列聚合 Mem0Provider 与可选 GraphitiProvider。Provider 之间不互相依赖；它们共享的是安全存储、错误模型和生命周期规范，不是彼此的内部状态。

## 架构不变量

```text
1. DesktopEngine 是 Windows 与原生窗口状态的唯一写入者
2. CharacterEngine 是 PMX 骨骼、Morph 和渲染状态的唯一写入者
3. AudioEngine 是活动播放会话的唯一所有者
4. BrainEngine 只生成语义 Proposal，不拥有执行权限
5. BehaviorPlanner 是 Proposal 进入执行层的唯一入口
6. CompanionOrchestrator 只组合和路由，不吞并各 Engine 业务逻辑
7. Phase I 长期记忆写入必须经过敏感内容过滤和 outbox；Phase II 多 Provider 写入必须统一经过 MemoryPolicy
8. SessionStore 是可审计、可删除的本地事实源，派生记忆索引必须可在本机重建
9. 所有记忆数据、索引、缓存、日志和备份只能存放在本机
10. 记忆提取、归并、embedding 与重排只能使用本机 API，禁止远程 fallback
11. 当前版本不定义或注册远程 LLM Provider；所有 LLM 与 Embedding API 只能使用回环地址
```

---
# 90. 当前技术决策总结

本项目第一版正式采用：

```text
Tauri 2
+
Rust
+
windows-rs
+
Vite
+
TypeScript
+
Three.js
+
@moeru/three-mmd
+
Ammo MMD Physics
+
Win32
+
DWM
```

窗口策略：

```text
Character Local Transparent Window
```

而不是：

```text
Fullscreen Overlay
```

桌面 Physics：

```text
Rust
```

角色 Physics：

```text
MMD Runtime
```

角色位置：

```text
Native Desktop Coordinate
```

而不是：

```text
Three.js World Position
```

鼠标穿透：

```text
GetCursorPos
+
Hit Region
+
setIgnoreCursorEvents
```

Windows 感知：

```text
EnumWindows
+
DwmGetWindowAttribute
+
SetWinEventHook
```

UI 感知：

```text
Windows UI Automation
```

应用编排：

```text
CompanionOrchestrator 作为 Composition Root
+
只负责生命周期、事件路由和跨 Engine 工作流
+
不实现 Desktop / Character / Brain / Audio 的内部业务
```

行为与动作：

```text
MotionCatalog（稳定语义动作 ID）
+
BehaviorProposal → BehaviorPlanner → BehaviorIntent
+
System Physical > User > Audio > AI > Idle
+
AI 只能从用户启用的 allow-list 建议动作
```

AudioEngine：

```text
TtsProvider / SttProvider
+
AudioController / SFX / Playback
+
AudioEvent / LipSyncFrame
+
AudioEngine 不直接修改 PMX Morph
```

AI 接入方式：

```text
Rust BrainSupervisor / BrainClient
+
Python LocalLlmProvider
+
本机 OpenAI-compatible loopback API
+
无 API Key、无代理、无重定向、无云端 fallback
```

记忆系统：

```text
Python ConversationSessionStore + MemoryPort
+
本地 SQLite（会话事实源与 outbox）
+
LocalLlmProvider + Mem0 本机推理（无远程 fallback）
+
Phase I 本机 Mem0Memory（Semantic + Episodic）
+
Phase II MemoryManager / MemoryPolicy + 本机 Graphiti（Temporal Graph）
+
user / character / session 作用域隔离
+
记忆、会话、Prompt 与派生上下文禁止离开本机
```

视觉能力：

```text
Rust 侧 VisionProvider trait（先定义接口，第一版不启用）
+
未来可复用支持多模态的本机 LLM Provider
+
显式权限开关 + 用户可感知的截屏提示，禁止后台静默截屏
```

密钥安全：

```text
keyring（Windows Credential Manager）
或 Tauri Stronghold
+
WebView 不持有任何密钥，也没有直接网络权限
```

---

# 91. 开发起点

实际编码时，请严格从以下顺序开始：

```text
01
Tauri transparent window

↓

02
Three.js transparent canvas

↓

03
PMX Runtime

↓

04
PMX model visible

↓

05
Morph + Blink

↓

06
Cursor + Hit Region

↓

07
Click Through

↓

08
Native Drag

↓

09
Desktop Coordinates

↓

10
Gravity

↓

11
EnumWindows

↓

12
Window Platform
```

到第 12 步以后，桌面伴侣的原生与视觉骨架成立。继续按依赖顺序接入：

```text
13
MotionCatalog + BehaviorPlanner

↓

14
AudioController + LipSyncFrame

↓

15
ConversationOrchestrator + LocalLlmProvider
```

进入第 15 步以前必须能够证明：

```text
AgentResponse 不能直接到达 MotionController
AI 只能看到 AvailableAction[]
BehaviorProposal 可以被拒绝或延迟
Falling / Dragged / Landing 拥有系统最高优先级
AudioEngine 结束或取消时嘴型一定归零
```

到第 12 步：

> 软件不再只是 PMX Viewer，而是真正成为 Windows Desktop Companion。

到第 15 步：

> 软件才成为具备可控语义行为、声音和 AI 决策能力的 Desktop Companion；智能能力没有破坏 Desktop 与 Character 的底层所有权。
