use crate::{error, kodi_rpc_types::*};

use url::Url;

use async_jsonrpc_client::{
    HttpClient, Notification as WsNotification, Output, Params, PubsubTransport, Transport,
    WsClient, WsSubscription,
};

use hyper::http::{Request, StatusCode};
use hyper::{client::conn::Builder, Body};
use tokio::net::TcpStream;

pub struct GetResult {
    pub bytes: actix_web::web::Bytes,
    pub local_addr: std::net::SocketAddr,
}

// Used when a call has no parameters; never actually serialized
#[derive(Debug, serde::Serialize)]
pub struct NoParameters {}

const NO_PARAMS: Option<NoParameters> = None;

// Used for discarding results of a request
#[derive(Debug)]
pub struct Discard {}

impl<'de> serde::Deserialize<'de> for Discard {
    fn deserialize<D>(_deserializer: D) -> Result<Discard, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(Discard {})
    }
}

// used for retrieving the schema _and mostly_ determining the client address for this address
pub async fn jsonrpc_get(url: &Url) -> Result<GetResult, error::Error> {
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
            log::error!("Error in connection: {}", e);
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
            //error::Error::OtherError(Box::new(err))
            error::Error::MsgError(format!("Failed to handle request: {:?}", err))
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

// async fn jsonrpc_ping(url: &Url) -> Result<(), error::Error> {
//     let client = HttpClient::new(url.as_str())?;
//     let response = client.request("JSONRPC.Ping", None).await?;
//     match response {
//         Output::Success(_) => Ok(()),
//         Output::Failure(value) => Err(error::Error::JsonrpcPingError(value)),
//     }
// }

// it seems the JSONRPC port is not accessible from here.. nor is it easy for user to change.
pub async fn http_jsonrpc_get_expert_settings(url: &Url) -> Result<(), error::Error> {
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
        Output::Failure(value) => Err(error::Error::JsonrpcError(value)),
    }
}

pub async fn http_jsonrpc_get_setting(
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
        Output::Failure(value) => Err(error::Error::JsonrpcError(value)),
    }
}

pub struct WsJsonRPCSession {
    client: WsClient,
}

pub async fn connect(url: &Url) -> Result<WsJsonRPCSession, error::Error> {
    let client = WsClient::new(url.as_str()).await?;
    let response = client.request("JSONRPC.Ping", None).await?;
    match response {
        Output::Success(_) => Ok(WsJsonRPCSession { client }),
        Output::Failure(value) => Err(error::Error::JsonrpcError(value)),
    }
}

pub async fn player_stop(
    session: &mut WsJsonRPCSession,
    player_id: PlayerId,
) -> Result<Discard, error::Error> {
    request(session, "Player.Stop", Some(PlayerStopParams { player_id })).await
}

pub async fn player_seek(
    session: &mut WsJsonRPCSession,
    player_id: PlayerId,
    value: Seek,
) -> Result<PlayerSeekReturns, error::Error> {
    request(
        session,
        "Player.Seek",
        Some(PlayerSeekParams { player_id, value }),
    )
    .await
}

pub async fn get_players(
    session: &mut WsJsonRPCSession,
) -> Result<serde_json::Value, error::Error> {
    request(session, "Player.GetPlayers", NO_PARAMS).await
}

pub async fn get_active_players(
    session: &mut WsJsonRPCSession,
) -> Result<PlayerGetActivePlayersResponse, error::Error> {
    request(session, "Player.GetActivePlayers", NO_PARAMS).await
}

pub struct Subscription {
    ws_subscription: WsSubscription<WsNotification, ()>,
}

impl Subscription {
    pub async fn next(&mut self) -> Option<Notification> {
        loop {
            match self.ws_subscription.next().await.map(|notification| {
                log::debug!("notification: {:?}", notification);
                match serde_json::from_value(
                    serde_json::to_value(&notification).expect("Failed to serialize notification"),
                ) {
                    Ok(x) => Some(x),
                    Err(_) => None,
                }
            }) {
                Some(Some(x)) => return Some(x),
                None => return None,
                Some(None) => (), // loop
            }
        }
    }
}

