use fltk::{
    app,
    button::Button,
    dialog,
    enums::{Align, Color},
    frame::Frame,
    prelude::*,
    valuator::HorNiceSlider,
    window::Window,
};
use mpv_ipc::{Mpv, MpvConfig, MpvEvent};
use std::sync::Arc;

fn format_time(secs: f64) -> String {
    let total = secs.max(0.0) as u64;
    format!("{}:{:02}", total / 60, total % 60)
}

const WIN_W: i32 = 1280;
const WIN_H: i32 = 720;
const BAR_H: i32 = 48;
const MARGIN: i32 = 10;
const BTN_W: i32 = 70;
const CTRL_H: i32 = 28;
const TIME_W: i32 = 120;

const SLIDER_X: i32 = MARGIN + 2 * (BTN_W + MARGIN);
const TIME_X: i32 = WIN_W - MARGIN - TIME_W;
const SLIDER_W: i32 = TIME_X - SLIDER_X;

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

fn show_progress(overlay: &mut Window, time_display: &mut Frame, slider: &mut HorNiceSlider) {
    time_display.set_label(&format!(
        "{} / {}",
        format_time(slider.value()),
        format_time(slider.maximum())
    ));
    let label_w = time_display.measure_label().0 + 8;
    let label_x = overlay.w() - MARGIN - label_w;
    time_display.resize(label_x, 10, label_w, CTRL_H);
    slider.resize(SLIDER_X, 14, (label_x - SLIDER_X - MARGIN).max(20), 20);
    overlay.init_sizes();
}

fn main() -> mpv_ipc::Result<()> {
    let _app = app::App::default();

    let mut win = Window::new(100, 100, WIN_W, WIN_H, "mpv-ipc FLTK");
    let mut video = Window::new(0, 0, WIN_W, WIN_H, "");
    video.set_color(Color::Black);
    video.end();

    let mut overlay = Window::new(0, WIN_H - BAR_H, WIN_W, BAR_H, "");
    overlay.set_color(Color::from_rgb(20, 20, 20));
    let mut open_btn = Button::new(MARGIN, 10, BTN_W, CTRL_H, "Open");
    let mut play_btn = Button::new(MARGIN + BTN_W + MARGIN, 10, BTN_W, CTRL_H, "Pause");
    let mut slider = HorNiceSlider::new(SLIDER_X, 14, SLIDER_W, 20, "");
    slider.set_color(Color::from_rgb(90, 90, 90));
    let mut time_display = Frame::new(TIME_X, 10, TIME_W, CTRL_H, "0:00 / 0:00");
    time_display.set_align(Align::Right | Align::Inside);
    time_display.set_label_color(Color::White);
    overlay.end();
    overlay.resizable(&slider);

    let mut spacer = Frame::new(0, 0, 1, 1, "");
    spacer.hide();
    win.end();
    win.resizable(&spacer);
    win.size_range(480, 320, 0, 0);
    win.clone().center_screen();
    win.show();
    show_progress(&mut overlay, &mut time_display, &mut slider);

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
    fade_overlay(&overlay, 190);

    let mpv_clone = Arc::clone(&mpv);
    open_btn.set_callback(move |_| {
        let mut chooser = dialog::NativeFileChooser::new(dialog::FileDialogType::BrowseFile);
        chooser.set_title("Select a video");
        chooser.set_filter("Video Files\t*.{mp4,mkv,avi,mov,webm,flv,ts,m4v}");
        if let Ok(dialog::FileDialogAction::Success) = chooser.try_show() {
            mpv_clone
                .command(vec![
                    "loadfile".into(),
                    chooser.filename().to_string_lossy().into_owned().into(),
                    "replace".into(),
                ])
                .ok();
        }
    });

    let mpv_clone = Arc::clone(&mpv);
    play_btn.set_callback(move |_| {
        mpv_clone.command(vec!["cycle".into(), "pause".into()]).ok();
    });

    let mpv_clone = Arc::clone(&mpv);
    slider.set_callback(move |s| {
        mpv_clone
            .command(vec![
                "seek".into(),
                s.value().into(),
                "absolute+exact".into(),
            ])
            .ok();
    });

    let (event_sender, event_receiver) = app::channel::<MpvEvent>();
    let mpv_clone = Arc::clone(&mpv);
    std::thread::spawn(move || {
        while let Ok(event) = mpv_clone.recv_event() {
            event_sender.send(event);
        }
    });

    while win.visible() && app::wait() {
        while let Some(event) = event_receiver.recv() {
            println!("{:?}", event);
            if let MpvEvent::PropertyChange { name, data, .. } = event {
                match name.as_str() {
                    "pause" => {
                        let paused = data.and_then(|v| v.as_bool()).unwrap_or(false);
                        play_btn.set_label(if paused { "Play" } else { "Pause" });
                    }
                    "time-pos" => {
                        if let Some(pos) = data.and_then(|v| v.as_f64()) {
                            slider.set_value(pos);
                            show_progress(&mut overlay, &mut time_display, &mut slider);
                        }
                    }
                    "duration" => {
                        if let Some(dur) = data.and_then(|v| v.as_f64()) {
                            slider.set_range(0.0, dur);
                            show_progress(&mut overlay, &mut time_display, &mut slider);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    mpv.stop()?;
    Ok(())
}
