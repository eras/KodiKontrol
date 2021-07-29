use cursive::traits::*;
use cursive::view::Margins;
use cursive::views::{Button, Dialog, DummyView, LinearLayout, TextView};
use cursive::{Cursive, CursiveExt};

use crate::{kodi_control, kodi_control::KodiControl, kodi_rpc_types};

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

fn playlist_prev(siv: &mut Cursive) {
    let ui_data: &mut UiData = siv.user_data().unwrap();
    util::sync_panic_error(|| Ok(ui_data.kodi_control.lock().unwrap().playlist_prev()?));
    siv.focus_name("prev").expect("Failed to focus prev");
}

fn pause_play(siv: &mut Cursive) {
    let ui_data: &mut UiData = siv.user_data().unwrap();
    util::sync_panic_error(|| Ok(ui_data.kodi_control.lock().unwrap().play_pause()?));
    siv.focus_name("play_pause")
        .expect("Failed to focus play pause");
}

fn playlist_next(siv: &mut Cursive) {
    let ui_data: &mut UiData = siv.user_data().unwrap();
    util::sync_panic_error(|| Ok(ui_data.kodi_control.lock().unwrap().playlist_next()?));
    siv.focus_name("next").expect("Failed to focus next");
}

impl std::fmt::Display for kodi_rpc_types::GlobalTime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{:02}:{:02}", self.hours, self.minutes, self.seconds)
    }
}

fn update_time(siv: &mut Cursive, properties: kodi_rpc_types::PlayerPropertyValue) {
    siv.call_on_name("kodi_time", |view: &mut TextView| {
        let time = properties
            .time
            .map(|x| x.to_string())
            .unwrap_or(String::from("-"));
        let total_time = properties
            .total_time
            .map(|x| x.to_string())
            .unwrap_or(String::from("-"));
        view.set_content(format!("{} / {}", time, total_time));
    });
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
                    Some(position) => format!("Position: {}", position + 1),
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
        };
        siv.set_user_data(ui_data);
        siv.set_theme(Self::create_theme(siv.current_theme().clone()));

        let playlist_position = TextView::new("Waiting..").with_name("kodi_playlist_position");
        let time = TextView::new("").with_name("kodi_time");

        let buttons = LinearLayout::horizontal()
            .child(Button::new_raw("   \u{23ee}   ", playlist_prev).with_name("prev"))
            .child(Button::new_raw("   \u{23ef}   ", pause_play).with_name("play_pause"))
            .child(Button::new_raw("   \u{23ed}   ", playlist_next).with_name("next"))
            .child(DummyView)
            .child(Button::new_raw("Quit", quit));

        let view = LinearLayout::vertical()
            .child(playlist_position)
            .child(time)
            .child(DummyView)
            .child(buttons);

        siv.add_layer(
            Dialog::around(LinearLayout::horizontal().child(view).full_width())
                .title("KoKo")
                .padding(Margins::lrtb(3, 3, 2, 2)),
        );

        siv.add_global_callback('q', |s| s.quit());
        siv.add_global_callback('<', playlist_prev);
        siv.add_global_callback('>', playlist_next);
        siv.add_global_callback(' ', pause_play);

        let polling_thread = {
            let cb_sink = siv.cb_sink().clone();
            std::thread::spawn(move || Self::poll_updates(exit, kodi_control, cb_sink))
        };

        Ok(Ui {
            siv,
            polling_thread,
        })
    }

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
            }
        {
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
                        cb_sink
                            .send(Box::new(|s| update_time(s, info)))
                            .map_err(|err| Error::CrossbeamSendError(err.to_string()))?;

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
        self.polling_thread
            .join()
            .expect("Failed to join polling thread");
    }
}
