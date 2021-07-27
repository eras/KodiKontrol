use clap::ArgMatches;
use kodi_kontrol::{server, version::get_version};

use std::collections::HashMap;
use std::path::PathBuf;

use trust_dns_resolver::AsyncResolver;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    ResolveError(#[from] trust_dns_resolver::error::ResolveError),
}

async fn resolve_address(args: &ArgMatches) -> Result<std::net::IpAddr, Error> {
    let resolver = AsyncResolver::tokio_from_system_conf()?;

    let kodi_address: std::net::IpAddr = match args.value_of("kodi") {
        Some(host) => resolver.lookup_ip(host).await?.iter().next().unwrap(),
        None => "127.0.0.1".parse().unwrap(),
    };

    Ok(kodi_address)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();

    let args = clap::App::new("koko")
        .version(get_version().as_str())
        .author("Erkki Seppälä <erkki.seppala@vincit.fi>")
        .about("Remote Kontroller and streamer for Kodi")
        .arg(
            clap::Arg::new("SOURCE")
                .required(true)
                .index(1)
                .multiple(true)
                .about("File to stream"),
        )
        .arg(
            clap::Arg::new("kodi")
                .long("kodi")
                .short('k')
                .takes_value(true)
                .about("Address of the host running Kodi; defaults to localhost"),
        )
        .arg(
            clap::Arg::new("user")
                .long("user")
                .short('u')
                .takes_value(true)
                .about("Username of the user for Kodi"),
        )
        .arg(
            clap::Arg::new("password")
                .long("pass")
                .short('p')
                .takes_value(true)
                .about("Password for the user"),
        )
        .get_matches();

    let kodi_address = {
        match resolve_address(&args).await {
            Ok(x) => x,
            Err(err) => {
                eprintln!("error: {:?}", err);
                return Ok(());
            }
        }
    };

    let mut files = HashMap::new();
    let mut urls_order = HashMap::new();
    let mut file_counts = HashMap::new();

    let mut order_index = 0usize;

    for source in args.values_of("SOURCE").unwrap() {
        let path: PathBuf = source
            .to_string()
            .parse()
            .expect("Failed to parse filename");
        let url_name = path
            .file_stem()
            .unwrap()
            .to_str()
            .expect("TODO: filename is required to be valid UTF8")
            .to_string();

        let mut count = if file_counts.contains_key(&url_name) {
            let count = file_counts.get(&url_name).unwrap() + 1;
            file_counts.insert(url_name.clone(), count);
            count
        } else {
            let count = 1;
            file_counts.insert(url_name.clone(), count);
            count
        };

        fn name(base: &str, count: u32) -> String {
            if count == 1 {
                base.to_string()
            } else {
                format!("{} #{}", base, count)
            }
        }

        // maybe this algorithm gives wild names in some corner cases..
        while files.contains_key(&name(&url_name, count)) {
            count += 1;
        }
        files.insert(name(&url_name, count), String::from(source));
        urls_order.insert(name(&url_name, count), order_index);
        order_index += 1;
    }

    let app_data = server::make_app_data_holder(server::AppData {
        files,
        urls_order,
        kodi_address,
        previously_logged_file: None,
    });
    let (session_tx, session_rx) = tokio::sync::oneshot::channel::<server::Session>();
    let app_join: tokio::task::JoinHandle<Result<(), kodi_kontrol::error::Error>> =
        tokio::task::spawn(async move {
            log::debug!("Waiting session");
            let session = session_rx.await.expect("Failed to receive session");
            log::debug!("Got session");
            match session.finish().await {
                Ok(()) => {
                    log::info!("Exiting");
                    Ok(())
                }
                Err(error) => {
                    eprintln!("Setup with error: {}", error);
                    actix_rt::System::current().stop();
                    Ok(())
                }
            }
        });

    match server::Session::new(app_data, session_tx).await {
        Ok(()) => (),
        Err(err) => {
            eprintln!("error: {:?}", err);
            return Ok(());
        }
    }

    match app_join.await.expect("Failed to join app_join") {
        Ok(()) => Ok(()),
        Err(err) => {
            eprintln!("error: {:?}", err);
            return Ok(());
        }
    }
}
