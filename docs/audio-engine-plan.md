# AudioEngine 实施方案

状态：2026-09-12 已实现 A0–A2 的 Windows TTS、原生播放、IPC、设置音色与实测 RMS 主链。当前运行说明见 [语音模块架构](voice-architecture.md)。2026-09-13 A3 的 Qwen3-TTS 本地模型服务与设置切换已接入，见 [模型服务](qwen-tts-service.md)；SFX 尚待实现，A4 按用户要求仅保留接口；设备拔插、声学延迟和干净机器安装验收仍待执行。

## 1. 推荐路线

第一阶段实现 **Windows 本机 TTS + Rust 原生播放 + 实测 RMS 口型**，复用前端 SpeechController。Windows SpeechSynthesizer 可以枚举已安装音色并将文本合成为音频流，因此适合作为不依赖额外模型服务的首个实现。[Microsoft 文档](https://learn.microsoft.com/en-us/uwp/api/windows.media.speechsynthesis.speechsynthesizer)

最低 Rust 版本已随 rodio 调整为 1.87，本次在 Rust 1.93.1 上构建。播放层已接入并锁定 `rodio = 0.22.2`，WAV 解码使用 `hound = 3.5.1`：它提供音频播放、停止和播放位置接口。实现时锁定通过本项目 Windows/MSVC 与 Rust 版本验证的版本；0.22 已把旧 Sink 命名改为 Player，不照搬旧示例。[rodio 文档](https://docs.rs/rodio/latest/rodio/)、[迁移说明](https://github.com/RustAudio/rodio/blob/master/UPGRADE.md)

已在本机完成编译、中文合成与连续播放/取消验收；实际设备拔插和声学延迟尚未测量。若高层播放器无法满足设备失效检测或口型延迟指标，再在 Playback 内替换为 WASAPI；不同时维护两套生产播放器。

| 路线 | 作用与取舍 |
|---|---|
| Windows TTS + Rust 播放（首选） | 先获得 PCM、可控停止和真实嘴型；音色质量取决于已安装语音包 |
| 本机模型 TTS（后续） | 增强角色音色，需要用户配置服务、模型及硬件；通过同一 TtsProvider 接入 |
| WebSpeechEngine（现有） | 保留浏览器开发预览及显式调试选择；不作为运行中自动重播的后备引擎 |

第一阶段不新增云端语音依赖，Python Brain Sidecar 不承担音频设备或 TTS 生命周期。Rust AudioRuntime 与 BrainSupervisor 独立创建和释放，Brain 故障时仍可试听系统语音。

## 2. 所有权与数据流

```text
CharacterResponse.speech.text / 点击问候 / 设置试听
                         ↓
SpeechController（现有：前端请求、取消信号、角色帧归零）
                         ↓
TauriSpeechEngine（已实现：映射 IPC 和 observer）
                         ↓
Rust AudioRuntime（唯一的原生活动播放会话所有者）
       ├─ ModelService → Windows / Qwen Provider → WAV 块 → PCM
       └─ stream_playback → 自适应缓冲 → 当前默认输出设备
                          └─ 实时 RMS（音素时间戳后续接入）
                         ↓
audio://event（小型状态与帧数据，带 sessionId）
                         ↓
TauriSpeechEngine → SpeechController → SpeechMotionTarget
                                            ↓
                             CharacterRuntime → PMX Morph
```

Rust 的 AudioRuntime 相当于目标架构中的原生 AudioController。第一阶段不再新增一个 TypeScript AudioController 包住 SpeechController。前端请求状态只负责 UI 和 observer 映射，实际播放、停止与资源释放以 Rust 为准；Rust 也不持有 CharacterRuntime。

## 3. 文件与接口

先在现有目录内落地，功能稳定后再按需要迁移到 `audio/`：

| 文件 | 职责 |
|---|---|
| `src-tauri/src/speech/provider.rs`（扩展现有） | TtsProvider 采用后台线程内同步调用，显式传入 Cancellation 和 deadline；音色枚举由 WindowsTtsProvider 提供，STT 暂不实现 |
| `src-tauri/src/speech/windows_tts.rs`（已新增） | Windows 原生音色、合成操作、参数映射和取消 |
| `src-tauri/src/speech/runtime.rs`（已新增） | AudioRuntime、会话顺序、时限、事件、关机清理 |
| `src-tauri/src/speech/playback.rs`（已新增） | 有界 WAV 解码和 PCM 格式校验 |
| `src-tauri/src/speech/stream_playback.rs`（已新增） | 自适应缓冲、PCM 播放、停止、播放进度及实时 RMS |
| `src-tauri/src/commands/audio.rs`（已新增） | 参数校验、调用方权限、音色列表、启动和取消 |
| `src/speech/TauriSpeechEngine.ts`（已新增） | 实现现有 SpeechEngine；在调用前注册事件并映射 observer |
| `src/speech/AudioIpc.ts`（已新增） | 原生音频 DTO、Zod 校验、命令和事件名 |
| `src/app/bootstrap.ts`（修改） | 注入原生引擎，统一 AI / 点击 / 试听；退出时清理 |
| `src/config/SettingsPanel.ts`（修改） | 原生音色和能力、试听错误、配置迁移 |

一个真实 Provider 时直接注入 WindowsTtsProvider；出现第二个实现再增加 Provider 选择器，不预建空注册表、空 SFX Manager 或未调用的 command。

当前 IPC（以 AudioIpc.ts、speech/model.rs 与 commands/audio.rs 为准）：

```ts
type SessionKey = { sessionId: string; generation: number; sequence: number };
type AudioStart = {
  key: SessionKey;
  speech: { text: string; voiceId?: string; language?: string; rate?: number; pitch?: number; volume?: number };
  timeoutSeconds?: number; // 2–60 秒，默认 30 秒
};

// list_audio_voices() -> VoiceInfo[]
// open_audio_session() -> generation: number
// start_audio({ request: AudioStart }) -> void（入队确认）
// cancel_audio({ key: SessionKey }) -> boolean
// get_audio_status() -> { generation, workerAlive, lastEvent }

// 单个事件主题 audio://event，type 为以下判别字段：
// preparing | started | frame | completed | cancelled | failed
// 公共字段：sessionId、generation、sequence、eventSequence
// frame：positionMs、level；首版不生成音素时间戳
// failed：error: { code, message }；不携带文本正文或音频
```

`start_audio` 只等待入队确认，合成和播放由后台执行。前端必须先完成监听注册再调用，以免漏掉快速开始/结束事件；验证并丢弃外来代次、旧 session 或乱序事件。`SpeechEngine.speak()` 在完成/取消终态解决，在失败时拒绝；禁止以 start 命令返回代表播放完成。

播放、取消和连接代次命令只允许角色窗口调用；设置窗口只能查询音色/状态，并沿用 voice-preview 向角色窗口请求试听。Rust 校验窗口身份和 Tauri capability，generation 与 source 不作为权限凭据。

音频字节留在 Rust。当前 `TtsProvider` 直接返回完整 WAV 字节，`decode_wav` 从 WAV 头读取采样率和声道，并校验格式与大小；不另行携带重复元数据或空口型列表，也不通过 serde 将音频传给 WebView。`SpeechAudio` 仅保留给尚未启用的 STT 契约；本阶段不扩宽 CSP 或开放任意文件 URL。

## 4. 取消、竞态与退出

状态为 `preparing → playing → completed`，从 preparing 或 playing 均可进入 cancelled / failed。每个被接受的 session 只能产生一个终态，终态后不再发 frame。

- SpeechController 的 AbortSignal 映射为带完整会话标识的 cancel 命令，前端立即归零；Rust 先取消旧合成、停止旧播放器，再启用新语音。只有确认旧播放已停止才允许新声源开始；停止异常进入失败状态。
- 若 cancel 比 start 更早到达，Rust 记录当前代次已取消的 sequence 上界，迟到 start 不能复活。新 start 同样提高接纳序号下界；旧 start 和旧 cancel 都不能影响新 session。
- 不可取消的合成操作在后台结束后丢弃结果，不允许其晚到结果触发播放。合成不得持有会话状态锁跨越阻塞调用；取消命令必须能被独立处理。
- 页面重载建立新代次，Rust 取消旧代次的会话并拒绝旧请求。窗口销毁和应用退出也由 Rust 停止音频；不依赖 WebView 最后一次事件能送达。
- 事件通道失效时前端归零，使用 get_audio_status 对账并请求取消；该 session 不自动重播。重连成功后才能开始下一次语音。

沿用 SpeechController 的前端终态防重；适配器的取消回执不再重复向 UI 发布第二个 cancelled。保持持续监听数量有界，应用关闭时释放监听、合成对象和输出设备。合成队列仅保留最新待处理请求；限制同时运行的合成任务，不能因底层取消未完成而无限创建线程或合成对象。音频回调不执行 IPC、日志或阻塞锁操作；通过有界帧队列交给事件线程。

## 5. 音频与口型

首版按整段文本合成，支持接口已有的最大 8000 字符输入，并设置可配置的合成超时。建议初始预算为 30 秒合成等待、120 秒音频时长、64 MiB 解码 PCM；超限返回明确错误，文字保留。实际预算以测试调整，不能只在全部分配后检查。

解码后统一为有限值的交错 f32 PCM，保留有效采样率与声道数，由播放器负责输出格式适配。校验解码长度、声道数、采样率和实际时长。首版仅接入验证过的 PCM/WAV，trait 中列出的 MP3/Ogg 不意味着必须首批支持。

RMS 建议按 20 ms 音频窗计算，先平方再跨采样点和声道求平均开方，避免立体声相位抵消。增加噪声门、平滑和幅度限幅，最多以 25 Hz 向前端发帧。只分析语音声道，不分析包含 SFX 的最终混音；音量在播放层只应用一次，静音时发送 level=0。

帧选择使用播放器报告的音频位置，不以合成开始时间或 setInterval 累加值代替。播放器位置仍可能包含设备缓冲延迟，须做扬声器/耳机真机校准；不承诺仅靠 get_pos 达到采样级同步。提供 viseme 时，按同一播放时间轴映射 a/i/u/e/o；没有时只做幅度口型，不轮换伪造音素。

当前气泡显示与 TTS 独立。首版保留文字显示策略，避免 TTS 失败隐藏回复；后续若让播放延长气泡寿命，应由应用编排层订阅事件，SpeechBubble 不依赖播放器。

## 6. 音色、参数与失败

Windows TTS 使用安装于本机的音色，不自动下载语音包。音色不足时设置页解释缺少对应语言资源，允许用户选择已有音色。Windows API 的音色枚举和异步合成由 windows-rs 暴露；实现前需添加对应 Cargo feature，并验证 COM/WinRT 初始化和线程要求。[windows-rs 接口](https://microsoft.github.io/windows-docs-rs/doc/windows/Media/SpeechSynthesis/struct.SpeechSynthesizer.html)

保留现有 `speechLanguage/rate/pitch/volume`，Provider 声明支持范围并在 Rust 二次校验；不支持的参数在设置页禁用并说明。禁止用改变播放速度来模拟独立语调，否则时长和口型时间戳会失配。

WebView voiceURI 与原生 voiceId 不保证相同。当前可在设置页切换 Windows 与 Qwen Provider，模型选择独立保存，voiceId 使用当前 Provider 的音色标识。旧配置按语言、名称尝试匹配并要求设置页试听核对，匹配失败选择该语言默认音色并显示提示。所有改动保持旧配置可读。

错误至少区分 `voice_unavailable`、`synthesis_timeout`、`unsupported_audio`、`audio_limit_exceeded`、`output_unavailable`、`playback_failed`。音频错误不改变 Brain readiness、不删除文字、不回滚已执行的行为。设备失联终止本次播放，下一次用户请求重新探测默认设备，不自动从头播报旧回复。

## 7. 后续扩展

| 阶段 | 内容 | 完成条件 |
|---|---|---|
| A0 技术验证 | Windows 音色/合成、rodio 编译、停止、播放位置与设备故障验证 | 在目标 Windows、MSVC 和当前工具链上确认可行并锁版本 |
| A1 原生语音闭环 | AudioRuntime、TauriSpeechEngine、配置迁移、取消及退出 | AI 回复、点击、试听共用原生语音；两条语音不重叠 |
| A2 真实口型 | RMS、播放时间轴、静音与设备延迟验证 | 实测幅度驱动嘴型，所有终态归零 |
| A3 音效/本地音色 | 有真实素材后新增 SFX；有明确服务后新增本机 TTS 适配 | SFX 并发上限和独立音量，不驱动嘴型；本机模型失败不影响文字 |
| A4 语音输入 | 独立录音会话、SttProvider、识别后填入输入框 | 用户显式开始/停止，拒绝权限可恢复，文字确认后再发送 |

A3 的本机服务沿用回环地址、禁代理、禁重定向要求，协议按实际服务适配，不假定所有“兼容接口”都支持音频。未经单独设计不扩展为云端语音。

A4 首版建议按键录音，开始时停止 TTS，防止把角色声音录回；暂不做常驻监听、唤醒词或回声消除。录音建议限长 60 秒并限缓冲，取消/拒绝/退出立即释放设备并清空音频。Rust 的整段 SttProvider 与前端 interim 接口目前不一致，首版只输出 final，不虚构部分结果；具体识别后端在实现阶段根据本机服务选择。

## 8. 验收与提交拆分

先交付 A0–A2，A3、A4 分开推进。按“原生合成/播放 → IPC 与配置 → RMS 与设备验收”拆提交，每个阶段都能独立调试；目录迁移留在功能稳定以后。当前原生代码位于 speech/，Qwen Python 服务位于 tts-service/，随项目统一进行版本管理。

- 自动化：用假 Provider/播放器验证取消早于 start、A/B 乱序、旧事件、重复终态、超时、超限、重载、退出释放；覆盖 SpeechController 的现有行为。
- 真机：中文/英文音色、静音、快速连续试听、播放中关闭语音、切换默认设备、拔掉设备、页面重载和退出后无残留音频；验证 Brain 停止时系统试听仍可工作。
- 性能：采集请求到开始播放的分阶段耗时、取消到停止延迟、口型偏差和峰值内存。初始验收目标为取消后 200 ms 内停止可闻音频、口型偏差不超过 100 ms；这些是待测目标，不是已验证结果。
- 回归：`pnpm check`、Rust 单元测试，以及真实 Windows 播放检查。仅改文档不需要重新运行这些测试。
- 发布：确认增加的 Rust 依赖随程序构建，干净 Windows 机器可使用已安装系统音色；Python Sidecar 的独立打包缺口仍按原计划解决，不混入 AudioEngine。


## 9. 本次验证记录（2026-09-12）

- 本机音色：Microsoft Huihui、Yaoyao、Kangkang（均为 zh-CN）；未安装 en-US 音色，英文实际播放未验收。
- 测试中文句子输出 16 kHz 单声道 WAV，时长约 3.15 秒；一次热启动合成+解码约 69 ms，属于单次样本而非性能基准。
- 原生播放输出约 76 帧非零 RMS；连续第二次播放取消后约 10 ms 收到播放流释放确认，不代表扬声器声学停止延迟。
- 实际 Tauri WebView 验证：preparing → started → completed；完成后 active=false、level=0；快速连续请求取消旧句，新的句子完成。
- 设置页可列出 3 个音色，直接启动播放被 ACL 拒绝；真实试听按钮经角色窗口成功播放；播放中重载使旧代次失效。
- 修复 WinRT 与 WASAPI 的线程 COM apartment 冲突；播放期间保持 MTA，使同一 worker 能连续合成。音频对象仅在初始化线程中创建和释放。
- 可重复检查：`cargo run --manifest-path src-tauri/Cargo.toml --offline --example audio-smoke` 只合成解码；追加 `-- --play` 会实际播放并检查取消/关闭。

- 退出验收：实际播放期间关闭角色主窗口，桌宠进程及其两级 Python 子进程均已退出；开发验证实例已关闭。

- 最终回归：pnpm check 通过 74 项前端测试；Rust 离线测试通过 37 项；pnpm build 成功，保留原有 Ammo 浏览器模块和大包提示。
