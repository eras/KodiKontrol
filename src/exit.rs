use std::sync::{Arc, Mutex};
use tokio::sync::watch;

pub struct Exit {
    level_rx: watch::Receiver<bool>,
    level_tx: Arc<Mutex<watch::Sender<bool>>>,
    crossbeam_tx: Arc<Mutex<Vec<crossbeam_channel::Sender<()>>>>,
}

impl Clone for Exit {
    fn clone(&self) -> Self {
        let level_rx = self.level_rx.clone();
        let level_tx = self.level_tx.clone();
        let crossbeam_tx = self.crossbeam_tx.clone();
        Exit {
            level_rx,
            level_tx,
            crossbeam_tx,
        }
    }
}

impl Exit {
    pub fn new() -> Exit {
        let (level_tx, level_rx) = watch::channel(false);
        let level_tx = Arc::new(Mutex::new(level_tx));
        let crossbeam_tx = Arc::new(Mutex::new(vec![]));
        Exit {
            level_rx,
            level_tx,
            crossbeam_tx,
        }
    }

    pub fn signal(&self) {
        self.level_tx
            .lock()
            .unwrap()
            .send(true)
            .expect("Failed to send exit signal");
        match self.crossbeam_tx.lock() {
            Ok(tx) => {
                for tx in tx.iter() {
                    let _ignore = tx.send(());
                }
            }
            Err(_) => log::error!("exit::signal failed to lock crossbeam_tx"),
        }
    }

    pub async fn wait(&mut self) {
        while !*self.level_rx.borrow() {
            self.level_rx
                .changed()
                .await
                .expect("Failed to wait for exit level change");
        }
    }

    pub fn crossbeam_subscribe(&mut self) -> crossbeam_channel::Receiver<()> {
        let (tx, rx) = crossbeam_channel::unbounded();
        self.crossbeam_tx.lock().unwrap().push(tx);
        if *self.level_rx.borrow() {
            match self.crossbeam_tx.lock() {
                Ok(tx) => {
                    for tx in tx.iter() {
                        let _ignore = tx.send(());
                    }
                }
                Err(_) => log::error!("exit::signal failed to lock crossbeam_tx"),
            }
        }
        rx
    }
}
