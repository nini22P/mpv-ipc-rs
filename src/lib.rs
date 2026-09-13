use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex, OnceLock};

pub use models::*;

mod error;
mod events;
mod ipc;
mod models;
mod process;

pub use error::{Error, Result};

static NEXT_INSTANCE_ID: AtomicU64 = AtomicU64::new(0);
static NEXT_REQUEST_ID: AtomicU32 = AtomicU32::new(1);

pub struct Mpv {
    id: String,
    process: Arc<Mutex<std::process::Child>>,
    ipc_timeout: std::time::Duration,
    event_rx: Mutex<Receiver<MpvEvent>>,
}

impl Mpv {
    pub fn start(mpv_config: MpvConfig) -> Result<Self> {
        static SETLOCALE: OnceLock<()> = OnceLock::new();
        SETLOCALE.get_or_init(|| unsafe {
            let locale = std::ffi::CString::new("C").unwrap();
            libc::setlocale(libc::LC_NUMERIC, locale.as_ptr());
        });

        let id = format!(
            "{}_{}",
            std::process::id(),
            NEXT_INSTANCE_ID.fetch_add(1, Ordering::SeqCst)
        );

        process::start_mpv_process(mpv_config, &id)
    }

    pub fn command(&self, command: Vec<serde_json::Value>) -> Result<serde_json::Value> {
        let request_id = NEXT_REQUEST_ID.fetch_add(1, Ordering::SeqCst);
        let response = ipc::send_command(
            MpvCommand {
                command,
                request_id,
            },
            &self.id,
            self.ipc_timeout,
        )?;
        if response.error == "success" {
            Ok(response.data.unwrap_or_default())
        } else {
            Err(Error::MpvCommandError(response.error))
        }
    }

    pub fn try_recv_event(&self) -> Result<Option<MpvEvent>> {
        match self.event_rx.lock().unwrap().try_recv() {
            Ok(event) => Ok(Some(event)),
            Err(std::sync::mpsc::TryRecvError::Empty) => Ok(None),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                Err(Error::IpcError("Event channel disconnected".to_string()))
            }
        }
    }

    pub fn stop(&self) -> Result<()> {
        use log::{error, info};

        let mut child = self.process.lock().unwrap();
        info!(
            "Attempting to kill mpv process (PID: {}) for instance '{}'...",
            child.id(),
            self.id,
        );
        match child.kill() {
            Ok(_) => {
                let _ = child.wait();
                info!(
                    "mpv process (PID: {}) for instance '{}' killed successfully.",
                    child.id(),
                    self.id,
                );
                Ok(())
            }
            Err(e) => {
                let error_message = format!(
                    "Failed to kill mpv process (PID: {}) for instance '{}': {}",
                    child.id(),
                    self.id,
                    e,
                );
                error!("{}", error_message);
                Err(Error::MpvProcessError(error_message))
            }
        }
    }
}

impl Drop for Mpv {
    fn drop(&mut self) {
        let mut child = self.process.lock().unwrap();
        let _ = child.kill();
        let _ = child.wait();
    }
}
