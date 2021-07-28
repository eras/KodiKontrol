use cursive::traits::*;
use cursive::view::Margins;
use cursive::views::{Button, Dialog, DummyView, LinearLayout, TextView};
use cursive::{Cursive, CursiveExt};

use crate::kodi_control::KodiControl;

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

impl Ui {
    pub fn new(kodi_control: KodiControl) -> Ui {
        let mut siv = Cursive::default();
        let ui_data = UiData { kodi_control };
        siv.set_user_data(ui_data);
        siv.set_theme(Self::create_theme(siv.current_theme().clone()));

        //siv.clear_global_callbacks(cursive::event::Event::CtrlChar('c'));

        siv.add_layer(TextView::new("Hello World!\nPress q to quit."));

        // let select = SelectView::<String>::new()
        //     // .on_submit(on_submit)
        //     .with_name("select")
        //     .fixed_size((10, 5));
        let buttons = LinearLayout::horizontal()
            .child(Button::new_raw("   \u{23ee}   ", playlist_prev).with_name("prev"))
            .child(Button::new_raw("   \u{23ef}   ", pause_play).with_name("play_pause"))
            .child(Button::new_raw("   \u{23ed}   ", playlist_next).with_name("next"))
            .child(DummyView)
            .child(Button::new_raw("Quit", Cursive::quit));

        siv.add_layer(
            Dialog::around(LinearLayout::horizontal().child(buttons).full_width())
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