pub async fn subscribe(session: &mut WsJsonRPCSession) -> Result<Subscription, error::Error> {
    let ws_subscription = session
        .client
        .subscribe_all()
        .await
        .map_err(|err| error::Error::JsonrpcWsClientError(err))?;
    Ok(Subscription { ws_subscription })
}

fn value_to_params(value: serde_json::Value) -> Option<Params> {
    match value {
        serde_json::Value::Object(map) => Some(Params::Map(map)),
        serde_json::Value::Array(array) => Some(Params::Array(array)),
        _ => None,
    }
}

async fn request<Request: serde::Serialize, Response: serde::de::DeserializeOwned>(
    session: &mut WsJsonRPCSession,
    name: &str,
    request: Option<Request>,
) -> Result<Response, error::Error>
where
    Response: std::fmt::Debug,
{
    let response = session
        .client
        .request(
            name,
            request.map(|x| {
                let value = serde_json::to_value(x).expect("Cannot serialize request");
                value_to_params(value).expect("Serde_json output doesn't conform params")
            }),
        )
        .await?;
    log::debug!("RPC response: {:?}", response);
    match response {
        Output::Success(value) => match serde_json::from_value(value.clone().result) {
            Ok(result) => {
                log::debug!("Parse OK: {:?}", result);
                Ok(result)
            }
            Err(err) => {
                log::error!("Parse failed: {:?}", err);
                Err(err)?
            }
        },
        Output::Failure(value) => Err(error::Error::JsonrpcError(value)),
    }
}

pub async fn player_open(
    session: &mut WsJsonRPCSession,
    item: PlayerOpenParamsItem,
) -> Result<Discard, error::Error> {
    request(session, "Player.Open", Some(PlayerOpenParams { item })).await
}

pub async fn player_play_pause(
    session: &mut WsJsonRPCSession,
    player_id: PlayerId,
    play: GlobalToggle,
) -> Result<Discard, error::Error> {
    request(
        session,
        "Player.PlayPause",
        Some(PlayerPlayPauseParams { player_id, play }),
    )
    .await
}

pub async fn player_goto(
    session: &mut WsJsonRPCSession,
    player_id: PlayerId,
    to: GoTo,
) -> Result<Discard, error::Error> {
    request(
        session,
        "Player.GoTo",
        Some(PlayerGoToParams { player_id, to }),
    )
    .await
}

pub async fn player_get_properties(
    session: &mut WsJsonRPCSession,
    player_id: PlayerId,
    properties: Vec<PlayerPropertyName>,
) -> Result<PlayerPropertyValue, error::Error> {
    request(
        session,
        "Player.GetProperties",
        Some(PlayerGetPropertiesParams {
            player_id,
            properties,
        }),
    )
    .await
}

pub async fn playlist_add(
    session: &mut WsJsonRPCSession,
    playlist_id: PlaylistId,
    files: Vec<String>,
) -> Result<Discard, error::Error> {
    request(
        session,
        "Playlist.Add",
        Some(PlaylistAddParams {
            playlist_id,
            items: files
                .into_iter()
                .map(|file| PlaylistItem::File { file })
                .collect(),
        }),
    )
    .await
}

pub async fn playlist_clear(
    session: &mut WsJsonRPCSession,
    playlist_id: PlaylistId,
) -> Result<Discard, error::Error> {
    request(
        session,
        "Playlist.Clear",
        Some(PlaylistClearParams { playlist_id }),
    )
    .await
}

pub async fn gui_activate_window(
    session: &mut WsJsonRPCSession,
    window: GUIWindow,
    parameters: Vec<String>,
) -> Result<Discard, error::Error> {
    request(
        session,
        "GUI.ActivateWindow",
        Some(GUIActivateWindowParams { window, parameters }),
    )
    .await
}

pub async fn jsonrpc_introspect(
    session: &mut WsJsonRPCSession,
) -> Result<serde_json::Value, error::Error> {
    let response = session.client.request("JSONRPC.Introspect", None).await?;
    match response {
        Output::Success(response) => Ok(response.result),
        Output::Failure(value) => Err(error::Error::JsonrpcError(value)),
    }
}
