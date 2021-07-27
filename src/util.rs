use crate::error;
use std::future::Future;

pub async fn handle_errors<F>(function: F) -> ()
where
    F: Future<Output = Result<(), error::Error>> + Send + 'static,
    // F: Fn() -> Result<(), error::Error>,
{
    match function.await {
        Ok(()) => (),
        Err(err) => log::error!("augh, error: {:?}", err),
    }
}

pub async fn get_errors<F>(function: F) -> Result<(), error::Error>
where
    F: Future<Output = Result<(), error::Error>> + Send + 'static,
    // F: Fn() -> Result<(), error::Error>,
{
    function.await
}

pub fn far_future() -> tokio::time::Instant {
    // copied from tokio :D
    tokio::time::Instant::now() + tokio::time::Duration::from_secs(86400 * 365 * 30)
}
