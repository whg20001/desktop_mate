use std::{
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{mpsc, Arc},
    thread,
    time::{Duration, Instant},
};

use parking_lot::{Mutex, RwLock};
use tauri::Manager;
use uuid::Uuid;

use super::{
    client::BrainClient,
    locality::LocalEndpoint,
    model::{BrainPhase, BrainSettings, BrainStatus},
};

const TOKEN_HEADER: &str = "x-desktop-companion-token";
const READY_TIMEOUT: Duration = Duration::from_secs(10);
const PROBE_INTERVAL: Duration = Duration::from_secs(2);
const MAX_CONSECUTIVE_PROBE_FAILURES: u8 = 3;
const MAX_FAST_RESTARTS: u32 = 5;
const STABLE_RUNTIME: Duration = Duration::from_secs(30);

enum Control {
    Restart,
    Shutdown,
}

#[derive(Clone)]
struct RuntimePaths {
    app_data_root: PathBuf,
    data_dir: PathBuf,
    settings_path: PathBuf,
    sidecar_root: PathBuf,
}

pub struct BrainSupervisor {
    status: Arc<RwLock<BrainStatus>>,
    client: Arc<RwLock<Option<Arc<BrainClient>>>>,
    settings: Arc<RwLock<BrainSettings>>,
    paths: Option<RuntimePaths>,
    control: Mutex<Option<mpsc::Sender<Control>>>,
    worker: Mutex<Option<thread::JoinHandle<()>>>,
}

impl BrainSupervisor {
    pub fn initialize(app: &tauri::App) -> Result<Arc<Self>, String> {
        let app_data_root = app
            .path()
            .app_local_data_dir()
            .map_err(|error| error.to_string())?;
        let data_dir = app_data_root.join("brain");
        fs::create_dir_all(&data_dir).map_err(|error| error.to_string())?;
        let sidecar_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .ok_or_else(|| "cannot resolve the project root".to_string())?
            .join("brain-sidecar");
        let paths = RuntimePaths {
            app_data_root,
            settings_path: data_dir.join("settings.json"),
            data_dir,
            sidecar_root,
        };
        let settings = load_settings(&paths.settings_path)?;
        let (sender, receiver) = mpsc::channel();
        let status = Arc::new(RwLock::new(BrainStatus {
            phase: BrainPhase::Stopped,
            ready: false,
            pid: None,
            restart_count: 0,
            detail: "brain sidecar has not started".into(),
        }));
        let client = Arc::new(RwLock::new(None));
        let settings = Arc::new(RwLock::new(settings));
        let worker_status = status.clone();
        let worker_client = client.clone();
        let worker_settings = settings.clone();
        let worker_paths = paths.clone();
        let worker = thread::Builder::new()
            .name("brain-sidecar-supervisor".into())
            .spawn(move || {
                run_supervisor(
                    receiver,
                    worker_status,
                    worker_client,
                    worker_settings,
                    worker_paths,
                )
            })
            .map_err(|error| error.to_string())?;

        Ok(Arc::new(Self {
            status,
            client,
            settings,
            paths: Some(paths),
            control: Mutex::new(Some(sender)),
            worker: Mutex::new(Some(worker)),
        }))
    }

    pub fn unavailable(detail: impl Into<String>) -> Arc<Self> {
        Arc::new(Self {
            status: Arc::new(RwLock::new(BrainStatus::unavailable(detail))),
            client: Arc::new(RwLock::new(None)),
            settings: Arc::new(RwLock::new(BrainSettings::default())),
            paths: None,
            control: Mutex::new(None),
            worker: Mutex::new(None),
        })
    }

    pub fn status(&self) -> BrainStatus {
        self.status.read().clone()
    }

    pub fn settings(&self) -> BrainSettings {
        self.settings.read().clone()
    }

    pub fn configure(&self, settings: BrainSettings) -> Result<BrainSettings, String> {
        let settings = validate_settings(settings)?;
        let paths = self.paths.as_ref().ok_or_else(|| self.status().detail)?;
        if *self.settings.read() == settings {
            return Ok(settings);
        }
        save_settings(&paths.settings_path, &settings)?;
        *self.settings.write() = settings.clone();
        self.control
            .lock()
            .as_ref()
            .ok_or_else(|| "brain supervisor is stopped".to_string())?
            .send(Control::Restart)
            .map_err(|_| "brain supervisor is stopped".to_string())?;
        Ok(settings)
    }

