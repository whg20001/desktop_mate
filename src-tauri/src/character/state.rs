use serde::Serialize;

use crate::desktop::surface::DesktopSurface;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CharacterMode {
    Idle,
    Dragged,
    Falling,
    Landing,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CharacterState {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub mode: CharacterMode,
    pub grabbed: bool,
    pub support_surface: Option<DesktopSurface>,
    #[serde(skip)]
    pub grab_offset_x: f32,
    #[serde(skip)]
    pub grab_offset_y: f32,
    #[serde(skip)]
    pub foot_offset_x: f32,
    #[serde(skip)]
    pub foot_offset_y: f32,
}

impl CharacterState {
    pub fn new(x: f32, y: f32, window_width: f32, window_height: f32) -> Self {
        Self {
            x,
            y,
            vx: 0.0,
            vy: 0.0,
            mode: CharacterMode::Falling,
            grabbed: false,
            support_surface: None,
            grab_offset_x: 0.0,
            grab_offset_y: 0.0,
            foot_offset_x: window_width / 2.0,
            foot_offset_y: window_height * 0.9,
        }
    }

    /// Replaces the renderer-provided foot anchor without moving the foot in
    /// desktop coordinates. During a drag the native window must remain under
    /// the cursor, so the correction is intentionally skipped.
    pub fn update_foot_offset(&mut self, foot_offset_x: f32, foot_offset_y: f32) {
        if !self.grabbed {
            self.y += self.foot_offset_y - foot_offset_y;
        }
        self.foot_offset_x = foot_offset_x;
        self.foot_offset_y = foot_offset_y;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn updating_model_foot_offset_keeps_desktop_foot_height_anchored() {
        let mut state = CharacterState::new(600.0, 200.0, 600.0, 900.0);
        let old_foot_y = state.y + state.foot_offset_y;

        state.update_foot_offset(320.0, 850.0);

        assert_eq!(state.y + state.foot_offset_y, old_foot_y);
    }

    #[test]
    fn updating_foot_offset_does_not_move_a_grabbed_window() {
        let mut state = CharacterState::new(600.0, 200.0, 600.0, 900.0);
        state.grabbed = true;

        state.update_foot_offset(320.0, 850.0);

        assert_eq!((state.x, state.y), (600.0, 200.0));
    }
}
