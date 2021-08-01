use crate::exit::Exit;
use futures::{pin_mut, stream::StreamExt};
use mdns::{Record, RecordKind};
use std::{net::IpAddr, time::Duration};
use tokio::select;

use std::collections::HashSet;

const HTTP_SERVICE_NAME: &str = "_xbmc-jsonrpc-h._tcp.local";
//const WS_SERVICE_NAME: &str = "_xbmc-jsonrpc._tcp.local";

pub type Error = mdns::Error;

#[rustfmt::skip::macros(select)]
pub async fn discover_first(hostname: &str) -> Result<Option<IpAddr>, Error> {
    let stream = mdns::discover::all(HTTP_SERVICE_NAME, Duration::from_secs(1))?.listen();
    pin_mut!(stream);

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(5000);

    log::info!("Starting discovering for {}", hostname);

    while let Some(response) = select! {
	response = stream.next() => {
	    match response {
		Some(Ok(response)) => Some(response),
		Some(Err(_)) => None,
		None => None,
	    }
	}
	_deadline = tokio::time::sleep_until(deadline) => {
	    None
	}
    } {
        for record in response.records() {
            if record.name == hostname {
                if let Some(ip) = to_ip_addr(&record) {
                    log::info!("Found ip {}", ip);
                    return Ok(Some(ip));
                }
            }
        }
    }
    log::info!("Found no ip");
    Ok(None)
}

#[rustfmt::skip::macros(select)]
pub async fn discover(tx: crossbeam_channel::Sender<Record>, mut exit: Exit) -> Result<(), Error> {
    let discovery: tokio::task::JoinHandle<Result<(), Error>> = tokio::spawn(async move {
        // Use a short polling period due to
        // https://github.com/dylanmckay/mdns/pull/25 not yet merged:
        let stream = mdns::discover::all(HTTP_SERVICE_NAME, Duration::from_secs(1))?.listen();
        pin_mut!(stream);

        log::info!("Starting discovery");

        let mut seen = HashSet::new();

        while let Some(Ok(response)) = stream.next().await {
            log::info!("Got a record");
            for record in response.records() {
                log::info!("Passing record");
                // lol
                let record_str = format!("{:?}", record);
                if !seen.contains(&record_str) {
                    seen.insert(record_str);
                    if let Err(_) = tx.send(record.clone()) {
                        log::info!("Failed to pass record: exiting");
                        return Ok(());
                    }
                }
            }
        }
        log::info!("Finished discovery");
        Ok(())
    });

    select! {
    	result = discovery => {
	    log::info!("Discovery exiting (due to discovery terminated)");
            Ok(result.unwrap()?)
    	},
    	_exit = exit.wait() => {
	    log::info!("Discovery exiting (due to exit activated)");
            Ok(())
    	}
    }
}

pub fn to_ip_addr(record: &Record) -> Option<IpAddr> {
    match record.kind {
        RecordKind::A(addr) => Some(addr.into()),
        RecordKind::AAAA(addr) => Some(addr.into()),
        _ => None,
    }
}
