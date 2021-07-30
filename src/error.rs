use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    // #[error(transparent)]
    // ConfigError(#[from] crate::config::Error),

    // #[error(transparent)]
    // ClickMuteError(#[from] crate::click_mute::Error),
    #[error("unsupported path")]
    UnsupportedPath(String), // message

    #[error(transparent)]
    IOError(#[from] std::io::Error),

    #[error(transparent)]
    HyperError(#[from] hyper::Error),

    // #[error(transparent)]
    // HttpError(#[from] hyper::http::HttpError),
    #[error("HTTP error {}", .0)]
    HttpErrorCode(hyper::http::StatusCode),

    #[error(transparent)]
    JsonrpcClientError(#[from] async_jsonrpc_client::HttpClientError),

    #[error(transparent)]
    JsonrpcWsClientError(#[from] async_jsonrpc_client::WsClientError),

    #[error("Failed to communicate over JSONRPC")]
    JsonrpcError(async_jsonrpc_client::Failure),

    #[error(transparent)]
    JsonDecodeError(#[from] serde_json::Error),

    #[error(transparent)]
    KodiControlError(#[from] crate::kodi_control::Error),

    #[error(transparent)]
    UiControlError(#[from] crate::ui::Error),

    #[error(transparent)]
    ConfigError(#[from] crate::config::Error),

    // This would make the Error non-Sendable which is an issue
    // #[error(transparent)]
    // OtherError(#[from] Box<dyn std::error::Error>),
    #[error("Error: {}", .0)]
    MsgError(String),
}
