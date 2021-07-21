use kodi_kontrol::{version::get_version, server};

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
    let kodi_address : std::net::IpAddr = args.value_of("kodi").unwrap_or("127.0.0.1").parse().unwrap();
    let file = args.value_of("file");

    match file {
	None => {
	    println!("You need to provide a file to stream");
	    actix_rt::System::current().stop();
	    Ok(())
	}
	Some (file) => {
	    match server::doit(kodi_address, file.to_string()).await {
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
