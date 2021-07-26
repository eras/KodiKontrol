use std::future::Future;
use std::sync::Mutex;

use crate::{error, kodi_rpc, version::get_version};

use url::Url;

use std::path::PathBuf;

use actix_files::NamedFile;
use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer, Responder};

use std::collections::HashMap;

use futures::{channel::mpsc, StreamExt};

pub async fn info_page(_req: HttpRequest) -> impl Responder {
    format!("koko v{}", get_version())
}

type AppDataHolder = web::Data<Mutex<AppData>>;

pub async fn static_files(req: HttpRequest) -> HttpResponse {
    let data = req.app_data::<AppDataHolder>().unwrap(); // we assume setup configures app_data
    let addr = req.peer_addr().unwrap(); // documentation says this is not None
    let app_data = data.lock().unwrap();
    // TODO: handle IPv4 inside IPv6
    if addr.ip() == app_data.kodi_address {
        let filename = req.match_info().query("filename");
        match app_data.files.get(filename) {
            Some(path) => {
                let path: PathBuf = path.parse().unwrap();
                eprintln!("Opening file {:?} -> {:?}", filename, path);
                NamedFile::open(path)
                    .expect("failed to open file")
                    .into_response(&req)
            }
            None => {
                eprintln!("Did not find filename {:?}", filename);
                HttpResponse::new(actix_web::http::StatusCode::from_u16(404u16).unwrap())
            }
        }
    } else {
        eprintln!("Request from invalid address: {:?}", addr);
        HttpResponse::new(actix_web::http::StatusCode::from_u16(401u16).unwrap())
    }
}

pub struct AppData {
    pub kodi_address: std::net::IpAddr,
    pub files: HashMap<String, String>,
}

pub fn make_app_data_holder(app_data: AppData) -> AppDataHolder {
    return web::Data::new(Mutex::new(app_data));
}

pub fn configure(cfg: &mut web::ServiceConfig, app_data: AppDataHolder) {
    cfg.app_data(app_data)
        .route("/", web::get().to(info_page))
        .route("/file/{filename}", web::get().to(static_files))
        .route("/file/{filename}", web::head().to(static_files));
}

pub async fn handle_errors<F>(function: F) -> ()
where
    F: Future<Output = Result<(), error::Error>> + Send + 'static,
    // F: Fn() -> Result<(), error::Error>,
{
    match function.await {
        Ok(()) => (),
        Err(err) => eprintln!("augh, error: {:?}", err),
    }
}

async fn handle_ctrl_c(mut exit_signal: mpsc::Sender<()>) {
    if let Ok(_) = tokio::signal::ctrl_c().await {
        eprintln!("Got ctrl-c");
        exit_signal.try_send(()).expect("Failed to send ctrl c");
    }
}

pub async fn doit(app_data: AppDataHolder) -> Result<(), error::Error> {
    let url = Url::parse(
        format!(
            "http://{}:8080/jsonrpc",
            app_data.lock().unwrap().kodi_address
        )
        .as_str(),
    )?;
    let wsurl = Url::parse(
        format!(
            "ws://{}:9090/jsonrpc",
            app_data.lock().unwrap().kodi_address
        )
        .as_str(),
    )?;
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

    let filename = app_data
        .lock()
        .unwrap()
        .files
        .keys()
        .next() // just pick the first one for now
        .unwrap()
        .clone();

    let server = HttpServer::new(move || {
        let app_data = app_data.clone();
        App::new().configure(move |cfg| configure(cfg, app_data))
    })
    .bind((result.local_addr.ip(), 0))?;

    use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
    const FRAGMENT: &AsciiSet = &CONTROLS
        .add(b' ')
        .add(b'"')
        .add(b'<')
        .add(b'>')
        .add(b'`')
        .add(b'%')
        .add(b'#');

    let filename_escaped = utf8_percent_encode(&filename, FRAGMENT).to_string();

    let url = Url::parse(format!("http://{}/file/", server.addrs()[0]).as_str())
        .unwrap()
        .join(&filename_escaped)
        .unwrap();

    let (stop_server_tx, stop_server_rx) = tokio::sync::oneshot::channel();

    let (sigint_tx, mut sigint_rx) = mpsc::channel(1);
    tokio::spawn(handle_ctrl_c(sigint_tx));

    tokio::task::spawn(async move {
        handle_errors(async move {
            let mut stream = kodi_rpc::ws_jsonrpc_subscribe(&mut jsonrpc_session).await?;

            eprintln!("Playing: {}", &url);
            let player =
                kodi_rpc::ws_jsonrpc_player_open_file(&mut jsonrpc_session, url.as_str()).await?;
            eprintln!("Playing result: {:?}", player);

            let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(10000);

	    let mut player_id = 0u32;

            while let Some(notification) =
                match tokio::time::timeout_at(deadline, stream.next()).await {
                    Ok(x) => x,
                    _ => None,
                }
            {
                eprintln!("Got notification: {:?}", notification);
		use kodi_rpc::*;
		match notification {
		    Notification::PlayerOnPlay(data) => {
			eprintln!("Cool, proceed");
			player_id = data.data.player.player_id;
			break;
		    },
		    other => {
			eprintln!("Some other notification? {:?}", other);
		    }
		}
            }

            let active_players =
                kodi_rpc::ws_jsonrpc_get_active_players(&mut jsonrpc_session).await?;
            eprintln!("active_players: {:?}", active_players);

	    loop {
		tokio::select!{
		    notification = stream.next() => {
			use kodi_rpc::*;
			match notification {
			    None | Some(Notification::PlayerOnStop(_)) => {
				eprintln!("End of playback, trying to stop..");
				stop_server_tx.send(()).expect("Failed to send to stop_server channel");
				break;
			    },
			    Some(other) => {
				eprintln!("Some other notification? {:?}", other);
			    }
			}
		    },
		    _int = sigint_rx.next() => {
			_int.expect("Failed to receive sigint");
			eprintln!("Ctrl-c, trying to stop..");
			kodi_rpc::ws_jsonrpc_player_stop(&mut jsonrpc_session, player_id).await.expect("TODO failed to stop playersies");
			stop_server_tx.send(()).expect("Failed to send to stop_server channel");
			break;
		    }
		}
	    }
	    Ok(())
        })
        .await;
    });

    tokio::select! {
        done = server.run() => {
            done.map_err(error::Error::IOError)?
        },
        _ = stop_server_rx => {
        // so what happens to server now?
        //server.system_exit();
        }
    }

    println!("fin");

    Ok(())
}
