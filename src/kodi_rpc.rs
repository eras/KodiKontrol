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

pub async fn ws_jsonrpc_connect(url: &Url) -> Result<WsJsonRPCSession, error::Error> {
    let client = WsClient::new(url.as_str()).await?;
    let response = client.request("JSONRPC.Ping", None).await?;
    match response {
        Output::Success(_) => Ok(WsJsonRPCSession { client }),
        Output::Failure(value) => Err(error::Error::JsonrpcError(value)),
    }
}

type PlayerId = u32;

pub async fn ws_jsonrpc_player_stop(
    session: &mut WsJsonRPCSession,
    player_id: PlayerId,
) -> Result<serde_json::Value, error::Error> {
    let response = session
        .client
        .request(
            "Player.Stop",
            Some(Params::Map(
                vec![(
                    String::from("playerid"),
                    serde_json::Value::Number(serde_json::Number::from(player_id)),
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

pub async fn ws_jsonrpc_get_players(
    session: &mut WsJsonRPCSession,
) -> Result<serde_json::Value, error::Error> {
    let response = session.client.request("Player.GetPlayers", None).await?;
    match response {
        Output::Success(response) => Ok(response.result),
        Output::Failure(value) => Err(error::Error::JsonrpcError(value)),
    }
}

#[derive(Debug, Serialize)]
pub enum GUIWindow {
    #[serde(rename = "accesspoints")]
    Accespoints,
    #[serde(rename = "addon")]
    Addon,
    #[serde(rename = "addonbrowser")]
    AddonBrowser,
    #[serde(rename = "addoninformation")]
    AddonInformation,
    #[serde(rename = "addonsettings")]
    AddonSettings,
    #[serde(rename = "appearancesettings")]
    AppearanceSettings,
    #[serde(rename = "busydialog")]
    BusyDialog,
    #[serde(rename = "busydialognocancel")]
    BusyDialogNoCancel,
    #[serde(rename = "contentsettings")]
    ContentSettings,
    #[serde(rename = "contextmenu")]
    ContextMenu,
    #[serde(rename = "eventlog")]
    EventLog,
    #[serde(rename = "extendedprogressdialog")]
    ExtendedProgressDialog,
    #[serde(rename = "favourites")]
    Favourites,
    #[serde(rename = "filebrowser")]
    Filebrowser,
    #[serde(rename = "filemanager")]
    Filemanager,
    #[serde(rename = "fullscreengame")]
    FullscreenGame,
    #[serde(rename = "fullscreeninfo")]
    FullscreenInfo,
    #[serde(rename = "fullscreenlivetv")]
    FullscreenLiveTv,
    #[serde(rename = "fullscreenlivetvinput")]
    FullscreenLiveTvInput,
    #[serde(rename = "fullscreenlivetvpreview")]
    FullscreenLiveTvPreview,
    #[serde(rename = "fullscreenradio")]
    FullscreenRadio,
    #[serde(rename = "fullscreenradioinput")]
    FullscreenRadioInput,
    #[serde(rename = "fullscreenradiopreview")]
    FullscreenRadioPreview,
    #[serde(rename = "fullscreenvideo")]
    FullscreenVideo,
    #[serde(rename = "gameadvancedsettings")]
    GameAdvancedSettings,
    #[serde(rename = "gamecontrollers")]
    GameControllers,
    #[serde(rename = "gameosd")]
    GameOsd,
    #[serde(rename = "gamepadinput")]
    GamePadInput,
    #[serde(rename = "games")]
    Games,
    #[serde(rename = "gamesettings")]
    GameSettings,
    #[serde(rename = "gamestretchmode")]
    GameStretchMode,
    #[serde(rename = "gamevideofilter")]
    GameVideoFilter,
    #[serde(rename = "gamevideorotation")]
    GameVideoRotation,
    #[serde(rename = "gamevolume")]
    GameVolume,
    #[serde(rename = "home")]
    Home,
    #[serde(rename = "infoprovidersettings")]
    InfoproviderSettings,
    #[serde(rename = "interfacesettings")]
    InterfaceSettings,
    #[serde(rename = "libexportsettings")]
    LibexportSettings,
    #[serde(rename = "locksettings")]
    LockSettings,
    #[serde(rename = "loginscreen")]
    LoginScreen,
    #[serde(rename = "mediafilter")]
    MediaFilter,
    #[serde(rename = "mediasettings")]
    MediaSettings,
    #[serde(rename = "mediasource")]
    MediaSource,
    #[serde(rename = "movieinformation")]
    MovieInformation,
    #[serde(rename = "music")]
    Music,
    #[serde(rename = "musicinformation")]
    MusicInformation,
    #[serde(rename = "musicosd")]
    MusicOsd,
    #[serde(rename = "musicplaylist")]
    MusicPlaylist,
    #[serde(rename = "musicplaylisteditor")]
    MusicPlaylistEditor,
    #[serde(rename = "networksetup")]
    NetworkSetup,
    #[serde(rename = "notification")]
    Notification,
    #[serde(rename = "numericinput")]
    NumericInput,
    #[serde(rename = "okdialog")]
    OkDialog,
    #[serde(rename = "osdaudiosettings")]
    OsdAudioSettings,
    #[serde(rename = "osdcmssettings")]
    OsdCmsSettings,
    #[serde(rename = "osdsubtitlesettings")]
    OsdSubtitleSettings,
    #[serde(rename = "osdvideosettings")]
    OsdVideoSettings,
    #[serde(rename = "peripheralsettings")]
    PeripheralSettings,
    #[serde(rename = "pictureinfo")]
    PictureInfo,
    #[serde(rename = "pictures")]
    Pictures,
    #[serde(rename = "playercontrols")]
    PlayerControls,
    #[serde(rename = "playerprocessinfo")]
    PlayerProcessInfo,
    #[serde(rename = "playersettings")]
    PlayerSettings,
    #[serde(rename = "profiles")]
    Profiles,
    #[serde(rename = "profilesettings")]
    ProfileSettings,
    #[serde(rename = "programs")]
    Programs,
    #[serde(rename = "progressdialog")]
    ProgressDialog,
    #[serde(rename = "pvrchannelguide")]
    PvrChannelGuide,
    #[serde(rename = "pvrchannelmanager")]
    PvrChannelManager,
    #[serde(rename = "pvrchannelscan")]
    PvrChannelScan,
    #[serde(rename = "pvrgroupmanager")]
    PvrGroupManager,
    #[serde(rename = "pvrguideinfo")]
    PvrGuideInfo,
    #[serde(rename = "pvrguidesearch")]
    PvrGuideSearch,
    #[serde(rename = "pvrosdchannels")]
    PvrOsdChannels,
    #[serde(rename = "pvrosdguide")]
    PvrOsdGuide,
    #[serde(rename = "pvrosdteletext")]
    PvrOsdTeletext,
    #[serde(rename = "pvrradiordsinfo")]
    PvrRadiordsInfo,
    #[serde(rename = "pvrrecordinginfo")]
    PvrRecordingInfo,
    #[serde(rename = "pvrsettings")]
    PvrSettings,
    #[serde(rename = "pvrtimersetting")]
    PvrTimerSetting,
    #[serde(rename = "pvrupdateprogress")]
    PvrUpdateProgress,
    #[serde(rename = "radiochannels")]
    RadioChannels,
    #[serde(rename = "radioguide")]
    RadioGuide,
    #[serde(rename = "radiorecordings")]
    RadioRecordings,
    #[serde(rename = "radiosearch")]
    RadioSearch,
    #[serde(rename = "radiotimerrules")]
    RadioTimerRules,
    #[serde(rename = "radiotimers")]
    RadioTimers,
    #[serde(rename = "screencalibration")]
    ScreenCalibration,
    #[serde(rename = "screensaver")]
    ScreenSaver,
    #[serde(rename = "seekbar")]
    Seekbar,
    #[serde(rename = "selectdialog")]
    SelectDialog,
    #[serde(rename = "servicesettings")]
    ServiceSettings,
    #[serde(rename = "settings")]
    Settings,
    #[serde(rename = "shutdownmenu")]
    ShutdownMenu,
    #[serde(rename = "skinsettings")]
    SkinSettings,
    #[serde(rename = "sliderdialog")]
    SliderDialog,
    #[serde(rename = "slideshow")]
    SlideShow,
    #[serde(rename = "smartplaylisteditor")]
    SmartPlaylistEditor,
    #[serde(rename = "smartplaylistrule")]
    SmartPlaylistRule,
    #[serde(rename = "songinformation")]
    SongInformation,
    #[serde(rename = "splash")]
    Splash,
    #[serde(rename = "startup")]
    Startup,
    #[serde(rename = "startwindow")]
    StartWindow,
    #[serde(rename = "submenu")]
    SubMenu,
    #[serde(rename = "subtitlesearch")]
    SubtitleSearch,
    #[serde(rename = "systeminfo")]
    SystemInfo,
    #[serde(rename = "systemsettings")]
    SystemSettings,
    #[serde(rename = "teletext")]
    Teletext,
    #[serde(rename = "textviewer")]
    TextViewer,
    #[serde(rename = "tvchannels")]
    TvChannels,
    #[serde(rename = "tvguide")]
    TvGuide,
    #[serde(rename = "tvrecordings")]
    TvRecordings,
    #[serde(rename = "tvsearch")]
    TvSearch,
    #[serde(rename = "tvtimerrules")]
    TvTimerRules,
    #[serde(rename = "tvtimers")]
    TvTimers,
    #[serde(rename = "videobookmarks")]
    VideoBookmarks,
    #[serde(rename = "videomenu")]
    VideoMenu,
    #[serde(rename = "videoosd")]
    VideoOsd,
    #[serde(rename = "videoplaylist")]
    VideoPlaylist,
    #[serde(rename = "videos")]
    Videos,
    #[serde(rename = "videotimeseek")]
    VideoTimeSeek,
    #[serde(rename = "virtualkeyboard")]
    VirtualKeyboard,
    #[serde(rename = "visualisation")]
    Visualisation,
    #[serde(rename = "visualisationpresetlist")]
    VisualisationPresetList,
    #[serde(rename = "volumebar")]
    Volumebar,
    #[serde(rename = "weather")]
    Weather,
    #[serde(rename = "yesnodialog")]
    YesNoDialog,
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

    pub playerid: PlayerId,

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
    #[serde(rename="playerid")]
    pub player_id: PlayerId,
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
        Output::Failure(value) => Err(error::Error::JsonrpcError(value)),
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
        Output::Failure(value) => Err(error::Error::JsonrpcError(value)),
    }
}

pub async fn ws_jsonrpc_introspect(
    session: &mut WsJsonRPCSession,
) -> Result<serde_json::Value, error::Error> {
    let response = session.client.request("JSONRPC.Introspect", None).await?;
    match response {
        Output::Success(response) => Ok(response.result),
        Output::Failure(value) => Err(error::Error::JsonrpcError(value)),
    }
}
