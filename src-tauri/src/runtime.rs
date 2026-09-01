use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager, PhysicalPosition, WebviewWindow};

use crate::{
    character::state::{CharacterMode, CharacterState},
    desktop::{
        physics::update_character_physics, surface::monitor_floor_for_point, world::DesktopWorld,
    },
    error::{AppError, AppResult},
    windows::{cursor, enumeration, monitor},
};

const COMPANION_TITLE: &str = "Windows AI 桌面伴侣";
const PHYSICS_STEP: f32 = 1.0 / 60.0;
const DESKTOP_REFRESH: Duration = Duration::from_millis(125);
const MONITOR_REFRESH: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HitShape {
    Rect,
    Ellipse,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HitRegion {
    pub shape: HitShape,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl HitRegion {
    fn contains(&self, x: f32, y: f32) -> bool {
        if self.width <= 0.0 || self.height <= 0.0 {
            return false;
        }
        match self.shape {
            HitShape::Rect => {
                x >= self.x && y >= self.y && x <= self.x + self.width && y <= self.y + self.height
            }
            HitShape::Ellipse => {
                let nx = (x - (self.x + self.width / 2.0)) / (self.width / 2.0);
                let ny = (y - (self.y + self.height / 2.0)) / (self.height / 2.0);
                nx * nx + ny * ny <= 1.0
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HitRegionPayload {
    pub regions: Vec<HitRegion>,
    pub foot_x: f32,
    pub foot_y: f32,
    pub scale_factor: f32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CursorPayload {
    x: f32,
    y: f32,
    inside_character: bool,
}

pub struct RuntimeState {
    pub character: Mutex<CharacterState>,
    pub desktop: RwLock<DesktopWorld>,
    hit_regions: RwLock<Vec<HitRegion>>,
    scale_factor: RwLock<f32>,
    cursor_ignored: AtomicBool,
    shutdown: AtomicBool,
}

impl RuntimeState {
    pub fn new(character: CharacterState, scale_factor: f32) -> Self {
        Self {
            character: Mutex::new(character),
            desktop: RwLock::new(DesktopWorld::default()),
            hit_regions: RwLock::new(Vec::new()),
            scale_factor: RwLock::new(scale_factor.max(0.25)),
            cursor_ignored: AtomicBool::new(false),
            shutdown: AtomicBool::new(false),
        }
    }

    pub fn update_hit_regions(&self, payload: HitRegionPayload) -> AppResult<()> {
        if payload.regions.len() > 16
            || !payload.scale_factor.is_finite()
            || payload.scale_factor <= 0.0
            || !payload.foot_x.is_finite()
            || !payload.foot_y.is_finite()
            || payload.regions.iter().any(|region| {
                !region.x.is_finite()
                    || !region.y.is_finite()
                    || !region.width.is_finite()
                    || !region.height.is_finite()
                    || region.width < 0.0
                    || region.height < 0.0
            })
        {
            return Err(AppError::InvalidState("命中区域参数无效".into()));
        }

        *self.hit_regions.write() = payload.regions;
        *self.scale_factor.write() = payload.scale_factor;
        let mut character = self.character.lock();
        character.update_foot_offset(
            payload.foot_x * payload.scale_factor,
            payload.foot_y * payload.scale_factor,
        );
        Ok(())
    }

    pub fn begin_drag(&self) -> AppResult<()> {
        let cursor = cursor::get_cursor_position()?;
        let mut character = self.character.lock();
        character.grabbed = true;
        character.mode = CharacterMode::Dragged;
        character.support_surface = None;
        character.vx = 0.0;
        character.vy = 0.0;
        character.grab_offset_x = cursor.x as f32 - character.x;
        character.grab_offset_y = cursor.y as f32 - character.y;
        Ok(())
    }

    pub fn end_drag(&self) {
        let mut character = self.character.lock();
        if character.grabbed {
            character.grabbed = false;
            character.mode = CharacterMode::Falling;
            character.vx = 0.0;
            character.vy = 0.0;
        }
    }

    fn hit_test(&self, local_x: f32, local_y: f32) -> bool {
        self.hit_regions
            .read()
            .iter()
            .any(|region| region.contains(local_x, local_y))
    }
}

impl Drop for RuntimeState {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
    }
}

pub fn start_runtime(window: WebviewWindow, state: Arc<RuntimeState>) -> AppResult<()> {
    thread::Builder::new()
        .name("desktop-companion-runtime".into())
        .spawn(move || runtime_loop(window, state))
        .map(|_| ())
        .map_err(|error| AppError::InvalidState(format!("启动桌面运行线程失败: {error}")))
}

fn runtime_loop(window: WebviewWindow, state: Arc<RuntimeState>) {
    let mut previous = Instant::now();
    let mut physics_accumulator = 0.0_f32;
    let mut desktop_refresh = Instant::now() - DESKTOP_REFRESH;
    let mut monitor_refresh = Instant::now() - MONITOR_REFRESH;
    let mut last_emitted_mode = CharacterMode::Falling;
    let mut last_state_emit = Instant::now() - Duration::from_secs(1);
    let mut last_window_position: Option<(i32, i32)> = None;
    #[cfg(debug_assertions)]
    let mut last_debug_log = Instant::now() - Duration::from_secs(1);

    while !state.shutdown.load(Ordering::Acquire) {
        let now = Instant::now();
        let elapsed = now.duration_since(previous).as_secs_f32().min(0.05);
        previous = now;
        physics_accumulator += elapsed;

        if now.duration_since(desktop_refresh) >= DESKTOP_REFRESH {
            refresh_desktop(
                &state,
                now.duration_since(monitor_refresh) >= MONITOR_REFRESH,
            );
            desktop_refresh = now;
            if now.duration_since(monitor_refresh) >= MONITOR_REFRESH {
                monitor_refresh = now;
            }
        }

        let cursor_position = cursor::get_cursor_position().ok();
        if let Some(cursor_position) = cursor_position {
            state.desktop.write().cursor = cursor_position;
            let scale_factor = *state.scale_factor.read();
            let mut character = state.character.lock();

            if character.grabbed {
                if cursor::is_left_button_down() {
                    character.x = cursor_position.x as f32 - character.grab_offset_x;
                    character.y = cursor_position.y as f32 - character.grab_offset_y;
                } else {
                    character.grabbed = false;
                    character.mode = CharacterMode::Falling;
                }
            }

            let local_x = (cursor_position.x as f32 - character.x) / scale_factor;
            let local_y = (cursor_position.y as f32 - character.y) / scale_factor;
            let inside = character.grabbed || state.hit_test(local_x, local_y);
            drop(character);

            update_click_through(&window, &state, !inside);
            let _ = window.emit(
                "cursor://position",
                CursorPayload {
                    x: local_x,
                    y: local_y,
                    inside_character: inside,
                },
            );
        }

        while physics_accumulator >= PHYSICS_STEP {
            let world = state.desktop.read().clone();
            let mut character = state.character.lock();
            update_character_physics(&mut character, &world, PHYSICS_STEP);
            physics_accumulator -= PHYSICS_STEP;
        }

        let snapshot = state.character.lock().clone();
        #[cfg(debug_assertions)]
        if now.duration_since(last_debug_log) >= Duration::from_secs(1) {
            eprintln!(
                "[character-state] mode={:?} window_y={:.2} foot_y={:.2} support_surface={:?}",
                snapshot.mode,
                snapshot.y,
                snapshot.y + snapshot.foot_offset_y,
                snapshot.support_surface
            );
            last_debug_log = now;
        }
        let next_position = (snapshot.x.round() as i32, snapshot.y.round() as i32);
        if last_window_position != Some(next_position) {
            if window
                .set_position(PhysicalPosition::new(next_position.0, next_position.1))
                .is_err()
            {
                break;
            }
            last_window_position = Some(next_position);
        }
        if snapshot.mode != last_emitted_mode
            || now.duration_since(last_state_emit) >= Duration::from_millis(250)
        {
            let _ = window.emit("character://state", snapshot.clone());
            last_emitted_mode = snapshot.mode;
            last_state_emit = now;
        }

        thread::sleep(Duration::from_millis(8));
    }
}

fn refresh_desktop(state: &RuntimeState, include_monitors: bool) {
    let windows = enumeration::enumerate_windows(COMPANION_TITLE);
    let monitors = include_monitors.then(monitor::enumerate_monitors);
    let foreground = enumeration::foreground_window();
    let mut world = state.desktop.write();
    if let Ok(windows) = windows {
        world.windows = windows;
    }
    if let Some(Ok(monitors)) = monitors {
        world.monitors = monitors;
    }
    world.foreground_window = foreground;
}

fn update_click_through(window: &WebviewWindow, state: &RuntimeState, ignore: bool) {
    let previous = state.cursor_ignored.swap(ignore, Ordering::AcqRel);
    if previous != ignore {
        let _ = window.set_ignore_cursor_events(ignore);
    }
}

pub fn initialize(app: &tauri::App) -> AppResult<Arc<RuntimeState>> {
    let window = app
        .get_webview_window("character")
        .ok_or_else(|| AppError::InvalidState("找不到 character 窗口".into()))?;
    let position = window
        .outer_position()
        .map_err(|error| AppError::InvalidState(format!("读取窗口位置失败: {error}")))?;
    let size = window
        .outer_size()
        .map_err(|error| AppError::InvalidState(format!("读取窗口尺寸失败: {error}")))?;
    let scale_factor = window.scale_factor().unwrap_or(1.0) as f32;
    let character = CharacterState::new(
        position.x as f32,
        position.y as f32,
        size.width as f32,
        size.height as f32,
    );
    let state = Arc::new(RuntimeState::new(character, scale_factor));
    refresh_desktop(&state, true);
    {
        let world = state.desktop.read();
        let mut character = state.character.lock();
        let window_center_x = character.x + size.width as f32 / 2.0;
        let window_center_y = character.y + size.height as f32 / 2.0;
        if let Some(floor) = monitor_floor_for_point(&world, window_center_x, window_center_y) {
            character.y = floor.y - character.foot_offset_y;
            character.vy = 0.0;
            character.mode = CharacterMode::Idle;
            character.support_surface = Some(floor.surface);
            window
                .set_position(PhysicalPosition::new(
                    character.x.round() as i32,
                    character.y.round() as i32,
                ))
                .map_err(|error| {
                    AppError::InvalidState(format!("设置初始窗口位置失败: {error}"))
                })?;
        }
    }
    start_runtime(window, Arc::clone(&state))?;
    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ellipse_hit_test_rejects_bounding_box_corners() {
        let region = HitRegion {
            shape: HitShape::Ellipse,
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 80.0,
        };
        assert!(region.contains(60.0, 60.0));
        assert!(!region.contains(10.0, 20.0));
    }

    #[test]
    fn rectangle_hit_test_includes_edges() {
        let region = HitRegion {
            shape: HitShape::Rect,
            x: -10.0,
            y: 5.0,
            width: 20.0,
            height: 30.0,
        };
        assert!(region.contains(-10.0, 5.0));
        assert!(region.contains(10.0, 35.0));
        assert!(!region.contains(10.1, 35.0));
    }
}
