use cursive::traits::*;
use cursive::view::Margins;
use cursive::views::{Button, Dialog, DummyView, LinearLayout, OnEventView, ProgressBar, TextView};
use cursive::{Cursive, CursiveExt};

use crate::{kodi_control, kodi_control::KodiControl, kodi_rpc_types, ui_seek::UiSeek, version};

use crate::{error, exit, util};

use crossbeam_channel::{select, tick};

use std::sync::{Arc, Mutex};

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Crossbeam send error in Ui: {}", .0)]
    CrossbeamSendError(String),

    #[error(transparent)]
    KodiControlError(#[from] kodi_control::Error),
}

pub struct Ui {
    siv: Cursive,
    polling_thread: std::thread::JoinHandle<()>,
}

struct UiData {
    kodi_control: Arc<Mutex<KodiControl>>,
    exit: exit::Exit,
    last_known_seconds: u32,
}

#[derive(Debug)]
pub struct Control {
    cb_sink: crossbeam_channel::Sender<Box<dyn FnOnce(&mut Cursive) + 'static + Send>>,
}

impl Control {
    pub fn quit(&self) {
        match self.cb_sink.send(Box::new(|s| s.quit())) {
            Ok(()) => (),
            Err(_) => (), // ignore. maybe ui exited.
        }
    }
}

fn quit(siv: &mut Cursive) {
    let ui_data: &mut UiData = siv.user_data().unwrap();
    ui_data.exit.signal();
    Cursive::quit(siv);
}

fn with_kodi<F, Ret>(siv: &mut Cursive, label: Option<&str>, func: F) -> Ret
where
    F: FnOnce(&mut KodiControl) -> Result<Ret, kodi_control::Error>,
{
    let ui_data: &mut UiData = siv.user_data().unwrap();
    let kodi_control = ui_data.kodi_control.clone();
    match label {
        None => (),
        Some(label) => siv
            .focus_name(label)
            .expect(format!("Failed to focus {}", label).as_str()),
    }
    let ret = util::sync_panic_error(|| {
        let mut control = kodi_control.lock().unwrap();
        Ok(func(&mut control)?)
    });
    ret
}

fn playlist_prev(siv: &mut Cursive) {
    with_kodi(siv, Some("playlist_prev"), |kc| kc.playlist_prev());
}

fn step(siv: &mut Cursive, label: &str, step: kodi_rpc_types::Step) {
    let seek = kodi_rpc_types::Seek::RelativeStep { step };
    //with_kodi(siv, Some(label), |kc| kc.async_seek(seek));
    let info = with_kodi(siv, Some(label), |kc| kc.seek(seek));
    update_time_from_seek_info(siv, info);
}

fn bwd_step_short(siv: &mut Cursive) {
    step(siv, "bwd_step_short", kodi_rpc_types::Step::SmallBackward);
}

fn bwd_step_long(siv: &mut Cursive) {
    step(siv, "bwd_step_long", kodi_rpc_types::Step::BigBackward);
}

fn fwd_step_short(siv: &mut Cursive) {
    step(siv, "fwd_step_short", kodi_rpc_types::Step::SmallForward);
}

fn fwd_step_long(siv: &mut Cursive) {
    step(siv, "fwd_step_long", kodi_rpc_types::Step::BigForward);
}

fn pause_play(siv: &mut Cursive) {
    with_kodi(siv, Some("play_pause"), |kc| kc.play_pause());
}

fn playlist_next(siv: &mut Cursive) {
    with_kodi(siv, Some("playlist_next"), |kc| kc.playlist_next());
}

impl std::fmt::Display for kodi_rpc_types::GlobalTime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{:02}:{:02}", self.hours, self.minutes, self.seconds)
    }
}

fn update_time(
    siv: &mut Cursive,
    time: Option<kodi_rpc_types::GlobalTime>,
    total_time: Option<kodi_rpc_types::GlobalTime>,
    percentage: Option<f64>,
) {
    match &time {
        None => (),
        Some(time) => {
            let ui_data: &mut UiData = siv.user_data().unwrap();
            ui_data.last_known_seconds =
                time.hours as u32 * 3600 + time.minutes as u32 * 60 + time.seconds as u32;
        }
    }
    siv.call_on_name("kodi_time", |view: &mut TextView| {
        let time = time.map(|x| x.to_string()).unwrap_or(String::from("-"));
        let total_time = total_time
            .map(|x| x.to_string())
            .unwrap_or(String::from("-"));
        view.set_content(format!("{} / {}", time, total_time));
    });
    match percentage {
        None => (),
        Some(percentage) => {
            let _ = siv.call_on_name("progress", |view: &mut ProgressBar| {
                view.set_value(percentage as usize);
            });
        }
    }
}

