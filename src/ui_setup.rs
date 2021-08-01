use cursive::traits::*;
use cursive::utils::span::SpannedString;
use cursive::view::Margins;
use cursive::views::{
    Button, Dialog, DummyView, EditView, LinearLayout, ScrollView, SelectView, TextView,
};
use cursive::{Cursive, CursiveExt};

use crate::ui_callback::*;

use std::collections::BTreeMap;

use crate::{config, ui_discovery, version};

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("error: {}", .0)]
    Message(String), // message
}

pub struct UiSetup {
    siv: Cursive,
}

pub struct UiSetupData {
    config: config::Config,
    config_file: String,
}

type ConfigHostsView = SelectView<(usize, String, config::Host)>;

fn edit_remove(siv: &mut Cursive, index: usize) {
    siv.call_on_name("config_hosts", |view: &mut ConfigHostsView| {
        view.remove_item(index);
        for shift_index in index..view.len() {
            let label_item = view.get_item_mut(shift_index).unwrap();
            label_item.1 .0 -= 1;
        }
    });
    siv.pop_layer();
}

fn make_edit_view(label: &str, name: &str, width: usize, content: String) -> Box<dyn View> {
    Box::new(
        LinearLayout::horizontal()
            .child(TextView::new(label))
            .child(
                EditView::new()
                    .content(content)
                    .with_name(name)
                    .fixed_width(width),
            ),
    )
}

#[derive(Clone)]
struct UpdateDialog {
    ok_callback: Callback<(String, config::Host), bool>,
    remove_callback: Callback<(), ()>,
}

impl UpdateDialog {
    fn open(siv: &mut Cursive, (label, host): (String, config::Host), show_remove: bool) -> Self {
        let ok_callback = Callback::new();
        let remove_callback = Callback::new();

        siv.add_layer({
            let dialog = Dialog::around(
                LinearLayout::vertical()
                    .child(make_edit_view("      Label: ", "label", 20, label.clone()))
                    .child(make_edit_view(
                        "   Hostname: ",
                        "hostname",
                        40,
                        host.hostname.clone().unwrap_or(label.clone()),
                    ))
                    .child(make_edit_view(
                        "       Port: ",
                        "port",
                        6,
                        host.port
                            .clone()
                            .map(|x| x.to_string())
                            .unwrap_or(String::from("")),
                    ))
                    .child(make_edit_view(
                        "Server port: ",
                        "listen_port",
                        6,
                        host.listen_port
                            .clone()
                            .map(|x| x.to_string())
                            .unwrap_or(String::from("")),
                    ))
                    .child(make_edit_view(
                        "   Username: ",
                        "username",
                        40,
                        host.username.clone().unwrap_or(String::from("")),
                    ))
                    .child(make_edit_view(
                        "   Password: ",
                        "password",
                        40,
                        host.password.clone().unwrap_or(String::from("")),
                    )),
            )
            .button("Ok", {
                let ok_callback = ok_callback.clone();
                move |siv| {
                    let label = edit_view_content(siv, "label");
                    let host = Self::make_host_from_edit(siv);
                    match ok_callback.call(siv, (label, host)) {
                        None | Some(true) => {
                            siv.pop_layer();
                        }
                        Some(_) => (),
                    }
                }
            });
            let dialog = if show_remove {
                dialog.button("Remove", {
                    let remove_callback = remove_callback.clone();
                    move |siv| {
                        let _ = remove_callback.call(siv, ());
                    }
                })
            } else {
                dialog
            };
            dialog.button("Cancel", |siv| {
                siv.pop_layer();
            })
        });

        Self {
            ok_callback,
            remove_callback,
        }
    }

    fn on_ok<F>(mut self, callback: F) -> Self
    where
        F: Fn(&mut Cursive, (String, config::Host)) -> bool + 'static,
    {
        self.ok_callback.set(callback);
        self
    }

    fn on_remove<F>(mut self, callback: F) -> Self
    where
        F: Fn(&mut Cursive, ()) -> () + 'static,
    {
        self.remove_callback.set(callback);
        self
    }

    fn make_host_from_edit(siv: &mut Cursive) -> config::Host {
        // config::Host {

        // }

        // TODO: better handling/indication of errors

        let hostname = empty_to_none(edit_view_content(siv, "hostname"));
        let port: Option<u16> = edit_view_content(siv, "port").parse().ok();
        let listen_port: Option<u16> = edit_view_content(siv, "listen_port").parse().ok();
        let username = empty_to_none(edit_view_content(siv, "username"));
        let password = empty_to_none(edit_view_content(siv, "password"));

        config::Host {
            hostname,
            port,
            username,
            password,
            listen_port,
        }
    }
}

