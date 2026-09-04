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
| AI | Rust trait `AgentProvider`（OpenAI 兼容 API Key / OAuth，运行时可切换） |
| Memory | Rust trait `MemoryProvider`（默认 SQLite，可替换为 mem0 / Zep / Letta 等） |
| Vision（预留） | Rust trait `VisionProvider`（接口先行，默认不启用） |
| Audio Engine | `TtsProvider` + SFX + Playback + LipSync 帧契约（Provider 与密钥在 Rust 侧） |

需要注意一个当前技术变化：

Three.js 在 r170 已经将原来的 MMD 模块标记为 deprecated，因此项目不要把旧版 Three.js `MMDLoader` 直接写死在业务代码里。现在推荐把 MMD 放在独立 `MmdRuntime` 抽象层。目前 `@moeru/three-mmd` 仍在维护，并提供 PMX/MMD runtime、动画、toon material 以及独立 Ammo 物理插件。

同样的抽象原则也适用于 BrainEngine 与 AudioEngine：业务代码只依赖 `AgentProvider` / `MemoryProvider` / `VisionProvider` / `TtsProvider` 等稳定 port，不直接依赖某个模型、记忆或语音服务商。跨 Engine 的工作流由 CompanionOrchestrator 组合；动作建议统一经过 BehaviorPlanner。详见“AI Layer”“Memory Interface”“Vision Interface”和“AudioEngine Architecture”章节。

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
          ┌───────────────────┼───────────────────┐
          ▼                   ▼                   ▼
   DesktopEngine       CharacterEngine       BrainEngine
       Rust               Three.js              Rust
   Windows/Physics      PMX/Animation       LLM/Memory/Vision
   Window/UIA           Morph/LookAt        BehaviorProposal
          │                   ▲                   │
          │                   │                   │
          └── DesktopEvent ───┼── BehaviorIntent ┘
                              │
                        BehaviorPlanner
                    （校验、仲裁、调度、降级）
                              │
               ┌──────────────┴──────────────┐
               ▼                             ▼
        CharacterEngine                 AudioEngine
        Motion / Emotion          TTS / SFX / Playback
               ▲                             │
               └──── LipSyncFrame / AudioEvent
```

`CompanionOrchestrator` 是应用层编排器，不是第五个业务 Engine。它负责组合各 Engine、订阅事件、转发契约和释放资源，但不实现 Windows 物理、PMX 动画、LLM 请求或音频解码。

`ConversationOrchestrator` 则是 BrainEngine 内部的会话编排器，负责 Memory、DesktopContext、可选 Vision 与 AgentProvider 的调用。两者不要混为一个巨型类：

```text
CompanionOrchestrator     → 跨 Engine 的应用工作流
ConversationOrchestrator  → BrainEngine 内的一次对话工作流
```

核心数据契约：

```text
DesktopEngine   → DesktopEvent / SemanticDesktopContext
BrainEngine     → AgentResponse / BehaviorProposal
BehaviorPlanner → BehaviorIntent / SpeechIntent
AudioEngine     → AudioEvent / LipSyncFrame
CharacterEngine → CharacterEvent / VisualState
```

禁止通过共享可变对象跨 Engine 操作内部状态。跨边界只能传递可验证的数据契约或调用窄接口。

核心原则仍然是：

```text
Rust = Windows 世界 + 安全 Provider 主机
Three.js = 角色视觉世界
BrainEngine = 语义决策
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

## BrainEngine（Rust）负责

```text
AgentProvider
ConversationOrchestrator
MemoryProvider
VisionProvider（可选）
语义桌面上下文理解
生成 speech / emotion / action 建议
```

