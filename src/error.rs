pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("mpv process failed: {0}")]
    MpvProcessError(String),
    #[error("IPC communication error: {0}")]
    IpcError(String),
}