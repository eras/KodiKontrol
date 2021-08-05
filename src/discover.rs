use crate::exit::Exit;
use futures::{pin_mut, stream::StreamExt};
use mdns::{Record, RecordKind};
use std::{net::IpAddr, time::Duration};
use tokio::select;

use std::collections::{HashMap, HashSet};

const HTTP_SERVICE_NAME: &str = "_xbmc-jsonrpc-h._tcp.local";
//const WS_SERVICE_NAME: &str = "_xbmc-jsonrpc._tcp.local";

pub type Error = mdns::Error;

#[derive(Debug, Clone)]
pub struct Service {
    pub name: String,
    pub address: IpAddr,
    pub port: Option<u16>,
}

#[rustfmt::skip::macros(select)]
pub async fn discover_first(hostname: &str) -> Result<Option<Service>, Error> {
    let stream = mdns::discover::all(HTTP_SERVICE_NAME, Duration::from_secs(1))?.listen();
    pin_mut!(stream);

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(5000);

    log::info!("Starting discovering for {}", hostname);

    // keep track of all addresses seen, so we are able to resolve SRV records as well
    let mut target_info = HashMap::new();

    // we received a matching SRV but were unable to resolve it; the info is here
    let mut pending_srv = None;

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
                match &record.kind {
                    RecordKind::SRV { .. } => {
                        pending_srv = Some(record.clone());
                    }
                    RecordKind::A(_) | RecordKind::AAAA(_) => {
                        if let Some(ip) = to_ip_addr(&record) {
                            log::info!("Found ip {}", ip);
                            return Ok(Some(Service {
                                name: record.name.clone(),
                                address: ip,
                                port: None,
                            }));
                        }
                    }
                    _ => {
                        // ignore rest
                    }
                }
            } else {
                if let Some(ip) = to_ip_addr(&record) {
                    target_info.insert(record.name.clone(), ip);
                }
            }
            match &pending_srv {
                Some(Record {
                    name,
                    kind: RecordKind::SRV { port, target, .. },
                    ..
                }) => {
                    if let Some(ip) = target_info.get(target) {
                        log::info!("Found ip and port {}:{}", ip, port);
                        return Ok(Some(Service {
                            name: name.clone(),
                            address: ip.clone(),
                            port: Some(*port),
                        }));
                    }
                }
                _ => (),
            }
        }
    }
    log::info!("Found no ip");
    Ok(None)
}

#[rustfmt::skip::macros(select)]
pub async fn discover(tx: crossbeam_channel::Sender<Service>, mut exit: Exit) -> Result<(), Error> {
    let discovery: tokio::task::JoinHandle<Result<(), Error>> = tokio::spawn(async move {
        // Use a short polling period due to
        // https://github.com/dylanmckay/mdns/pull/25 not yet merged:
        let stream = mdns::discover::all(HTTP_SERVICE_NAME, Duration::from_secs(1))?.listen();
        pin_mut!(stream);

        log::info!("Starting discovery");

        // keep track of all addresses seen, so we are able to resolve SRV records as well
        let mut target_info = HashMap::new();

        // filter out duplicate information sent to subscriber
        let mut seen = HashSet::new();

        // pending SRV requests by the target
        let mut pending_srv_target: HashMap<String, Vec<Record>> = HashMap::new();

        while let Some(Ok(response)) = stream.next().await {
            for record in response.records() {
                log::info!("Got a record {:?}", record);
                if !seen.contains(&record.clone()) {
                    if let Some(ip) = to_ip_addr(&record) {
                        target_info.insert(record.name.clone(), ip);
                    }
                    log::info!("New record");
                    seen.insert(record.clone());
                    match &record.kind {
                        RecordKind::A(_) | RecordKind::AAAA(_) => {
                            let address = to_ip_addr(&record)
                                .expect("to_ip_addr failed to resolve A or AAAA address");
                            let service = Service {
                                name: record.name.clone(),
                                address: address.clone(),
                                port: None,
                            };
                            if let Err(_) = tx.send(service) {
                                log::info!("Failed to pass record: exiting");
                                return Ok(());
                            }
                            if let Some(srvs) = pending_srv_target.get(&record.name) {
                                for srv in srvs {
                                    // todo: handle TTL :)
                                    match srv {
                                        Record {
                                            kind: RecordKind::SRV { port, .. },
                                            ..
                                        } => {
                                            let service = Service {
                                                name: srv.name.clone(),
                                                address: address.clone(),
                                                port: Some(*port),
                                            };
                                            if let Err(_) = tx.send(service) {
                                                log::info!("Failed to pass record: exiting");
                                                return Ok(());
                                            }
                                        }
                                        _ => (),
                                    }
                                }
                                pending_srv_target.remove(&record.name);
                            }
                        }
                        RecordKind::SRV { target, port, .. } => {
                            if let Some(address) = target_info.get(target) {
                                let service = Service {
                                    name: record.name.clone(),
                                    address: address.clone(),
                                    port: Some(*port),
                                };
                                if let Err(_) = tx.send(service) {
                                    log::info!("Failed to pass record: exiting");
                                    return Ok(());
                                }
                            } else {
                                if !pending_srv_target.contains_key(target) {
                                    pending_srv_target.insert(target.clone(), vec![]);
                                }
                                let pending = pending_srv_target.get_mut(target).unwrap();
                                (*pending).push(record.clone());
                            }
                        }
                        _ => (),
                    }
                } else {
                    log::info!(
                        "Skipped duplicate record. Previous record: {:?}",
                        seen.get(&record.clone()).unwrap()
                    );
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
