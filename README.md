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
- 双击角色打开独立配置窗口，可设置角色外观、系统音色、语言、音量、语速、音调及非敏感 API 预设；
- 原生窗口拖拽、Rust 重力、任务栏工作区地面和窗口顶部碰撞；
- Win32/DWM 窗口枚举，多显示器负坐标和 Per-Monitor DPI 数据；
- SpeechController、TTS/STT Provider 与 RMS/viseme 动作接口；当前使用无密钥的 Web Speech 调试适配器，STT 尚未启用；
- `AgentProvider`、`MemoryProvider`、`VisionProvider` 接口占位；未启用 AI 网络、密钥或截屏能力。

## 开发环境

需要 Windows 10/11、WebView2、Node.js、pnpm、Rust stable MSVC，以及 Visual Studio 2022 Build Tools 中的 **Desktop development with C++** 工作负载。

```powershell
pnpm install
pnpm check
pnpm tauri dev
```

启动后单击角色可演示系统语音、气泡和嘴型联动。语音模块的分层与后续 AI/STT 接线方式见 [语音模块架构](docs/voice-architecture.md)。

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
