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
| TTS | 独立 Speech Adapter |

需要注意一个当前技术变化：

Three.js 在 r170 已经将原来的 MMD 模块标记为 deprecated，因此项目不要把旧版 Three.js `MMDLoader` 直接写死在业务代码里。现在推荐把 MMD 放在独立 `MmdRuntime` 抽象层。目前 `@moeru/three-mmd` 仍在维护，并提供 PMX/MMD runtime、动画、toon material 以及独立 Ammo 物理插件。

同样的抽象原则也适用于 AI / Memory / Vision：业务代码只依赖 `AgentProvider` / `MemoryProvider` / `VisionProvider` 三个 trait，不直接依赖某一个模型服务商 SDK 或某一个具体记忆框架。详见「AI Layer」「Memory Interface」「Vision Interface」章节。

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

最终建议：

```text
┌───────────────────────────────────────────────┐
│             Desktop Companion                │
│                                               │
│                Tauri Shell                    │
├───────────────────────────────────────────────┤
│                                               │
│              Rust Native Core                 │
│                                               │
│ ┌───────────────────────────────────────────┐ │
│ │ Windows Runtime                           │ │
│ │                                           │ │
│ │ MonitorManager                            │ │
│ │ WindowManager                             │ │
│ │ CursorManager                             │ │
│ │ HitTestManager                            │ │
│ │ DesktopWorld                              │ │
│ │ UIAutomationService                       │ │
│ └───────────────────────────────────────────┘ │
│                                               │
│ ┌───────────────────────────────────────────┐ │
│ │ Character Host                            │ │
│ │                                           │ │
│ │ DesktopPosition                           │ │
│ │ Velocity                                  │ │
│ │ Gravity                                   │ │
│ │ Drag                                      │ │
│ │ Collision                                 │ │
│ │ SupportSurface                            │ │
│ └───────────────────────────────────────────┘ │
│                                               │
├───────────────────────────────────────────────┤
│              Tauri IPC / Events               │
├───────────────────────────────────────────────┤
│                                               │
│           TypeScript Character Runtime        │
│                                               │
│ ┌───────────────────────────────────────────┐ │
│ │ Three.js                                  │ │
│ │                                           │ │
│ │ Scene                                     │ │
│ │ Camera                                    │ │
│ │ Renderer                                  │ │
│ │ PMX                                       │ │
│ │ Skeleton                                  │ │
│ │ Morph                                     │ │
│ │ MMD Physics                               │ │
│ └───────────────────────────────────────────┘ │
│                                               │
│ ┌───────────────────────────────────────────┐ │
│ │ Animation Runtime                         │ │
│ │                                           │ │
│ │ Idle                                      │ │
│ │ Walk                                      │ │
│ │ Sit                                       │ │
│ │ Fall                                      │ │
│ │ Drag                                      │ │
│ │ LookAt                                    │ │
│ │ Blink                                     │ │
│ │ LipSync                                   │ │
│ └───────────────────────────────────────────┘ │
│                                               │
├───────────────────────────────────────────────┤
│               Intelligence                    │
│                                               │
│ LLM → Emotion → Behavior → Character          │
│                                               │
└───────────────────────────────────────────────┘
```

核心原则：

```text
Rust = Desktop World

Three.js = Character World
```

不要反过来。

---

# 5. Rust 与 Three.js 的职责边界

## Rust 负责

```text
Windows坐标
窗口位置
显示器
DPI
鼠标
拖拽
重力
桌面碰撞
窗口检测
前台应用
任务栏
UI Automation
角色窗口移动
角色Desktop位置
配置
持久化
```

## TypeScript / Three.js 负责

```text
PMX
Mesh
Skeleton
Morph
MMD Physics
动画
LookAt
Blink
LipSync
材质
Lighting
角色点击形状计算
```

## LLM 不允许直接控制

```text
Bone.rotation
Mesh.position
Window HWND
```

LLM 只能输出：

```json
{
  "action": "walk_to",
  "emotion": "happy",
  "speech": "我去那边看看。"
}
```

然后由 Behavior Layer 转换。

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

最终目录推荐：

