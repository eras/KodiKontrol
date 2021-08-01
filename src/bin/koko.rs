use kodi_kontrol::{config, exit, kodi_control, server, ui, ui_setup, util, version::get_version};

use directories::ProjectDirs;
use std::path::Path;

use std::collections::HashMap;
use std::path::PathBuf;

use trust_dns_resolver::error::ResolveError;
use trust_dns_resolver::AsyncResolver;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Failed to resolve host {}: {}", .0, .1)]
    ResolveNameError(String, String),

    #[error(transparent)]
    ResolveError(#[from] ResolveError),

    #[error(transparent)]
    LoggingSetupError(#[from] LoggingSetupError),

    #[error(transparent)]
    SetupError(#[from] ui_setup::Error),

    #[error("Cannot find file {}", .0.to_string_lossy())]
    FileNotFoundError(PathBuf),

    #[error("No sources provided")]
    NoSourcesError(),

    #[error("Failed to parse time: {}", .0)]
    ParseTimeError(String),

    #[error("Failure to process path: {}", .0)]
    UnsupportedPath(String),

    #[error("Failed to read config: {}", .0)]
    ConfigIOError(#[from] std::io::Error),

    #[error(transparent)]
    ConfigError(#[from] config::Error),

    #[error(transparent)]
    ParseIntError(#[from] std::num::ParseIntError),
}

async fn resolve_address(hostname_arg: Option<String>) -> Result<std::net::IpAddr, Error> {
    let resolver = AsyncResolver::tokio_from_system_conf()?;

    let kodi_address: std::net::IpAddr = match &hostname_arg {
        Some(host) => resolver
            .lookup_ip(host.clone())
            .await
            .map_err(|err| Error::ResolveNameError(host.clone(), err.to_string()))?
            .iter()
            .next()
            .unwrap(),
        None => "127.0.0.1".parse().unwrap(),
    };

    Ok(kodi_address)
}

#[derive(Error, Debug)]
pub enum LoggingSetupError {
    #[error(transparent)]
    InitLoggingError(#[from] log4rs::config::InitError),

    #[error(transparent)]
    LogFileOpenError(#[from] std::io::Error),
}

fn init_logging(enable: bool) -> Result<(), LoggingSetupError> {
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

fn get_config_file(config_file_arg: Option<&str>) -> Result<String, Error> {
    let joined_pathbuf;
    let joined_path;
    // argument overrides all automation
    let config_file: &Path = if let Some(config_file) = config_file_arg {
        Path::new(config_file)
    } else {
        let config_file = Path::new(&config::FILENAME);
        // does the default config filename exist? if so, go with that
        let config_file: &Path = if config_file.exists() {
            config_file
        } else {
            // otherwise, choose the XDG directory if it can be created
            (if let Some(proj_dirs) = ProjectDirs::from("", "Erkki Seppälä", "koko") {
                let config_dir = proj_dirs.config_dir();
                joined_pathbuf = config_dir.join("koko.ini");
                joined_path = joined_pathbuf.as_path();
                Some(&joined_path)
            } else {
                None
            })
            .unwrap_or(&config_file)
        };
        config_file
    };
    let config_file = if let Some(path) = config_file.to_str() {
        path
    } else {
        return Err(Error::UnsupportedPath(
            "Sorry, unsupported config file path (needs to be legal UTF8)".to_string(),
        ));
    };
    Ok(config_file.to_string())
}

async fn player_mode(args: clap::ArgMatches, config: config::Config) -> Result<(), Error> {
    let exit = exit::Exit::new();
    let host = config.get_host(args.value_of("kodi"))?;

    let kodi_address = resolve_address(host.hostname).await?;
    let kodi_port = args.value_of("kodi_port").unwrap().parse::<u16>()?;
    let http_server_port = {
        let server_port = args
            .value_of("server_port")
            .map(|x| x.parse::<u16>())
            .transpose()?;
        server_port
            .or(host.listen_port)
            .or(config.listen_port)
            .unwrap_or(0)
    };

    let ip_access_control = !args.is_present("public");

    let kodi_auth = {
        match (
            // mix'n match
            args.value_of("user")
                .map(|x| String::from(x))
                .or(host.username),
            args.value_of("password")
                .map(|x| String::from(x))
                .or(host.password),
        ) {
            (Some(user), Some(pass)) => Some((user.clone(), pass.clone())),
            _ => None,
        }
    };

    let start_seconds = args
        .value_of("start")
        .map(|x| parse_time_as_seconds(x).unwrap());

    let mut files = HashMap::new();
    let mut urls_order = HashMap::new();
    let mut url_counts = HashMap::new();

    let mut order_index = 0usize;

    if args.occurrences_of("SOURCE") == 0 {
        return Err(Error::NoSourcesError());
    }

    for source in args.values_of_os("SOURCE").unwrap() {
        let path: PathBuf = Path::new(source).to_path_buf();
        if !path.exists() {
            return Err(Error::FileNotFoundError(path));
        }
        let url = path.file_stem().unwrap().to_string_lossy().to_string();

        let mut count = if url_counts.contains_key(&url) {
            let count: &u32 = url_counts.get(&url).unwrap();
            let count = count + 1;
            url_counts.insert(url.clone(), count);
            count
        } else {
            let count = 1;
            url_counts.insert(url.clone(), count);
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
        while files.contains_key(&name(&url, count)) {
            count += 1;
        }
        files.insert(name(&url, count), path);
        urls_order.insert(name(&url, count), order_index);
        order_index += 1;
    }

    let app_data = server::make_app_data_holder(server::AppData {
        files,
        urls_order,
        kodi_address,
        ip_access_control,
        kodi_auth,
        previously_logged_file: None,
    });
    let (session_tx, session_rx) = tokio::sync::oneshot::channel::<server::Session>();
    let app_join: tokio::task::JoinHandle<Result<(), kodi_kontrol::error::Error>> = {
        let exit = exit.clone();
        tokio::task::spawn(async move {
            log::debug!("Waiting session");
            match session_rx.await {
                Err(_err) => {
                    // at this point we're exiting already.. right?
                    Ok(())
                }
                Ok(session) => {
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

    let session_result = server::Session::new(
        app_data,
        kodi_port,
        http_server_port,
        session_tx,
        exit.clone(),
        kodi_control_args,
    )
    .await;
    ui_control.quit();
    ui_join.await.expect("Failed to join ui_join");
    match app_join.await.expect("Failed to join app_join") {
        Ok(()) => (),
        Err(err) => eprintln!("error: {:?}", err),
    }

    match session_result {
        Ok(()) => (),
        Err(err) => eprintln!("error: {:?}", err),
    }
    Ok(())
}

async fn setup_mode(
    _args: clap::ArgMatches,
    config: config::Config,
    config_file: &str,
) -> Result<(), Error> {
    let config_file = String::from(config_file);
    match tokio::task::spawn_blocking(move || {
        let ui_setup = ui_setup::UiSetup::new(config, config_file.as_str());
        Ok(ui_setup.run()?)
    })
    .await
    {
        Ok(res) => res,
        Err(_) => Ok(()), // ignore
    }
}

async fn actual_main() -> Result<(), Error> {
    let args = clap::App::new("koko")
        .version(get_version().as_str())
        .author("Erkki Seppälä <erkki.seppala@vincit.fi>")
        .about("Remote Kontroller and streamer for Kodi")
        .arg(
            clap::Arg::new("SOURCE")
                .index(1)
                .multiple(true)
                .about("File to stream"),
        )
        .arg(
            clap::Arg::new("config")
                .long("config")
                .short('c')
                .takes_value(true)
                .about(
                    format!(
                        "Config file to load, defaults to {}",
                        get_config_file(None)?
                    )
                    .as_str(),
                ),
        )
        .arg(
            clap::Arg::new("setup")
                .long("setup")
                .about("Do setup (interactive config editor with host discovery)"),
        )
        .arg(
            clap::Arg::new("kodi")
                .long("kodi")
                .short('k')
                .takes_value(true)
                .about("Address of the host running Kodi; defaults to localhost"),
        )
        .arg(
            clap::Arg::new("kodi_port")
                .long("port")
                .default_value("8080")
                .takes_value(true)
                .about("Port to use for HTTP connection (9090 will always be used for WebSocket)")
                .validator(|arg| match arg.parse::<u16>() {
                    Ok(_) => Ok(()),
                    Err(err) => Err(err.to_string()),
                }),
        )
        .arg(
            clap::Arg::new("server_port")
                .long("listen")
                .takes_value(true)
                .about("Port to use for serverin HTTP data; default is 0, meaning automatic")
                .validator(|arg| match arg.parse::<u16>() {
                    Ok(_) => Ok(()),
                    Err(err) => Err(err.to_string()),
                }),
        )
        .arg(
            clap::Arg::new("user")
                .long("user")
                .short('u')
                .default_value("kodi")
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
        .arg(
            clap::Arg::new("public")
                .long("public")
                .about("Don't do IP-based access control"),
        )
        .get_matches();

    init_logging(args.is_present("debug"))?;

    let config_file = get_config_file(args.value_of("config"))?;
    let config = config::Config::load(&config_file)?;

    if args.is_present("setup") {
        setup_mode(args, config, &config_file).await
    } else {
        player_mode(args, config).await
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    match actual_main().await {
        Ok(()) => Ok(()),
        Err(err) => {
            eprintln!("error: {}", err);
            Ok(())
        }
    }
}
