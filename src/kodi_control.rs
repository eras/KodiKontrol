use crate::{error, exit, kodi_rpc, util::*};

use url::Url;

use futures::{channel::mpsc, StreamExt};

#[derive(Debug)]
pub struct KodiControl {}

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

pub async fn rpc_handler(
    mut jsonrpc_session: kodi_rpc::WsJsonRPCSession,
    urls: Vec<Url>,
    mut sigint_rx: mpsc::Receiver<()>,
    stop_server_tx: tokio::sync::oneshot::Sender<()>,
    rpc_handler_done_tx: tokio::sync::oneshot::Sender<Result<(), error::Error>>,
    mut exit: exit::Exit,
) {
    let result = get_errors(async move {
        let mut stream = kodi_rpc::ws_jsonrpc_subscribe(&mut jsonrpc_session).await?;

        use kodi_rpc::*;

        let playlist_id = 1;
        log::info!("Playing: {:?}", &urls);
        assert!(urls.len() > 0);
        let use_playlist = urls.len() > 1;
        if !use_playlist {
            let url = &urls[0];
            let item = PlayerOpenParamsItem::PlaylistItem(PlaylistItem::File {
                file: url.to_string(),
            });
            let player = kodi_rpc::ws_jsonrpc_player_open(&mut jsonrpc_session, item).await?;
            log::debug!("Playing result: {:?}", player);
        } else {
            // let items = kodi_rpc::ws_jsonrpc_playlist_get_items(&mut jsonrpc_session, playlist_id).await?;
            // log::info!("Existing playlist: {:?}", items);
            kodi_rpc::ws_jsonrpc_playlist_clear(&mut jsonrpc_session, playlist_id).await?;
            let player = kodi_rpc::ws_jsonrpc_playlist_add(
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
            let player = kodi_rpc::ws_jsonrpc_player_open(&mut jsonrpc_session, item).await?;
            log::debug!("Playing result: {:?}", player);
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
            WaitingTimeout(tokio::time::Instant),
            WaitingLast,
        }

        #[derive(Debug)]
        enum Event {
            Notification(Notification),
            SigInt,
            Deadline,
            Exit,
        }

        let mut state = State::WaitingStart;

        while let Some(notification) = tokio::select! {
                        notification = stream.next() => {
                match notification {
                    Some(ev) => Some(Event::Notification(ev)),
                    None => None,
                            }
                        }
                        _int = sigint_rx.next() => Some(Event::SigInt),
            _delay = tokio::time::sleep_until(match state {
                State::WaitingTimeout(deadline) => deadline,
                _ => far_future(),
            }) => {
                Some(Event::Deadline)
            }
        _exit = exit.wait() => {
            Some(Event::Exit)
        }
                    }
        {
            log::debug!("Got notification: {:?}", notification);
            use kodi_rpc::*;

            match notification {
                Event::Notification(Notification::PlayerOnAVStart(data)) => {
                    log::debug!("Cool, proceed");
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
                    log::debug!("Player properties: {:?}", props);
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
                        log::debug!("End of playback, trying to stop..");
                        finish(&mut jsonrpc_session, player_id, playlist_id, use_playlist).await?;
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
                    finish(&mut jsonrpc_session, player_id, playlist_id, use_playlist).await?;
                    break; // exit the loop
                }
                Event::SigInt | Event::Exit => {
                    log::info!("Ctrl-c or exit, trying to stop..");
                    finish(&mut jsonrpc_session, player_id, playlist_id, use_playlist).await?;

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
            }
        }

        Ok(())
    })
    .await;
    rpc_handler_done_tx
        .send(result)
        .expect("Failed to send rpc_handler_done");
}
