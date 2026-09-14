use fltk::{
    app, button::Button, dialog, enums::Color, frame::Frame, prelude::*, valuator::HorNiceSlider,
    window::Window,
};
use mpv_ipc::{Mpv, MpvConfig, MpvEvent};
use std::sync::Arc;
use std::time::Duration;

fn format_time(secs: f64) -> String {
    let total = secs.max(0.0) as u64;
    format!("{}:{:02}", total / 60, total % 60)
}

const BAR_HEIGHT: i32 = 90;

#[cfg(target_os = "windows")]
#[link(name = "user32")]
unsafe extern "system" {
    fn SetWindowPos(hwnd: isize, after: isize, x: i32, y: i32, cx: i32, cy: i32, flags: u32)
    -> i32;
    fn GetWindowLongPtrW(hwnd: isize, index: i32) -> isize;
    fn SetWindowLongPtrW(hwnd: isize, index: i32, value: isize) -> isize;
    fn GetClassLongPtrW(hwnd: isize, index: i32) -> isize;
    fn SetClassLongPtrW(hwnd: isize, index: i32, value: isize) -> isize;
    fn SetLayeredWindowAttributes(hwnd: isize, key: u32, alpha: u8, flags: u32) -> i32;
}

fn raise_overlay(win: &Window) {
    #[cfg(target_os = "windows")]
    unsafe {
        SetWindowPos(win.raw_handle() as isize, 0, 0, 0, 0, 0, 1 | 2 | 0x10);
    }
}

#[cfg(target_os = "windows")]
fn fade_overlay(win: &Window, alpha: u8) {
    unsafe {
        const GCL_STYLE: i32 = -26;
        const GWL_EXSTYLE: i32 = -20;
        const CS_OWNDC: isize = 0x20;
        const WS_EX_LAYERED: isize = 0x80000;
        let h = win.raw_handle() as isize;
        let cls = GetClassLongPtrW(h, GCL_STYLE);
        let cls_ret = SetClassLongPtrW(h, GCL_STYLE, cls & !CS_OWNDC);
        let before = GetWindowLongPtrW(h, GWL_EXSTYLE);
        let set_ret = SetWindowLongPtrW(h, GWL_EXSTYLE, before | WS_EX_LAYERED);
        let after = GetWindowLongPtrW(h, GWL_EXSTYLE);
        let layered_ok = SetLayeredWindowAttributes(h, 0, alpha, 0x2) != 0;
        let ret = SetWindowPos(h, 0, 0, 0, 0, 0, 1 | 2 | 0x10 | 0x20);
        println!(
            "fade: cls=0x{:x}->cls_ret=0x{:x} before=0x{:x} set_ret=0x{:x} after=0x{:x} layered_ok={} swp={}",
            cls, cls_ret, before, set_ret, after, layered_ok, ret
        );
    }
}

fn main() -> mpv_ipc::Result<()> {
    let _app = app::App::default();

    let mut win = Window::new(100, 100, 800, 600, "mpv-ipc FLTK");
    win.make_resizable(true);

    let mut video = Window::new(0, 0, 800, 600, "");
    video.set_color(Color::Black);
    video.end();

    let mut overlay = Window::new(0, 510, 800, BAR_HEIGHT, "");
    overlay.set_color(Color::from_rgb(20, 20, 20));
    let mut open_btn = Button::new(10, 10, 80, 30, "Open");
    let mut play_btn = Button::new(100, 10, 80, 30, "Pause");
    let mut slider = HorNiceSlider::new(190, 15, 600, 20, "");
    slider.set_color(Color::from_rgb(90, 90, 90));
    let mut time_display = Frame::new(10, 45, 400, 30, "0:00 / 0:00");
    overlay.end();

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

    raise_overlay(&overlay);

    let mpv_open = Arc::clone(&mpv);
    open_btn.set_callback(move |_| {
        if let Some(file) = dialog::file_chooser(
            "Select a video",
            "*.{mp4,mkv,avi,mov,webm,flv,ts,m4v}",
            ".",
            false,
        ) {
            mpv_open
                .command(vec!["loadfile".into(), file.into(), "replace".into()])
                .ok();
        }
    });

    let mpv_pause = Arc::clone(&mpv);
    play_btn.set_callback(move |_| {
        mpv_pause.command(vec!["cycle".into(), "pause".into()]).ok();
    });

    let mpv_slider = Arc::clone(&mpv);
    slider.set_callback(move |s| {
        mpv_slider
            .command(vec![
                "seek".into(),
                s.value().into(),
                "absolute+exact".into(),
            ])
            .ok();
    });

    let mut last_size = (win.w(), win.h());
    while win.visible() {
        let mut changed = false;
        let (ww, wh) = (win.w(), win.h());
        if (ww, wh) != last_size {
            last_size = (ww, wh);
            video.resize(0, 0, ww, wh);
            overlay.resize(0, wh - BAR_HEIGHT, ww, BAR_HEIGHT);
            raise_overlay(&overlay);
            fade_overlay(&overlay, 190);
            win.redraw();
        }

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
