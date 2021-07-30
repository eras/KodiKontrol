use clap::ArgMatches;
use kodi_kontrol::{exit, kodi_control, server, ui, util, version::get_version};

use std::collections::HashMap;
use std::path::PathBuf;

use trust_dns_resolver::AsyncResolver;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    ResolveError(#[from] trust_dns_resolver::error::ResolveError),

    #[error(transparent)]
    SetupError(#[from] SetupError),

    #[error("Failed to parse time: {}", .0)]
    ParseTimeError(String),
}

async fn resolve_address(args: &ArgMatches) -> Result<std::net::IpAddr, Error> {
    let resolver = AsyncResolver::tokio_from_system_conf()?;

    let kodi_address: std::net::IpAddr = match args.value_of("kodi") {
        Some(host) => resolver.lookup_ip(host).await?.iter().next().unwrap(),
        None => "127.0.0.1".parse().unwrap(),
    };

    Ok(kodi_address)
}

#[derive(Error, Debug)]
pub enum SetupError {
    #[error(transparent)]
    InitLoggingError(#[from] log4rs::config::InitError),

    #[error(transparent)]
    LogFileOpenError(#[from] std::io::Error),
}

fn init_logging(enable: bool) -> Result<(), SetupError> {
    if !enable {
        return Ok(());
    }
    use log::LevelFilter;
    use log4rs::append::file::FileAppender;
    use log4rs::config::{Appender, Config, Root};
    use log4rs::encode::pattern::PatternEncoder;

    let logfile = FileAppender::builder()
        .encoder(Box::new(PatternEncoder::new("{d} {l} {M} {m}\n")))
        .build("koko.log")?;

    let config = Config::builder()
        .appender(Appender::builder().build("logfile", Box::new(logfile)))
        .build(
            Root::builder()
                .appender("logfile")
                .build(LevelFilter::Debug),
        )
        .map_err(|x| log4rs::config::InitError::BuildConfig(x))?;

    log4rs::init_config(config).map_err(|x| log4rs::config::InitError::SetLogger(x))?;

    Ok(())
}

// 1h4m3s -> 1*3600 + 4*60 + 3
fn parse_time_as_seconds(str: &str) -> Result<u32, Error> {
    enum State {
        Begin,
        Value(u32),
        Mul,
    }
    enum Class {
        Digit(u8),
        Multiplier(u32),
    }
    let mut state = State::Begin;
    let mut seconds = 0u32;
    for char in str.chars() {
        let class = match char {
            char if char >= '0' && char <= '9' => Class::Digit(char as u8 - '0' as u8),
            'h' => Class::Multiplier(3600),
            'm' => Class::Multiplier(60),
            's' => Class::Multiplier(1),
            char => {
                return Err(Error::ParseTimeError(format!(
                    "Invalid character: {}",
                    char
                )))
            }
        };
        match class {
            Class::Digit(digit) => {
                let value = match state {
                    State::Value(value) => value,
                    _ => 0,
                };
                state = State::Value(value * 10 + (digit as u32));
            }
            Class::Multiplier(mul) => {
                let value = match state {
                    State::Value(value) => value,
                    _ => return Err(Error::ParseTimeError(String::from("Unexpected 'm'"))),
                };
                seconds += value * mul;
                state = State::Mul;
            }
        }
    }

    match state {
        State::Mul => Ok(seconds),
        _ => Err(Error::ParseTimeError(String::from(
            "Expected time specifier at the end",
        )))?,
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let exit = exit::Exit::new();

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
        .arg(
            clap::Arg::new("start")
                .long("start")
                .short('s')
                .takes_value(true)
                .about("Start position, like 5m, or 5m5s, or 5s")
                .validator(|arg| match parse_time_as_seconds(arg) {
                    Ok(_) => Ok(()),
                    Err(err) => Err(err.to_string()),
                }),
        )
        .arg(
            clap::Arg::new("debug")
                .long("debug")
                .short('d')
                .about("Write debug information"),
        )
        .get_matches();

    match init_logging(args.is_present("debug")) {
        Ok(()) => (),
        Err(err) => {
            eprintln!("error: {:?}", err);
            return Ok(());
        }
    }

    let kodi_address = {
        match resolve_address(&args).await {
            Ok(x) => x,
            Err(err) => {
                eprintln!("error: {:?}", err);
                return Ok(());
            }
        }
    };

    let start_seconds = args
        .value_of("start")
        .map(|x| parse_time_as_seconds(x).unwrap());

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
            let count: &u32 = file_counts.get(&url_name).unwrap();
            let count = count + 1;
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
    let app_join: tokio::task::JoinHandle<Result<(), kodi_kontrol::error::Error>> = {
        let exit = exit.clone();
        tokio::task::spawn(async move {
            log::debug!("Waiting session");
            let session = session_rx.await.expect("Failed to receive session");
            log::debug!("Got session");
            match session.finish().await {
                Ok(()) => {
                    exit.signal();
                    log::info!("Exiting");
                    Ok(())
                }
                Err(error) => {
                    eprintln!("Setup with error: {}", error);
                    actix_rt::System::current().stop();
                    Ok(())
                }
            }
        })
    };

    let (ui_control_tx, ui_control_rx) = tokio::sync::oneshot::channel::<ui::Control>();
    let (kodi_control, kodi_control_rx) = kodi_control::KodiControl::new();
    let ui_join = tokio::task::spawn_blocking({
        let exit = exit.clone();
        move || {
            let mut ui = util::sync_panic_error(|| Ok(ui::Ui::new(kodi_control, exit)?));

            ui_control_tx
                .send(ui.control())
                .expect("Failed to send to ui_control_tx");

            ui.run();

            ui.finish();
            eprintln!("Exiting..");
        }
    });
    let ui_control = ui_control_rx
        .await
        .expect("Failed to receive from ui_control_rx");

    let kodi_control_args = kodi_control::Args {
        kodi_control_rx,
        start_seconds,
    };

    match server::Session::new(app_data, session_tx, exit.clone(), kodi_control_args).await {
        Ok(()) => (),
        Err(err) => {
            eprintln!("error: {:?}", err);
            return Ok(());
        }
    }
    ui_control.quit();

    ui_join.await.expect("Failed to join ui_join");

    match app_join.await.expect("Failed to join app_join") {
        Ok(()) => Ok(()),
        Err(err) => {
            eprintln!("error: {:?}", err);
            return Ok(());
        }
    }
}