BrainEngine 可以根据场景选择一个**语义动作 ID**，但输出的是 `BehaviorProposal`，不是已经获得执行权的命令。

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
│   ├── brain/                         // BrainEngine
│   │   ├── conversation.rs            // ConversationOrchestrator
│   │   ├── proposal.rs                // BehaviorProposal / EmotionProposal
│   │   ├── validation.rs              // 结构、范围、权限与记忆写入校验
│   │   ├── secrets.rs
│   │   ├── ai/
│   │   │   ├── provider.rs
│   │   │   ├── manager.rs
│   │   │   └── providers/
│   │   ├── memory/
│   │   │   ├── provider.rs
│   │   │   ├── manager.rs
│   │   │   └── providers/
│   │   └── vision/
│   │       ├── provider.rs
│   │       ├── manager.rs
│   │       └── capture.rs
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
| `MotionController.ts` + `MotionCatalog.ts` | 动作执行与动作目录 | 保持模型实现细节，不接收 AgentResponse 原始 JSON |
| `src/speech/SpeechController.ts` | AudioEngine 语音原型 | 逐步由 `AudioController` 统一 TTS、SFX、打断和播放会话 |
| Rust `ai/`、`memory/`、`vision/` | BrainEngine Provider 契约 | 增加 Manager、ConversationOrchestrator 与结构化校验 |
| Rust `speech/provider.rs` | AudioEngine Provider 契约 | 增加 Provider Manager、受控播放和 RMS/Viseme 输出 |

当前仍缺少、且应优先于真实 LLM 接入的组件：

```text
BehaviorTypes
BehaviorPlanner
BehaviorScheduler
BehaviorPolicy
CompanionOrchestrator facade
BrainBridge / ConversationOrchestrator
AudioController / SFX / 原生或受控播放适配器
```

迁移时先增加 facade 和契约，再移动目录；不要把“大规模改路径”与“改变运行行为”放在同一个提交中。

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

Intelligence 相关 command 同样遵循 semantic 风格，并且全部在 Rust 侧完成实际网络请求（详见「AI Layer」「Memory Interface」「Vision Interface」章节）：

```text
send_chat_message

list_ai_providers
set_ai_provider
test_ai_provider
set_provider_credentials
clear_provider_credentials
get_provider_summary

list_memory_providers
set_memory_provider
clear_memory

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

Intelligence 相关事件单独归类为 `ai://` 前缀，驱动对话气泡与"思考中"状态，不驱动骨骼：

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
          校验 → 仲裁 → 调度 → 生成 BehaviorIntent
                         │
                         ▼
                  BehaviorScheduler
              抢占 / 排队 / 冷却 / 恢复
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

最终数据流：

```text
Frontend（对话输入 / 文本气泡）
      │  invoke("send_chat_message")
      ▼
Rust: ConversationOrchestrator
      ├─ MemoryProvider.search()       ← 记忆检索
      ├─ SemanticDesktopContext        ← 只有语义事实，不暴露 HWND
      ├─ VisionProvider.describe()     ← 可选且必须显式授权
      └─ AvailableAction[]             ← 当前允许 AI 建议的动作摘要
      │
      ▼
AgentProvider.chat(AgentRequest)
      │
      ▼
AgentResponse（仍是不可信外部输出）
      │
      ▼
Schema / Safety / Range Validation
      │
      ▼
BrainResult
      ├─ speech ──────────────────────► AudioEngine
      ├─ behaviorProposal ────────────► BehaviorPlanner
      ├─ emotionProposal ─────────────► BehaviorPlanner
      └─ memoryWriteProposal ─────────► MemoryPolicy → MemoryProvider.add()
                                             │
                                             ▼
                                      BehaviorIntent
                                             │
                                             ▼
                                      CharacterEngine
```

`AvailableAction[]` 只包含动作 ID、描述和场景标签，来源于 MotionCatalog 与用户配置。它不包含本地文件路径、VMD 文件名、骨骼名称或 Morph 参数。这样 AI 能根据场景选择动作，但无法越过动作目录和执行策略。

## AgentProvider：模型服务商抽象

不允许业务代码直接依赖某一个 SDK（例如 `async-openai`），而是定义统一 trait，所有服务商都实现它：

```rust
#[async_trait::async_trait]
pub trait AgentProvider: Send + Sync {
    /// Provider 的唯一标识，例如 "openai" / "azure-openai" / "ollama" / "deepseek"
    fn id(&self) -> &str;

    /// 是否支持多模态输入（图片），供 VisionProvider 复用同一个 Provider
    fn supports_vision(&self) -> bool { false }

    /// 一次性返回完整结果
    async fn chat(&self, request: AgentRequest) -> Result<AgentResponse, AgentError>;

    /// 流式返回，用于气泡逐字显示；默认可以用 chat() 模拟（一次性 yield 一个 chunk）
    async fn chat_stream(
        &self,
        request: AgentRequest,
        on_delta: Box<dyn Fn(String) + Send>,
    ) -> Result<AgentResponse, AgentError> {
        let response = self.chat(request).await?;
        on_delta(response.speech.clone());
        Ok(response)
    }
}
```

