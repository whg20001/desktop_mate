use super::model::{AudioError, Cancellation, VoiceInfo, MAX_AUDIO_BYTES};
use super::provider::{AudioChunk, ProviderTimings, SpeechSynthesisRequest, TtsProvider};
use super::windows_tts::WindowsTtsProvider;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::{Duration, Instant},
};

fn failure(message: impl Into<String>) -> AudioError {
    AudioError::new("model_service_failed", message)
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelEntry {
    pub id: String,
    pub name: String,
    pub model_path: String,
    pub revision: Option<String>,
    pub conda_env: String,
    pub python_path: Option<PathBuf>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Catalog {
    default_model: String,
    models: Vec<ModelEntry>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatus {
    pub selected_model: String,
    pub name: String,
    pub phase: String,
    pub detail: String,
    pub device: Option<String>,
    pub pid: Option<u32>,
    pub speakers: Vec<String>,
    pub models: Vec<ModelEntry>,
}
struct Message {
    header: Value,
    audio: Vec<u8>,
}
struct Service {
    child: Mutex<Child>,
    reader_stop: Arc<AtomicBool>,
    input: Mutex<ChildStdin>,
    messages: Mutex<mpsc::Receiver<Message>>,
    status: Arc<Mutex<ModelStatus>>,
    reader: Mutex<Option<thread::JoinHandle<()>>>,
    monitor: Mutex<Option<thread::JoinHandle<()>>>,
    started: Instant,
}
impl Service {
    fn launch(
        entry: &ModelEntry,
        root: &Path,
        models: &[ModelEntry],
    ) -> Result<Arc<Self>, AudioError> {
        let python = resolve_python(entry)?;
        let mut command = Command::new(&python);
        command
            .arg("-u")
            .arg(root.join("service.py"))
            .arg("--model")
            .arg(&entry.model_path);
        if let Some(revision) = &entry.revision {
            command.arg("--revision").arg(revision);
        }
        // Equivalent interpreter/environment to conda activate, with DLL paths restored.
        let prefix = python
            .parent()
            .ok_or_else(|| failure("无效的 Python 路径"))?;
        let mut paths = vec![
            prefix.to_path_buf(),
            prefix.join("Library/bin"),
            prefix.join("Scripts"),
        ];
        paths.extend(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        ));
        command
            .env(
                "PATH",
                std::env::join_paths(paths).map_err(|e| failure(e.to_string()))?,
            )
            .env("CONDA_PREFIX", prefix)
            .env("PYTHONUTF8", "1")
            .env("HF_HUB_OFFLINE", "1")
            .env("TRANSFORMERS_OFFLINE", "1")
            .env("HF_HUB_DISABLE_TELEMETRY", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
        let mut child = command
            .spawn()
            .map_err(|e| failure(format!("无法启动 {}: {e}", entry.conda_env)))?;
        let input = child
            .stdin
            .take()
            .ok_or_else(|| failure("无法建立服务输入管道"))?;
        let output = child
            .stdout
            .take()
            .ok_or_else(|| failure("无法建立服务输出管道"))?;
        let status = Arc::new(Mutex::new(ModelStatus {
            selected_model: entry.id.clone(),
            name: entry.name.clone(),
            phase: "loading".into(),
            detail: "正在加载本地模型…".into(),
            device: None,
            pid: Some(child.id()),
            speakers: Vec::new(),
            models: models.to_vec(),
        }));
        let reader_status = status.clone();
        let (send, receive) = mpsc::sync_channel(4);
        let reader_stop = Arc::new(AtomicBool::new(false));
        let stop = reader_stop.clone();
        let reader = thread::spawn(move || {
            let mut stream = BufReader::new(output);
            loop {
                let result = read_message(&mut stream);
                let message = match result {
                    Ok(m) => m,
                    Err(error) => {
                        let mut s = reader_status.lock();
                        if s.phase != "failed" {
                            s.detail = error.message;
                        }
                        s.phase = "failed".into();
                        s.pid = None;
                        break;
                    }
                };
                match message.header["type"].as_str() {
                    Some("ready") => {
                        let mut s = reader_status.lock();
                        s.phase = "ready".into();
                        s.detail = "本地模型已就绪".into();
                        s.device = message.header["device"].as_str().map(String::from);
                        s.speakers = serde_json::from_value(message.header["speakers"].clone())
                            .unwrap_or_default();
                    }
                    Some("failed") => {
                        let mut s = reader_status.lock();
                        s.phase = "failed".into();
                        s.detail = message.header["message"]
                            .as_str()
                            .unwrap_or("模型加载失败")
                            .to_string();
                    }
                    _ => {
                        let mut pending = message;
                        loop {
                            if stop.load(Ordering::Acquire) {
                                return;
                            }
                            match send.try_send(pending) {
                                Ok(()) => break,
                                Err(mpsc::TrySendError::Disconnected(_)) => return,
                                Err(mpsc::TrySendError::Full(message)) => {
                                    pending = message;
                                    thread::sleep(Duration::from_millis(5));
                                }
                            }
                        }
                    }
                }
            }
        });
        let service = Arc::new(Self {
            child: Mutex::new(child),
            reader_stop,
            input: Mutex::new(input),
            messages: Mutex::new(receive),
            status,
            reader: Mutex::new(Some(reader)),
            monitor: Mutex::new(None),
            started: Instant::now(),
        });
        let weak = Arc::downgrade(&service);
        *service.monitor.lock() = Some(thread::spawn(move || loop {
            thread::sleep(Duration::from_millis(100));
            let Some(service) = weak.upgrade() else {
                break;
            };
            if service.status.lock().phase != "loading" {
                break;
            }
            if service.started.elapsed() >= Duration::from_secs(180) {
                {
                    let mut status = service.status.lock();
                    status.phase = "failed".into();
                    status.detail = "模型加载超时，请检查环境后重试".into();
                }
                let mut child = service.child.lock();
                let _ = child.kill();
                let _ = child.wait();
                break;
            }
        }));
        Ok(service)
    }
    fn send(&self, value: Value) -> Result<(), AudioError> {
        let mut input = self.input.lock();
        serde_json::to_writer(&mut *input, &value).map_err(|_| failure("语音服务连接中断"))?;
        input
            .write_all(b"\n")
            .and_then(|_| input.flush())
            .map_err(|_| failure("语音服务连接中断"))
    }
    fn stop(&self) {
        self.reader_stop.store(true, Ordering::Release);
        {
            let mut status = self.status.lock();
            if status.phase != "failed" {
                status.detail = "模型服务已停止，请重新启动".into();
            }
            status.phase = "failed".into();
            status.pid = None;
        }
        let mut child = self.child.lock();
        let _ = child.kill();
        let _ = child.wait();
        drop(child);
        if let Some(reader) = self.reader.lock().take() {
            let _ = reader.join();
        }
        if let Some(monitor) = self.monitor.lock().take() {
            if monitor.thread().id() != thread::current().id() {
                let _ = monitor.join();
            }
        }
    }
    fn synthesize(
        &self,
        request: &SpeechSynthesisRequest,
        cancel: &Cancellation,
        deadline: Instant,
    ) -> Result<Vec<u8>, AudioError> {
        self.exchange(request, cancel, deadline, false, &mut |_| Ok(()))
            .map(|r| r.0)
    }
    fn exchange(
        &self,
        request: &SpeechSynthesisRequest,
        cancel: &Cancellation,
        deadline: Instant,
        stream: bool,
        on_chunk: &mut dyn FnMut(AudioChunk) -> Result<(), AudioError>,
    ) -> Result<(Vec<u8>, ProviderTimings), AudioError> {
        cancel.check()?;
        if self.status.lock().phase != "ready" {
            return Err(failure("语音模型尚未就绪，请查看声音设置"));
        }
        let id = uuid::Uuid::new_v4().to_string();
        let speakers = self.status.lock().speakers.clone();
        let speaker = request
            .voice_id
            .as_deref()
            .filter(|id| speakers.iter().any(|s| s == id))
            .unwrap_or_else(|| {
                if speakers.iter().any(|s| s == "serena") {
                    "serena"
                } else {
                    speakers.first().map(String::as_str).unwrap_or("")
                }
            });
        let receive = self.messages.lock();
        self.send(json!({"type":"synthesize", "stream":stream, "id":id, "text":request.text, "speaker":speaker,
            "language":request.language, "timeoutSeconds":deadline.saturating_duration_since(Instant::now()).as_secs().max(2)}))?;
        let mut cancelled_at = None;
        let mut callback_error = None;
        let mut next_chunk = 0_u64;
        let mut total_bytes = 0_usize;
        loop {
            if (cancel.is_cancelled() || Instant::now() >= deadline) && cancelled_at.is_none() {
                let _ = self.send(json!({"type":"cancel", "id":id}));
                cancelled_at = Some(Instant::now());
            }
            if cancelled_at.is_some_and(|t: Instant| t.elapsed() > Duration::from_secs(3)) {
                self.stop();
                return Err(if cancel.is_cancelled() {
                    AudioError::cancelled()
                } else {
                    AudioError::new("synthesis_timeout", "模型生成超时，请重新启动模型")
                });
            }
            match receive.recv_timeout(Duration::from_millis(20)) {
                Ok(message) => {
                    if message.header["id"] != id {
                        self.stop();
                        return Err(failure("模型响应会话不匹配"));
                    }
                    if cancel.is_cancelled() {
                        if message.header["type"] == "chunk" {
                            continue;
                        }
                        return Err(AudioError::cancelled());
                    }
                    if cancelled_at.is_some() {
                        // Drain already queued chunks through the terminal acknowledgement.
                        if message.header["type"] == "chunk" {
                            continue;
                        }
                        return Err(callback_error.unwrap_or_else(|| {
                            AudioError::new("synthesis_timeout", "模型生成超时")
                        }));
                    }
                    let timings: ProviderTimings =
                        serde_json::from_value(message.header["timings"].clone())
                            .unwrap_or_default();
                    if message.header["type"] == "chunk" && stream {
                        total_bytes = total_bytes.saturating_add(message.audio.len());
                        if message.header["index"].as_u64() != Some(next_chunk)
                            || total_bytes > MAX_AUDIO_BYTES
                        {
                            self.stop();
                            return Err(failure("模型分块顺序或总大小无效"));
                        }
                        next_chunk += 1;
                        if let Err(error) = on_chunk(AudioChunk {
                            wav: message.audio,
                            timings,
                        }) {
                            callback_error = Some(error);
                            let _ = self.send(json!({"type":"cancel", "id":id}));
                            cancelled_at = Some(Instant::now());
                        }
                        continue;
                    }
                    if message.header["type"] == "done" && stream && next_chunk > 0 {
                        return Ok((Vec::new(), timings));
                    }
                    if message.header["type"] == "audio" && !stream {
                        return Ok((message.audio, timings));
                    }
                    return Err(failure("模型生成失败，请缩短文本或检查模型状态"));
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => return Err(failure("模型服务已退出，请在声音设置中重新启动")),
            }
        }
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        self.stop();
    }
}

fn read_message(reader: &mut impl BufRead) -> Result<Message, AudioError> {
    let mut header = Vec::new();
    (&mut *reader)
        .take(65537)
        .read_until(b'\n', &mut header)
        .map_err(|_| failure("无法读取模型响应"))?;
    if header.is_empty() || header.len() > 65536 || header.last() != Some(&b'\n') {
        return Err(failure("模型进程退出或响应格式无效"));
    }
    let header: Value =
        serde_json::from_slice(&header).map_err(|_| failure("模型响应不是有效 JSON"))?;
    let length = if matches!(header["type"].as_str(), Some("audio" | "chunk")) {
        header["length"]
            .as_u64()
            .filter(|n| *n > 0 && *n <= MAX_AUDIO_BYTES as u64)
            .ok_or_else(|| failure("模型音频大小超限"))? as usize
    } else {
        0
    };
    if header["type"] == "chunk" && length > 1024 * 1024 {
        return Err(failure("单个音频块超过 1 MiB"));
    }
    let mut audio = vec![0; length];
    reader
        .read_exact(&mut audio)
        .map_err(|_| failure("模型音频不完整"))?;
    Ok(Message { header, audio })
}
fn resolve_python(entry: &ModelEntry) -> Result<PathBuf, AudioError> {
    let mut candidates = Vec::new();
    if let Some(path) = &entry.python_path {
        candidates.push(path.clone());
    }
    if let Some(home) = std::env::var_os("USERPROFILE") {
        candidates.push(
            PathBuf::from(home)
                .join(".conda/envs")
                .join(&entry.conda_env)
                .join("python.exe"),
        );
    }
    if let Some(conda) = std::env::var_os("CONDA_EXE") {
        if let Some(root) = Path::new(&conda).parent().and_then(Path::parent) {
            candidates.push(root.join("envs").join(&entry.conda_env).join("python.exe"));
        }
    }
    candidates.into_iter().find(|p| p.is_file()).ok_or_else(|| {
        failure(format!(
            "找不到 Conda 环境 {}，请在模型清单中设置 pythonPath",
            entry.conda_env
        ))
    })
}
struct Selection {
    id: String,
    service: Option<Arc<Service>>,
    error: Option<String>,
}
pub struct ModelService {
    root: PathBuf,
    models: Vec<ModelEntry>,
    selected: Mutex<Selection>,
    settings_path: PathBuf,
}
impl ModelService {
    pub fn new(data_dir: &Path) -> Result<Arc<Self>, AudioError> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("tts-service");
        let catalog: Catalog = serde_json::from_slice(
            &fs::read(root.join("models.json")).map_err(|e| failure(e.to_string()))?,
        )
        .map_err(|e| failure(e.to_string()))?;
        let mut ids = std::collections::HashSet::new();
        if catalog
            .models
            .iter()
            .any(|m| m.id == "windows" || m.id.is_empty() || !ids.insert(&m.id))
        {
            return Err(failure("模型清单包含重复或无效 ID"));
        }
        fs::create_dir_all(data_dir).map_err(|e| failure(e.to_string()))?;
        let settings_path = data_dir.join("audio-model.json");
        let selected = fs::read(&settings_path)
            .ok()
            .and_then(|b| serde_json::from_slice::<String>(&b).ok())
            .unwrap_or(catalog.default_model);
        let manager = Arc::new(Self {
            root,
            models: catalog.models,
            settings_path,
            selected: Mutex::new(Selection {
                id: "windows".into(),
                service: None,
                error: None,
            }),
        });
        // A missing environment/model must not prevent the desktop from starting.
        if let Err(error) = manager.select(&selected) {
            let mut state = manager.selected.lock();
            if manager.models.iter().any(|model| model.id == selected) {
                state.id = selected;
            }
            state.error = Some(error.message);
        }
        Ok(manager)
    }
    pub fn select(&self, id: &str) -> Result<(), AudioError> {
        let entry = if id == "windows" {
            None
        } else {
            Some(
                self.models
                    .iter()
                    .find(|m| m.id == id)
                    .ok_or_else(|| failure("未知的语音模型"))?,
            )
        };
        if let Some(entry) = entry {
            resolve_python(entry)?;
        }
        let mut selection = self.selected.lock();
        if selection.id == id
            && selection
                .service
                .as_ref()
                .is_some_and(|s| s.status.lock().phase != "failed")
        {
            return Ok(());
        }
        if let Some(old) = selection.service.take() {
            old.stop();
        }
        selection.id = id.into();
        selection.error = None;
        if let Some(entry) = entry {
            match Service::launch(entry, &self.root, &self.models) {
                Ok(service) => selection.service = Some(service),
                Err(error) => {
                    selection.error = Some(error.message.clone());
                    return Err(error);
                }
            }
        }
        save_selection(&self.settings_path, id)?;
        Ok(())
    }
    pub fn status(&self) -> ModelStatus {
        let selected = self.selected.lock();
        if let Some(service) = &selected.service {
            return service.status.lock().clone();
        }
        ModelStatus {
            selected_model: selected.id.clone(),
            name: if selected.id == "windows" {
                "Windows 系统语音".into()
            } else {
                self.models
                    .iter()
                    .find(|model| model.id == selected.id)
                    .map(|model| model.name.clone())
                    .unwrap_or_else(|| selected.id.clone())
            },
            phase: if selected.error.is_some() {
                "failed"
            } else {
                "ready"
            }
            .into(),
            detail: selected
                .error
                .clone()
                .unwrap_or_else(|| "系统语音可用".into()),
            device: None,
            pid: None,
            speakers: Vec::new(),
            models: self.models.clone(),
        }
    }
    pub fn voices(&self) -> Result<Vec<VoiceInfo>, AudioError> {
        let status = self.status();
        if status.selected_model == "windows" {
            return WindowsTtsProvider::voices();
        }
        if status.phase != "ready" {
            return Err(failure(status.detail));
        }
        Ok(status
            .speakers
            .iter()
            .map(|speaker| VoiceInfo {
                id: speaker.clone(),
                name: speaker.clone(),
                language: "".into(),
                is_default: speaker == "serena",
            })
            .collect())
    }
    pub fn shutdown(&self) {
        if let Some(service) = self.selected.lock().service.take() {
            service.stop();
        }
    }
}
impl TtsProvider for ModelService {
    fn synthesize(
        &self,
        request: &SpeechSynthesisRequest,
        cancel: &Cancellation,
        deadline: Instant,
    ) -> Result<Vec<u8>, AudioError> {
        let (id, service) = {
            let selection = self.selected.lock();
            (selection.id.clone(), selection.service.clone())
        };
        if id == "windows" {
            WindowsTtsProvider.synthesize(request, cancel, deadline)
        } else {
            service
                .ok_or_else(|| failure("模型服务未启动"))?
                .synthesize(request, cancel, deadline)
        }
    }
    fn stream(
        &self,
        request: &SpeechSynthesisRequest,
        cancel: &Cancellation,
        deadline: Instant,
        chunk: &mut dyn FnMut(AudioChunk) -> Result<(), AudioError>,
    ) -> Result<ProviderTimings, AudioError> {
        let (id, service) = {
            let state = self.selected.lock();
            (state.id.clone(), state.service.clone())
        };
        if id == "windows" {
            WindowsTtsProvider.stream(request, cancel, deadline, chunk)
        } else {
            service
                .ok_or_else(|| failure("模型服务未启动"))?
                .exchange(request, cancel, deadline, true, chunk)
                .map(|r| r.1)
        }
    }
}
impl Drop for ModelService {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    #[test]
    fn owned_service_cancels_recovers_and_stops_on_model_switch() {
        let root = std::env::temp_dir().join(format!("tts-service-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("service.py"),
            r#"import json,sys,threading,time
lock=threading.Lock()
cancel=threading.Event()
def send(value,data=b''):
    with lock:
        sys.stdout.buffer.write(json.dumps(value).encode()+b'\n'+data)
        sys.stdout.buffer.flush()
send({'type':'ready','device':'test','speakers':['serena']})
def synth(request):
    cancelled=cancel.wait(0.15)
    if cancelled: send({'type':'error','id':request['id'],'code':'cancelled'})
    elif request.get('stream'):
        for index in range(12):
            if cancel.is_set():
                send({'type':'error','id':request['id'],'code':'cancelled'})
                return
            send({'type':'chunk','id':request['id'],'index':index,'length':3},b'abc')
        send({'type':'done','id':request['id'],'timings':{'modelMs':123}})
    else: send({'type':'audio','id':request['id'],'length':3},b'abc')
for line in sys.stdin:
    request=json.loads(line)
    if request['type']=='cancel': cancel.set()
    if request['type']=='synthesize':
        cancel.clear()
        threading.Thread(target=synth,args=(request,),daemon=True).start()
"#,
        )
        .unwrap();
        let python = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("brain-sidecar/.venv/Scripts/python.exe");
        let entry = ModelEntry {
            id: "test".into(),
            name: "test".into(),
            model_path: "test".into(),
            revision: None,
            conda_env: "test".into(),
            python_path: Some(python),
        };
        let service = Service::launch(&entry, &root, &[entry.clone()]).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while service.status.lock().phase == "loading" {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(service.status.lock().phase, "ready");
        let request = SpeechSynthesisRequest {
            text: "test".into(),
            voice_id: Some("serena".into()),
            language: Some("en-US".into()),
            rate: None,
            pitch: None,
            volume: None,
        };
        let cancel = Cancellation::default();
        let trigger = cancel.clone();
        let timer = thread::spawn(move || {
            thread::sleep(Duration::from_millis(30));
            trigger.cancel();
        });
        assert_eq!(
            service
                .synthesize(&request, &cancel, Instant::now() + Duration::from_secs(3))
                .unwrap_err()
                .code,
            "cancelled"
        );
        timer.join().unwrap();
        assert_eq!(
            service
                .synthesize(
                    &request,
                    &Cancellation::default(),
                    Instant::now() + Duration::from_secs(3)
                )
                .unwrap(),
            b"abc"
        );
        let mut chunks = 0;
        let (_, timings) = service
            .exchange(
                &request,
                &Cancellation::default(),
                Instant::now() + Duration::from_secs(3),
                true,
                &mut |chunk| {
                    assert_eq!(chunk.wav, b"abc");
                    chunks += 1;
                    thread::sleep(Duration::from_millis(5));
                    Ok(())
                },
            )
            .unwrap();
        assert_eq!(chunks, 12);
        assert_eq!(timings.model_ms, 123.0);
        let error = service
            .exchange(
                &request,
                &Cancellation::default(),
                Instant::now() + Duration::from_secs(3),
                true,
                &mut |_| Err(AudioError::new("test_output", "output failed")),
            )
            .unwrap_err();
        assert_eq!(error.code, "test_output");
        assert_eq!(
            service
                .synthesize(
                    &request,
                    &Cancellation::default(),
                    Instant::now() + Duration::from_secs(3)
                )
                .unwrap(),
            b"abc"
        );
        let manager = ModelService {
            root: root.clone(),
            models: vec![entry],
            settings_path: root.join("audio-model.json"),
            selected: Mutex::new(Selection {
                id: "test".into(),
                service: Some(service.clone()),
                error: None,
            }),
        };
        assert!(manager.select("unregistered").is_err());
        assert_eq!(manager.status().selected_model, "test");
        manager.select("windows").unwrap();
        assert!(service.child.lock().try_wait().unwrap().is_some());
        assert_eq!(manager.status().selected_model, "windows");
        assert_eq!(
            serde_json::from_slice::<String>(&fs::read(root.join("audio-model.json")).unwrap())
                .unwrap(),
            "windows"
        );
        manager.shutdown();
        drop(manager);
        drop(service);
        fs::remove_file(root.join("audio-model.json")).unwrap();
        fs::remove_file(root.join("service.py")).unwrap();
        fs::remove_dir(root).unwrap();
    }

    #[test]
    fn framed_audio_is_bounded_and_complete() {
        let message =
            read_message(&mut Cursor::new(b"{\"type\":\"audio\",\"length\":3}\nabc")).unwrap();
        assert_eq!(message.audio, b"abc");
        assert!(read_message(&mut Cursor::new(
            b"{\"type\":\"audio\",\"length\":999999999}\n"
        ))
        .is_err());
        assert!(read_message(&mut Cursor::new(b"{\"type\":\"audio\",\"length\":3}\na")).is_err());
        assert!(read_message(&mut Cursor::new(vec![b'x'; 65537])).is_err());
    }
}

fn save_selection(path: &Path, id: &str) -> Result<(), AudioError> {
    let temporary = path.with_extension("tmp");
    let mut file = fs::File::create(&temporary).map_err(|e| failure(e.to_string()))?;
    file.write_all(&serde_json::to_vec(id).unwrap())
        .and_then(|_| file.sync_all())
        .map_err(|e| failure(e.to_string()))?;
    drop(file);
    use std::os::windows::ffi::OsStrExt;
    use windows::{
        core::PCWSTR,
        Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        },
    };
    let source: Vec<u16> = temporary.as_os_str().encode_wide().chain(Some(0)).collect();
    let target: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    unsafe {
        MoveFileExW(
            PCWSTR(source.as_ptr()),
            PCWSTR(target.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    }
    .map_err(|e| failure(e.to_string()))
}
