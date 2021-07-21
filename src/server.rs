use crate::{error, kodi_rpc, version::get_version};

use url::Url;

use std::path::PathBuf;

use actix_files::NamedFile;
use actix_web::{web, App, HttpResponse, HttpRequest, HttpServer, Responder};

use std::collections::HashMap;

pub async fn info_page(_req: HttpRequest) -> impl Responder {
    format!("koko v{}", get_version())
}

pub async fn static_files(req: HttpRequest) -> HttpResponse {
    let app_data = req.app_data::<AppData>().unwrap(); // we assume setup configures app_data

    let filename = req.match_info().query("filename");
    let path: PathBuf = app_data.files.get(filename).unwrap().parse().unwrap();
    println!("Opening file {:?}", path);
    NamedFile::open(path).expect("failed to open file").into_response(&req)
}

struct AppData {
    files: HashMap<String, String>,
}

pub fn configure(cfg: &mut web::ServiceConfig, filename: String) {
    cfg.app_data(AppData { files: vec![(String::from("file"), filename.clone())].into_iter().collect()})
	.route("/", web::get().to(info_page))
	.route("/file/{filename}", web::get().to(static_files))
	.route("/file/{filename}", web::head().to(static_files));
}

pub async fn doit(kodi_address: std::net::IpAddr, filename: String) -> Result<(), error::Error> {
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

    // let server = make_server((result.local_addr.ip(), 0), filename);
    
    let server =
	HttpServer::new(move || {
	    let filename = filename.clone();
	    App::new()
		.configure(move |cfg| configure(cfg, filename.clone()))
	})
	.bind((result.local_addr.ip(), 0))?;
    
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