fn update_time_from_seek_info(siv: &mut Cursive, seek: kodi_rpc_types::PlayerSeekReturns) {
    update_time(siv, seek.time, seek.total_time, seek.percentage)
}

fn update_time_from_properties(siv: &mut Cursive, properties: kodi_rpc_types::PlayerPropertyValue) {
    update_time(
        siv,
        properties.time,
        properties.total_time,
        Some(properties.percentage),
    )
}

fn enter_seek_digit(siv: &mut Cursive, digit: char) {
    let cb_sink = siv.cb_sink().clone();
    let ui_seek = UiSeek::new(digit).set_callback(move |delta| {
        let _ = cb_sink.send(Box::new(move |siv| {
            let ui_data: &UiData = siv.user_data().unwrap();
            log::debug!("Seeking {} (old pos={})", delta, ui_data.last_known_seconds);
            let seek = kodi_rpc_types::Seek::RelativeSeconds { seconds: delta };
            let seek_finished = with_kodi(siv, None, |kc| kc.seek(seek));
            log::debug!("Seeking finished at {:?}", seek_finished);
        }));
    });
    siv.add_layer(
        Dialog::around(ui_seek)
            .title(format!("Seek"))
            .padding(Margins::lrtb(3, 3, 2, 2)),
    );
}

#[derive(Debug)]
struct KodiInfoCallback {
    cb_sink: crossbeam_channel::Sender<Box<dyn FnOnce(&mut Cursive) + 'static + Send>>,
}

impl kodi_control::KodiInfoCallback for KodiInfoCallback {
    fn playlist_position(&mut self, position: Option<kodi_rpc_types::PlaylistPosition>) {
        match self.cb_sink.send(Box::new(move |siv| {
            siv.call_on_name("kodi_playlist_position", |view: &mut TextView| {
                view.set_content(match position {
                    Some(position) => format!("#{}", position + 1),
                    None => String::from(""),
                });
            });
        })) {
            Ok(()) => (),
            Err(_) => (), // ignore. maybe ui exited.
        }
    }
}

impl Ui {
    pub fn new(kodi_control: KodiControl, exit: exit::Exit) -> Result<Ui, Error> {
        let mut siv = Cursive::default();
        let kodi_control = Arc::new(Mutex::new(kodi_control));
        kodi_control
            .lock()
            .unwrap()
            .set_callback(Box::new(KodiInfoCallback {
                cb_sink: siv.cb_sink().clone(),
            }))?;
        let ui_data = UiData {
            kodi_control: kodi_control.clone(),
            exit: exit.clone(),
            last_known_seconds: 0,
        };
        siv.set_user_data(ui_data);
        siv.set_theme(Self::create_theme(siv.current_theme().clone()));

        let playlist_position = TextView::new("Waiting..").with_name("kodi_playlist_position");
        let time = TextView::new("").with_name("kodi_time");

        let progress = ProgressBar::new()
            .range(0, 100)
            .with_label(|_value: usize, _bounds: (usize, usize)| -> String { String::from("") })
            .with_name("progress");

        // https://en.wikipedia.org/wiki/Media_control_symbols
        let buttons = LinearLayout::horizontal()
            .child(Button::new_raw("   \u{23ee}   ", playlist_prev).with_name("playlist_prev"))
            .child(
                Button::new_raw("   \u{23ea}\u{23ea}  ", bwd_step_long).with_name("bwd_step_long"),
            )
            .child(Button::new_raw("   \u{23ea}   ", bwd_step_short).with_name("bwd_step_short"))
            .child(Button::new_raw("   \u{23ef}   ", pause_play).with_name("play_pause"))
            .child(Button::new_raw("   \u{23e9}   ", fwd_step_short).with_name("fwd_step_short"))
            .child(
                Button::new_raw("   \u{23e9}\u{23e9}  ", fwd_step_long).with_name("fwd_step_long"),
            )
            .child(Button::new_raw("   \u{23ed}   ", playlist_next).with_name("playlist_next"))
            .child(DummyView)
            .child(Button::new_raw("Quit", quit));

        let view = LinearLayout::vertical()
            .child(progress)
            .child(DummyView)
            .child(playlist_position)
            .child(time)
            .child(DummyView)
            .child(buttons)
            .full_width()
            .wrap_with(OnEventView::new)
            .on_event('<', bwd_step_long)
            .on_event(',', bwd_step_short)
            .on_event('.', fwd_step_short)
            .on_event('>', fwd_step_long)
            .on_event(
                cursive::event::Event::Key(cursive::event::Key::PageUp),
                playlist_prev,
            )
            .on_event('[', playlist_prev)
            .on_event(
                cursive::event::Event::Key(cursive::event::Key::PageDown),
                playlist_next,
            )
            .on_event(']', playlist_next)
            .on_event(' ', pause_play);

        let view = "-0123456789".chars().fold(view, |view, digit| {
            view.on_event(digit, move |siv: &mut Cursive| {
                enter_seek_digit(siv, digit);
            })
        });

        siv.add_global_callback('q', |s| s.quit());

        siv.add_layer(
            Dialog::around(LinearLayout::horizontal().child(view).full_width())
                .title(format!("KodiKontrol {}", version::get_version()))
                .padding(Margins::lrtb(3, 3, 2, 2)),
        );

        let polling_thread = {
            let cb_sink = siv.cb_sink().clone();
            std::thread::spawn(move || Self::poll_updates(exit, kodi_control, cb_sink))
        };

        Ok(Ui {
            siv,
            polling_thread,
        })
    }