```text
desktop-companion/
│
├── src/
│
│   ├── app/
│   │   ├── bootstrap.ts
│   │   └── lifecycle.ts
│
│   ├── renderer/
│   │   ├── CharacterRenderer.ts
│   │   ├── CameraController.ts
│   │   └── RenderLoop.ts
│
│   ├── character/
│   │   │
│   │   ├── CharacterRuntime.ts
│   │   ├── CharacterController.ts
│   │   ├── CharacterState.ts
│   │
│   │   ├── mmd/
│   │   │   ├── MmdRuntime.ts
│   │   │   ├── MoeruMmdRuntime.ts
│   │   │   ├── BoneMap.ts
│   │   │   ├── MorphMap.ts
│   │   │   └── ModelManifest.ts
│   │
│   │   ├── animation/
│   │   │   ├── AnimationController.ts
│   │   │   ├── IdleController.ts
│   │   │   ├── BlinkController.ts
│   │   │   ├── LookAtController.ts
│   │   │   └── LipSyncController.ts
│   │
│   │   └── interaction/
│   │       ├── HitRegionController.ts
│   │       └── PointerController.ts
│
│   ├── desktop/
│   │   ├── DesktopBridge.ts
│   │   └── DesktopTypes.ts
│
│   ├── ipc/
│   │   ├── commands.ts
│   │   ├── events.ts
│   │   └── schemas.ts
│
│   ├── ui/
│   │   ├── SpeechBubble.ts
│   │   ├── ContextMenu.ts
│   │   └── SettingsPanel.ts        // Provider 配置界面（不持有密钥，只调用 command）
│
│   ├── intelligence/
│   │   │
│   │   │   // 前端只做 IPC 客户端 + UI 呈现，不直接持有任何 API Key，
│   │   │   // 也不直接发起对模型服务商的网络请求
│   │   │
│   │   ├── ConversationBridge.ts   // send_chat_message / ai:// 事件订阅
│   │   ├── BehaviorPlanner.ts      // AgentResponse → Character 行为意图
│   │   └── EmotionController.ts
│   │
│   └── main.ts
│
├── src-tauri/
│
│   ├── src/
│   │
│   │   ├── lib.rs
│   │
│   │   ├── commands/
│   │   │   ├── mod.rs
│   │   │   ├── character.rs
│   │   │   ├── desktop.rs
│   │   │   ├── models.rs
│   │   │   └── intelligence.rs     // send_chat_message / provider 相关 command
│   │
│   │   ├── windows/
│   │   │   ├── mod.rs
│   │   │   ├── character_window.rs
│   │   │   ├── monitor.rs
│   │   │   ├── cursor.rs
│   │   │   ├── enumeration.rs
│   │   │   ├── foreground.rs
│   │   │   └── events.rs
│   │
│   │   ├── desktop/
│   │   │   ├── mod.rs
│   │   │   ├── world.rs
│   │   │   ├── surface.rs
│   │   │   └── physics.rs
│   │
│   │   ├── character/
│   │   │   ├── mod.rs
│   │   │   ├── state.rs
│   │   │   ├── movement.rs
│   │   │   └── drag.rs
│   │
│   │   ├── automation/
│   │   │   ├── mod.rs
│   │   │   ├── uia.rs
│   │   │   └── element.rs
│   │
│   │   ├── ai/
│   │   │   ├── mod.rs
│   │   │   ├── provider.rs         // AgentProvider trait + AgentRequest/Response
│   │   │   ├── manager.rs          // AgentManager：注册/热切换 provider
│   │   │   ├── conversation.rs     // ConversationOrchestrator
│   │   │   ├── safety.rs           // Safety / Validation
│   │   │   ├── secrets.rs          // keyring / Stronghold 封装
│   │   │   └── providers/
│   │   │       ├── mod.rs
│   │   │       ├── openai_compatible.rs
│   │   │       ├── anthropic.rs
│   │   │       └── oauth.rs
│   │
│   │   ├── memory/
│   │   │   ├── mod.rs
│   │   │   ├── provider.rs         // MemoryProvider trait + 数据结构
│   │   │   ├── manager.rs
│   │   │   └── providers/
│   │   │       ├── mod.rs
│   │   │       ├── sqlite.rs       // 默认实现
│   │   │       └── remote.rs       // mem0 / Zep / Letta 等通用适配器
│   │
│   │   ├── vision/
│   │   │   ├── mod.rs
│   │   │   ├── provider.rs         // VisionProvider trait + 数据结构（先定义，不启用）
│   │   │   ├── manager.rs
│   │   │   ├── capture.rs          // Windows 截屏，受权限开关控制
│   │   │   └── providers/
│   │   │       ├── mod.rs
│   │   │       └── multimodal_agent.rs
│   │
│   │   ├── models/
│   │   │   ├── mod.rs
│   │   │   ├── import.rs
│   │   │   └── manifest.rs
│   │
│   │   └── storage/
│   │       ├── mod.rs
│   │       └── settings.rs
│   │
│   ├── capabilities/
│   │   └── default.json
│   │
│   ├── Cargo.toml
│   └── tauri.conf.json
│
└── package.json
```

> `ai/`、`memory/`、`vision/` 三个目录是本手册在基础架构之上新增的 **Brain Engine** 子模块，彼此之间只通过各自的 trait 交互，互不感知具体实现，方便后续替换模型服务商、替换记忆框架、或接入真实视觉模型。

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

