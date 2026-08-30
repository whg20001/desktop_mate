use serde::Serialize;

#[derive(Clone, Copy, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopPoint {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorInfo {
    pub id: String,
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    pub work_left: i32,
    pub work_top: i32,
    pub work_right: i32,
    pub work_bottom: i32,
    pub dpi: u32,
    pub primary: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopWindow {
    pub hwnd: isize,
    pub process_id: u32,
    pub title: String,
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    pub visible: bool,
    pub minimized: bool,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopWorld {
    pub monitors: Vec<MonitorInfo>,
    pub windows: Vec<DesktopWindow>,
    pub foreground_window: Option<isize>,
    pub cursor: DesktopPoint,
}
