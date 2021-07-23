use kodi_kontrol::{version::get_version, server};

use trust_dns_resolver::AsyncResolver;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();

    let args = clap::App::new("koko")
        .version(get_version().as_str())
        .author("Erkki Seppälä <erkki.seppala@vincit.fi>")
        .about("Remote Kontroller and streamer for Kodi")
        .arg(
            clap::Arg::new("file")
                .long("file")
                .short('f')
                .takes_value(true)
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

    let kodi_address : std::net::IpAddr =
	match args.value_of("kodi") {
	    Some(host) => resolver.lookup_ip(host).await?.iter().next().unwrap(),
	    None => "127.0.0.1".parse().unwrap()
	};
    let file = args.value_of("file");

    match file {
	None => {
	    println!("You need to provide a file to stream");
	    actix_rt::System::current().stop();
	    Ok(())
	}
	Some (file) => {
	    let app_data = server::make_app_data_holder(
		server::AppData {
		    files: vec![(String::from("file"), file.to_string())].into_iter().collect()
		}
	    );
	    match server::doit(kodi_address, app_data).await {
		Ok(()) => {
		    eprintln!("Setup ok");
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
}
