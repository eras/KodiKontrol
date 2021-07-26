use crate::error;

use url::Url;

use async_jsonrpc_client::{
    HttpClient, Notification as WsNotification, Output, Params, PubsubTransport, Transport, WsClient, WsSubscription,
};
use serde::Deserialize;

use hyper::http::{Request, StatusCode};
use hyper::{client::conn::Builder, Body};
use tokio::net::TcpStream;

pub struct GetResult {
    pub bytes: actix_web::web::Bytes,
    pub local_addr: std::net::SocketAddr,
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
//         Output::Failure(_) => Err(error::Error::JsonrpcPingError()),
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
        Output::Failure(_) => Err(error::Error::JsonrpcPingError()),
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
        Output::Failure(_) => Err(error::Error::JsonrpcPingError()),
    }
}

pub struct WsJsonRPCSession {
    client: WsClient,
}

pub async fn ws_jsonrpc_connect(url: &Url) -> Result<WsJsonRPCSession, error::Error> {
    let client = WsClient::new(url.as_str()).await?;
    let response = client.request("JSONRPC.Ping", None).await?;
    match response {
        Output::Success(_) => Ok(WsJsonRPCSession { client }),
        Output::Failure(_) => Err(error::Error::JsonrpcPingError()),
    }
}

pub async fn ws_jsonrpc_player_stop(
    session: &mut WsJsonRPCSession,
    player_id: &str,
) -> Result<serde_json::Value, error::Error> {
    let response = session
        .client
        .request(
            "Player.Stop",
            Some(Params::Map(
                vec![(
                    String::from("playerid"),
                    serde_json::Value::String(String::from(player_id)),
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

pub async fn ws_jsonrpc_get_players(
    session: &mut WsJsonRPCSession,
) -> Result<serde_json::Value, error::Error> {
    let response = session.client.request("Player.GetPlayers", None).await?;
    match response {
        Output::Success(response) => Ok(response.result),
        Output::Failure(_) => Err(error::Error::JsonrpcPingError()),
    }
}

#[derive(Debug, Deserialize)]
pub enum PlayerType {
    #[serde(rename = "internal")]
    Internal,

    #[serde(rename = "external")]
    External,

    #[serde(rename = "remote")]
    Remote,
}

#[derive(Debug, Deserialize)]
pub struct PlayerGetActivePlayer {
    #[serde(rename = "type")]
    pub type_: String,

    pub playerid: u32,

    pub playertype: PlayerType,
}

#[derive(Debug, Deserialize)]
pub struct ItemUnknown {}

#[derive(Debug, Deserialize)]
pub struct ItemMovie {
    title: String,

    #[serde(default)]
    year: u32,
}

#[derive(Debug, Deserialize)]
pub struct ItemEpisode {
    #[serde(default)]
    episode: u32,

    #[serde(default)]
    season: u32,

    #[serde(default)]
    showtitle: String,

    title: String,
}

#[derive(Debug, Deserialize)]
pub struct ItemMusicVideo {
    #[serde(default)]
    album: String,

    #[serde(default)]
    artist: String,

    title: String,
}

#[derive(Debug, Deserialize)]
pub struct ItemSong {
    #[serde(default)]
    album: String,

    #[serde(default)]
    artist: String,

    title: String,

    #[serde(default)]
    track: u32,
}

#[derive(Debug, Deserialize)]
pub struct ItemPicture {
    pub file: String
}

#[derive(Debug, Deserialize)]
pub struct ItemChannel {
    pub channeltype: String,
    pub id: u32,
    pub title: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum Item {
    #[serde(rename="unknown")]
    Unknown(ItemUnknown),

    #[serde(rename="movie")]
    Movie(ItemMovie),

    #[serde(rename="episode")]
    Episode(ItemEpisode),

    #[serde(rename="musicVideo")]
    MusicVideo(ItemMusicVideo),

    #[serde(rename="song")]
    Song(ItemSong),

    #[serde(rename="picture")]
    Picture(ItemPicture),

    #[serde(rename="channel")]
    Channel(ItemChannel),
}

#[derive(Debug, Deserialize)]
pub struct Player {
    pub playerid: u32,
    pub speed: f64,
}

// Map({"data": Object({"item": Object({"title": String("file"), "type": String("movie")}), "player": Object({"playerid": Number(0), "speed": Number(1)})}), "sender": String("xbmc")})
#[derive(Debug, Deserialize)]
pub struct PlayerNotificationsData {
    pub item: Item,
    pub player: Player,
}

#[derive(Debug, Deserialize)]
pub struct PlayerStopNotificationsData {
    pub item: Item,
    pub end: bool,
}

#[derive(Debug, Deserialize)]
pub struct NotificationInfo<Content> {
    pub data: Content,
    pub sender: String		// "xbmc"
}

pub type PlayerGetActivePlayersResponse = Vec<PlayerGetActivePlayer>;

#[derive(Debug, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum Notification {
    #[serde(rename = "Player.OnPlay")]
    PlayerOnPlay(NotificationInfo<PlayerNotificationsData>),

    #[serde(rename = "Player.OnAVChange")]
    PlayerOnAVChange(NotificationInfo<PlayerNotificationsData>),

    #[serde(rename = "Player.OnAVStart")]
    PlayerOnAVStart(NotificationInfo<PlayerNotificationsData>),

    #[serde(rename = "Player.OnPause")]
    PlayerOnPause(NotificationInfo<PlayerNotificationsData>),

    #[serde(rename = "Player.OnStop")]
    PlayerOnStop(NotificationInfo<PlayerStopNotificationsData>),

    #[serde(rename = "Player.OnResume")]
    PlayerOnResume(NotificationInfo<PlayerNotificationsData>),
}

pub async fn ws_jsonrpc_get_active_players(
    session: &mut WsJsonRPCSession,
) -> Result<PlayerGetActivePlayersResponse, error::Error> {
    let response = session
        .client
        .request("Player.GetActivePlayers", None)
        .await?;
    match response {
        Output::Success(response) => {
            println!("got result: {:?}", response.result);
            let players = serde_json::from_value(response.result)?;
            Ok(players)
        }
        Output::Failure(_) => Err(error::Error::JsonrpcPingError()),
    }
}

pub struct Subscription {
    ws_subscription: WsSubscription<WsNotification, ()>,
}

impl Subscription {
    pub async fn next(&mut self) -> Option<Notification> {
	loop {
	    match self.ws_subscription.next().await.map(|notification| {
		eprintln!("notification: {:?}", notification);
		match serde_json::from_value(serde_json::to_value(&notification).expect("Failed to serialize notification")) {
		    Ok(x) => Some(x),
		    Err(_) => None
		}
	    }) {
		Some(Some(x)) => return Some(x),
		None => return None,
		Some(None) => () // loop
	    }
	}
    }
}

pub async fn ws_jsonrpc_subscribe(
    session: &mut WsJsonRPCSession,
) -> Result<Subscription, error::Error> {
    let ws_subscription =
	session
        .client
        .subscribe_all()
        .await
        .map_err(|err| error::Error::JsonrpcWsClientError(err))?;
    Ok(Subscription { ws_subscription })
}

pub async fn ws_jsonrpc_player_open_file(
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

pub async fn ws_jsonrpc_introspect(
    session: &mut WsJsonRPCSession,
) -> Result<serde_json::Value, error::Error> {
    let response = session.client.request("JSONRPC.Introspect", None).await?;
    match response {
        Output::Success(response) => Ok(response.result),
        Output::Failure(_) => Err(error::Error::JsonrpcPingError()),
    }
}
