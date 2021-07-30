use std::sync::Mutex;

use crate::{error, exit, kodi_control, kodi_rpc, version::get_version};

use url::Url;

use std::path::PathBuf;

use actix_files::NamedFile;
use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer, Responder};

use std::collections::HashMap;

use futures::channel::mpsc;

use tokio::select;

pub async fn info_page(_req: HttpRequest) -> impl Responder {
    format!("koko v{}", get_version())
}

type AppDataHolder = web::Data<Mutex<AppData>>;

pub async fn static_files(req: HttpRequest) -> HttpResponse {
    let data = req.app_data::<AppDataHolder>().unwrap(); // we assume setup configures app_data
    let addr = req.peer_addr().unwrap(); // documentation says this is not None
    let mut app_data = data.lock().unwrap();
    // TODO: handle IPv4 inside IPv6
    if addr.ip() == app_data.kodi_address || !app_data.ip_access_control {
        let filename = req.match_info().query("filename");
        match app_data.files.get(filename) {
            Some(path) => {
                let path: PathBuf = path.parse().unwrap();

                let same_as_before = match &app_data.previously_logged_file {
                    Some(file) if file == filename => true,
                    Some(_) | None => false,
                };
                if !same_as_before {
                    log::info!("Opening file {:?} -> {:?}", filename, path);
                }
                app_data.previously_logged_file = Some(String::from(filename));
                NamedFile::open(path)
                    .expect("failed to open file")
                    .into_response(&req)
            }
            None => {
                log::error!("Did not find filename {:?}", filename);
                HttpResponse::new(actix_web::http::StatusCode::from_u16(404u16).unwrap())
            }
        }
    } else {
        log::error!("Request from invalid address: {:?}", addr);
        HttpResponse::new(actix_web::http::StatusCode::from_u16(401u16).unwrap())
    }
}

pub struct AppData {
    pub kodi_address: std::net::IpAddr,
    pub ip_access_control: bool,
    pub kodi_auth: Option<(String, String)>,
    pub files: HashMap<String, String>,
    pub urls_order: HashMap<String, usize>,
    pub previously_logged_file: Option<String>,
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

async fn handle_ctrl_c(mut exit_signal: mpsc::Sender<()>) {
    if let Ok(_) = tokio::signal::ctrl_c().await {
        log::info!("Got ctrl-c");
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

#[derive(Debug)]
pub struct Session {
    rpc_handler_done_rx: tokio::sync::oneshot::Receiver<Result<(), error::Error>>,
}

impl Session {
    // will return once the server has finished
    pub async fn new(
        app_data: AppDataHolder,
        result: tokio::sync::oneshot::Sender<Session>,
        exit: exit::Exit,
        kodi_control_args: kodi_control::Args,
    ) -> Result<(), error::Error> {
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
        let auth = app_data.lock().unwrap().kodi_auth.clone();
        let jsonrpc_info = kodi_rpc::jsonrpc_get(&url, &auth).await?;

        let mut jsonrpc_session: kodi_rpc::WsJsonRPCSession = kodi_rpc::connect(&wsurl).await?;

        // let introspect = ws_jsonrpc_introspect(&mut self.jsonrpc_session).await?;
        // log::debug!("introspect: {}", introspect);
        // let mut file = std::fs::File::create("introspect.json").expect("create failed");
        // file.write_all(introspect.to_string().as_bytes())
        //     .expect("write failed");

        let players = kodi_rpc::get_players(&mut jsonrpc_session).await?;
        log::debug!("players: {}", players);

        // let mut file = std::fs::File::create("jsonrpc.json").expect("create failed");
        // file.write_all(&result.bytes).expect("write failed");

        // let server = make_server((result.local_addr.ip(), 0), filename);

        let files = app_data.lock().unwrap().files.clone();
        let urls_order = app_data.lock().unwrap().urls_order.clone();

        let (rpc_handler_done_tx, rpc_handler_done_rx) = tokio::sync::oneshot::channel();
        let (stop_server_tx, stop_server_rx) = tokio::sync::oneshot::channel();
        let (server_info_tx, server_info_rx) = tokio::sync::oneshot::channel();

        tokio::spawn({
            let exit = exit.clone();
            async move {
                let server_info = server_info_rx.await.expect("Failed to receive server_info");
                let mut ordered_urls: Vec<(usize, Url)> = files
                    .iter()
                    .map(|(url, _file)| {
                        (
                            urls_order.get(url).unwrap().clone(),
                            url_for_file(server_info, url).expect("Failed to create URL for file"),
                        )
                    })
                    .collect();
                ordered_urls.sort();
                let urls: Vec<Url> = ordered_urls.into_iter().map(|(_k, v)| v).collect();

                let (sigint_tx, sigint_rx) = mpsc::channel(1);
                tokio::spawn(handle_ctrl_c(sigint_tx));

                tokio::task::spawn(kodi_control::rpc_handler(
                    jsonrpc_session,
                    urls.clone(),
                    sigint_rx,
                    stop_server_tx,
                    rpc_handler_done_tx,
                    exit.clone(),
                    kodi_control_args,
                ));

                let session = Session {
                    rpc_handler_done_rx,
                };

                result
                    .send(session)
                    .expect("Failed to send result to caller");
            }
        });

        Self::run_server(
            app_data,
            jsonrpc_info.local_addr.ip(),
            server_info_tx,
            stop_server_rx,
        )
        .await;

        exit.signal();

        Ok(())
    }

    #[rustfmt::skip::macros(select)]
    async fn run_server(
        app_data: AppDataHolder,
        local_ip: std::net::IpAddr,
        server_info_tx: tokio::sync::oneshot::Sender<std::net::SocketAddr>,
        stop_server_rx: tokio::sync::oneshot::Receiver<()>,
    ) {
        let server = HttpServer::new(move || {
            let app_data = app_data.clone();
            App::new().configure(move |cfg| configure(cfg, app_data))
        })
        .bind((local_ip, 0))
        .expect("failed to construct server");

        server_info_tx
            .send(server.addrs()[0])
            .expect("Failed to send server_info");

        select! {
            done = server.run() => {
                done.map_err(error::Error::IOError).expect("Failed to run server")
            },
            _ = stop_server_rx => {
                // so what happens to server now?
                //server.system_exit();
            }
        }
    }
    pub async fn finish(self: Self) -> Result<(), error::Error> {
        &self
            .rpc_handler_done_rx
            .await
            .expect("Failed to receive from rpc_handler")?;

        log::info!("fin");

        Ok(())
    }
}
