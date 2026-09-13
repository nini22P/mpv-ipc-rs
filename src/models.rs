use serde::{Deserialize, Serialize};
use serde_json::Value;

fn default_mpv_path() -> String {
    "mpv".to_string()
}

fn default_ipc_timeout() -> u64 {
    2000
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MpvConfig {
    #[serde(default = "default_mpv_path")]
    pub path: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub observed_properties: Vec<String>,
    #[serde(default = "default_ipc_timeout")]
    pub ipc_timeout_ms: u64,
    #[serde(default)]
    pub show_mpv_output: bool,
}

impl Default for MpvConfig {
    fn default() -> Self {
        Self {
            path: default_mpv_path(),
            args: Vec::new(),
            observed_properties: Vec::new(),
            ipc_timeout_ms: default_ipc_timeout(),
            show_mpv_output: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct MpvCommand {
    pub command: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MpvCommandResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    pub error: String,
    pub request_id: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EndFileReason {
    Eof,
    Stop,
    Quit,
    Error,
    Redirect,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "kebab-case")]
pub enum MpvEvent {
    StartFile {
        playlist_entry_id: u32,
    },
    EndFile {
        reason: EndFileReason,
        playlist_entry_id: u32,
    },
    FileLoaded,
    VideoReconfig,
    AudioReconfig,
    Seek,
    PlaybackRestart,
    PropertyChange {
        id: u32,
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        data: Option<Value>,
    },
    Idle,
    Tick,
    ClientMessage {
        args: Vec<String>,
    },
    LogMessage {
        prefix: String,
        level: String,
        text: String,
    },
    QueueOverflow,
}
