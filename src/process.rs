use log::{debug, error, info, trace};
use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::ipc::get_ipc_pipe;
use crate::{Mpv, MpvConfig};

pub fn start_mpv_process(mpv_config: MpvConfig, id: &str) -> crate::Result<Mpv> {
    let ipc_pipe = get_ipc_pipe(id);
    let ipc_timeout = Duration::from_millis(mpv_config.ipc_timeout_ms);

    debug!("Initializing mpv for instance '{}'...", id);

    // libmpv profile: https://github.com/mpv-player/mpv/blob/master/etc/builtin.conf#L21
    let mut args = vec![
        format!("--input-ipc-server={}", ipc_pipe),
        "--profile=libmpv".to_string(),
    ];

    args.extend(mpv_config.args.iter().cloned());

    debug!("Using IPC pipe: {}", ipc_pipe);

    let mpv_path = mpv_config.path;

    debug!(
        "Spawning mpv process for instance '{}' with args: {} {}",
        id,
        mpv_path,
        args.join(" ")
    );

    let args_clone = args.clone();
    let show_mpv_output = mpv_config.show_mpv_output;

    match Command::new(mpv_path.clone())
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(mut child) => {
            let log_queue: Arc<Mutex<VecDeque<String>>> = Arc::new(Mutex::new(VecDeque::new()));
            let log_queue_clone = Arc::clone(&log_queue);
            let id_clone = id.to_string();

            if let Some(stdout) = child.stdout.take() {
                thread::spawn(move || {
                    let reader = BufReader::new(stdout);
                    for line in reader.lines().map_while(Result::ok) {
                        if show_mpv_output {
                            trace!("mpv stdout [{}] {}", id_clone, line);
                        }
                        if let Ok(mut queue) = log_queue_clone.lock() {
                            queue.push_back(line);
                            if queue.len() > 100 {
                                queue.pop_front();
                            }
                        }
                    }
                });
            }

            match wait_for_ipc_server(&ipc_pipe, ipc_timeout, id) {
                Ok(startup_duration) => {
                    info!(
                        "mpv IPC server for instance '{}' is ready. Startup took {}ms.",
                        id,
                        startup_duration.as_millis()
                    );
                }
                Err(e) => {
                    let mut error_message = format!(
                        "mpv startup failed for instance '{}'. Collected stdout:",
                        id,
                    );
                    error!("{}", error_message);
                    error_message.push('\n');
                    if let Ok(mut queue) = log_queue.lock() {
                        while let Some(line) = queue.pop_front() {
                            error!("mpv stdout [{}] {}", id, line);
                            error_message.push_str(&format!("mpv stdout [{}] {}\n", id, line));
                        }
                    }
                    error!("{}", e);
                    error_message.push_str(&e);
                    let _ = child.kill();
                    return Err(crate::Error::MpvProcessError(error_message));
                }
            }

            info!(
                "mpv process (PID: {}) started for instance '{}'. Initialization complete.",
                child.id(),
                id,
            );

            let child = Arc::new(Mutex::new(child));
            let listener_child = Arc::clone(&child);
            let process_id = child.lock().unwrap().id();

            let (event_tx, event_rx) = std::sync::mpsc::channel();
            let id_clone = id.to_string();
            let observed_properties = mpv_config.observed_properties;
            thread::spawn(move || {
                crate::events::start_event_listener(
                    &id_clone,
                    process_id,
                    ipc_timeout,
                    observed_properties,
                    event_tx,
                    listener_child,
                );
            });

            Ok(Mpv {
                id: id.to_string(),
                process: child,
                ipc_timeout,
                event_rx: Mutex::new(event_rx),
            })
        }
        Err(e) => {
            let error_message = format!(
                "Failed to start mpv: {}. Is mpv installed and in your PATH?",
                e
            );
            error!("For instance '{}': {}", id, error_message);
            debug!(
                "The command that failed for instance '{}' was: {} {}",
                id,
                mpv_path,
                args_clone.join(" ")
            );
            Err(crate::Error::MpvProcessError(error_message))
        }
    }
}

pub fn wait_for_ipc_server(
    ipc_pipe: &str,
    ipc_timeout: Duration,
    id: &str,
) -> Result<Duration, String> {
    let start = Instant::now();
    let pipe = Path::new(ipc_pipe);

    while start.elapsed() < ipc_timeout {
        if pipe.exists() {
            let elapsed = start.elapsed();
            return Ok(elapsed);
        }
        thread::sleep(Duration::from_millis(50));
    }

    Err(format!(
        "Timed out after {:?} waiting for IPC server for instance '{}' at '{}'",
        ipc_timeout, id, ipc_pipe,
    ))
}
