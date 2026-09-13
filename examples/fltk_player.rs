use fltk::{
    app, button::Button, dialog, enums::Color, frame::Frame, prelude::*, valuator::HorNiceSlider,
    window::Window,
};
use mpv_ipc::{Mpv, MpvCommand, MpvConfig, MpvEvent};
use std::sync::Arc;
use std::time::Duration;

fn format_time(secs: f64) -> String {
    let total = secs.max(0.0) as u64;
    format!("{}:{:02}", total / 60, total % 60)
}

fn main() -> mpv_ipc::Result<()> {
    let _app = app::App::default();

    let mut win = Window::new(100, 100, 800, 600, "mpv-ipc FLTK Player");
    win.make_resizable(true);

    let mut video = Window::new(0, 0, 800, 520, "");
    video.set_color(Color::Black);
    video.end();

    let mut open_btn = Button::new(10, 540, 80, 30, "Open");
    let mut play_btn = Button::new(100, 540, 80, 30, "Pause");
    let mut slider = HorNiceSlider::new(190, 545, 600, 20, "");
    let mut time_display = Frame::new(10, 570, 400, 30, "0:00 / 0:00");

    win.end();
    win.show();

    let wid = video.raw_handle() as i64;

    let mpv = Arc::new(Mpv::start(MpvConfig {
        args: vec![
            format!("--wid={}", wid),
            "--force-window".to_string(),
            "--keep-open=yes".to_string(),
            "--hwdec=auto-safe".to_string(),
        ],
        observed_properties: vec![
            "pause".to_string(),
            "time-pos".to_string(),
            "duration".to_string(),
        ],
        ..Default::default()
    })?);

    let mpv_open = Arc::clone(&mpv);
    open_btn.set_callback(move |_| {
        if let Some(file) = dialog::file_chooser(
            "Select a video",
            "*.{mp4,mkv,avi,mov,webm,flv,ts,m4v}",
            ".",
            false,
        ) {
            mpv_open
                .command(MpvCommand {
                    command: vec!["loadfile".into(), file.into(), "replace".into()],
                    request_id: None,
                })
                .ok();
        }
    });

    let mpv_pause = Arc::clone(&mpv);
    play_btn.set_callback(move |_| {
        mpv_pause
            .command(MpvCommand {
                command: vec!["cycle".into(), "pause".into()],
                request_id: None,
            })
            .ok();
    });

    let mpv_slider = Arc::clone(&mpv);
    slider.set_callback(move |s| {
        mpv_slider
            .command(MpvCommand {
                command: vec!["seek".into(), s.value().into(), "absolute+exact".into()],
                request_id: None,
            })
            .ok();
    });

    while win.visible() {
        let mut changed = false;

        while let Some(event) = mpv.try_recv_event()? {
            println!("{:?}", event);
            match event {
                MpvEvent::PropertyChange { name, data, .. } => match name.as_str() {
                    "pause" => {
                        let paused = data.and_then(|v| v.as_bool()).unwrap_or(false);
                        play_btn.set_label(if paused { "Play" } else { "Pause" });
                        changed = true;
                    }
                    "time-pos" => {
                        if let Some(pos) = data.and_then(|v| v.as_f64()) {
                            slider.set_value(pos);
                            changed = true;
                        }
                    }
                    "duration" => {
                        if let Some(dur) = data.and_then(|v| v.as_f64()) {
                            slider.set_range(0.0, dur);
                            changed = true;
                        }
                    }
                    _ => {}
                },
                _ => {}
            }
        }

        if changed {
            time_display.set_label(&format!(
                "{} / {}",
                format_time(slider.value()),
                format_time(slider.maximum())
            ));
            win.redraw();
        }

        app::check();
        std::thread::sleep(Duration::from_millis(20));
    }

    mpv.stop()?;
    Ok(())
}
