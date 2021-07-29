use serde::{Deserialize, Serialize};

pub type PlayerId = u32;
pub type PlaylistId = i32; // 0..2, but default is -1
pub type PlaylistPosition = i32; // positive but default is -1

// "Playlist.Item"
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PlaylistItem {
    File { file: String },
}

#[derive(Debug, Serialize)]
pub struct PlaylistAddParams {
    #[serde(rename = "playlistid")]
    pub playlist_id: PlaylistId,

    #[serde(rename = "item")]
    pub items: Vec<PlaylistItem>,
}

#[derive(Debug, Serialize)]
pub struct PlaylistClearParams {
    #[serde(rename = "playlistid")]
    pub playlist_id: PlaylistId,
}

#[derive(Debug, Serialize)]
pub struct PlaylistGetItemsParams {
    #[serde(rename = "playlistid")]
    pub playlist_id: PlaylistId,
}

#[derive(Debug, Deserialize)]
pub enum ActivePlayerType {
    #[serde(rename = "internal")]
    Internal,

    #[serde(rename = "external")]
    External,

    #[serde(rename = "remote")]
    Remote,
}

#[derive(Debug, Deserialize)]
pub enum PlayerType {
    #[serde(rename = "video")]
    Video,

    #[serde(rename = "audio")]
    Audio,

    #[serde(rename = "picture")]
    Picture,
}

impl Default for PlayerType {
    fn default() -> PlayerType {
        PlayerType::Video
    }
}

#[derive(Debug, Deserialize)]
pub struct PlayerGetActivePlayer {
    #[serde(rename = "type")]
    pub type_: String,

    pub playerid: PlayerId,

