use std::sync::Mutex;

use kodi_kontrol::error;
use std::io::Write;

use url::Url;

use std::path::PathBuf;

use async_jsonrpc_client::{
    BatchTransport, HttpClient, Output, Params, Response, Transport, WsClient, WsClientError,
};

use actix_files::NamedFile;
use actix_web::{web, App, HttpResponse, HttpRequest, HttpServer, Responder};

use hyper::http::{Request, StatusCode};
use hyper::{client::conn::Builder, Body};
use tokio::net::TcpStream;

use std::collections::HashMap;

use serde::Serialize;

struct GetResult {
    bytes: actix_web::web::Bytes,
    local_addr: std::net::SocketAddr,
}

// used for retrieving the schema _and mostly_ determining the client address for this address
async fn jsonrpc_get(url: &Url) -> Result<GetResult, error::Error> {
    let host = url
        .host()
        .ok_or(error::Error::MsgError(String::from("url is missing host")))?
        .to_owned();
    let port = url.port().unwrap_or(80);

    let target_stream = TcpStream::connect(format!("{}:{}", &host, port)).await?;

    let local_addr = target_stream.local_addr()?;

    let (mut request_sender, connection) = Builder::new()
        .handshake::<TcpStream, Body>(target_stream)
        .await?;

    // spawn a task to poll the connection and drive the HTTP state
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("Error in connection: {}", e);
        }
    });

    let path = {
        let path = String::from(url.path());
        match url.query() {
            None => path,
            Some(query) => path + "?" + query,
        }
    };

    let request = Request::builder()
        .uri(path)
        .header("Host", format!("{}", host))
        .header("Accept", "application/json")
        .method("GET")
        .body(Body::from(""))
        .map_err(|err| {
            // can't deal with this without boxing?!
            error::Error::OtherError(Box::new(err))
        })?;

    let response = request_sender.send_request(request).await?;
    let response_status = response.status();
    let body = response.into_body();
    let bytes = hyper::body::to_bytes(body).await?;
    if response_status != StatusCode::OK {
        Err(error::Error::HttpErrorCode(response_status))
    } else {
        Ok(GetResult { bytes, local_addr })
    }
}

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
    let result = jsonrpc_get(&url).await?;
    // println!(
    //     "http request done from {}: {:?}",
    //     result.local_addr.ip(),
    //     result.bytes
    // );

    // let _settings = http_jsonrpc_get_expert_settings(&url).await?;
    // let _settings = http_jsonrpc_get_setting(&url, "jsonrpc.tcpport").await?;
    // println!("_settings: {}", _settings);

    let mut jsonrpc_session = ws_jsonrpc_connect(&wsurl).await?;

    // let introspect = ws_jsonrpc_introspect(&mut jsonrpc_session).await?;
    // println!("introspect: {}", introspect);
    // let mut file = std::fs::File::create("introspect.json").expect("create failed");
    // file.write_all(introspect.to_string().as_bytes())
    //     .expect("write failed");

    let players = ws_jsonrpc_get_players(&mut jsonrpc_session).await?;
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
	let player = ws_jsonrpc_player_open_file(
    	    &mut jsonrpc_session,
    	    url.as_str(),
	).await;
	println!("Result: {:?}", player);
    });
    
    server.run().await.map_err(error::Error::IOError)?;

    Ok (())
}

// async fn jsonrpc_ping(url: &Url) -> Result<(), error::Error> {
//     let client = HttpClient::new(url.as_str())?;
//     let response = client.request("JSONRPC.Ping", None).await?;
//     match response {
//         Output::Success(_) => Ok(()),
//         Output::Failure(_) => Err(error::Error::JsonrpcPingError()),
//     }
// }

// it seems the JSONRPC port is not accessible from here.. nor is it easy for user to change.
async fn http_jsonrpc_get_expert_settings(url: &Url) -> Result<(), error::Error> {
    let client = HttpClient::new(url.as_str())?;
    let response = client
        .request(
            "Settings.GetSettings",
            Some(Params::Map(
                vec![(
                    String::from("level"),
                    serde_json::Value::String(String::from("expert")),
                )]
                .into_iter()
                .collect(),
            )),
        )
        .await?;
    match response {
        Output::Success(_) => Ok(()),
        Output::Failure(_) => Err(error::Error::JsonrpcPingError()),
    }
}

async fn http_jsonrpc_get_setting(
    url: &Url,
    setting: &str,
) -> Result<serde_json::Value, error::Error> {
    let client = HttpClient::new(url.as_str())?;
    let response = client
        .request(
            "Settings.GetSettingValue",
            Some(Params::Map(
                vec![(
                    String::from("setting"),
                    serde_json::Value::String(String::from(setting)),
                )]
                .into_iter()
                .collect(),
            )),
        )
        .await?;
    match response {
        Output::Success(response) => Ok(response.result),
        Output::Failure(_) => Err(error::Error::JsonrpcPingError()),
    }
}

struct WsJsonRPCSession {
    client: WsClient,
}

async fn ws_jsonrpc_connect(url: &Url) -> Result<WsJsonRPCSession, error::Error> {
    let client = WsClient::new(url.as_str()).await?;
    let response = client.request("JSONRPC.Ping", None).await?;
    match response {
        Output::Success(_) => Ok(WsJsonRPCSession { client }),
        Output::Failure(_) => Err(error::Error::JsonrpcPingError()),
    }
}

async fn ws_jsonrpc_get_players(
    session: &mut WsJsonRPCSession,
) -> Result<serde_json::Value, error::Error> {
    let response = session.client.request("Player.GetPlayers", None).await?;
    match response {
        Output::Success(response) => Ok(response.result),
        Output::Failure(_) => Err(error::Error::JsonrpcPingError()),
    }
}

async fn ws_jsonrpc_player_open_file(
    session: &mut WsJsonRPCSession,
    file: &str,
) -> Result<(), error::Error> {
    let response = session
        .client
        .request(
            "Player.Open",
            Some(Params::Map(
                vec![(
                    String::from("item"),
                    serde_json::Value::Object(
                        vec![(
                            String::from("file"),
                            serde_json::Value::String(String::from(file)),
                        )]
                        .into_iter()
                        .collect(),
                    ),
                )]
                .into_iter()
                .collect(),
            )),
        )
        .await?;
    match response {
        Output::Success(_) => Ok(()),
        Output::Failure(_) => Err(error::Error::JsonrpcPingError()),
    }
}

async fn ws_jsonrpc_introspect(
    session: &mut WsJsonRPCSession,
) -> Result<serde_json::Value, error::Error> {
    let response = session.client.request("JSONRPC.Introspect", None).await?;
    match response {
        Output::Success(response) => Ok(response.result),
        Output::Failure(_) => Err(error::Error::JsonrpcPingError()),
    }
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
