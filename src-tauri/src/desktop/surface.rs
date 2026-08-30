use serde::Serialize;

use super::world::{DesktopWorld, MonitorInfo};

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum DesktopSurface {
    MonitorFloor { monitor_id: String },
    WindowTop { hwnd: isize, title: String },
}

#[derive(Clone, Debug)]
pub struct SurfaceHit {
    pub y: f32,
    pub surface: DesktopSurface,
}

pub fn find_crossed_surface(
    world: &DesktopWorld,
    foot_x: f32,
    previous_foot_y: f32,
    current_foot_y: f32,
) -> Option<SurfaceHit> {
    let mut hits = Vec::new();

    for window in &world.windows {
        let top = window.top as f32;
        if foot_x >= window.left as f32
            && foot_x <= window.right as f32
            && previous_foot_y <= top
            && current_foot_y >= top
        {
            hits.push(SurfaceHit {
                y: top,
                surface: DesktopSurface::WindowTop {
                    hwnd: window.hwnd,
                    title: window.title.clone(),
                },
            });
        }
    }

    let mut crossed_monitor = false;
    for monitor in &world.monitors {
        let floor = monitor.work_bottom as f32;
        if foot_x >= monitor.work_left as f32
            && foot_x <= monitor.work_right as f32
            && previous_foot_y <= floor
            && current_foot_y >= floor
        {
            crossed_monitor = true;
            hits.push(SurfaceHit {
                y: floor,
                surface: DesktopSurface::MonitorFloor {
                    monitor_id: monitor.id.clone(),
                },
            });
        }
    }

    if !crossed_monitor {
        if let Some(monitor) = select_monitor_for_point(world, foot_x, current_foot_y) {
            let floor = monitor.work_bottom as f32;
            if current_foot_y >= floor {
                hits.push(SurfaceHit {
                    y: floor,
                    surface: DesktopSurface::MonitorFloor {
                        monitor_id: monitor.id.clone(),
                    },
                });
            }
        }
    }

    hits.into_iter()
        .min_by(|left, right| left.y.total_cmp(&right.y))
}

pub fn monitor_floor_for_point(
    world: &DesktopWorld,
    point_x: f32,
    point_y: f32,
) -> Option<SurfaceHit> {
    let monitor = select_monitor_for_point(world, point_x, point_y)?;

    Some(SurfaceHit {
        y: monitor.work_bottom as f32,
        surface: DesktopSurface::MonitorFloor {
            monitor_id: monitor.id.clone(),
        },
    })
}

fn select_monitor_for_point(
    world: &DesktopWorld,
    point_x: f32,
    point_y: f32,
) -> Option<&MonitorInfo> {
    world
        .monitors
        .iter()
        .find(|monitor| {
            point_x >= monitor.left as f32
                && point_x < monitor.right as f32
                && point_y >= monitor.top as f32
                && point_y < monitor.bottom as f32
        })
        .or_else(|| {
            world
                .monitors
                .iter()
                .filter(|monitor| point_x >= monitor.left as f32 && point_x < monitor.right as f32)
                .min_by(|left, right| {
                    vertical_distance(left, point_y).total_cmp(&vertical_distance(right, point_y))
                })
        })
        .or_else(|| world.monitors.iter().find(|monitor| monitor.primary))
        .or_else(|| world.monitors.first())
}

fn vertical_distance(monitor: &MonitorInfo, y: f32) -> f32 {
    if y < monitor.top as f32 {
        monitor.top as f32 - y
    } else if y >= monitor.bottom as f32 {
        y - monitor.bottom as f32
    } else {
        0.0
    }
}

pub fn surface_still_supports(
    world: &DesktopWorld,
    surface: &DesktopSurface,
    foot_x: f32,
    foot_y: f32,
) -> bool {
    const TOLERANCE: f32 = 3.0;
    match surface {
        DesktopSurface::MonitorFloor { monitor_id } => world.monitors.iter().any(|monitor| {
            &monitor.id == monitor_id
                && foot_x >= monitor.work_left as f32
                && foot_x <= monitor.work_right as f32
                && (foot_y - monitor.work_bottom as f32).abs() <= TOLERANCE
        }),
        DesktopSurface::WindowTop { hwnd, .. } => world.windows.iter().any(|window| {
            window.hwnd == *hwnd
                && foot_x >= window.left as f32
                && foot_x <= window.right as f32
                && (foot_y - window.top as f32).abs() <= TOLERANCE
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::world::{DesktopPoint, DesktopWindow, MonitorInfo};

    fn world() -> DesktopWorld {
        DesktopWorld {
            monitors: vec![MonitorInfo {
                id: "left".into(),
                left: -1920,
                top: 0,
                right: 0,
                bottom: 1080,
                work_left: -1920,
                work_top: 0,
                work_right: 0,
                work_bottom: 1040,
                dpi: 144,
                primary: false,
            }],
            windows: vec![DesktopWindow {
                hwnd: 42,
                process_id: 7,
                title: "Editor".into(),
                left: -1500,
                top: 240,
                right: -500,
                bottom: 900,
                visible: true,
                minimized: false,
            }],
            foreground_window: Some(42),
            cursor: DesktopPoint::default(),
        }
    }

    #[test]
    fn finds_window_before_monitor_floor_on_negative_monitor() {
        let hit = find_crossed_surface(&world(), -800.0, 200.0, 1100.0).expect("surface");
        assert_eq!(hit.y, 240.0);
        assert!(matches!(
            hit.surface,
            DesktopSurface::WindowTop { hwnd: 42, .. }
        ));
    }

    #[test]
    fn finds_monitor_floor_when_no_window_overlaps() {
        let hit = find_crossed_surface(&world(), -1800.0, 900.0, 1100.0).expect("surface");
        assert_eq!(hit.y, 1040.0);
        assert!(matches!(hit.surface, DesktopSurface::MonitorFloor { .. }));
    }

    #[test]
    fn hard_monitor_floor_recovers_a_character_already_below_it() {
        let hit = find_crossed_surface(&world(), -1800.0, 1100.0, 1120.0).expect("surface");
        assert_eq!(hit.y, 1040.0);
        assert!(matches!(hit.surface, DesktopSurface::MonitorFloor { .. }));
    }

    #[test]
    fn initial_floor_uses_the_monitor_containing_the_window() {
        let hit = monitor_floor_for_point(&world(), -800.0, 400.0).expect("surface");
        assert_eq!(hit.y, 1040.0);
        assert!(matches!(
            hit.surface,
            DesktopSurface::MonitorFloor { ref monitor_id } if monitor_id == "left"
        ));
    }
}
