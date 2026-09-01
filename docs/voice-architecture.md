# 语音模块架构

## 目标

语音模块负责把文本转换为可播放语音，并把播放生命周期和嘴型帧交给角色动作层。它不负责生成 AI 内容，也不允许 WebView 持有模型服务密钥。

    用户输入 / STT
          ↓
    ConversationOrchestrator（Rust，后续实现）
          ↓
    AgentProvider → AgentResponse.speech
          ↓
    TtsProvider → Audio + optional VisemeCue
          ↓
    SpeechEngine → Speaker + SpeechController
                                ├─ SpeechControllerEvent → Behavior / UI
                                └─ SpeechMotionFrame → CharacterRuntime → MotionController → PMX Morph

## 当前实现

| 层 | 位置 | 职责 |
|---|---|---|
| Provider 契约 | src-tauri/src/speech/provider.rs | 定义 TTS、STT、音频、识别结果和可选 viseme 时间戳；真实服务商以后在 Rust 侧实现 |
| Engine 契约 | src/speech/SpeechTypes.ts | 屏蔽合成、音频传输和播放方式，只向上暴露播放生命周期与嘴型帧 |
| 生命周期 | src/speech/SpeechController.ts | 单一活动语音、打断、取消、错误传播，以及 AI/交互/系统来源标记 |
| 调试 Engine | src/speech/WebSpeechEngine.ts | 使用 WebView 的系统语音合成，不需要密钥；当前嘴型是近似节奏 |
| 动作适配 | CharacterRuntime / MotionController | 接收归一化音量和可选 a/i/u/e/o viseme，驱动 PMX Morph 与轻微说话动作 |
| 用户配置 | CharacterSettings / SettingsPanel | 保存语音开关、系统音色、语言、音量、语速和音调，并通过角色窗口的 SpeechController 试听 |

WebSpeechEngine 只是本地调试适配器。Web Speech API 不提供原始 PCM，因此无法计算真实 RMS；后续接入真实 TTS 时，Engine 应发送音频分析得到的 RMS，或直接发送 Provider 返回的音素时间戳。

Engine 和 Recognizer 不维护独立 ID；服务商身份应由后续 Rust Provider Manager 统一管理，避免前后端保存两份来源信息。

## 稳定契约

### 文本到语音

前端统一调用：

    await speechController.speak({
      text: response.speech,
      source: 'ai',
      language: 'zh-CN',
    });

source 只标记文本来源，不授予 AI 控制骨骼的权限。AI 的 action 和 emotion 必须继续经过 Safety / BehaviorPlanner 校验；语音模块只广播 preparing、started、completed、cancelled、failed 生命周期事件，动作层可据此选择点头、倾听或恢复 Idle。

### 声音配置

设置窗口通过 settings://change 的 voice-preview 事件请求试听，实际播放仍由角色窗口持有的 SpeechController 完成，因此试听、点击问候和未来 AI 回复共享同一个打断与嘴型生命周期。

声音配置保存在非敏感的角色设置中。speechVoiceId 当前对应 Windows WebView2 枚举的系统音色；speechLanguage、speechRate、speechPitch 和 speechVolume 可直接映射到后续 Rust SpeechSynthesisRequest。切换真实 TTS Provider 后，Provider 密钥仍不得写入这些设置。


### 嘴型

第一阶段：

    Audio RMS (0..1) → SpeechMotionFrame.level → Mouth Open

Provider 有音素时间戳时：

    VisemeCue(a/i/u/e/o) → SpeechMotionFrame.viseme → 对应 PMX Morph

语音帧优先于原有定时 Talking 嘴型；语音结束或取消时必须发送静音帧，确保 Morph 回到零。

### 打断与所有权

SpeechController 同时只拥有一个活动语音。新的 speak() 会先取消旧请求，AbortSignal 是 Engine 的唯一取消入口。Controller 负责动作帧归零，Engine 负责停止合成和扬声器资源。

## 后续接入顺序

1. 实现 Rust TtsProvider 和 Provider Manager，不在 WebView 保存 API Key。
2. 增加 Tauri Speech Engine：调用 Rust 合成，使用受控资源 URL 或原生播放器播放；大音频不要作为 JSON 数组反复发送。
3. 播放端生成 RMS 帧，或使用 VisemeCue 驱动精确口型。
4. ConversationOrchestrator 校验 AgentResponse 后，把 speech 文本交给语音控制器，把 action / emotion 分别交给行为层。
5. 最后实现 SttProvider 与麦克风会话；录音必须由用户显式触发，并增加可见的录音状态和独立权限。

当前没有启用云端 TTS、STT、麦克风、语音密钥或新的网络权限。
