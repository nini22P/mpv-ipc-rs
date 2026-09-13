use std::io::{BufRead, BufReader, Read, Write};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

#[cfg(windows)]
use std::fs::OpenOptions;
#[cfg(unix)]
use std::os::unix::net::UnixStream;

use log::{error, trace};

use crate::MpvCommand;
use crate::MpvCommandResponse;
use crate::Result;

static NEXT_REQUEST_ID: AtomicU32 = AtomicU32::new(1);

#[cfg(windows)]
pub const IPC_PIPE_BASE: &str = r"\\.\pipe\mpv_ipc_socket";
#[cfg(unix)]
pub const IPC_PIPE_BASE: &str = "/tmp/mpv_ipc_socket";

pub fn get_ipc_pipe(id: &str) -> String {
    format!("{}_{}_{}", IPC_PIPE_BASE, std::process::id(), id)
}

pub fn send_command(
    mut mpv_command: MpvCommand,
    id: &str,
    ipc_timeout: Duration,
) -> Result<MpvCommandResponse> {
    if mpv_command.request_id.is_none() {
        mpv_command.request_id = Some(NEXT_REQUEST_ID.fetch_add(1, Ordering::SeqCst));
    }

    trace!(
        "-> SEND [{}] {}",
        id,
        serde_json::to_string(&mpv_command).unwrap_or_default()
    );

    let ipc_pipe = get_ipc_pipe(id);

    #[cfg(windows)]
    {
        let pipe = match OpenOptions::new().read(true).write(true).open(&ipc_pipe) {
            Ok(p) => p,
            Err(e) => {
                let err_msg = format!("Failed to open named pipe at '{}': {}", ipc_pipe, e);
                error!("For instance '{}': {}", id, err_msg);
                return Err(crate::Error::IpcError(err_msg));
            }
        };

        process_mpv_command(pipe, mpv_command, id, ipc_timeout)
    }

    #[cfg(unix)]
    {
        let stream = match UnixStream::connect(ipc_pipe) {
            Ok(s) => s,
            Err(e) => {
                let err_msg = format!("Failed to connect to Unix socket at '{}': {}", ipc_pipe, e);
                error!("For instance '{}': {}", id, err_msg);
                return Err(crate::Error::IpcError(err_msg));
            }
        };

        process_mpv_command(stream, mpv_command, key, ipc_timeout)
    }
}

fn process_mpv_command<S: Read + Write>(
    mut stream: S,
    mpv_command: MpvCommand,
    id: &str,
    ipc_timeout: Duration,
) -> Result<MpvCommandResponse> {
    let expected_request_id = mpv_command.request_id.unwrap();

    let command_json = serde_json::to_string(&mpv_command);

    if let Err(e) = command_json {
        let err_msg = format!("Failed to serialize command to JSON: {}", e);
        error!("For instance '{}': {}", id, err_msg);
        return Err(crate::Error::IpcError(err_msg));
    }

    let json_bytes = command_json.unwrap();

    if let Err(e) = stream.write_all(json_bytes.as_bytes()) {
        let err_msg = format!("Failed to write command to IPC stream: {}", e);
        error!("For instance '{}': {}", id, err_msg);
        return Err(crate::Error::IpcError(err_msg));
    }
    if let Err(e) = stream.write_all(b"\n") {
        let err_msg = format!("Failed to write newline to IPC stream: {}", e);
        error!("For instance '{}': {}", id, err_msg);
        return Err(crate::Error::IpcError(err_msg));
    }
    if let Err(e) = stream.flush() {
        let err_msg = format!("Failed to flush IPC stream: {}", e);
        error!("For instance '{}': {}", id, err_msg);
        return Err(crate::Error::IpcError(err_msg));
    }

    let mut reader = BufReader::new(stream);

    let start_time = Instant::now();

    loop {
        if start_time.elapsed() > ipc_timeout {
            let err_msg = format!(
                "Timeout: Did not receive a response for request_id {} within {:?}",
                expected_request_id, ipc_timeout
            );
            error!("For instance '{}': {}", id, err_msg);
            return Err(crate::Error::IpcError(err_msg));
        }

        let mut response_string = String::new();

        if let Err(e) = reader.read_line(&mut response_string) {
            let err_msg = format!("Failed to read response from IPC stream: {}", e);
            error!("For instance '{}': {}", id, err_msg);
            return Err(crate::Error::IpcError(err_msg));
        }

        if response_string.is_empty() {
            let err_msg = "IPC stream closed before receiving a command response.".to_string();
            error!("For instance '{}': {}", id, err_msg);
            return Err(crate::Error::IpcError(err_msg));
        }

        match serde_json::from_str::<MpvCommandResponse>(&response_string) {
            Ok(response) => {
                if response.request_id == expected_request_id {
                    trace!(
                        "<- RECV [{}] {}",
                        id,
                        serde_json::to_string(&response).unwrap_or_default()
                    );
                    return Ok(response);
                } else {
                    trace!(
                        "<- IGNORED [{}]: Stale response for request_id {}. Expected {}. Body: {}",
                        id,
                        response.request_id,
                        expected_request_id,
                        response_string.trim()
                    );
                    continue;
                }
            }
            Err(_) => {
                trace!("<- IGNORED [{}]: {}", id, response_string.trim());
                continue;
            }
        };
    }
}
