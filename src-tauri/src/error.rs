use serde::{Serialize, Serializer};

/// 统一错误类型。所有 `#[tauri::command]` 返回 `Result<T, AppError>`，
/// 前端拿到的是 `{ kind, message }`。
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    Message(String),

    #[error("配置读写失败: {0}")]
    Config(String),

    #[error("数据库错误: {0}")]
    Db(String),

    #[error("系统调用失败: {0}")]
    System(String),

}

impl AppError {
    pub fn msg(m: impl Into<String>) -> Self {
        Self::Message(m.into())
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Self::Message(_) => "message",
            Self::Config(_) => "config",
            Self::Db(_) => "db",
            Self::System(_) => "system",
        }
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        Self::Config(e.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        Self::Config(e.to_string())
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Db(e.to_string())
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("AppError", 2)?;
        s.serialize_field("kind", self.kind())?;
        s.serialize_field("message", &self.to_string())?;
        s.end()
    }
}

pub type AppResult<T> = Result<T, AppError>;
