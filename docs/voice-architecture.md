# 语音模块架构

Windows 原生 AudioEngine 已接入，并新增 Qwen3-TTS 本地模型服务及模型切换，见 [模型服务](qwen-tts-service.md)。设计取舍、接口和后续计划见 [AudioEngine 实施方案](audio-engine-plan.md)。

## 当前链路

```text
ConversationPanel → BrainBridge → Rust BrainClient
                                      ↓
                    Python ConversationOrchestrator
                                      ↓
                    CharacterResponse（Rust / Zod 校验）
                                      ↓
                           src/app/bootstrap.ts
            ┌─────────────────────────┼──────────────────┐
            ↓                         ↓                  ↓
      SpeechBubble(text)      BehaviorPlanner       speech.text
                               → CharacterRuntime        ↓
                                                 SpeechController
                                                        ↓
                                                TauriSpeechEngine
                                                        ↓
                                       Rust AudioRuntime（单 worker）
                                          ├─ ModelService → Windows / Qwen Provider
                                          └─ stream_playback（启动缓冲、连续 PCM 播放）
                                               └─ 实时 RMS → audio://event
                                                        ↓
                                      SpeechController → SpeechMotionTarget
                                                        ↓
                                      CharacterRuntime / MotionController
```

Windows 与 Qwen `TtsProvider` 均返回 WAV 字节，由 `decode_wav` 校验文件头并读取音频格式；RMS 从解码后的 PCM 计算，合成结果不重复保存格式元数据或空口型列表。

AI 回复、点击问候与设置试听共享角色窗口中的 SpeechController。Python 只提供文本、情绪和动作建议，不控制扬声器。气泡仍独立显示，TTS 失败保留文字；STT 尚未接入。

## 实现边界

| 位置 | 职责 |
|---|---|
| `src/speech/SpeechController.ts` | 前端请求、打断、归零和生命周期；dispose 同时释放底层引擎 |
| `src/speech/TauriSpeechEngine.ts` / `AudioIpc.ts` | 原生 IPC、运行时校验、旧事件隔离、超时与状态对账 |
| `src-tauri/src/speech/runtime.rs` | 一个活动任务和一个最新待处理请求；先释放旧播放再开始新播放 |
| `src-tauri/src/speech/windows_tts.rs` | 本机 Windows 音色、可取消/限时合成、WinRT apartment 生命周期 |
| `src-tauri/src/speech/playback.rs` | 有界 WAV 解码和 PCM 格式校验 |
| `src-tauri/src/speech/stream_playback.rs` | 自适应缓冲、连续 PCM 播放、设备错误与播放进度；20 ms RMS 窗、噪声门和平滑 |
| `src-tauri/src/commands/audio.rs` | 查询音色/状态；只有角色窗口可启动或取消播放 |
| `src/character/animation/MotionController.ts` | 无音素时使用 a 开口幅度；有 viseme 时使用对应 Morph；终态立即归零 |
| `src/config/SettingsPanel.ts` | 原生音色列表、旧音色匹配、试听与错误反馈 |

浏览器开发预览仍使用 WebSpeechEngine，其幅度和音素为周期近似值。Tauri 运行时使用原生 AudioRuntime，按设置选择 Windows 或 Qwen Provider；失败后不自动重播其他引擎。

## 调用与配置

```ts
if (settings.speechEnabled && response.speech?.text) {
  await speechController.speak({
    text: response.speech.text,
    source: 'ai',
    language: settings.speechLanguage,
    voiceId: settings.speechVoiceId || undefined,
    rate: settings.speechRate,
    pitch: settings.speechPitch,
    volume: settings.speechVolume,
  });
}
```

CharacterResponse.speech 是可选的 `{ text }`。actionIntent 与 emotion 经 BehaviorPlanner / BehaviorPolicy；BehaviorScheduler 处理动作抢占与冷却。语音只通过 SpeechMotionTarget 输入嘴型。

现有非敏感设置继续使用 speechVoiceId / speechLanguage / speechRate / speechPitch / speechVolume。原生 voiceId 与旧 WebView voiceURI 不保证相同：先匹配已有原生 ID，再尝试语言/名称，失败时选择该语言已安装音色并提示试听确认。不自动安装语音包；没有对应音色时返回 voice_unavailable。

设置窗口仍通过 settings://change 的 voice-preview 请求角色窗口试听。Tauri capability 与 Rust 窗口身份检查共同限制播放命令；音频字节不进入 IPC，CSP 没有放宽。

## 生命周期与恢复

前端先注册监听再发 start_audio，只有原生终态才完成 speak。会话以 UUID、generation 和 sequence 标识，事件另带 eventSequence；过期请求和乱序帧被丢弃。后台只有单个 worker，取消早于 start 也不会复活旧请求。

播放正常结束、失败、取消都输出静音终态，角色立即清空嘴型及残留的程序化 Talking。原生页面重载取消旧代次；角色主窗口销毁后应用退出，不让隐藏设置页保留音频和 Brain 工作线程。

播放期间前端每秒读取状态以恢复遗漏的终态；连接失败请求取消并返回错误，下次请求重新建立连接。前端请求的合成时限为 180 秒（原生请求可设 2–180 秒），解码最多 64 MiB / 120 秒。设备错误终止本次播放，下一次请求重新打开默认输出设备。

## 验证与后续

已通过本机中文合成、连续播放/取消、实际设置试听、原生帧归零和播放中重载检查；可运行 audio-smoke 示例重复验证，详见实施方案第 9 节。

Qwen3-TTS 本机模型已接入，SFX 与真实音素时间戳仍待实现；STT 当前仅保留接口。设备拔插、英文语音包、声学停止延迟和安装包环境尚待真机补测。当前不含云端语音或常驻录音。

### 2026-09-14：Qwen 音频分块与诊断

Qwen 已接入生成中的编码帧分块解码，通过 `chunk → done/error` 管道协议交给 Rust 有界 PCM 队列，一边生成一边播放。声音配置可查看排队、生成、解码、首块播放和缓冲欠载。文本输入仍为完整请求，STT 保留接口；推理吞吐不足时仍会欠载。详见 [Qwen 服务文档](qwen-tts-service.md#分块与耗时统计2026-09-14)。
