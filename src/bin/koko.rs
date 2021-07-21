use kodi_kontrol::{error, kodi_rpc};
use std::io::Write;

use url::Url;

use std::path::PathBuf;

use actix_files::NamedFile;
use actix_web::{web, App, HttpResponse, HttpRequest, HttpServer, Responder};

use hyper::http::{Request, StatusCode};
use hyper::{client::conn::Builder, Body};

use std::collections::HashMap;

use serde::Serialize;

async fn info_page(_req: HttpRequest) -> impl Responder {
    format!("koko v{}", get_version())
}

struct StaticFile {
    filename: String
}

impl Responder for StaticFile {
    fn respond_to(self, _req: &HttpRequest) -> HttpResponse {
        let body = "42";

        // Create response and set content type
        HttpResponse::Ok()
            .content_type("application/json")
            .body(body)
    }
}

// async fn static_files(req: HttpRequest) -> Result<NamedFile, error::Error> {
//     let path: PathBuf = req.match_info().query("filename").parse().unwrap();
//     Ok(NamedFile::open(path)?)
// }

async fn static_files(req: HttpRequest) -> HttpResponse {
    let app_data = req.app_data::<AppData>().unwrap(); // we assume setup configures app_data

    let filename = req.match_info().query("filename");
    let path: PathBuf = app_data.files.get(filename).unwrap().parse().unwrap();
    println!("Opening file {:?}", path);
    NamedFile::open(path).expect("failed to open file").into_response(&req)
    // HttpResponse::Ok()
    //     .content_type("application/json")
    //     .body(body)
}

struct AppData {
    files: HashMap<String, String>,
}

//begin
#[derive(Serialize)]
struct MyObj {
    name: &'static str,
}

// Responder
impl Responder for MyObj {
    fn respond_to(self, _req: &HttpRequest) -> HttpResponse {
        let body = serde_json::to_string(&self).unwrap();

        // Create response and set content type
        HttpResponse::Ok()
            .content_type("application/json")
            .body(body)
    }
}

async fn index() -> impl Responder {
    MyObj { name: "user" }
}

//end

fn get_version() -> String {
    String::from(option_env!("GIT_DESCRIBE").unwrap_or_else(|| env!("VERGEN_SEMVER")))
}

async fn doit(kodi_address: std::net::IpAddr, filename: String) -> Result<(), error::Error> {
    let url = Url::parse(format!("http://{}:8080/jsonrpc", kodi_address).as_str())?;
    let wsurl = Url::parse(format!("ws://{}:9090/jsonrpc", kodi_address).as_str())?;
    let result = kodi_rpc::jsonrpc_get(&url).await?;
    // println!(
    //     "http request done from {}: {:?}",
    //     result.local_addr.ip(),
    //     result.bytes
    // );

    // let _settings = http_jsonrpc_get_expert_settings(&url).await?;
    // let _settings = http_jsonrpc_get_setting(&url, "jsonrpc.tcpport").await?;
    // println!("_settings: {}", _settings);

    let mut jsonrpc_session = kodi_rpc::ws_jsonrpc_connect(&wsurl).await?;

    // let introspect = ws_jsonrpc_introspect(&mut jsonrpc_session).await?;
    // println!("introspect: {}", introspect);
    // let mut file = std::fs::File::create("introspect.json").expect("create failed");
    // file.write_all(introspect.to_string().as_bytes())
    //     .expect("write failed");

    let players = kodi_rpc::ws_jsonrpc_get_players(&mut jsonrpc_session).await?;
    println!("players: {}", players);

    // let mut file = std::fs::File::create("jsonrpc.json").expect("create failed");
    // file.write_all(&result.bytes).expect("write failed");

    let server =
	HttpServer::new(move || {
            App::new()
		.app_data(AppData { files: vec![(String::from("file"), filename.clone())].into_iter().collect()})
		.route("/", web::get().to(info_page))
		.route("/file/{filename}", web::get().to(static_files))
		.route("/file/{filename}", web::head().to(static_files))
	})
	.bind((result.local_addr.ip(), 0))?;
    
    // {
    //     Ok(x) => {
    //         x.run().await.map_err(error::Error::IOError)?;
    // 	    Ok()
    //     }
    //     Err(x) => Err(error::Error::IOError(x)),
    // };

    let url = Url::parse(format!("http://{}/file/file", server.addrs()[0]).as_str()).unwrap();
    tokio::task::spawn(async move {
	println!("Playing: {}", &url);
	let player = kodi_rpc::ws_jsonrpc_player_open_file(
    	    &mut jsonrpc_session,
    	    url.as_str(),
	).await;
	println!("Result: {:?}", player);
    });
    
    server.run().await.map_err(error::Error::IOError)?;

    Ok (())
}

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
	    match doit(kodi_address, file.to_string()).await {
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
