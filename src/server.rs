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
    pub urls_order: HashMap<String, usize>,
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

fn url_for_file(addr: std::net::SocketAddr, file: &str) -> Result<Url, error::Error> {
    use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
    const FRAGMENT: &AsciiSet = &CONTROLS
        .add(b' ')
        .add(b'"')
        .add(b'<')
        .add(b'>')
        .add(b'`')
        .add(b'%')
        .add(b'#');

    let filename_escaped = utf8_percent_encode(file, FRAGMENT).to_string();

    Ok(Url::parse(format!("http://{}/file/", addr).as_str())?.join(&filename_escaped)?)
}

pub fn far_future() -> tokio::time::Instant {
    // copied from tokio :D
    tokio::time::Instant::now() + tokio::time::Duration::from_secs(86400 * 365 * 30)
}

async fn finish(
    jsonrpc_session: &mut kodi_rpc::WsJsonRPCSession,
    player_id: kodi_rpc::PlayerId,
    playlist_id: kodi_rpc::PlaylistId,
    use_playlist: bool,
) -> Result<(), error::Error> {
    kodi_rpc::ws_jsonrpc_player_stop(jsonrpc_session, player_id)
        .await
        .expect("TODO failed to stop playersies");
    if use_playlist {
        kodi_rpc::ws_jsonrpc_playlist_clear(jsonrpc_session, playlist_id)
            .await
            .expect("TODO failed to clear playlist");
    }
    kodi_rpc::ws_jsonrpc_gui_activate_window(
        jsonrpc_session,
        kodi_rpc::GUIWindow::Home,
        vec![String::from("required parameter")],
    )
    .await
    .expect("TODO failed to go Home");
    Ok(())
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

    let files = app_data.lock().unwrap().files.clone();
    let urls_order = app_data.lock().unwrap().urls_order.clone();

    let server = HttpServer::new(move || {
        let app_data = app_data.clone();
        App::new().configure(move |cfg| configure(cfg, app_data))
    })
    .bind((result.local_addr.ip(), 0))?;

    let mut ordered_urls: Vec<(usize, Url)> = files
        .iter()
        .map(|(url, _file)| {
            (
                urls_order.get(url).unwrap().clone(),
                url_for_file(server.addrs()[0], url).expect("Failed to create URL for file"),
            )
        })
        .collect();
    ordered_urls.sort();
    let urls: Vec<Url> = ordered_urls.into_iter().map(|(_k, v)| v).collect();

    let (stop_server_tx, stop_server_rx) = tokio::sync::oneshot::channel();

    let (sigint_tx, mut sigint_rx) = mpsc::channel(1);
    tokio::spawn(handle_ctrl_c(sigint_tx));

    let rpc_handler: tokio::task::JoinHandle<()> = tokio::task::spawn(async move {
        handle_errors(async move {
            let mut stream = kodi_rpc::ws_jsonrpc_subscribe(&mut jsonrpc_session).await?;

            use kodi_rpc::*;

            let playlist_id = 1;
            eprintln!("Playing: {:?}", &urls);
            assert!(urls.len() > 0);
            let use_playlist = urls.len() > 1;
            if !use_playlist {
                let url = &urls[0];
                let item = PlayerOpenParamsItem::PlaylistItem(PlaylistItem::File {
                    file: url.to_string(),
                });
                let player = kodi_rpc::ws_jsonrpc_player_open(&mut jsonrpc_session, item).await?;
                eprintln!("Playing result: {:?}", player);
            } else {
                // let items = kodi_rpc::ws_jsonrpc_playlist_get_items(&mut jsonrpc_session, playlist_id).await?;
                // eprintln!("Existing playlist: {:?}", items);
                kodi_rpc::ws_jsonrpc_playlist_clear(&mut jsonrpc_session, playlist_id).await?;
                let player = kodi_rpc::ws_jsonrpc_playlist_add(
                    &mut jsonrpc_session,
                    playlist_id,
                    urls.iter().map(|url| url.to_string()).collect(),
                )
                .await?;
                eprintln!("Enqueued result: {:?}", player);

                let item = PlayerOpenParamsItem::PlaylistPos {
                    playlist_id,
                    position: 0,
                };
                let player = kodi_rpc::ws_jsonrpc_player_open(&mut jsonrpc_session, item).await?;
                eprintln!("Playing result: {:?}", player);
            }

            kodi_rpc::ws_jsonrpc_gui_activate_window(
                &mut jsonrpc_session,
                GUIWindow::FullscreenVideo,
                vec![String::from("required parameter")],
            )
            .await?;

            let mut player_id = 0u32;

            let mut playlist_position = 0;

            enum State {
                WaitingStart,
                WaitingTimeout,
                WaitingLast,
            }

            #[derive(Debug)]
            enum Event {
                Notification(Notification),
                SigInt,
                Deadline,
            }

            let mut state = State::WaitingStart;

            let mut deadline = None;

            while let Some(notification) = tokio::select! {
                        notification = stream.next() => {
                    match notification {
                    Some(ev) => Some(Event::Notification(ev)),
                    None => None,
                            }
                        }
                        _int = sigint_rx.next() => Some(Event::SigInt),
            _delay = tokio::time::sleep_until(match deadline {
                        None => far_future(),
                Some(deadline) => deadline
            }) => {
                    Some(Event::Deadline)
                }
                    }
            {
                eprintln!("Got notification: {:?}", notification);
                use kodi_rpc::*;

                match notification {
                    Event::Notification(Notification::PlayerOnAVStart(data)) => {
                        eprintln!("Cool, proceed");
                        match state {
                            State::WaitingStart => {
                                player_id = data.data.player.player_id;
                            }
                            _ => (),
                        }

                        let props = kodi_rpc::ws_jsonrpc_player_get_properties(
                            &mut jsonrpc_session,
                            player_id,
                            vec![
                                PlayerPropertyName::CurrentVideoStream,
                                PlayerPropertyName::Position,
                            ],
                        )
                        .await?;
                        eprintln!("Player properties: {:?}", props);
                        playlist_position = props.playlist_position;

                        state = State::WaitingLast;
                    }
                    Event::Notification(Notification::PlayerOnStop(_stop)) => {
                        let end = {
                            let props = kodi_rpc::ws_jsonrpc_player_get_properties(
                                &mut jsonrpc_session,
                                player_id,
                                vec![
                                    PlayerPropertyName::CurrentVideoStream,
                                    PlayerPropertyName::Position,
                                ],
                            )
                            .await?;
                            match &props.current_video_stream {
                                Some(PlayerVideoStream { codec, .. }) if codec.is_empty() => true,
                                None => true,
                                Some(_) => false,
                            }
                        };
                        if end {
                            eprintln!("End of playback, trying to stop..");
                            finish(&mut jsonrpc_session, player_id, playlist_id, use_playlist)
                                .await?;
                            break; // exit the loop
                        } else {
                            // another trick! we expect the new media to start playing in a short while.
                            deadline = Some(
                                tokio::time::Instant::now()
                                    + std::time::Duration::from_millis(5000),
                            );
                            state = State::WaitingTimeout;
                        }
                    }
                    Event::Notification(_) => (), // ignore
                    Event::Deadline => {
                        assert!(match state {
                            State::WaitingTimeout => true,
                            _ => false,
                        });
                        // so it appears we have finished playing; do the finishing steps
                        finish(&mut jsonrpc_session, player_id, playlist_id, use_playlist).await?;
                        break; // exit the loop
                    }
                    Event::SigInt => {
                        eprintln!("Ctrl-c, trying to stop..");
                        finish(&mut jsonrpc_session, player_id, playlist_id, use_playlist).await?;

                        match stop_server_tx.send(()) {
                            Ok(()) => (),
                            Err(_) => {
                                // we're _fine_ if we cannot send to this channel: the select has already terminated at that point
                                eprintln!("rpc_handler failed to send to stop_server_tx");
                            }
                        }
                        break; // exit the loop
                    }
                }
            }

            // let active_players =
            //     kodi_rpc::ws_jsonrpc_get_active_players(&mut jsonrpc_session).await?;
            // eprintln!("active_players: {:?}", active_players);

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

    rpc_handler.await.expect("Failed to join rpc_handler");

    println!("fin");

    Ok(())
}
