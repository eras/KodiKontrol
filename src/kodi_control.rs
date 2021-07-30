use async_trait::async_trait;

use crate::{error, exit, kodi_rpc, kodi_rpc_types, util::*};

use url::Url;

use thiserror::Error;

use tokio::select;

use futures::{channel::mpsc, StreamExt};

pub struct ControlContext {
    jsonrpc_session: kodi_rpc::WsJsonRPCSession,
    player_id: kodi_rpc_types::PlayerId,
    kodi_info_callback: Option<Box<dyn KodiInfoCallback>>,
}

#[async_trait]
trait ControlRequest<R>: std::fmt::Debug {
    async fn request(&mut self, context: ControlContext) -> (ControlContext, R);
}

#[async_trait]
pub trait ControlRequestWrapper: Send + std::fmt::Debug {
    async fn request_wrapper(&mut self, context: ControlContext) -> ControlContext;
}

pub type KodiControlReceiver = mpsc::Receiver<Box<dyn ControlRequestWrapper + Send>>;

pub struct KodiControl {
    channel: mpsc::Sender<Box<dyn ControlRequestWrapper + Send>>,
}

impl std::fmt::Debug for KodiControl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KodiControl")
            // .field("x", &self.x)
            // .field("y", &self.y)
            .finish()
    }
}

#[derive(Debug)]
struct KodiControlCallbackSync<R> {
    control_request: Box<dyn ControlRequest<R> + Send>,
    result_tx: crossbeam_channel::Sender<R>,
}

#[derive(Debug)]
struct KodiControlCallbackAsync {
    control_request: Box<dyn ControlRequest<()> + Send>,
}

#[async_trait]
impl<R> ControlRequestWrapper for KodiControlCallbackSync<R>
where
    R: 'static + Send + std::fmt::Debug,
{
    async fn request_wrapper(&mut self, context: ControlContext) -> ControlContext {
        let (context, retval) = self.control_request.request(context).await;
        self.result_tx.send(retval).unwrap();
        context
    }
}

#[async_trait]
impl ControlRequestWrapper for KodiControlCallbackAsync {
    async fn request_wrapper(&mut self, context: ControlContext) -> ControlContext {
        let (context, ()) = self.control_request.request(context).await;
        context
    }
}

#[derive(Debug)]
struct PropertiesRequest {
    properties: Vec<kodi_rpc_types::PlayerPropertyName>,
}

#[async_trait]
impl ControlRequest<Option<kodi_rpc_types::PlayerPropertyValue>> for PropertiesRequest {
    async fn request(
        &mut self,
        mut context: ControlContext,
    ) -> (ControlContext, Option<kodi_rpc_types::PlayerPropertyValue>) {
        // well, this seems to fail sometimes, so just return None in those cases
        let value = kodi_rpc::player_get_properties(
            &mut context.jsonrpc_session,
            context.player_id,
            self.properties.clone(),
        )
        .await;
        let value = match value {
            Ok(value) => Some(value),
            Err(err) => {
                log::error!("Failed to receive properties: {}", err);
                None
            }
        };
        (context, value)
    }
}

#[derive(Debug)]
struct PlayPauseRequest {}

#[async_trait]
impl ControlRequest<()> for PlayPauseRequest {
    async fn request(&mut self, mut context: ControlContext) -> (ControlContext, ()) {
        kodi_rpc::player_play_pause(
            &mut context.jsonrpc_session,
            context.player_id.clone(),
            kodi_rpc_types::GlobalToggle::Toggle,
        )
        .await
        .expect("TODO failed to play/pause player");
        (context, ())
    }
}

#[derive(Debug)]
struct SeekRequest {
    seek: kodi_rpc_types::Seek,
}

#[async_trait]
impl ControlRequest<kodi_rpc_types::PlayerSeekReturns> for SeekRequest {
    async fn request(
        &mut self,
        mut context: ControlContext,
    ) -> (ControlContext, kodi_rpc_types::PlayerSeekReturns) {
        let value = kodi_rpc::player_seek(
            &mut context.jsonrpc_session,
            context.player_id.clone(),
            self.seek.clone(),
        )
        .await
        .expect("Failed to seek");
        (context, value)
    }
}

