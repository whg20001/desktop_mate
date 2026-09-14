# Windows AI 桌面伴侣

基于开发手册搭建的 Windows 桌面伴侣核心骨架：Tauri 2 + Rust/Win32 负责桌面世界，Three.js + `@moeru/three-mmd` 负责陵光 PMX 角色世界。

## 当前能力

- 600×900 局部透明、无边框、无阴影、置顶且不显示任务栏按钮的 Character Window；
- 陵光 PMX、10 张纹理、21 个材质以及 Ammo 刚体物理加载；
- `MmdRuntime` 适配层，业务代码不直接依赖具体 PMX 引擎；
- 从模型实际骨骼投影生成头部、身体、手部命中区域；
- 全局光标驱动点击穿透与 LookAt，点击角色会触发问候动作、系统语音、说话口型和气泡；
- 统一程序化动作控制器生成 Idle、Greeting、Dragged、Falling、Landing 与 Talking，无需 VMD；
- 自动眨眼、可选呼吸（默认关闭）和方向正确的 LookAt；
- 双击角色打开独立的 4:3 配置中心，通过“助手模型 / 声音配置 / 动作配置”三页管理角色外观、语音参数、AI 可选动作及非敏感 API 预设；
- 原生窗口拖拽、Rust 重力、任务栏工作区地面和窗口顶部碰撞；
- Win32/DWM 窗口枚举，多显示器负坐标和 Per-Monitor DPI 数据；
- Windows 原生 TTS / Qwen3-TTS 本地模型切换、Rust/rodio 播放与实测 RMS 口型，支持音色配置、试听、取消、重载及退出清理；浏览器预览使用 Web Speech，STT 尚未启用；
- Rust `BrainSupervisor` 自动启动、鉴权、探活、重启并关闭 Python Brain Sidecar，Brain 故障不会阻止桌宠窗口启动；
- Python `ConversationOrchestrator` 提供多轮会话、本地 OpenAI-compatible LLM 与结构化情绪/动作建议；前端 `bootstrap.ts` 展示 Speech Bubble 并接通 AI 语音；
- SQLite `MemoryEventStore` 保存记忆事实、审批与 outbox，Mem0/Qdrant 和可选 Graphiti 为派生索引；支持后台提取、召回、修改、删除及崩溃后恢复；
- Brain、LLM、Embedding 与 Mem0 全链路仅允许回环地址；禁用代理、重定向、云端 Provider 与遥测。

## 开发环境

需要 Windows 10/11、WebView2、Node.js、pnpm、Rust 1.87+ MSVC（本次使用 1.93.1）、`uv`，以及 Visual Studio 2022 Build Tools 中的 **Desktop development with C++** 工作负载。不需要 Docker。

```powershell
pnpm install
uv sync --project brain-sidecar
pnpm check
pnpm tauri dev
```

LLM 与 Embedding 当前只保留 Provider-neutral 的本机 OpenAI-compatible 接口，不预设或自动安装任何模型服务。需要对话时，在设置页填写回环 Base URL、模型名称和实际 Embedding 维度；未配置或未启动本机服务时，角色窗口仍会正常运行，设置页将 Brain 标记为 `Degraded`。

`pnpm tauri dev` 会自动启动 Python Sidecar，应用退出时自动关闭；不要单独长期运行 Sidecar。会话、Mem0 历史、Qdrant 向量与脱敏日志位于 `%LOCALAPPDATA%\com.desktopmate.companion\brain`。启动后可在角色下方输入对话；双击角色进入设置页，可查看 Brain 状态并管理本地记忆。已接通的 AI 语音链路见 [语音模块架构](docs/voice-architecture.md)；Windows 原生 TTS、播放、真实口型的实现记录及后续 STT 计划见 [AudioEngine 实施方案](docs/audio-engine-plan.md)。

Qwen3-TTS 的 Conda 环境检查、模型启动和切换说明见 [Qwen 模型服务](docs/qwen-tts-service.md)。

本阶段只使用调试运行，不需要执行下方生产打包命令。

生产构建：

```powershell
pnpm tauri build
```

只检查前端：

```powershell
pnpm build
```

检查 PMX 元数据（先运行 `pnpm dev`）：

```powershell
pnpm inspect:pmx "http://127.0.0.1:1420/LinGuang/%E9%99%B5%E5%85%89-%E7%8F%A0%E7%BB%A3%E8%81%94%E5%8A%A83.0.pmx"
```

## 素材许可

`model_material/LinGuang` 的模型版权属于深空之眼，模型编辑者为神帝宇。原许可明确：允许改造，但禁止二次配布、商业用途及规则中列出的不当用途。项目仅可在该许可范围内本地使用；代码许可不覆盖模型和纹理素材。详见 `model_material/LinGuang/使用规则.txt`。