    pub fn ready_client(&self) -> Result<Arc<BrainClient>, String> {
        let status = self.status();
        if !matches!(status.phase, BrainPhase::Ready | BrainPhase::Degraded) {
            return Err(format!("brain is not ready: {}", status.detail));
        }
        self.client
            .read()
            .clone()
            .ok_or_else(|| "brain client is unavailable".to_string())
    }

    pub fn shutdown(&self) {
        if let Some(sender) = self.control.lock().take() {
            let _ = sender.send(Control::Shutdown);
        }
    }
}

impl Drop for BrainSupervisor {
    fn drop(&mut self) {
        if let Some(sender) = self.control.get_mut().take() {
            let _ = sender.send(Control::Shutdown);
        }
        if let Some(worker) = self.worker.get_mut().take() {
            let _ = worker.join();
        }
    }
}

fn run_supervisor(
    receiver: mpsc::Receiver<Control>,
    status: Arc<RwLock<BrainStatus>>,
    client: Arc<RwLock<Option<Arc<BrainClient>>>>,
    settings: Arc<RwLock<BrainSettings>>,
    paths: RuntimePaths,
) {
    let mut restart_count = 0;
    let mut consecutive_failures: u32 = 0;
    'supervisor: loop {
        let port = match reserve_loopback_port() {
            Ok(port) => port,
            Err(error) => {
                set_status(&status, BrainPhase::Failed, None, restart_count, error);
                if wait_for_restart(&receiver, Duration::from_secs(30)) {
                    continue;
                }
                break;
            }
        };
        let token = new_session_token();
        let endpoint = match LocalEndpoint::parse(&format!("http://127.0.0.1:{port}")) {
            Ok(endpoint) => endpoint,
            Err(error) => {
                set_status(
                    &status,
                    BrainPhase::Failed,
                    None,
                    restart_count,
                    error.to_string(),
                );
                break;
            }
        };
        let brain_client = match BrainClient::new(endpoint, &token) {
            Ok(client) => Arc::new(client),
            Err(error) => {
                set_status(&status, BrainPhase::Failed, None, restart_count, error);
                break;
            }
        };
        *client.write() = Some(brain_client);

        set_status(
            &status,
            if restart_count == 0 {
                BrainPhase::Starting
            } else {
                BrainPhase::Restarting
            },
            None,
            restart_count,
            "starting local Python brain sidecar",
        );
        let mut child = match spawn_sidecar(&paths, port, &token) {
            Ok(child) => child,
            Err(error) => {
                *client.write() = None;
                restart_count = restart_count.saturating_add(1);
                consecutive_failures = consecutive_failures.saturating_add(1);
                set_status(&status, BrainPhase::Failed, None, restart_count, error);
                if !restart_delay(&receiver, consecutive_failures) {
                    break;
                }
                continue;
            }
        };
        let pid = child.id();
        #[cfg(windows)]
        let _job = match WindowsJob::assign(&child) {
            Ok(job) => job,
            Err(error) => {
                stop_child(&mut child, port, &token);
                *client.write() = None;
                restart_count = restart_count.saturating_add(1);
                consecutive_failures = consecutive_failures.saturating_add(1);
                set_status(
                    &status,
                    BrainPhase::Failed,
                    None,
                    restart_count,
                    format!("cannot contain brain sidecar in a Windows Job Object: {error}"),
                );
                if !restart_delay(&receiver, consecutive_failures) {
                    break;
                }
                continue;
            }
        };
        set_status(
            &status,
            BrainPhase::Starting,
            Some(pid),
            restart_count,
            "waiting for authenticated readiness",
        );

        let readiness_deadline = Instant::now() + READY_TIMEOUT;
        let mut last_probe = Probe::Unreachable;
        while Instant::now() < readiness_deadline {
            if let Ok(control) = receiver.try_recv() {
                stop_child(&mut child, port, &token);
                match control {
                    Control::Shutdown => break 'supervisor,
                    Control::Restart => continue 'supervisor,
                }
            }
            if child.try_wait().ok().flatten().is_some() {
                break;
            }
            last_probe = probe(port, &token);
            if !matches!(last_probe, Probe::Unreachable) {
                break;
            }
            thread::sleep(Duration::from_millis(200));
        }

        match last_probe {
            Probe::Ready => {
                set_status(
                    &status,
                    BrainPhase::Ready,
                    Some(pid),
                    restart_count,
                    "local brain and all required dependencies are ready",
                );
            }
            Probe::Degraded => set_status(
                &status,
                BrainPhase::Degraded,
                Some(pid),
                restart_count,
                "sidecar is alive but local model or memory dependencies are not ready",
            ),
            Probe::Unreachable => {
                stop_child(&mut child, port, &token);
                restart_count = restart_count.saturating_add(1);
                consecutive_failures = consecutive_failures.saturating_add(1);
                set_status(
                    &status,
                    BrainPhase::Restarting,
                    None,
                    restart_count,
                    "sidecar did not become reachable",
                );
                if !restart_delay(&receiver, consecutive_failures) {
                    break;
                }
                continue;
            }
        }

        let mut probe_failures = 0;
        let stable_since = Instant::now();
        loop {
            match receiver.recv_timeout(PROBE_INTERVAL) {
                Ok(Control::Shutdown) => {
                    stop_child(&mut child, port, &token);
                    break 'supervisor;
                }
                Ok(Control::Restart) => {
                    stop_child(&mut child, port, &token);
                    continue 'supervisor;
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    stop_child(&mut child, port, &token);
                    break 'supervisor;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }

            match child.try_wait() {
                Ok(Some(exit)) => {
                    restart_count = restart_count.saturating_add(1);
                    consecutive_failures = consecutive_failures.saturating_add(1);
                    set_status(
                        &status,
                        BrainPhase::Restarting,
                        None,
                        restart_count,
                        format!("brain sidecar exited unexpectedly ({exit})"),
                    );
                    break;
                }
                Err(error) => {
                    restart_count = restart_count.saturating_add(1);
                    consecutive_failures = consecutive_failures.saturating_add(1);
                    set_status(
                        &status,
                        BrainPhase::Restarting,
                        None,
                        restart_count,
                        format!("brain sidecar process check failed: {error}"),
                    );
                    break;
                }
                Ok(None) => {}
            }

            match probe(port, &token) {
                Probe::Ready => {
                    probe_failures = 0;
                    if stable_since.elapsed() >= STABLE_RUNTIME {
                        consecutive_failures = 0;
                    }
                    set_status(
                        &status,
                        BrainPhase::Ready,
                        Some(pid),
                        restart_count,
                        "local brain and all required dependencies are ready",
                    );
                }
                Probe::Degraded => {
                    probe_failures = 0;
                    if stable_since.elapsed() >= STABLE_RUNTIME {
                        consecutive_failures = 0;
                    }
                    set_status(
                        &status,
                        BrainPhase::Degraded,
                        Some(pid),
                        restart_count,
                        "sidecar is alive but local model or memory dependencies are not ready",
                    );
                }
                Probe::Unreachable => {
                    probe_failures += 1;
                    if probe_failures >= MAX_CONSECUTIVE_PROBE_FAILURES {
                        stop_child(&mut child, port, &token);
                        restart_count = restart_count.saturating_add(1);
                        consecutive_failures = consecutive_failures.saturating_add(1);
                        set_status(
                            &status,
                            BrainPhase::Restarting,
                            None,
                            restart_count,
                            "brain sidecar stopped responding",
                        );
                        break;
                    }
                }
            }
        }
        *client.write() = None;
        if !restart_delay(&receiver, consecutive_failures) {
            break;
        }
    }
    *client.write() = None;
    set_status(
        &status,
        BrainPhase::Stopped,
        None,
        restart_count,
        "brain sidecar stopped",
    );
    let _ = settings;
}

fn reserve_loopback_port() -> Result<u16, String> {
    TcpListener::bind(("127.0.0.1", 0))
        .and_then(|listener| listener.local_addr())
        .map(|address| address.port())
        .map_err(|error| format!("cannot reserve a loopback port: {error}"))
}

fn new_session_token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

fn spawn_sidecar(paths: &RuntimePaths, port: u16, token: &str) -> Result<Child, String> {
    let virtualenv_python = paths
        .sidecar_root
        .join(".venv")
        .join("Scripts")
        .join("python.exe");
    let executable = std::env::var_os("DESKTOP_COMPANION_PYTHON")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            if virtualenv_python.is_file() {
                virtualenv_python
            } else {
                PathBuf::from("python")
            }
        });
    let source = paths.sidecar_root.join("src");
    if !source.is_dir() {
        return Err(format!(
            "brain sidecar source directory is missing: {}",
            source.display()
        ));
    }
    let mut command = Command::new(executable);
    command
        .arg("-m")
        .arg("desktop_companion_brain")
        .current_dir(&paths.sidecar_root)
        .env("PYTHONPATH", &source)
        .env("DESKTOP_COMPANION_BRAIN_HOST", "127.0.0.1")
        .env("DESKTOP_COMPANION_BRAIN_PORT", port.to_string())
        .env("DESKTOP_COMPANION_BRAIN_TOKEN", token)
        .env("DESKTOP_COMPANION_APP_DATA_ROOT", &paths.app_data_root)
        .env("DESKTOP_COMPANION_BRAIN_DATA_DIR", &paths.data_dir)
        .env("MEM0_TELEMETRY", "false")
        .env("ANONYMIZED_TELEMETRY", "false")
        .env("POSTHOG_DISABLED", "true")
        .env("NO_PROXY", "127.0.0.1,localhost,::1")
        .env_remove("HTTP_PROXY")
        .env_remove("HTTPS_PROXY")
        .env_remove("ALL_PROXY")
        .env_remove("http_proxy")
        .env_remove("https_proxy")
        .env_remove("all_proxy")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    command
        .spawn()
        .map_err(|error| format!("cannot start local Python brain sidecar: {error}"))
}

