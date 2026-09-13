use log::{debug, error, info, warn};
use std::io::{BufRead, BufReader, Write};
use std::process::Child;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[cfg(windows)]
use std::fs::OpenOptions;
#[cfg(unix)]
use std::os::unix::net::UnixStream;

use crate::{ipc::get_ipc_pipe, MpvEvent};

pub fn start_event_listener(
    id: &str,
    process_id: u32,
    ipc_timeout: Duration,
    observed_properties: Vec<String>,
    event_tx: Sender<MpvEvent>,
    child: Arc<Mutex<Child>>,
) {
    let ipc_pipe = get_ipc_pipe(id);

    let max_retries = 5;
    let mut retry_count = 0;

    loop {
        {
            let mut child_guard = child.lock().unwrap();
            if process_id != child_guard.id() {
                info!(
                    "Stopping stale listener for old PID {} for instance '{}', detected new mpv process (PID {}).",
                    process_id,
                    id,
                    child_guard.id(),
                );
                break;
            }
            if child_guard.try_wait().unwrap_or(None).is_some() {
                info!(
                    "Stopping listener for associated mpv process (PID {}) for instance '{}', as the process has terminated.",
                    child_guard.id(),
                    id,
                );
                break;
            }
        }

        retry_count += 1;

        debug!(
            "Event listener for mpv process (PID: {}) for instance '{}' connecting... (attempt {}/{})",
            process_id, id, retry_count, max_retries,
        );

        #[cfg(windows)]
        let stream_result = OpenOptions::new().read(true).write(true).open(&ipc_pipe);

        #[cfg(unix)]
        let stream_result = UnixStream::connect(&ipc_pipe);

        match stream_result {
            Ok(mut stream) => {
                info!(
                    "Successfully connected event listener for mpv process (PID: {}) for instance '{}'.",
                    process_id,
                    id,
                );

                retry_count = 0;

                let mut successful_properties = Vec::new();
                let mut failed_properties = Vec::new();

                for (obs_id, property) in observed_properties.iter().enumerate() {
                    let cmd_str = format!(
                        r#"{{"command": ["observe_property", {}, "{}"]}}"#,
                        obs_id + 1,
                        property
                    );

                    let write_result = stream
                        .write_all(cmd_str.as_bytes())
                        .and_then(|_| stream.write_all(b"\n"))
                        .and_then(|_| stream.flush());

                    match write_result {
                        Ok(_) => {
                            successful_properties.push(property.clone());
                        }
                        Err(_) => {
                            failed_properties.push(property.clone());
                            break;
                        }
                    }
                }

                if !successful_properties.is_empty() {
                    info!(
                        "Successfully observed properties for mpv process (PID: {}) for instance '{}': {:?}",
                        process_id,
                        id,
                        successful_properties,
                    );
                }
                if !failed_properties.is_empty() {
                    warn!(
                        "Failed to observe properties for mpv process (PID: {}) for instance '{}': {:?}",
                        process_id, id, failed_properties
                    );
                }

                let reader = BufReader::new(stream);
                for line_result in reader.lines() {
                    match line_result {
                        Ok(line) => {
                            if let Ok(payload) = serde_json::from_str::<MpvEvent>(&line) {
                                if let Err(e) = event_tx.send(payload) {
                                    error!(
                                        "Event channel closed for instance '{}' (PID: {}): {}",
                                        id, process_id, e,
                                    );
                                    return;
                                }
                            } else if line.contains("\"event\"") {
                                warn!(
                                    "Failed to parse mpv event line as JSON for mpv process (PID: {}) for instance '{}'. Line: '{}'",
                                    process_id,
                                    id,
                                    line,
                                );
                            }
                        }
                        Err(e) => {
                            error!(
                                "Error reading from mpv IPC for mpv process (PID: {}) for instance '{}': {}",
                                process_id,
                                id,
                                e,
                            );
                            break;
                        }
                    }
                }
                info!(
                    "Event listener for mpv process (PID: {}) for instance '{}' has disconnected.",
                    process_id, id,
                );
                std::thread::sleep(ipc_timeout);
                continue;
            }
            Err(e) => {
                debug!(
                    "Failed to connect to IPC for mpv process (PID: {}) for instance '{}' (attempt {}/{}): {}",
                    process_id,
                    id,
                    retry_count,
                    max_retries,
                    e,
                );

                if retry_count >= max_retries {
                    error!(
                        "Max retries reached for mpv process (PID: {}) for instance '{}'. mpv IPC connection failed.",
                        process_id, id,
                    );
                    break;
                }

                debug!(
                    "Retrying IPC connection for mpv process (PID: {}) for instance '{}'...",
                    process_id, id,
                );
                std::thread::sleep(ipc_timeout);
            }
        }
    }
}