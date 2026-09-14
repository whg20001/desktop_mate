# Qwen3-TTS 本地模型服务

## 使用方式

双击角色打开配置中心，在“声音配置 → 语音模型”中查看正在使用的模型、加载状态、GPU 和可用音色数量。
选择模型后点击“启动并切换”，立即停止旧语音并记住模型选择；该操作独立于底部“保存全部配置”。
Qwen 就绪后选择说话人并试听，点击“保存全部配置”保存音色和播放参数。切回 Windows 系统语音会结束 Qwen 子进程并释放显存。

默认模型为 `Qwen3-TTS-12Hz-0.6B-CustomVoice`，默认说话人为 `serena`。
该模型使用内置说话人，不是录音克隆模型，也不支持 1.7B CustomVoice 的指令式风格控制。
当前服务不修改模型的语速和音调，因此界面禁用这两个控件，切回 Windows 时恢复原设置；音量在播放器中应用。
STT 按当前需求保持接口，不实现录音或识别。

## 环境和模型

桌宠直接运行指定 Conda 环境内的 Python，并补齐 Conda 的 DLL 搜索路径，不改变用户终端的活动环境。
这与手动执行以下命令使用同一环境：

```powershell
conda activate qwen3-tts
python tts-service/check_environment.py
```

需要 CUDA 版 PyTorch、匹配的 torchaudio、qwen-tts、transformers、accelerate 和 soundfile。
GPU 使用 `cuda:0`、bfloat16 和 SDPA；不要求安装 FlashAttention。只安装 CPU 版 PyTorch时会明确报告 CUDA 不可用。
模型推理使用本地缓存并强制离线，不在应用启动时下载权重。模型缓存必须同时包含主模型、文本 tokenizer 和 `speech_tokenizer`。

本机模型缓存已迁移到 `D:\model\_download\qwenSTT`，保留 `blobs / refs / snapshots` 结构。当前 `modelPath` 指向 `D:/model/_download/qwenSTT/snapshots/85e237c12c027371202489a0ec509ded67b5e4b5`，桌宠和独立启动脚本均读取同一份清单。目录名沿用用户指定的 `qwenSTT`，其中存放的仍是 Qwen3-TTS 语音合成模型。

模型清单位于 `tts-service/models.json`。每项包含：

- `id`：稳定、唯一标识，不能使用保留值 `windows`。
- `name`：设置页显示名称。
- `modelPath`：Hugging Face 模型 ID，或完整本地模型目录（包含 config.json 的 snapshot 目录）。
- `revision`：缓存版本；本地目录可省略。
- `condaEnv`：Conda 环境名。
- `pythonPath`：可选的绝对 Python 路径，用于非默认 Conda 环境位置。

后续可在清单中增加兼容的 Qwen3 CustomVoice 模型，重启桌宠后从同一菜单切换。
Base、VoiceDesign 或其他模型家族需要适配其生成接口，不能仅修改模型名。
选择结果原子保存在 `%LOCALAPPDATA%/com.desktopmate.companion/audio-model.json`。

## 服务与 AudioEngine

```text
Settings → select_audio_model → ModelService → Conda Python service.py
SpeechController → TauriSpeechEngine → AudioRuntime → ModelService (TtsProvider)
                                                   ↓
                                         Qwen 编码帧 → 分块解码/WAV → 有界缓冲 → rodio → RMS → 嘴型
```

服务采用父进程独占的 stdin/stdout 管道，不开放 HTTP 端口；JSON 消息头加有界 WAV 字节，SDK 日志不会进入协议。
模型加载完成后才报告 ready，并返回实际 GPU 和说话人列表。加载失败、崩溃或超时会显示 failed，可从设置页重试。
单次合成最长 180 秒，输出最多 120 秒音频；前端总会话时限相应增加为 305 秒。
生成期间按 talker 推理步检查取消和时限；如取消后 3 秒仍未返回，结束该子进程并要求重新启动。
只有设置窗口可以切换模型，角色窗口仍独占播放会话。切换只取消现有音频，不关闭 Brain。
应用正常退出时主动结束服务；父进程意外退出导致 stdin EOF 时，服务自行退出以释放 GPU。