fn stop_child(child: &mut Child, port: u16, token: &str) {
    let _ = http_request(port, token, "POST", "/shutdown");
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if child.try_wait().ok().flatten().is_some() {
            return;
        }
        thread::sleep(Duration::from_millis(50));
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[derive(Clone, Copy)]
enum Probe {
    Ready,
    Degraded,
    Unreachable,
}

fn probe(port: u16, token: &str) -> Probe {
    match http_request(port, token, "GET", "/ready") {
        Ok(status) if status == 200 => Probe::Ready,
        Ok(status) if status == 503 => Probe::Degraded,
        Ok(_) => Probe::Unreachable,
        Err(_) => Probe::Unreachable,
    }
}

fn http_request(port: u16, token: &str, method: &str, path: &str) -> Result<u16, String> {
    let mut stream = TcpStream::connect_timeout(
        &format!("127.0.0.1:{port}")
            .parse()
            .map_err(|_| "invalid sidecar socket address".to_string())?,
        Duration::from_millis(500),
    )
    .map_err(|error| error.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_millis(800)))
        .map_err(|error| error.to_string())?;
    let body = if method == "POST" { "{}" } else { "" };
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{TOKEN_HEADER}: {token}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|error| error.to_string())?;
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|error| error.to_string())?;
    response
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|status| status.parse().ok())
        .ok_or_else(|| "sidecar returned an invalid HTTP response".to_string())
}