#[derive(Debug)]
struct NextRequest {}

#[async_trait]
impl ControlRequest<()> for NextRequest {
    async fn request(&mut self, mut context: ControlContext) -> (ControlContext, ()) {
        kodi_rpc::player_goto(
            &mut context.jsonrpc_session,
            context.player_id.clone(),
            kodi_rpc_types::GoTo::Next,
        )
        .await
        .expect("TODO failed to go to next track");
        (context, ())
    }
}

#[derive(Debug)]
struct PrevRequest {}

#[async_trait]
impl ControlRequest<()> for PrevRequest {
    async fn request(&mut self, mut context: ControlContext) -> (ControlContext, ()) {
        kodi_rpc::player_goto(
            &mut context.jsonrpc_session,
            context.player_id.clone(),
            kodi_rpc_types::GoTo::Previous,
        )
        .await
        .expect("TODO failed to go to next track");
        (context, ())
    }
}

pub trait KodiInfoCallback: Send + std::fmt::Debug {
    fn playlist_position(&mut self, position: Option<kodi_rpc_types::PlaylistPosition>);
}

#[derive(Debug)]
struct DefaultKodiInfoCallback {}

impl KodiInfoCallback for DefaultKodiInfoCallback {
    fn playlist_position(&mut self, _position: Option<kodi_rpc_types::PlaylistPosition>) {}
}

#[derive(Debug)]
struct SetCallbackRequest {
    kodi_info_callback: Option<Box<dyn KodiInfoCallback>>,
}

#[async_trait]
impl ControlRequest<()> for SetCallbackRequest {
    async fn request(&mut self, mut context: ControlContext) -> (ControlContext, ()) {
        context.kodi_info_callback = self.kodi_info_callback.take();
        (context, ())
    }
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("TrySendError in KodiControl: {}", .0)]
    TrySendError(String),
}

impl KodiControl {
    pub fn backwards(&mut self, _delta: std::time::Duration) {}
    pub fn forward(&mut self, _delta: std::time::Duration) {}
    pub fn playlist_next(&mut self) -> Result<(), Error> {
        self.sync_request(Box::new(NextRequest {}))
    }
    pub fn playlist_prev(&mut self) -> Result<(), Error> {
        self.sync_request(Box::new(PrevRequest {}))
    }
    pub fn play_pause(&mut self) -> Result<(), Error> {
        self.sync_request(Box::new(PlayPauseRequest {}))
    }
    pub fn set_callback(
        &mut self,
        kodi_info_callback: Box<dyn KodiInfoCallback>,
    ) -> Result<(), Error> {
        self.async_request(Box::new(SetCallbackRequest {
            kodi_info_callback: Some(kodi_info_callback),
        }))
    }
    pub fn properties(
        &mut self,
        properties: Vec<kodi_rpc_types::PlayerPropertyName>,
    ) -> Result<Option<kodi_rpc_types::PlayerPropertyValue>, Error> {
        self.sync_request(Box::new(PropertiesRequest { properties }))
    }
    pub fn seek(
        &mut self,
        seek: kodi_rpc_types::Seek,
    ) -> Result<kodi_rpc_types::PlayerSeekReturns, Error> {
        self.sync_request(Box::new(SeekRequest { seek }))
    }

    fn sync_request<R: 'static + Send + std::fmt::Debug>(
        &mut self,
        control_request: Box<dyn ControlRequest<R> + Send>,
    ) -> Result<R, Error> {
        let (result_tx, result_rx) = crossbeam_channel::bounded(1);
        let request_wrapper = Box::new(KodiControlCallbackSync {
            control_request,
            result_tx,
        });
        match self.channel.try_send(request_wrapper) {
            Ok(()) => Ok(result_rx.recv().unwrap()),
            Err(err) => Err(Error::TrySendError(format!("error: {}", err))),
        }
    }

    fn async_request(
        &mut self,
        control_request: Box<dyn ControlRequest<()> + Send>,
    ) -> Result<(), Error> {
        let request_wrapper = Box::new(KodiControlCallbackAsync { control_request });
        match self.channel.try_send(request_wrapper) {
            Ok(()) => Ok(()),
            Err(err) => Err(Error::TrySendError(format!("error: {}", err))),
        }
    }

    pub fn new() -> (
        KodiControl,
        mpsc::Receiver<Box<dyn ControlRequestWrapper + Send>>,
    ) {
        let (tx, rx) = mpsc::channel(64);
        let kodi_control = KodiControl { channel: tx };
        (kodi_control, rx)
    }
}