建议定义：

```text
Idle
Walk
Sit
Fall
Land
Dragged
Talk
Sleep
Interact
```

状态机：

```text
                  IDLE
                   │
          ┌────────┼─────────┐
          ▼        ▼         ▼
        WALK      TALK     SLEEP
          │
          ▼
        FALL
          │
          ▼
        LAND
          │
          ▼
        IDLE

MouseDown
   │
   ▼
DRAGGED
   │
MouseUp
   │
   ├── Surface
   │      ↓
   │     LAND
   │
   └── Air
          ↓
         FALL
```

---

# 50. Behavior 与 Animation 分离

例如：

```text
Behavior:
WalkTo(Chrome)

        ↓

Navigation

        ↓

MovementIntent:
velocity = +80 px/s

        ↓

Rust Window Movement
```

与此同时：

```text
AnimationIntent:
walk
```

TS 播放走路动作。

---

# 51. VMD 后续加入

建议目录：

```text
assets/
└── motions/
    ├── idle/
    ├── walk/
    ├── sit/
    ├── sleep/
    ├── wave/
    ├── happy/
    └── special/
```

定义：

```json
{
  "id": "walk_01",

  "tags": [
    "walk",
    "forward",
    "female"
  ],

  "loop": true
}
```

以后 Agent 不选：

```text
walk_001.vmd
```

而选：

```text
Action = WALK
```

AnimationSelector 决定具体 VMD。

---

# 52. Speech Architecture

最终：

```text
LLM
 ↓
Text
 ↓
TTS
 ↓
Audio
 ├─────────────→ Speaker
 │
 ↓
LipSync
 ↓
Morph
```

---

# 53. Lip Sync

第一版不要上复杂音素模型。

直接：

```text
Audio RMS
 ↓
Mouth Open
```

即可。

第二版：

```text
TTS phoneme timestamps
 ↓
A I U E O
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

Emotion 不直接：

```text
Morph = 1
```

而经过：

```text
EmotionController
```

例如：

```json
{
  "emotion": "happy",
  "intensity": 0.72
}
```

映射：

```text
smile morph = 0.7
eyes = 0.2
head pose = positive
idle animation = happy idle
```

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
Frontend (对话输入 / 文本气泡)
      │  invoke("send_chat_message")
      ▼
Rust: ConversationOrchestrator
      │
      ├─ MemoryProvider.search()      ← 记忆检索（见 Memory Interface）
      ├─ DesktopContext（现有语义上下文）
      ├─ VisionProvider.describe()    ← 可选，用户显式触发时才调用
      │
      ▼
AgentProvider.chat(AgentRequest)      ← 真正调用 LLM
      │
      ▼
AgentResponse
      │
      ▼
Safety / Validation
      │
      ▼
BehaviorPlanner → Character Runtime
      │
      └─ MemoryProvider.add()         ← 写回记忆（异步，不阻塞角色响应）
```

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
pub struct AgentRequest {
    pub system_prompt: String,           // 人设 / persona
    pub history: Vec<ChatMessage>,       // 短期对话历史（来自 MemoryProvider）
    pub retrieved_memory: Vec<String>,   // 长期记忆检索结果（RAG）
    pub desktop_context: Option<DesktopContextJson>, // 语义化桌面上下文
    pub vision_context: Option<String>,  // 可选：VisionProvider 的描述结果
    pub user_input: String,
}

pub struct AgentResponse {
    pub speech: String,
    pub emotion: Option<EmotionIntent>,
    pub action: Option<ActionIntent>,
    pub memory_write: Vec<MemoryWriteIntent>, // LLM 想记住的内容，仍需校验
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
ai://response { speech, emotion, action }  // 完整结构化结果，驱动行为与表情
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

  "action": {
    "type": "wave"
  },

  "memory": {
    "remember": ["用户今天说他明天要考试"]
  }
}
```

`memory.remember` 只是**意图**，最终是否写入、如何写入，由 Safety / Validation 与 `MemoryProvider` 决定，LLM 不能直接写库。

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

AI / Memory / Vision 三类 Provider 共用同一套配置与密钥管理方式，保持一致，避免每个模块各写一套设置逻辑。

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

这与「Desktop Engine / Character Engine / Brain Engine」三引擎解耦的思路一致：AI、记忆、视觉是 Brain Engine 内部三个独立可插拔的子模块，互不锁定。

---

# 59. 不允许 LLM 输出

```text
ExecutePowerShell
DeleteFile
ClickAnything
MoveMouse
```

Desktop Awareness 和 Desktop Control 必须分开。

第一阶段：

```text
Read-only Awareness
```

后续如果做 Agent 操作电脑：

```text
显式授权
+
Action Validation
+
危险操作确认
```

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

角色本身不出现菜单栏。

建议：

```text
Character

