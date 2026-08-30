use crate::{
    character::state::{CharacterMode, CharacterState},
    desktop::{
        surface::{find_crossed_surface, surface_still_supports},
        world::DesktopWorld,
    },
};

const GRAVITY: f32 = 1_800.0;
const MAX_FALL_SPEED: f32 = 2_400.0;

pub fn update_character_physics(state: &mut CharacterState, world: &DesktopWorld, delta: f32) {
    if state.grabbed {
        return;
    }

    let foot_x = state.x + state.foot_offset_x;
    let foot_y = state.y + state.foot_offset_y;
    if state.mode == CharacterMode::Landing {
        state.mode = CharacterMode::Idle;
        return;
    }

    if state.mode == CharacterMode::Idle {
        if let Some(surface) = &state.support_surface {
            if surface_still_supports(world, surface, foot_x, foot_y) {
                return;
            }
        }
        state.mode = CharacterMode::Falling;
        state.support_surface = None;
    }

    let previous_foot_y = state.y + state.foot_offset_y;
    state.vy = (state.vy + GRAVITY * delta).min(MAX_FALL_SPEED);
    state.x += state.vx * delta;
    state.y += state.vy * delta;
    let current_foot_y = state.y + state.foot_offset_y;

    if let Some(hit) = find_crossed_surface(
        world,
        state.x + state.foot_offset_x,
        previous_foot_y,
        current_foot_y,
    ) {
        state.y = hit.y - state.foot_offset_y;
        state.vx = 0.0;
        state.vy = 0.0;
        state.mode = CharacterMode::Landing;
        state.support_surface = Some(hit.surface);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::world::{DesktopPoint, MonitorInfo};

    #[test]
    fn falling_character_lands_on_work_area_floor() {
        let world = DesktopWorld {
            monitors: vec![MonitorInfo {
                id: "primary".into(),
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1080,
                work_left: 0,
                work_top: 0,
                work_right: 1920,
                work_bottom: 1040,
                dpi: 96,
                primary: true,
            }],
            windows: vec![],
            foreground_window: None,
            cursor: DesktopPoint::default(),
        };
        let mut character = CharacterState::new(500.0, 100.0, 600.0, 900.0);

        for _ in 0..300 {
            update_character_physics(&mut character, &world, 1.0 / 60.0);
            if character.mode == CharacterMode::Landing {
                break;
            }
        }

        assert_eq!(character.mode, CharacterMode::Landing);
        assert!((character.y + character.foot_offset_y - 1040.0).abs() < f32::EPSILON);
        assert_eq!(character.vy, 0.0);
    }

    #[test]
    fn idle_character_stays_supported_after_renderer_updates_foot_anchor() {
        let world = DesktopWorld {
            monitors: vec![MonitorInfo {
                id: "primary".into(),
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1080,
                work_left: 0,
                work_top: 0,
                work_right: 1920,
                work_bottom: 1040,
                dpi: 96,
                primary: true,
            }],
            windows: vec![],
            foreground_window: None,
            cursor: DesktopPoint::default(),
        };
        let mut character = CharacterState::new(500.0, 230.0, 600.0, 900.0);
        character.mode = CharacterMode::Idle;
        character.support_surface = Some(crate::desktop::surface::DesktopSurface::MonitorFloor {
            monitor_id: "primary".into(),
        });
        assert_eq!(character.y + character.foot_offset_y, 1040.0);

        character.update_foot_offset(320.0, 850.0);
        update_character_physics(&mut character, &world, 1.0 / 60.0);
        assert_eq!(character.mode, CharacterMode::Idle);
        assert_eq!(character.y + character.foot_offset_y, 1040.0);
    }
}