请求 / 响应契约（跨所有 provider 统一）：

```rust
pub struct AvailableAction {
    pub id: String,
    pub description: String,
    pub scenes: Vec<String>,
}

pub struct AgentRequest {
    pub system_prompt: String,
    pub history: Vec<ChatMessage>,
    pub retrieved_memory: Vec<String>,
    pub desktop_context: Option<DesktopContextJson>,
    pub vision_context: Option<String>,
    pub available_actions: Vec<AvailableAction>, // 只给 AI 当前 allow-list
    pub user_input: String,
}

pub struct AgentResponse {
    pub speech: String,
    pub emotion: Option<EmotionProposal>,
    pub behavior: Option<BehaviorProposal>,
    pub memory_write: Vec<MemoryWriteProposal>,
}

pub struct BehaviorProposal {
    pub action_id: String,
    pub intensity: Option<f32>,
    pub reason: Option<String>, // 仅供诊断，不参与底层执行
}
```

具体实现只需要把这套统一结构翻译成各家 API 的请求体：

```text
OpenAICompatibleProvider   → OpenAI / Azure OpenAI / DeepSeek / Moonshot /
                              SiliconFlow / Groq / Ollama / LM Studio 等
                              （只要是 /v1/chat/completions 兼容端点）
AnthropicProvider          → Claude Messages API
OAuthAgentProvider         → 包装以上任意 Provider，附加 OAuth Token 刷新
```

`OpenAICompatibleProvider` 覆盖了绝大多数场景：用户只需要在设置里填 `base_url` + `api_key` + `model`，就可以接入任意兼容服务，**不需要为每一个模型服务商单独写 Provider**。

## AgentManager：运行时可热切换

```rust
pub struct AgentManager {
    providers: HashMap<String, Arc<dyn AgentProvider>>,
    active: RwLock<String>,
}

impl AgentManager {
    pub fn active(&self) -> Arc<dyn AgentProvider> { /* ... */ }
    pub fn set_active(&self, provider_id: &str) -> Result<(), AgentError> { /* ... */ }
    pub fn register(&mut self, provider: Arc<dyn AgentProvider>) { /* ... */ }
}
```

对应 Tauri command：

```text
list_ai_providers
set_ai_provider(providerId)
test_ai_provider(providerId)   // 发一条测试消息，验证 key / base_url 是否可用
```

切换 provider 不需要重启角色，`ConversationOrchestrator` 永远只持有 `AgentManager`，不关心当前具体是哪一家。

## 流式响应事件

```text
ai://thinking          // 开始请求，前端可显示"思考中"动画
ai://delta   { text }  // 增量文本，驱动气泡逐字显示
ai://response { speech, emotionProposal, behaviorProposal } // 经 Rust 校验的语义建议
ai://error   { message, providerId }
```

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
  },
  "memoryWrite": ["用户今天说他明天要考试"]
}
```

上面的 `behavior` 只是 `BehaviorProposal`。BehaviorPlanner 必须再次检查 MotionCatalog、用户启用项、当前物理状态、优先级与冷却时间，批准后才生成 `BehaviorIntent`。

`memoryWrite` 同样只是建议，最终是否写入及如何写入由 MemoryPolicy 与 `MemoryProvider` 决定。LLM 不能直接写库。

---

# 56. Memory Interface（记忆系统接口）

记忆系统**不从零造轮子**，也**不写死某一个具体实现**。项目只约定一套 trait 与数据结构，具体存储 / 检索方式可以随时替换：先用最简单的本地 SQLite 实现跑通，以后可以换成向量检索、也可以换成 mem0 / Zep / Letta（MemGPT）之类的开源记忆框架，Conversation 与 Character 层完全不需要改动。

## 记忆的两个层级

```text
Short-term（会话记忆）
  最近若干轮对话，随会话保留，用于维持上下文连贯
  存储成本低，随时可以整体丢弃

