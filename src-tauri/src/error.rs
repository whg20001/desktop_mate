use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error, Serialize)]
#[serde(tag = "kind", content = "message", rename_all = "camelCase")]
pub enum AppError {
    #[error("Windows API 调用失败: {0}")]
    WindowsApi(String),
    #[error("模型处理失败: {0}")]
    Model(String),
    #[error("I/O 操作失败: {0}")]
    Io(String),
    #[error("无效状态: {0}")]
    InvalidState(String),
}

pub type AppResult<T> = Result<T, AppError>;
