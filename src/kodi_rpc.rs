use crate::error;

use url::Url;

use async_jsonrpc_client::{
    HttpClient, Output, Params, Transport, WsClient,
};

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
            error::Error::OtherError(Box::new(err))
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

pub async fn ws_jsonrpc_get_players(
    session: &mut WsJsonRPCSession,
) -> Result<serde_json::Value, error::Error> {
    let response = session.client.request("Player.GetPlayers", None).await?;
    match response {
        Output::Success(response) => Ok(response.result),
        Output::Failure(_) => Err(error::Error::JsonrpcPingError()),
    }
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