    #[rustfmt::skip::macros(select)]
    fn poll_updates(
        mut exit: exit::Exit,
        kodi_control: Arc<Mutex<KodiControl>>,
        cb_sink: crossbeam_channel::Sender<Box<dyn FnOnce(&mut Cursive) + 'static + Send>>,
    ) {
        enum Event {
            Tick,
        }
        let exit = exit.crossbeam_subscribe();
        let ticker = tick(std::time::Duration::from_millis(200));

        log::debug!("Starting polling");

        while let Some(event) = select! {
            recv(exit) -> _ => None,
            recv(ticker) -> _ => Some(Event::Tick),
        } {
            match event {
                Event::Tick => {
                    log::debug!("Tick");
                    let kodi_control = kodi_control.clone();
                    let cb_sink = cb_sink.clone();
                    let doit = move || -> Result<(), error::Error> {
                        let info = kodi_control.lock().unwrap().properties(vec![
                            kodi_rpc_types::PlayerPropertyName::TotalTime,
                            kodi_rpc_types::PlayerPropertyName::Percentage,
                            kodi_rpc_types::PlayerPropertyName::Time,
                            kodi_rpc_types::PlayerPropertyName::Speed,
                        ])?;
                        match info {
                            Some(info) => cb_sink
                                .send(Box::new(|s| update_time_from_properties(s, info)))
                                .map_err(|err| Error::CrossbeamSendError(err.to_string()))?,
                            None => (), // I guess we didn't get the data this time..
                        }

                        Ok(())
                    };
                    match doit() {
                        Ok(()) => log::debug!("Cool"),
                        Err(err) => log::debug!("error: {}", err),
                    }
                }
            }
        }
        log::debug!("Stopped polling");
    }

    fn create_theme(mut theme: cursive::theme::Theme) -> cursive::theme::Theme {
        use cursive::theme;
        use cursive::theme::{BaseColor::*, Color::*, PaletteColor::*};

        theme.shadow = false;
        theme.borders = theme::BorderStyle::Simple;
        theme.palette[Background] = Dark(Black);
        theme.palette[View] = Light(Black);
        theme.palette[Primary] = Light(White);
        theme.palette[TitlePrimary] = Light(White);

        theme
    }

    pub fn control(&mut self) -> Control {
        let cb_sink = self.siv.cb_sink().clone();
        Control { cb_sink }
    }

    pub fn run(&mut self) {
        self.siv.run();
    }

    pub fn finish(mut self) {
        let ui_data: UiData = self.siv.take_user_data().unwrap();
        ui_data.exit.signal();
        let _ignore = self.polling_thread.join();
    }
}