fn restart_delay(receiver: &mpsc::Receiver<Control>, restart_count: u32) -> bool {
    let delay = restart_backoff(restart_count);
    match receiver.recv_timeout(delay) {
        Ok(Control::Shutdown) | Err(mpsc::RecvTimeoutError::Disconnected) => false,
        Ok(Control::Restart) | Err(mpsc::RecvTimeoutError::Timeout) => true,
    }
}

fn wait_for_restart(receiver: &mpsc::Receiver<Control>, timeout: Duration) -> bool {
    match receiver.recv_timeout(timeout) {
        Ok(Control::Restart) | Err(mpsc::RecvTimeoutError::Timeout) => true,
        Ok(Control::Shutdown) | Err(mpsc::RecvTimeoutError::Disconnected) => false,
    }
}

fn restart_backoff(restart_count: u32) -> Duration {
    if restart_count > MAX_FAST_RESTARTS {
        return Duration::from_secs(30);
    }
    Duration::from_millis(250 * (1_u64 << restart_count.min(5)))
}

fn set_status(
    status: &RwLock<BrainStatus>,
    phase: BrainPhase,
    pid: Option<u32>,
    restart_count: u32,
    detail: impl Into<String>,
) {
    *status.write() = BrainStatus {
        phase,
        ready: phase == BrainPhase::Ready,
        pid,
        restart_count,
        detail: detail.into(),
    };
}