fn empty_to_none(value: String) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn edit_view_content(siv: &mut Cursive, name: &str) -> String {
    siv.call_on_name(name, move |view: &mut EditView| {
        (*view.get_content()).clone()
    })
    .unwrap()
}

fn add_dialog(siv: &mut Cursive) {
    let host = config::Host {
        ..Default::default()
    };
    UpdateDialog::open(siv, (String::from("kodi"), host), false).on_ok(
        move |siv: &mut Cursive, (label, host): (String, config::Host)| -> bool {
            siv.call_on_name("config_hosts", move |config_hosts: &mut ConfigHostsView| {
                let index = config_hosts.len();
                config_hosts.add_item(label.clone(), (index, label.clone(), host.clone()));
                true
            });
            true
        },
    );
}

fn edit_dialog(siv: &mut Cursive, (index, label, host): (usize, String, config::Host)) {
    UpdateDialog::open(siv, (label, host), true)
        .on_ok(
            move |siv: &mut Cursive, (label, host): (String, config::Host)| -> bool {
                siv.call_on_name("config_hosts", move |config_hosts: &mut ConfigHostsView| {
                    let label_item = config_hosts.get_item_mut(index).unwrap();
                    *label_item.0 = SpannedString::from(label.clone());
                    label_item.1 .1 = label.clone();
                    label_item.1 .2 = host;
                });
                true
            },
        )
        .on_remove(move |siv: &mut Cursive, ()| {
            edit_remove(siv, index);
        });
}

fn save(siv: &mut Cursive) {
    let host: BTreeMap<String, config::Host> = siv
        .call_on_name("config_hosts", move |config_hosts: &mut ConfigHostsView| {
            config_hosts
                .iter()
                .map(|(_name, index_label_host)| {
                    (index_label_host.1.clone(), index_label_host.2.clone()).clone()
                })
                .collect()
        })
        .unwrap();
    let ui_data: &mut UiSetupData = siv.user_data().unwrap();
    let config = config::Config {
        host,
        ..ui_data.config.clone()
    };
    match config.save(ui_data.config_file.as_str()) {
        Ok(()) => siv.quit(),
        Err(err) => siv.add_layer(Dialog::info(format!("Failed to save: {}", err))),
    }
}

fn cancel(siv: &mut Cursive) {
    siv.quit();
}

impl UiSetup {
    pub fn new(config: config::Config, config_file: &str) -> UiSetup {
        let mut siv = Cursive::default();

        let mut config_hosts = ConfigHostsView::new();

        {
            let mut index = 0;
            for (name, host) in &config.host {
                config_hosts.add_item(name, (index, name.clone(), host.clone()));
                index += 1;
            }
        }

        config_hosts.set_on_submit(|siv, index_label_host| {
            edit_dialog(siv, index_label_host.clone());
        });

        let config_hosts = config_hosts
            //.on_submit(on_submit)
            .with_name("config_hosts")
            .wrap_with(ScrollView::new)
            .show_scrollbars(true);

        let config_file_view = TextView::new(format!("Config file: {}", config_file));

        let hosts = LinearLayout::horizontal().child(
            Dialog::around(LinearLayout::vertical().child(config_hosts))
                .title("Config")
                .padding(Margins::lrtb(2, 2, 0, 0)),
        );

        let buttons = LinearLayout::horizontal()
            .child(Button::new("New host", add_dialog))
            .child(DummyView)
            .child(Button::new("Save and exit", save))
            .child(DummyView)
            .child(Button::new("Exit without saving", cancel));

        let top_level_view = LinearLayout::vertical()
            .child(config_file_view)
            .child(DummyView)
            .child(hosts)
            .child(DummyView)
            .child(buttons);

        siv.add_layer(
            Dialog::around(LinearLayout::horizontal().child(top_level_view))
                .title(format!("KodiKontrol {}", version::get_version()))
                .padding(Margins::lrtb(3, 3, 1, 1)),
        );

        let ui_data = UiSetupData {
            config,
            config_file: String::from(config_file),
        };
        siv.set_user_data(ui_data);
        UiSetup { siv }
    }

    pub fn run(mut self) -> Result<(), Error> {
        self.siv.run();
        Ok(())
    }
}