    pub playertype: ActivePlayerType,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum NotificationsItem {
    #[serde(rename = "unknown")]
    Unknown {},

    #[serde(rename = "movie")]
    Movie {
        title: String,

        #[serde(default)]
        year: u32,
    },

    #[serde(rename = "episode")]
    Episode {
        #[serde(default)]
        episode: u32,

        #[serde(default)]
        season: u32,

        #[serde(default)]
        showtitle: String,

        title: String,
    },

    #[serde(rename = "musicvideo")]
    MusicVideo {
        #[serde(default)]
        album: String,

        #[serde(default)]
        artist: String,

        title: String,
    },

    #[serde(rename = "song")]
    Song {
        #[serde(default)]
        album: String,

        #[serde(default)]
        artist: String,

        title: String,

        #[serde(default)]
        track: u32,
    },

    #[serde(rename = "picture")]
    Picture { file: String },

    #[serde(rename = "channel")]
    Channel {
        channeltype: String,
        id: u32,
        title: String,
    },
}

#[derive(Debug, Deserialize)]
pub struct Player {
    #[serde(rename = "playerid")]
    pub player_id: PlayerId,
    pub speed: f64,
}

// Map({"data": Object({"item": Object({"title": String("file"), "type": String("movie")}), "player": Object({"playerid": Number(0), "speed": Number(1)})}), "sender": String("xbmc")})
#[derive(Debug, Deserialize)]
pub struct PlayerNotificationsData {
    pub item: NotificationsItem,
    pub player: Player,
}

#[derive(Debug, Deserialize)]
pub struct PlayerStopNotificationsData {
    pub item: NotificationsItem,
    pub end: bool,
}

#[derive(Debug, Deserialize)]
pub struct NotificationInfo<Content> {
    pub data: Content,
    pub sender: String, // "xbmc"
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

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum PlayerOpenParamsItem {
    PlaylistPos {
        #[serde(rename = "playlistid")]
        playlist_id: PlaylistId,
        position: PlaylistPosition,
    },
    PlaylistItem(PlaylistItem),
}

#[derive(Debug, Serialize)]
pub struct PlayerOpenParams {
    pub item: PlayerOpenParamsItem,
}

#[derive(Debug, Serialize)]
pub enum PlayerPropertyName {
    #[serde(rename = "type")]
    Type,
    #[serde(rename = "partymode")]
    PartyMode,
    #[serde(rename = "speed")]
    Speed,
    #[serde(rename = "time")]
    Time,
    #[serde(rename = "percentage")]
    Percentage,
    #[serde(rename = "totaltime")]
    TotalTime,
    #[serde(rename = "playlistid")]
    PlaylistId,
    #[serde(rename = "position")]
    PlaylistPosition,
    #[serde(rename = "repeat")]
    Repeat,
    #[serde(rename = "shuffled")]
    Shuffled,
    #[serde(rename = "canseek")]
    CanSeek,
    #[serde(rename = "canchangespeed")]
    CanChangeSpeed,
    #[serde(rename = "canmove")]
    CanMove,
    #[serde(rename = "canzoom")]
    CanZoom,
    #[serde(rename = "canrotate")]
    CanRotate,
    #[serde(rename = "canshuffle")]
    CanShuffle,
    #[serde(rename = "canrepeat")]
    CanRepeat,
    #[serde(rename = "currentaudiostream")]
    CurrentAudioStream,
    #[serde(rename = "audiostreams")]
    AudioStreams,
    #[serde(rename = "subtitleenabled")]
    SubtitleEnabled,
    #[serde(rename = "currentsubtitle")]
    CurrentSubtitle,
    #[serde(rename = "subtitles")]
    Subtitles,
    #[serde(rename = "live")]
    Live,
    #[serde(rename = "currentvideostream")]
    CurrentVideoStream,
    #[serde(rename = "videostreams")]
    VideoStreams,
}

#[derive(Debug, Serialize)]
pub struct PlayerGetPropertiesParams {
    #[serde(rename = "playerid")]
    pub player_id: PlayerId,
    pub properties: Vec<PlayerPropertyName>,
}

#[derive(Debug, Deserialize)]
pub struct PlayerVideoStream {
    pub codec: String,
    pub height: u32,
    pub width: u32,
    pub index: u32,
    pub language: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct GlobalTime {
    pub hours: u8,
    pub milliseconds: i16, // apparently milliseconsd can be negative..
    pub minutes: u8,
    pub seconds: u8,
}

// Player.Property.Value
#[derive(Debug, Deserialize)]
pub struct PlayerPropertyValue {
    // TODO
    // "audiostreams": {
    //   "items": {
    //     "$ref": "Player.Audio.Stream"
    //   },
    //   "type": "array"
    // },
    // "currentaudiostream": {
    //   "$ref": "Player.Audio.Stream"
    // },
    // "currentsubtitle": {
    //   "$ref": "Player.Subtitle"
    // },
    // "subtitles": {
    //   "items": {
    //     "$ref": "Player.Subtitle"
    //   },
    //   "type": "array"
    // },
    // "repeat": {
    //   "$ref": "Player.Repeat",
    //   "default": "off"
    // },
    #[serde(default, rename = "canchangespeed")]
    pub can_change_speed: bool,
    #[serde(default, rename = "canmove")]
    pub can_move: bool,
    #[serde(default, rename = "canrepeat")]
    pub can_repeat: bool,
    #[serde(default, rename = "canrotate")]
    pub can_rotate: bool,
    #[serde(default, rename = "canseek")]
    pub can_seek: bool,
    #[serde(default, rename = "canshuffle")]
    pub can_shuffle: bool,
    #[serde(default, rename = "canzoom")]
    pub can_zoom: bool,
    #[serde(default, rename = "currentvideostream")]
    pub current_video_stream: Option<PlayerVideoStream>,
    #[serde(default, rename = "live")]
    pub live: bool,
    #[serde(default, rename = "partymode")]
    pub partymode: bool,
    #[serde(default, rename = "percentage")]
    pub percentage: f64,
    #[serde(default = "default_playlist_id", rename = "playlistid")]
    pub playlist_id: PlaylistId,
    #[serde(default = "default_playlist_position", rename = "position")]
    pub playlist_position: PlaylistPosition,
    #[serde(default, rename = "shuffled")]
    pub shuffled: bool,
    #[serde(default, rename = "speed")]
    pub speed: i32,
    #[serde(default, rename = "subtitleenabled")]
    pub subtitleenabled: bool,
    #[serde(default, rename = "time")]
    pub time: Option<GlobalTime>,
    #[serde(default, rename = "totaltime")]
    pub total_time: Option<GlobalTime>,
    #[serde(default, rename = "type")]
    pub type_: PlayerType,
    #[serde(default, rename = "videostreams")]
    pub video_streams: Vec<PlayerVideoStream>,
}

fn default_playlist_id() -> PlaylistId {
    return -1;
}

fn default_playlist_position() -> PlaylistPosition {
    return -1;
}

// Global.Toggle
#[derive(Debug)]
pub enum GlobalToggle {
    False,
    True,
    Toggle,
}

impl serde::Serialize for GlobalToggle {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match *self {
            GlobalToggle::False => serializer.serialize_bool(false),
            GlobalToggle::True => serializer.serialize_bool(true),
            GlobalToggle::Toggle => serializer.serialize_str("toggle"),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PlayerStopParams {
    #[serde(rename = "playerid")]
    pub player_id: PlayerId,
}

#[derive(Debug, Serialize)]
pub struct PlayerPlayPauseParams {
    #[serde(rename = "playerid")]
    pub player_id: PlayerId,
    pub play: GlobalToggle,
}

#[derive(Debug, Serialize)]
pub enum GoTo {
    #[serde(rename = "previous")]
    Previous,
    #[serde(rename = "next")]
    Next,
}

#[derive(Debug, Serialize)]
pub struct PlayerGoToParams {
    #[serde(rename = "playerid")]
    pub player_id: PlayerId,
    pub to: GoTo,
}

// GUI.ActivateWindow
#[derive(Debug, Serialize)]
pub struct GUIActivateWindowParams {
    pub window: GUIWindow,
    pub parameters: Vec<String>, // must have at least one value
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