独立调试启动（stdio 协议，需要客户端保持输入管道打开）：

```powershell
./tts-service/Start-Service.ps1
```

## 验证入口

```powershell
conda run -n qwen3-tts python tts-service/check_environment.py
conda run -n qwen3-tts python -m unittest discover -s tts-service/tests
cargo test --manifest-path src-tauri/Cargo.toml --offline
cargo run --manifest-path src-tauri/Cargo.toml --offline --example qwen-smoke -- --play
pnpm check
pnpm build
```

`qwen-smoke` 会加载真实 GPU 模型、合成、通过 AudioRuntime 取消生成、再次合成并播放，然后切回 Windows 并结束测试服务。
首次加载、真实生成速度和显存应以本机实测为准。当前桌宠使用音频分块生成与播放；输入文本仍为完整请求，Brain 的文本回复尚未流式接入。
当前仍面向开发环境：Python 环境与权重不打入安装包。

参考：[Qwen3-TTS 官方仓库](https://github.com/QwenLM/Qwen3-TTS)、[指定模型](https://huggingface.co/Qwen/Qwen3-TTS-12Hz-0.6B-CustomVoice)。

## 本机验收（2026-09-13）

- Python 3.12.14，qwen-tts 0.1.1，torch / torchaudio 2.11.0+cu128，setuptools 81.0.0；`pip check` 无冲突。
- RTX 5060 Ti（16 GB），CUDA 12.8，GPU 张量运算通过。已补齐 speech_tokenizer 权重，并校验下载文件 SHA-256。
- 实测模型加载 7.32 秒，返回 9 个说话人。中文短句合成和解码 5.58 秒，生成 2.72 秒、24 kHz 单声道音频。这是一次本机短句测试，速度随文本、音色和 GPU 负载变化。
- AudioRuntime 生成取消约 70 毫秒；取消后再次生成并播放成功，产生 54 个 RMS 帧事件。
- 实际菜单切换、vivian 音色试听、角色窗口切换权限限制、关闭管道退出、正常关窗后模型进程回收均通过。
- Windows 下必须先导入 NumPy / PyTorch / Qwen SDK，再启动阻塞 stdin 读取线程，否则可能发生原生库加载死锁。当前实现遵循此顺序；导入完成后立即启动读取线程，覆盖权重加载和推理期间的父进程退出。
- FlashAttention 和 SoX 未安装；本服务使用 SDPA，CustomVoice 合成链路已经实测通过，不依赖这两个可选组件。

## 分块与耗时统计（2026-09-14）

桌宠请求使用 `stream: true`，模型持续生成编码帧，Python 依次发送 `chunk`（请求 ID、连续 index、length、timings + WAV 字节），最后发送 `done` 或 `error`。完整 WAV 请求仅保留给对照验证工具；Windows Provider 通过默认适配器提供一个完整音频块。

`streaming.py` 对固定的 qwen-tts 0.1.1 做局部适配：观察 talker 完整的 16 组编码，每 6 帧产生首块（0.48 秒），之后每 12 帧产生一块（0.96 秒）；尾块单独补齐。使用前 25 帧作为解码上下文，并裁掉已输出样本。它在真实生成过程中输出，没有先生成整句再切 WAV；也不把 `non_streaming_mode=False` 当作流式输出开关。升级 SDK 前必须验证适配器，启动时会检查版本。CPU 推理线程限制为 4，避免小步 GPU 生成中的过多 CPU 并行开销。

Rust 使用独立生产线程接收和解码，音频设备线程连续消费。协议通道最多 4 个待处理消息，PCM 通道最多 3 块；单块 WAV 上限 1 MiB，累计音频不超过 120 秒。缓冲为空时输出静音且增加欠载计时，设备回调不阻塞。取消会停止播放、通知模型并排空旧请求直到终止消息，避免污染下一次请求。

声音配置中的“最近一次语音耗时”和 `get_audio_status.timings` 提供：

| 字段 | 含义 |
| --- | --- |
| queueMs / serviceQueueMs | Rust 工作队列 / Python 接收后等待处理的时间 |
| modelMs | 编码生成及文本准备的累计墙钟时间，排除编码解码和块输出耗时 |
| codecDecodeMs | Python 音频编码解码累计耗时，包含转换为 CPU 波形 |
| wavDecodeMs | Rust WAV → PCM 累计耗时 |
| firstChunkReceivedMs | Rust 请求受理至首块接收并解码完成 |
| firstPlaybackMs | Rust 请求受理至播放器首次报告消费进度；不是声卡物理发声测量 |
| synthesisDoneMs / totalMs | Rust 请求受理至生成结束 / 会话结束 |
| chunks / underrunMs | 收到块数 / 播放回调因缺少音频而补静音的时长 |

所有耗时单位为毫秒。阶段并行，不能直接相加。首块、生成结束、总时间都包含 Rust 排队；不包含 Brain 推理和前端 IPC 之前的等待。终止时 stderr 输出 `[audio-timing]` JSON，只有随机会话 ID 和计时，不记录用户文本。

本次真实菜单连续两次 vivian 试听，同一 PID 驻留：首块播放 1.72 / 0.92 秒，生成结束 6.62 / 5.61 秒，均为 4 块；后一轮生成累计 5.50 秒、解码累计约 0.12 秒。后一轮仍有 2.33 秒欠载，说明生成吞吐仍低于播放消耗速度，尚不能保证连续无停顿。更慢的独立测试也观察到约 2.48 秒首播、约 11.10 秒生成结束；这些是实际样本，不是性能承诺。

验证覆盖：编码帧顺序、上下文裁剪、EOS 与尾块、提前输出、错误时移除 hook；Rust 背压、旧请求排空、下一请求恢复、空缓冲恢复和 EOF；真实 GPU 断言首播早于生成结束。固定编码短句的分块拼接与整段解码长度相同，波形信噪比约 37.8 dB，分块解码并非逐样本完全相同。主观音质和长文本仍需要用户验收。

## 自适应启动缓冲（2026-09-14）

播放器观察至少两个音频块，用块就绪时间差 / 新块音频时长估计生成实时率（RTF）。RTF 大于 1 表示生成慢于播放。加权估计对减速反应更快、对加速更保守；目标储备按 `clamp(0.8 + 4 × max(RTF − 0.8, 0), 0.8, 6)` 秒计算。使用生产端时间戳，避免将播放器消费节奏误判为生成速度；当前估计仍包含调度、传输和背压的影响。

设备在启动储备满足之后才打开。收到生成结束后即使不足目标也播放余下音频，因此短句不会为凑足目标无限等待；较慢的短句可能整句生成完后才出声。较快的请求仍能在生成结束前播放。长句储备上限为 6 秒（允许最后一块跨过目标），不是等待整个长句，持续低于实时速度或中途减速仍可能耗尽储备。

播放中缓冲耗尽会进入重新缓冲状态：停止消费新到的小块，积累至最新目标或收到 EOF 后再继续。等待期间输出静音并计入欠载；启动前等待单独计时，不计入播放欠载。取消在启动等待和重新缓冲期间同样生效。PCM 通道仍最多 3 块，播放器另持有目标范围内的储备。

声音面板新增 `startupBufferMs`（首块之后的启动缓冲等待）、`startupAudioMs`（启动前储备的音频时长）、`bufferTargetMs`（当前目标）、`generationRtf`（当前速度估计）、`rebufferCount`（播放中重新缓冲次数）。所有毫秒字段与生成累计时间有重叠，不能相加为总时间。

同一 vivian 菜单试听连续两次实测：首播 5.84 / 5.91 秒，启动储备 2.96 / 3.36 秒，播放欠载均为 0，重新缓冲均为 0。与上一版约 1 秒首播但约 2.3 秒欠载相比，当前优先保障连续播放。该结果只代表已测短句，不承诺任意长文本无停顿。回归覆盖快速流提前消费、慢速储备扩大及上限、EOF 尾块放行和欠载后等待储备不丢样本。

启动缓冲中取消实测约 112 毫秒，取消前未启动播放，模型继续保持 ready。前端 77 项、Rust 42 项回归及生产构建通过。