fn validate_settings(settings: BrainSettings) -> Result<BrainSettings, String> {
    let settings = settings.normalized();
    for (name, value) in [
        ("LLM", &settings.llm_base_url),
        ("embedding", &settings.embedding_base_url),
    ] {
        if !value.is_empty() {
            LocalEndpoint::parse(value)
                .map_err(|error| format!("{name} endpoint was rejected: {error}"))?;
        }
    }
    for (name, value) in [
        ("LLM", &settings.llm_model),
        ("embedding", &settings.embedding_model),
    ] {
        if value.len() > 200 || value.chars().any(char::is_control) {
            return Err(format!("{name} model name is invalid"));
        }
    }
    if settings.llm_base_url.is_empty() != settings.llm_model.is_empty() {
        return Err("local LLM endpoint and model must be configured together".into());
    }
    if settings.embedding_base_url.is_empty() != settings.embedding_model.is_empty() {
        return Err("local embedding endpoint and model must be configured together".into());
    }
    Ok(settings)
}

fn load_settings(path: &Path) -> Result<BrainSettings, String> {
    let (settings, should_save) = if path.exists() {
        let content = fs::read_to_string(path).map_err(|error| error.to_string())?;
        let persisted: serde_json::Value =
            serde_json::from_str(&content).map_err(|error| error.to_string())?;
        let settings: BrainSettings =
            serde_json::from_value(persisted.clone()).map_err(|error| error.to_string())?;
        let settings = validate_settings(settings)?;
        let normalized = serde_json::to_value(&settings).map_err(|error| error.to_string())?;
        (settings, persisted != normalized)
    } else {
        (BrainSettings::default(), true)
    };
    if should_save {
        save_settings(path, &settings)?;
    }
    Ok(settings)
}

fn save_settings(path: &Path, settings: &BrainSettings) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "brain settings path has no parent".to_string())?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let content = serde_json::to_vec_pretty(settings).map_err(|error| error.to_string())?;
    let temporary = parent.join("settings.json.tmp");
    let mut file = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temporary)
        .map_err(|error| error.to_string())?;
    file.write_all(&content)
        .and_then(|_| file.sync_all())
        .map_err(|error| error.to_string())?;
    drop(file);
    replace_file(&temporary, path)
}

