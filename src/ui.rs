use cursive::traits::*;
use cursive::view::Margins;
use cursive::views::{Button, Dialog, DummyView, LinearLayout, TextView};
use cursive::{Cursive, CursiveExt};

use crate::{kodi_control, kodi_control::KodiControl, kodi_rpc_types};

pub struct Ui {
    siv: Cursive,
}

struct UiData {
    kodi_control: KodiControl,
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

fn playlist_prev(siv: &mut Cursive) {
    let ui_data: &mut UiData = siv.user_data().unwrap();
    ui_data.kodi_control.playlist_prev();
    siv.focus_name("prev").expect("Failed to focus prev");
}

fn pause_play(siv: &mut Cursive) {
    let ui_data: &mut UiData = siv.user_data().unwrap();
    ui_data.kodi_control.play_pause();
    siv.focus_name("play_pause")
        .expect("Failed to focus play pause");
}

fn playlist_next(siv: &mut Cursive) {
    let ui_data: &mut UiData = siv.user_data().unwrap();
    ui_data.kodi_control.playlist_next();
    siv.focus_name("next").expect("Failed to focus next");
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
    pub fn new(mut kodi_control: KodiControl) -> Ui {
        let mut siv = Cursive::default();
        kodi_control.set_callback(Box::new(KodiInfoCallback {
            cb_sink: siv.cb_sink().clone(),
        }));
        let ui_data = UiData { kodi_control };
        siv.set_user_data(ui_data);
        siv.set_theme(Self::create_theme(siv.current_theme().clone()));

        let info = TextView::new("Waiting..").with_name("kodi_playlist_position");

        let buttons = LinearLayout::horizontal()
            .child(Button::new_raw("   \u{23ee}   ", playlist_prev).with_name("prev"))
            .child(Button::new_raw("   \u{23ef}   ", pause_play).with_name("play_pause"))
            .child(Button::new_raw("   \u{23ed}   ", playlist_next).with_name("next"))
            .child(DummyView)
            .child(Button::new_raw("Quit", Cursive::quit));

        let view = LinearLayout::vertical()
            .child(info)
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

        Ui { siv }
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

    pub fn finish(self) {}
}
