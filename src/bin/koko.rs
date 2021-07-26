use kodi_kontrol::{server, version::get_version};

use std::path::PathBuf;

use trust_dns_resolver::AsyncResolver;

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

    let resolver = AsyncResolver::tokio_from_system_conf()?;

    let kodi_address: std::net::IpAddr = match args.value_of("kodi") {
        Some(host) => resolver.lookup_ip(host).await?.iter().next().unwrap(),
        None => "127.0.0.1".parse().unwrap(),
    };
    let source = args.value_of("SOURCE").unwrap();

    let path: PathBuf = source
        .to_string()
        .parse()
        .expect("Failed to parse filename");
    let file_base = path
        .file_stem()
        .unwrap()
        .to_str()
        .expect("TODO: filename is required to be valid UTF8")
        .to_string();

    let app_data = server::make_app_data_holder(server::AppData {
        files: vec![(file_base, source.to_string())].into_iter().collect(),
        kodi_address,
    });
    match server::doit(app_data).await {
        Ok(()) => {
            println!("Exiting");
            Ok(())
        }
        Err(error) => {
            eprintln!("Setup with error: {}", error);
            actix_rt::System::current().stop();
            Ok(())
        }
    }
}