async fn finish(
    jsonrpc_session: &mut kodi_rpc::WsJsonRPCSession,
    player_id: kodi_rpc_types::PlayerId,
    playlist_id: kodi_rpc_types::PlaylistId,
    use_playlist: bool,
) -> Result<(), error::Error> {
    kodi_rpc::player_stop(jsonrpc_session, player_id)
        .await
        .expect("TODO failed to stop playersies");
    if use_playlist {
        kodi_rpc::playlist_clear(jsonrpc_session, playlist_id)
            .await
            .expect("TODO failed to clear playlist");
    }
    kodi_rpc::gui_activate_window(
        jsonrpc_session,
        kodi_rpc_types::GUIWindow::Home,
        vec![String::from("required parameter")],
    )
    .await
    .expect("TODO failed to go Home");
    Ok(())
}

pub struct Args {
    pub kodi_control_rx: KodiControlReceiver,
    pub start_seconds: Option<u32>,
}

#[rustfmt::skip::macros(select)]
pub async fn rpc_handler(
    mut jsonrpc_session: kodi_rpc::WsJsonRPCSession,
    urls: Vec<Url>,
    mut sigint_rx: mpsc::Receiver<()>,
    stop_server_tx: tokio::sync::oneshot::Sender<()>,
    rpc_handler_done_tx: tokio::sync::oneshot::Sender<Result<(), error::Error>>,
    mut exit: exit::Exit,
    mut args: Args,
) {
    let mut kodi_info_callback: Box<dyn KodiInfoCallback> = Box::new(DefaultKodiInfoCallback {});
    let mut first_play = true;
    let result = get_errors(async move {
        let mut stream = kodi_rpc::subscribe(&mut jsonrpc_session).await?;

        use kodi_rpc_types::*;

        let playlist_id = 1;
        log::info!("Playing: {:?}", &urls);
        assert!(urls.len() > 0);
        let use_playlist = urls.len() > 1;
        if !use_playlist {
            let url = &urls[0];
            let item = PlayerOpenParamsItem::PlaylistItem(PlaylistItem::File {
                file: url.to_string(),
            });
            let player = kodi_rpc::player_open(&mut jsonrpc_session, item).await?;
            log::debug!("Playing result: {:?}", player);
        } else {
            // let items = kodi_rpc::ws_jsonrpc_playlist_get_items(&mut jsonrpc_session, playlist_id).await?;
            // log::info!("Existing playlist: {:?}", items);
            kodi_rpc::playlist_clear(&mut jsonrpc_session, playlist_id).await?;
            let player = kodi_rpc::playlist_add(
                &mut jsonrpc_session,
                playlist_id,
                urls.iter().map(|url| url.to_string()).collect(),
            )
            .await?;
            log::debug!("Enqueued result: {:?}", player);

            let item = PlayerOpenParamsItem::PlaylistPos {
                playlist_id,
                position: 0,
            };
            let player = kodi_rpc::player_open(&mut jsonrpc_session, item).await?;
            log::debug!("Playing result: {:?}", player);
        }

        kodi_rpc::gui_activate_window(
            &mut jsonrpc_session,
            GUIWindow::FullscreenVideo,
            vec![String::from("required parameter")],
        )
        .await?;

        let mut player_id = 0u32;

        let mut playlist_position = None;
        kodi_info_callback.playlist_position(playlist_position);

        enum State {
            WaitingStart,
            WaitingTimeout(tokio::time::Instant),
            WaitingLast,
        }

        #[derive(Debug)]
        enum Event {
            Notification(Notification),
            SigInt,
            Deadline,
            Exit,
            Control(Box<dyn ControlRequestWrapper + Send>),
        }

        let mut state = State::WaitingStart;

        while let Some(notification) = select! {
            notification = stream.next() => {
                match notification {
                    Some(ev) => Some(Event::Notification(ev)),
                    None => None,
                }
            }
            _int = sigint_rx.next() => Some(Event::SigInt),
            _delay = tokio::time::sleep_until(
		match state {
                    State::WaitingTimeout(deadline) => deadline,
                    _ => far_future(),
		}) => {
                Some(Event::Deadline)
            }
            _exit = exit.wait() => {
		Some(Event::Exit)
            }
            control_request = args.kodi_control_rx.next() => {
		control_request.map(|x| Event::Control(x))
            }
        } {
            log::debug!("Got notification: {:?}", notification);

            match notification {
                Event::Notification(Notification::PlayerOnAVStart(data)) => {
                    log::debug!("Cool, proceed");
                    match state {
                        State::WaitingStart => {
                            player_id = data.data.player.player_id;
                        }
                        _ => (),
                    }

                    let props = kodi_rpc::player_get_properties(
                        &mut jsonrpc_session,
                        player_id,
                        vec![
                            PlayerPropertyName::CurrentVideoStream,
                            PlayerPropertyName::PlaylistPosition,
                        ],
                    )
                    .await?;
                    log::debug!("Player properties: {:?}", props);
                    if use_playlist {
                        playlist_position = Some(props.playlist_position);
                    }
                    kodi_info_callback.playlist_position(playlist_position);

                    if first_play {
                        first_play = false;
                        match &args.start_seconds {
                            None => (),
                            Some(start_seconds) => {
                                use std::convert::TryFrom;
                                kodi_rpc::player_seek(
                                    &mut jsonrpc_session,
                                    player_id,
                                    Seek::RelativeSeconds {
                                        seconds: i32::try_from(start_seconds.clone()).map_err(
                                            |_| {
                                                error::Error::MsgError(format!(
                                                    "Cannot convert {} to signed 32-bit integer",
                                                    start_seconds
                                                ))
                                            },
                                        )?,
                                    },
                                )
                                .await?;
                            }
                        }
                    }

                    state = State::WaitingLast;
                }
                Event::Notification(Notification::PlayerOnStop(_stop)) => {
                    let end = {
                        let props = kodi_rpc::player_get_properties(
                            &mut jsonrpc_session,
                            player_id,
                            vec![
                                PlayerPropertyName::CurrentVideoStream,
                                PlayerPropertyName::PlaylistPosition,
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
                        log::debug!("End of playback, trying to stop..");
                        break; // exit the loop
                    } else {
                        // another trick! we expect the new media to start playing in a short while.
                        let deadline =
                            tokio::time::Instant::now() + std::time::Duration::from_millis(5000);
                        state = State::WaitingTimeout(deadline);
                    }
                }
                Event::Notification(_) => (), // ignore
                Event::Deadline => {
                    // so it appears we have finished playing; do the finishing steps
                    break; // exit the loop
                }
                Event::SigInt | Event::Exit => {
                    log::info!("Ctrl-c or exit, trying to stop..");

                    exit.signal();
                    match stop_server_tx.send(()) {
                        Ok(()) => (),
                        Err(_) => {
                            // we're _fine_ if we cannot send to this channel: the select has already terminated at that point
                            log::error!("rpc_handler failed to send to stop_server_tx");
                        }
                    }
                    break; // exit the loop
                }
                Event::Control(mut control_request) => {
                    let context = ControlContext {
                        jsonrpc_session,
                        player_id,
                        kodi_info_callback: Some(kodi_info_callback),
                    };
                    let context = control_request.request_wrapper(context).await;
                    jsonrpc_session = context.jsonrpc_session;
                    kodi_info_callback = context.kodi_info_callback.unwrap();
                }
            }
        }
        finish(&mut jsonrpc_session, player_id, playlist_id, use_playlist).await?;

        Ok(())
    })
    .await;
    rpc_handler_done_tx
        .send(result)
        .expect("Failed to send rpc_handler_done");
}
