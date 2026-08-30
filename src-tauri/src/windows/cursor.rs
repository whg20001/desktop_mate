use windows::Win32::{
    Foundation::POINT,
    UI::{
        Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON},
        WindowsAndMessaging::GetCursorPos,
    },
};

use crate::{
    desktop::world::DesktopPoint,
    error::{AppError, AppResult},
};

pub fn get_cursor_position() -> AppResult<DesktopPoint> {
    let mut point = POINT::default();
    unsafe { GetCursorPos(&mut point) }
        .map_err(|error| AppError::WindowsApi(format!("GetCursorPos: {error}")))?;
    Ok(DesktopPoint {
        x: point.x,
        y: point.y,
    })
}

pub fn is_left_button_down() -> bool {
    (unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) }) < 0
}