#[cfg(windows)]
fn replace_file(source: &Path, target: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let target: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
    unsafe {
        MoveFileExW(
            PCWSTR(source.as_ptr()),
            PCWSTR(target.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
        .map_err(|error| error.to_string())
    }
}

#[cfg(not(windows))]
fn replace_file(source: &Path, target: &Path) -> Result<(), String> {
    fs::rename(source, target).map_err(|error| error.to_string())
}

#[cfg(windows)]
struct WindowsJob(windows::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl WindowsJob {
    fn assign(child: &Child) -> windows::core::Result<Self> {
        use std::{mem::size_of, os::windows::io::AsRawHandle};
        use windows::{
            core::PCWSTR,
            Win32::{
                Foundation::HANDLE,
                System::JobObjects::{
                    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
                    SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
                    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                },
            },
        };
        unsafe {
            let job = Self(CreateJobObjectW(None, PCWSTR::null())?);
            let mut information = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            information.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                (&information as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )?;
            AssignProcessToJobObject(job.0, HANDLE(child.as_raw_handle()))?;
            Ok(job)
        }
    }
}

#[cfg(windows)]
impl Drop for WindowsJob {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAKE_READY_SERVER: &str = r#"
import json
import os
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

token = os.environ["DESKTOP_COMPANION_BRAIN_TOKEN"]
data_dir = Path(os.environ["DESKTOP_COMPANION_BRAIN_DATA_DIR"])
data_dir.mkdir(parents=True, exist_ok=True)

class Handler(BaseHTTPRequestHandler):
    def log_message(self, format, *args):
        return

    def authenticated(self):
        if self.headers.get("x-desktop-companion-token", "") == token:
            return True
        self.send_response(401)
        self.send_header("Content-Length", "0")
        self.end_headers()
        return False

    def do_GET(self):
        if not self.authenticated():
            return
        body = json.dumps({"status": "ok"}).encode()
        self.send_response(int(os.environ.get("FAKE_READY_STATUS", "200")))
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self):
        if not self.authenticated():
            return
        if self.path == "/shutdown":
            (data_dir / "graceful-shutdown.marker").write_text("received")
            self.send_response(200)
            self.send_header("Content-Length", "0")
            self.end_headers()
            threading.Thread(target=server.shutdown, daemon=True).start()
            return
        self.send_response(404)
        self.send_header("Content-Length", "0")
        self.end_headers()

server = ThreadingHTTPServer(("127.0.0.1", int(os.environ["DESKTOP_COMPANION_BRAIN_PORT"])), Handler)
if os.environ.get("FAKE_CRASH_ONCE") == "1":
    marker = data_dir / "crash-once.marker"
    if not marker.exists():
        marker.write_text("armed")
        threading.Timer(0.7, lambda: os._exit(17)).start()
server.serve_forever(poll_interval=0.05)
server.server_close()
"#;

    const FAKE_IMMEDIATE_EXIT: &str = r#"
import os
import time
from pathlib import Path

data_dir = Path(os.environ["DESKTOP_COMPANION_BRAIN_DATA_DIR"])
data_dir.mkdir(parents=True, exist_ok=True)
with (data_dir / "starts.log").open("a", encoding="utf-8") as stream:
    stream.write(f"{time.time()}\n")
raise SystemExit(23)
"#;

    fn test_paths(script: &str) -> (RuntimePaths, PathBuf) {
        let root =
            std::env::temp_dir().join(format!("desktop-companion-supervisor-{}", Uuid::new_v4()));
        let sidecar_root = root.join("sidecar");
        let package = sidecar_root.join("src").join("desktop_companion_brain");
        fs::create_dir_all(&package).unwrap();
        fs::write(package.join("__main__.py"), script).unwrap();
        let app_data_root = root.join("app-data");
        let data_dir = app_data_root.join("brain");
        fs::create_dir_all(&data_dir).unwrap();
        (
            RuntimePaths {
                settings_path: data_dir.join("settings.json"),
                app_data_root,
                data_dir,
                sidecar_root,
            },
            root,
        )
    }

    fn start_test_supervisor(
        paths: RuntimePaths,
    ) -> (
        mpsc::Sender<Control>,
        Arc<RwLock<BrainStatus>>,
        thread::JoinHandle<()>,
    ) {
        let (sender, receiver) = mpsc::channel();
        let status = Arc::new(RwLock::new(BrainStatus {
            phase: BrainPhase::Stopped,
            ready: false,
            pid: None,
            restart_count: 0,
            detail: "test supervisor has not started".into(),
        }));
        let worker_status = status.clone();
        let worker = thread::spawn(move || {
            run_supervisor(
                receiver,
                worker_status,
                Arc::new(RwLock::new(None)),
                Arc::new(RwLock::new(BrainSettings::default())),
                paths,
            )
        });
        (sender, status, worker)
    }

    fn wait_for_status(
        status: &Arc<RwLock<BrainStatus>>,
        timeout: Duration,
        predicate: impl Fn(&BrainStatus) -> bool,
    ) -> Option<BrainStatus> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            let snapshot = status.read().clone();
            if predicate(&snapshot) {
                return Some(snapshot);
            }
            thread::sleep(Duration::from_millis(25));
        }
        None
    }

    fn python_process_exists(pid: u32) -> bool {
        Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
            .output()
            .map(|output| {
                String::from_utf8_lossy(&output.stdout).contains(&format!(r#","{pid}","#))
            })
            .unwrap_or(false)
    }

    #[test]
    fn restart_backoff_is_bounded() {
        assert_eq!(restart_backoff(0), Duration::from_millis(250));
        assert_eq!(restart_backoff(3), Duration::from_secs(2));
        assert_eq!(restart_backoff(10), Duration::from_secs(30));
    }

    #[test]
    fn settings_reject_remote_endpoints_and_incomplete_pairs() {
        let mut settings = BrainSettings {
            llm_base_url: "http://8.8.8.8:11434/v1".into(),
            llm_model: "model".into(),
            memory_enabled: false,
            ..BrainSettings::default()
        };
        assert!(validate_settings(settings.clone()).is_err());
        settings.llm_base_url = "http://127.0.0.1:11434/v1".into();
        assert!(validate_settings(settings.clone()).is_ok());
        settings.llm_model.clear();
        assert!(validate_settings(settings).is_err());
    }

    #[test]
    fn settings_are_created_and_replaced_on_windows() {
        let root =
            std::env::temp_dir().join(format!("desktop-companion-settings-{}", Uuid::new_v4()));
        let path = root.join("settings.json");
        let first = BrainSettings::default();
        let second = BrainSettings {
            memory_enabled: false,
            ..BrainSettings::default()
        };

        save_settings(&path, &first).unwrap();
        save_settings(&path, &second).unwrap();
        assert_eq!(load_settings(&path).unwrap(), second);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn legacy_settings_are_rewritten_with_explicit_embedding_dimensions() {
        let root =
            std::env::temp_dir().join(format!("desktop-companion-settings-{}", Uuid::new_v4()));
        let path = root.join("settings.json");
        fs::create_dir_all(&root).unwrap();
        fs::write(&path, r#"{"memoryEnabled":false}"#).unwrap();

        let settings = load_settings(&path).unwrap();
        let migrated = fs::read_to_string(&path).unwrap();
        assert_eq!(settings.embedding_dimensions, 768);
        assert!(migrated.contains(r#""embeddingDimensions": 768"#));

        let _ = fs::remove_dir_all(root);
    }

    #[cfg(windows)]
    #[test]
    fn job_object_binding_failure_is_reported_for_an_exited_child() {
        let mut child = Command::new("cmd")
            .args(["/C", "exit", "0"])
            .spawn()
            .expect("test child should start");
        let exit = child.wait().expect("test child should exit");
        assert!(exit.success());
        assert!(WindowsJob::assign(&child).is_err());
        assert!(child.try_wait().unwrap().is_some());
    }

    #[test]
    fn session_tokens_are_ephemeral_and_not_guessable_identifiers() {
        let first = new_session_token();
        let second = new_session_token();
        assert_eq!(first.len(), 64);
        assert_ne!(first, second);
        assert!(first.chars().all(|character| character.is_ascii_hexdigit()));
    }

    #[test]
    fn unauthorized_or_unknown_http_services_are_not_accepted_as_degraded_sidecars() {
        for status_code in [401, 500] {
            let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
            let port = listener.local_addr().unwrap().port();
            let server = thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0_u8; 2048];
                let _ = stream.read(&mut request);
                let response = format!("HTTP/1.1 {status_code} Test\r\nContent-Length: 0\r\n\r\n");
                stream.write_all(response.as_bytes()).unwrap();
            });
            let result = probe(port, "test-session-token");
            server.join().unwrap();
            assert!(matches!(result, Probe::Unreachable));
        }
    }

    #[test]
    fn slow_start_remains_unready_until_authenticated_readiness_succeeds() {
        let script = format!("import time\ntime.sleep(0.8)\n{FAKE_READY_SERVER}");
        let (paths, root) = test_paths(&script);
        let (sender, status, worker) = start_test_supervisor(paths);
        thread::sleep(Duration::from_millis(250));
        let during_start = status.read().clone();
        let ready = wait_for_status(&status, Duration::from_secs(5), |value| value.ready);
        let _ = sender.send(Control::Shutdown);
        let joined = worker.join().is_ok();
        let _ = fs::remove_dir_all(root);

        assert_eq!(during_start.phase, BrainPhase::Starting);
        assert!(!during_start.ready);
        assert!(ready.is_some());
        assert!(joined);
    }

    #[test]
    fn immediate_exit_restarts_with_increasing_delays_without_killing_the_host() {
        let (paths, root) = test_paths(FAKE_IMMEDIATE_EXIT);
        let starts_path = paths.data_dir.join("starts.log");
        let (sender, status, worker) = start_test_supervisor(paths);
        let restarted = wait_for_status(&status, Duration::from_secs(5), |value| {
            value.restart_count >= 2
        });
        let _ = sender.send(Control::Shutdown);
        let joined = worker.join().is_ok();
        let starts = fs::read_to_string(starts_path)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| line.parse::<f64>().ok())
            .collect::<Vec<_>>();
        let _ = fs::remove_dir_all(root);

        assert!(restarted.is_some());
        assert!(joined);
        assert!(starts.len() >= 2);
        assert!(starts[1] - starts[0] >= 0.35);
        if starts.len() >= 3 {
            assert!(starts[2] - starts[1] >= 0.75);
        }
    }

    #[test]
    fn crash_after_ready_is_detected_and_the_sidecar_is_restarted() {
        let script = [
            "import os\n",
            "os.environ.update(dict(FAKE_CRASH_ONCE=str(1)))\n",
            FAKE_READY_SERVER,
        ]
        .concat();
        let (paths, root) = test_paths(&script);
        let (sender, status, worker) = start_test_supervisor(paths);
        let first = wait_for_status(&status, Duration::from_secs(5), |value| value.ready);
        let first_pid = first.as_ref().and_then(|value| value.pid);
        let replacement = wait_for_status(&status, Duration::from_secs(7), |value| {
            value.ready && value.pid.is_some() && value.pid != first_pid
        });
        let _ = sender.send(Control::Shutdown);
        let joined = worker.join().is_ok();
        let _ = fs::remove_dir_all(root);

        assert!(first_pid.is_some());
        assert!(replacement.is_some());
        assert!(joined);
    }

    #[test]
    fn dependency_failure_keeps_the_host_alive_in_degraded_state() {
        let script = [
            "import os\n",
            "os.environ.update(dict(FAKE_READY_STATUS=str(503)))\n",
            FAKE_READY_SERVER,
        ]
        .concat();
        let (paths, root) = test_paths(&script);
        let (sender, status, worker) = start_test_supervisor(paths);
        let degraded = wait_for_status(&status, Duration::from_secs(5), |value| {
            value.phase == BrainPhase::Degraded
        });
        let first_pid = degraded.as_ref().and_then(|value| value.pid);
        thread::sleep(PROBE_INTERVAL + Duration::from_millis(300));
        let stable = status.read().clone();
        let _ = sender.send(Control::Shutdown);
        let joined = worker.join().is_ok();
        let _ = fs::remove_dir_all(root);

        assert!(degraded.is_some());
        assert!(!stable.ready);
        assert_eq!(stable.phase, BrainPhase::Degraded);
        assert_eq!(stable.pid, first_pid);
        assert!(joined);
    }

    #[test]
    fn shutdown_is_graceful_and_leaves_no_sidecar_process() {
        let (paths, root) = test_paths(FAKE_READY_SERVER);
        let shutdown_marker = paths.data_dir.join("graceful-shutdown.marker");
        let (sender, status, worker) = start_test_supervisor(paths);
        let ready = wait_for_status(&status, Duration::from_secs(5), |value| value.ready);
        let pid = ready.as_ref().and_then(|value| value.pid);
        let _ = sender.send(Control::Shutdown);
        let joined = worker.join().is_ok();
        let final_status = status.read().clone();
        let graceful = shutdown_marker.is_file();
        let residual = pid.is_some_and(python_process_exists);
        let _ = fs::remove_dir_all(root);

        assert!(ready.is_some());
        assert!(joined);
        assert!(graceful);
        assert!(!residual);
        assert_eq!(final_status.phase, BrainPhase::Stopped);
        assert_eq!(final_status.pid, None);
    }
}
