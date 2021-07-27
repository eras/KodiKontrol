use std::sync::{Arc, Mutex};
use tokio::sync::watch;

pub struct Exit {
    level_rx: watch::Receiver<bool>,
    level_tx: Arc<Mutex<watch::Sender<bool>>>,
}

impl Clone for Exit {
    fn clone(&self) -> Self {
        let level_rx = self.level_rx.clone();
        let level_tx = self.level_tx.clone();
        Exit { level_rx, level_tx }
    }
}

impl Exit {
    pub fn new() -> Exit {
        let (level_tx, level_rx) = watch::channel(false);
        let level_tx = Arc::new(Mutex::new(level_tx));
        Exit { level_rx, level_tx }
    }

    pub fn signal(&self) {
        self.level_tx
            .lock()
            .unwrap()
            .send(true)
            .expect("Failed to send exit signal");
    }

    pub async fn wait(&mut self) {
        while !*self.level_rx.borrow() {
            self.level_rx
                .changed()
                .await
                .expect("Failed to wait for exit level change");
        }
    }
}