Long-term（长期记忆）
  被显式判定为"值得记住"的内容（用户偏好、重要事件、人物关系等）
  持久化保存，每次对话前做检索（RAG），而不是整份塞进 prompt
```

## MemoryProvider trait

```rust
#[async_trait::async_trait]
pub trait MemoryProvider: Send + Sync {
    fn id(&self) -> &str;

    /// 写入一条记忆（短期或长期，由 entry.kind 区分）
    async fn add(&self, entry: MemoryEntry) -> Result<MemoryId, MemoryError>;

    /// 语义 / 关键字检索，返回最相关的若干条
    async fn search(&self, query: MemoryQuery) -> Result<Vec<MemoryRecord>, MemoryError>;

    /// 获取某个会话最近 N 条短期记忆（不需要检索，直接按时间取）
    async fn get_recent(&self, session_id: &str, limit: usize) -> Result<Vec<MemoryRecord>, MemoryError>;

    /// 删除 / 遗忘
    async fn forget(&self, id: MemoryId) -> Result<(), MemoryError>;
}
```

数据结构：

```rust
pub struct MemoryEntry {
    pub session_id: String,
    pub kind: MemoryKind,       // ShortTerm | LongTerm
    pub role: String,           // "user" | "assistant" | "system"
    pub content: String,
    pub importance: f32,        // 0.0 ~ 1.0，供未来做遗忘 / 排序
    pub tags: Vec<String>,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

pub struct MemoryQuery {
    pub session_id: Option<String>,
    pub text: String,
    pub top_k: usize,
    pub filters: Vec<(String, String)>,
}

pub struct MemoryRecord {
    pub id: MemoryId,
    pub entry: MemoryEntry,
    pub score: f32,
}
```

## 默认实现与可替换实现

```text
SqliteMemoryProvider   （默认，随项目自带）
  - 复用已确定的 SQLite 存储
  - 第一版用"最近时间 + 关键字"做检索，足够覆盖 MVP 之后的第一版 AI 功能
  - 后续可以给同一张表加一列 embedding，用 sqlite-vec / usearch 之类的
    本地向量扩展做语义检索——这只是 SqliteMemoryProvider 内部实现变化，
    trait 与调用方完全不受影响

RemoteMemoryProvider  （可选，接第三方记忆服务）
  - 通用 HTTP 适配器：base_url + api_key，把 add / search / get_recent / forget
    映射到远端服务的 REST API
  - 可以用来接 mem0（OSS 或 Cloud）、Zep、Letta（原 MemGPT）等任意
    提供类似能力的开源 / 商业记忆框架
  - 只要实现同一个 trait，就可以在 Settings 里直接切换，不需要改
    ConversationOrchestrator 一行代码
```

## 调用位置

```text
对话开始前：
  ConversationOrchestrator
    → memory.get_recent(session_id, N)     // 短期上下文
    → memory.search(query = 用户输入)       // 长期记忆 RAG 检索
    → 拼进 AgentRequest.history / retrieved_memory

对话结束后：
  → memory.add(本轮 user + assistant 消息)          // 短期记忆，几乎总是写
  → 若 AgentResponse.memory_write 非空，经过 Safety
    校验后调用 memory.add(kind = LongTerm)           // 长期记忆，选择性写
```

## 目录与配置

```text
src-tauri/src/memory/
├── mod.rs
├── provider.rs        // MemoryProvider trait + 数据结构
├── manager.rs         // MemoryManager：持有当前 active provider
└── providers/
    ├── mod.rs
    ├── sqlite.rs       // 默认实现
    └── remote.rs       // 通用远端记忆服务适配器
```

对应 Tauri command：

```text
list_memory_providers
set_memory_provider(providerId)
clear_memory(sessionId)     // 用户可以随时清空/遗忘，必须提供
```

> 记忆是最容易牵扯隐私的模块：必须提供用户可见、可操作的"清空记忆" / "删除某条记忆"入口，不能只做成黑盒。

---

# 57. Vision Interface（VL / 视觉能力接口）

视觉能力**第一版不实现**，但接口与权限模型现在就定义好，避免以后接入视觉模型时重新设计权限系统。

## 设计目标

```text
现在：
  定义 trait + 数据结构 + 权限开关
  不接任何真实模型，也不做任何截屏

以后：
  接入任意一个多模态模型（GPT-4o / Claude / Qwen-VL / 本地 LLaVA via Ollama）
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

## 与 AgentProvider 的关系

多数多模态模型的图片输入其实走的是同一条 Chat API（把图片作为一条 message 内容），因此 `VisionProvider` 通常不需要独立实现，而是：

```text
OpenAICompatibleProvider 同时实现 AgentProvider 与 VisionProvider
（如果 supports_vision() = true）

VisionProvider.describe()
  = 构造一条包含图片的 AgentRequest
  → 复用同一个 HTTP client / 同一份密钥配置
  → 只是 prompt 与解析方式不同
```

只有当用户接入的是纯本地视觉模型（例如专门跑 LLaVA 的服务）时，才需要一个不依赖 AgentProvider 的独立实现。

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
    └── multimodal_agent.rs  // 包装某个 AgentProvider 实现 VisionProvider
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

AI / Memory / Vision 与云端 TTS Provider 共用同一套配置和密钥管理原则，避免每个模块各写一套安全逻辑。音色、音量等非敏感播放参数可以进入普通设置；API Key、OAuth Token 和云端语音凭据必须进入 OS 级安全存储。

## 两种接入方式

```text
1. API Key 模式（默认，覆盖绝大多数场景）
   用户在设置中填写：
     base_url
     api_key
     model
   适用：OpenAI、Azure OpenAI、DeepSeek、Moonshot、SiliconFlow、
        Groq、本地 Ollama / LM Studio 等任意 OpenAI 兼容端点

2. OAuth 模式（用于"用某账号登录"类服务）
   Authorization Code + PKCE：
     系统默认浏览器打开授权页
     本地回环 HTTP 服务器（或 Tauri deep-link）接收回调
     Token 存储与刷新在 Rust 侧完成，前端只知道"已登录 / 未登录"
```

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

AI 对话、记忆、视觉三者可以分别指向不同的服务商，例如：

```text
Chat  → DeepSeek（便宜、日常聊天）
Memory → 本地 SqliteMemoryProvider
Vision → GPT-4o（仅在用户主动触发时调用一次）
```

这与“统一编排层 + 四个职责域”的架构一致：AI、记忆、视觉是 BrainEngine 内部三个独立可插拔子模块；TTS 是 AudioEngine 的可插拔 Provider。它们共享安全规范，但不共享业务状态。

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

AI / Memory / Vision 的网络请求遵循同一原则：**只允许在 Rust 侧发起**，不属于 character window 的 capability。WebView 永远只通过语义化 command（例如 `send_chat_message`、`set_ai_provider`）与 Rust 通信，拿不到 API Key、OAuth Token，也拿不到原始网络权限。密钥的读写只能通过专门的 command 完成，command 只返回成功/失败或掩码后的展示信息（见「Provider 配置与密钥安全」章节）。

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
        └─ 动作配置
```

职责边界：

| 页面 | 保存内容 | 不允许做的事 |
|---|---|---|
| 助手模型 | 模型 ID、显示比例、色彩和非敏感连接参数 | 直接加载任意磁盘路径 |
| 声音配置 | 音色 ID、语言、音量、语速、音调 | 在 WebView 保存云端密钥 |
| 动作配置 | AI 动作开关、允许的语义动作 ID | 让 AI 直接填骨骼或 VMD 路径 |

模型和动作选择都依赖目录注册：

```text
CharacterCatalog → characterModelId
MotionCatalog    → enabledAiMotionIds
```

配置页只编辑设置和发出预览请求；模型加载、声音播放和动作执行仍由各自 Engine 完成。切换需要重载的资源时，界面必须明确提示“保存后下次启动生效”，不能伪装成已经热切换。

API Key、OAuth Token 和云端语音凭据必须通过 Rust command 写入安全存储，不能进入普通 CharacterSettings。

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
云同步
复杂设置UI
```

先做好：

```text
一个角色
真正活在Windows桌面上
```

> 注意区分“暂时不实现”与“不预留接口”。Phase 0 应先定义 `AgentProvider` / `MemoryProvider` / `VisionProvider` / `TtsProvider`、`BehaviorProposal` / `BehaviorIntent`、`LipSyncFrame` 和 Engine port，但只使用 Stub 或本地调试适配器。这样后续接入 LLM、记忆、真实语音和视觉时只需新增 Provider 与编排流程，不需要让 CharacterRuntime 反向依赖具体服务商。

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
AgentProvider / MemoryProvider / VisionProvider / TtsProvider Stub
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
# 81. Phase 10：BrainEngine

完成：

```text
AgentProvider 至少一个可用实现（推荐 OpenAICompatibleProvider）
AgentManager
MemoryProvider 默认实现（SqliteMemoryProvider）
ConversationOrchestrator
AvailableAction 注入
Schema / Safety / Range Validation
BehaviorProposal → BehaviorPlanner
SpeechIntent → AudioEngine
```

验收：

```text
角色可以进行多轮对话
对话具备短期上下文
可以在设置里热切换 Provider
LLM 只能从当前 MotionCatalog allow-list 建议 actionId
未知、禁用或状态冲突的动作被拒绝、延迟或替换
角色 Falling 时 AI Greeting 不会覆盖系统动作
LLM speech 进入 AudioEngine，而不是直接调用播放设备
用户可以清空或删除记忆
```

`VisionProvider` 保持已定义、默认关闭，留给后续显式授权的视觉功能。第一版 BrainEngine 不具备任意文件、Shell、鼠标或窗口控制能力。

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
    ├── AgentProvider
    ├── MemoryProvider
    ├── VisionProvider
    └── BehaviorProposal / MemoryWriteProposal
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
       ┌──────────────────────┼──────────────────────┐
       │                      │                      │
       ▼                      ▼                      ▼
 DesktopEngine          CharacterEngine         BrainEngine
     Rust                  Three.js                Rust
       │                      ▲                      │
       │ DesktopEvent         │ BehaviorIntent       │ BehaviorProposal
       └───────────────► BehaviorPlanner ◄───────────┘
                              │
                              ▼
                         AudioEngine
                     TTS / SFX / Playback
                              │
                       LipSyncFrame
                              │
                              ▼
                       CharacterEngine
```

四个职责域解耦以后，可以分别替换实现：

```text
PMX → VRM
Three.js → 其他 Renderer
Web Speech → 原生/云端 TTS
OpenAI Compatible → 其他 AgentProvider
SQLite Memory → mem0 / Zep / Letta
```

替换某一个实现时，其他 Engine 不应发生结构性修改。真正稳定的是 Engine port 和数据契约，而不是某个目录名或第三方库。

BrainEngine 内部的 AI、Memory、Vision 分别通过 Provider trait 插拔；AudioEngine 的 TTS/STT 同样通过 Provider trait 插拔。共享的是安全存储、错误模型和生命周期规范，不是彼此的内部状态。

## 架构不变量

```text
1. DesktopEngine 是 Windows 与原生窗口状态的唯一写入者
2. CharacterEngine 是 PMX 骨骼、Morph 和渲染状态的唯一写入者
3. AudioEngine 是活动播放会话的唯一所有者
4. BrainEngine 只生成语义 Proposal，不拥有执行权限
5. BehaviorPlanner 是 Proposal 进入执行层的唯一入口
6. CompanionOrchestrator 只组合和路由，不吞并各 Engine 业务逻辑
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
Rust 侧 AgentProvider trait
+
OpenAI 兼容 API Key 模式（默认，覆盖大多数服务商）
+
OAuth（可选，按需接入具体服务商）
+
运行时可热切换 Provider，不重启角色
```

记忆系统：

```text
Rust 侧 MemoryProvider trait
+
默认 SqliteMemoryProvider（短期上下文 + 长期记忆）
+
可整体替换为 mem0 / Zep / Letta（MemGPT）等开源记忆框架
```

视觉能力：

```text
Rust 侧 VisionProvider trait（先定义接口，第一版不启用）
+
默认可直接复用支持多模态的 AgentProvider
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
ConversationOrchestrator + AgentProvider
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