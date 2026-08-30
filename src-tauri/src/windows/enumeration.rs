use std::{ffi::c_void, mem::size_of};

use windows::{
    core::BOOL,
    Win32::{
        Foundation::{HWND, LPARAM, RECT},
        Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS},
        UI::WindowsAndMessaging::{
            EnumWindows, GetForegroundWindow, GetWindowLongW, GetWindowRect, GetWindowTextLengthW,
            GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindowVisible, GWL_EXSTYLE,
            WS_EX_TOOLWINDOW,
        },
    },
};

use crate::{
    desktop::world::DesktopWindow,
    error::{AppError, AppResult},
};

pub fn enumerate_windows(companion_title: &str) -> AppResult<Vec<DesktopWindow>> {
    let mut context = EnumerationContext {
        windows: Vec::new(),
        companion_title: companion_title.to_owned(),
    };
    let parameter = LPARAM((&mut context as *mut EnumerationContext) as isize);
    unsafe { EnumWindows(Some(enum_window), parameter) }
        .map_err(|error| AppError::WindowsApi(format!("EnumWindows: {error}")))?;
    Ok(context.windows)
}

pub fn foreground_window() -> Option<isize> {
    let hwnd = unsafe { GetForegroundWindow() };
    (!hwnd.is_invalid()).then_some(hwnd.0 as isize)
}

struct EnumerationContext {
    windows: Vec<DesktopWindow>,
    companion_title: String,
}

unsafe extern "system" fn enum_window(hwnd: HWND, parameter: LPARAM) -> BOOL {
    let context = &mut *(parameter.0 as *mut EnumerationContext);
    if let Some(window) = read_window(hwnd, &context.companion_title) {
        context.windows.push(window);
    }
    BOOL(1)
}

unsafe fn read_window(hwnd: HWND, companion_title: &str) -> Option<DesktopWindow> {
    if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
        return None;
    }
    let extended_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
    if extended_style & WS_EX_TOOLWINDOW.0 != 0 {
        return None;
    }

    let title_length = GetWindowTextLengthW(hwnd);
    if title_length <= 0 {
        return None;
    }
    let mut title_buffer = vec![0_u16; title_length as usize + 1];
    let copied = GetWindowTextW(hwnd, &mut title_buffer);
    if copied <= 0 {
        return None;
    }
    let title = String::from_utf16_lossy(&title_buffer[..copied as usize]);
    if title == companion_title || title == "Program Manager" {
        return None;
    }

    let rect = visible_window_rect(hwnd)?;
    if rect.right <= rect.left || rect.bottom <= rect.top {
        return None;
    }

    let mut process_id = 0_u32;
    GetWindowThreadProcessId(hwnd, Some(&mut process_id));
    Some(DesktopWindow {
        hwnd: hwnd.0 as isize,
        process_id,
        title,
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
        visible: true,
        minimized: false,
    })
}

unsafe fn visible_window_rect(hwnd: HWND) -> Option<RECT> {
    let mut rect = RECT::default();
    if DwmGetWindowAttribute(
        hwnd,
        DWMWA_EXTENDED_FRAME_BOUNDS,
        &mut rect as *mut RECT as *mut c_void,
        size_of::<RECT>() as u32,
    )
    .is_ok()
    {
        return Some(rect);
    }

    GetWindowRect(hwnd, &mut rect).ok().map(|_| rect)
}
