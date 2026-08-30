use std::mem::size_of;

use windows::{
    core::BOOL,
    Win32::{
        Foundation::{LPARAM, RECT},
        Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO},
        UI::{
            HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI},
            WindowsAndMessaging::MONITORINFOF_PRIMARY,
        },
    },
};

use crate::{
    desktop::world::MonitorInfo,
    error::{AppError, AppResult},
};

pub fn enumerate_monitors() -> AppResult<Vec<MonitorInfo>> {
    let mut monitors = Vec::<MonitorInfo>::new();
    let parameter = LPARAM((&mut monitors as *mut Vec<MonitorInfo>) as isize);

    if !unsafe { EnumDisplayMonitors(None, None, Some(enum_monitor), parameter) }.as_bool() {
        return Err(AppError::WindowsApi(format!(
            "EnumDisplayMonitors: {}",
            windows::core::Error::from_thread()
        )));
    }

    if monitors.is_empty() {
        return Err(AppError::WindowsApi("没有检测到显示器".into()));
    }
    Ok(monitors)
}

unsafe extern "system" fn enum_monitor(
    monitor: HMONITOR,
    _dc: HDC,
    _rect: *mut RECT,
    parameter: LPARAM,
) -> BOOL {
    let monitors = &mut *(parameter.0 as *mut Vec<MonitorInfo>);
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };

    if GetMonitorInfoW(monitor, &mut info).as_bool() {
        let mut dpi_x = 96_u32;
        let mut dpi_y = 96_u32;
        let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y);
        monitors.push(MonitorInfo {
            id: format!("monitor-{:#x}", monitor.0 as usize),
            left: info.rcMonitor.left,
            top: info.rcMonitor.top,
            right: info.rcMonitor.right,
            bottom: info.rcMonitor.bottom,
            work_left: info.rcWork.left,
            work_top: info.rcWork.top,
            work_right: info.rcWork.right,
            work_bottom: info.rcWork.bottom,
            dpi: dpi_x,
            primary: info.dwFlags & MONITORINFOF_PRIMARY != 0,
        });
    }
    BOOL(1)
}