Right Click
     ↓

Small Context Menu

聊天
设置
角色
退出
```

设置页面：

```text
单独 Tauri Window
```

只有需要时出现。

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

> 注意区分"暂时不实现"与"不预留接口"。`AgentProvider` / `MemoryProvider` / `VisionProvider` 三个 trait 建议在 Phase 0（工程骨架）阶段就定义好签名（不需要给出真实实现，可以先用一个什么都不做的 Stub 实现占位），这样后面接入 LLM、记忆、视觉时只是新增一个 Provider 实现并注册进对应的 Manager，不需要回头重构 Character Runtime、IPC 层或 Capability 配置。

---

# 71. Phase 0：工程骨架

目标：

```text
Tauri启动
+
透明窗口
+
Three Canvas
```

验收：

```text
启动EXE
看不到背景
看不到边框
看不到标题栏
任务栏无图标
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

# 78. Phase 7：完整行为状态机

加入：

```text
Idle
Walk
Fall
Land
Sit
Sleep
Dragged
```

验收：

```text
动作切换无明显跳帧
状态不会互相冲突
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

# 80. Phase 9：Speech

完成：

```text
TTS
Audio
LipSync
Speech Bubble
```

验收：

角色：

```text
说话
+
嘴型
+
气泡
```

---

# 81. Phase 10：LLM

完成：

```text
AgentProvider 至少一个可用实现（推荐先做 OpenAICompatibleProvider）
MemoryProvider 默认实现（SqliteMemoryProvider）
ConversationOrchestrator
Safety / Validation
Behavior Intent
```

验收：

```text
角色可以进行多轮对话
对话具备短期上下文（重启会话后不丢失最近几轮）
可以在设置里切换/更换 Provider 而不重启角色
LLM 输出经 Safety 校验后才会驱动 emotion / action
用户可以清空/删除记忆
```

`VisionProvider` 接口保持已定义、默认关闭的状态，留给后续版本接入真实视觉模型，不在本 Phase 范围内。

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

# 83. 第一版核心数据流

```text
                    Windows
                       │
              GetCursorPos / DWM
                       │
                       ▼
                 Rust Native
                       │
               DesktopWorld
                       │
              CharacterPhysics
                       │
                       ▼
                 Tauri Window
                       │
               Tauri Events
                       │
                       ▼
                  TypeScript
                       │
                Animation FSM
                       │
                       ▼
                  Three.js
                       │
                       ▼
                     PMX
```

点击：

```text
Global Cursor
     │
     ▼
Rust HitTest
     │
 ┌───┴────┐
 │        │
Character Transparent
 │        │
 ▼        ▼
input    setIgnoreCursorEvents(true)
```

---

# 84. 最终角色运行结构

```text
Character
│
├── Native Body
│
│   ├── Desktop Position
│   ├── Velocity
│   ├── Gravity
│   ├── Collision
│   └── Support Surface
│
├── Visual Body
│
│   ├── PMX
│   ├── Bone
│   ├── Morph
│   ├── IK
│   └── MMD Physics
│
├── Animation
│
│   ├── Idle
│   ├── Walk
│   ├── Sit
│   ├── Blink
│   ├── LookAt
│   └── LipSync
│
└── Brain
    │
    ├── Emotion
    ├── Behavior
    ├── Conversation
    └── Memory
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

```text
LLM
不能直接控制
底层系统
```

只能：

```text
LLM
 ↓
Intent
 ↓
Behavior Planner
 ↓
Character
```

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

只有达到这里，我才建议正式进入：

```text
TTS + AI
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

最终软件实际上会形成三个 Engine：

```text
            Desktop Companion

                   │
       ┌───────────┼────────────┐
       │           │            │
       ▼           ▼            ▼

 Desktop Engine Character Engine Brain Engine

      Rust        Three.js       Agent
       │             │             │
   Windows           PMX      AgentProvider
   Physics        Animation   MemoryProvider
   UIA             Morph      VisionProvider
       │             │             │
       └─────────────┼─────────────┘
                     │
                     ▼
                  Character
```

这三个 Engine 解耦之后，以后即使：

```text
PMX → VRM
```

或者：

```text
Three.js → 其他Renderer
```

Desktop Core 和 Agent Core 都不需要推翻。

Brain Engine 内部同样遵循这个解耦原则：AI、记忆、视觉分别对应 `AgentProvider` / `MemoryProvider` / `VisionProvider` 三个独立 trait，替换其中任意一个（例如更换模型服务商、把记忆换成 mem0、接入真实视觉模型）都不需要改动其余两个，也不需要改动 Desktop Engine 或 Character Engine。

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

到第 12 步以后：

> 你的软件就已经不再只是“PMX Viewer”，而是真正开始成为一个 Windows Desktop Companion。