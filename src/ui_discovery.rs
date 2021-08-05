use cursive::traits::*;
use cursive::view::ViewWrapper;
use cursive::views::{NamedView, SelectView};
use cursive::Cursive;

use crate::{discover, exit::Exit, ui_callback::*};

use crossbeam_channel::{unbounded, Receiver};

type CursiveCbSink = crossbeam_channel::Sender<Box<dyn FnOnce(&mut Cursive) + 'static + Send>>;

type DiscoveryHostsView = SelectView<discover::Service>;

pub struct UiDiscovery {
    hosts: NamedView<DiscoveryHostsView>,
    submit_callback: Callback<discover::Service, ()>,
}

impl ViewWrapper for UiDiscovery {
    cursive::wrap_impl!(self.hosts: NamedView<DiscoveryHostsView>);
}

impl UiDiscovery {
    pub fn new(exit: Exit, cursive_cb_sink: CursiveCbSink) -> UiDiscovery {
        let submit_callback = Callback::new();
        let mut hosts = DiscoveryHostsView::new();
        {
            let submit_callback = submit_callback.clone();
            hosts.set_on_submit(move |siv, record| {
                submit_callback.call(siv, record.clone());
            });
        }
        let hosts = hosts.with_name("discovery_hosts_view");
        let (discover_tx, discover_rx) = unbounded();
        {
            let exit = exit.clone();
            log::info!("Spawning discovery");
            tokio::spawn(async move { discover::discover(discover_tx, exit).await });
        }

        let _receiver_thread = {
            let exit = exit.clone();
            std::thread::spawn(move || Self::receiver(discover_rx, cursive_cb_sink, exit))
        };

        UiDiscovery {
            hosts,
            submit_callback,
        }
    }

    pub fn on_submit<F>(mut self, callback: F) -> Self
    where
        F: Fn(&mut Cursive, discover::Service) + 'static,
    {
        self.submit_callback.set(callback);
        self
    }

    fn on_entry(siv: &mut Cursive, service: discover::Service) {
        siv.call_on_name(
            "discovery_hosts_view",
            move |hosts: &mut DiscoveryHostsView| {
                hosts.add_item(
                    format!(
                        "{} at {}{}",
                        service.name,
                        service.address,
                        service
                            .port
                            .map(|x| format!(":{}", x))
                            .unwrap_or(String::from(""))
                    )
                    .clone(),
                    service,
                );
            },
        )
        .unwrap();
    }

    // TODO: support exit.Exit
    fn receiver(
        discover_rx: Receiver<discover::Service>,
        cursive_cb_sink: CursiveCbSink,
        _exit: Exit,
    ) {
        loop {
            if let Ok(entry) = discover_rx.recv() {
                let _ = cursive_cb_sink.send(Box::new(|siv| Self::on_entry(siv, entry)));
            } else {
                break;
            }
        }
    }
}
