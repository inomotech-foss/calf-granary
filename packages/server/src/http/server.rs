//! Accepting HTTP connections.

use core::error::Error;
use core::pin::pin;
use core::time::Duration;
use std::sync::Arc;

use axum::Router;
use hyper::server::conn::http1::Builder;
use hyper_util::rt::{TokioIo, TokioTimer};
use hyper_util::service::TowerToHyperService;
use tokio::net::TcpListener;
use tokio::sync::{Semaphore, watch};
use tokio::time;

/// How long a client may take to send the request headers.
const HEADER_TIMEOUT: Duration = Duration::from_secs(30);

/// The most connections served at the same time. Further clients wait.
const MAX_CONNECTIONS: u32 = 4096;

/// How long accepting waits after an error, such as too many open files.
const ACCEPT_BACKOFF: Duration = Duration::from_secs(1);

/// Serves the router until `shutdown` resolves, then waits for open
/// requests to finish.
pub(crate) async fn serve(
    listener: TcpListener,
    router: Router,
    shutdown: impl Future<Output = ()>,
) {
    let connections = Arc::new(Semaphore::new(MAX_CONNECTIONS as usize));
    let (stop, stopped) = watch::channel(());
    let mut builder = Builder::new();
    builder
        .timer(TokioTimer::new())
        .header_read_timeout(HEADER_TIMEOUT);
    let mut shutdown = pin!(shutdown);
    loop {
        let accepted = tokio::select! {
            () = &mut shutdown => break,
            permit = Arc::clone(&connections).acquire_owned() => {
                let Ok(permit) = permit else {
                    break;
                };
                tokio::select! {
                    () = &mut shutdown => break,
                    accepted = listener.accept() => accepted.map(|(stream, _)| (stream, permit)),
                }
            }
        };
        let (stream, permit) = match accepted {
            Ok(accepted) => accepted,
            Err(err) => {
                tracing::warn!(error = &err as &dyn Error, "failed to accept a connection");
                time::sleep(ACCEPT_BACKOFF).await;
                continue;
            }
        };
        let connection = builder
            .serve_connection(
                TokioIo::new(stream),
                TowerToHyperService::new(router.clone()),
            )
            .with_upgrades();
        let mut stopped = stopped.clone();
        tokio::spawn(async move {
            let mut connection = pin!(connection);
            let result = tokio::select! {
                result = connection.as_mut() => result,
                _ = stopped.changed() => {
                    connection.as_mut().graceful_shutdown();
                    connection.await
                }
            };
            if let Err(err) = result {
                tracing::debug!(error = &err as &dyn Error, "connection failed");
            }
            drop(permit);
        });
    }
    drop(stop);
    drop(connections.acquire_many(MAX_CONNECTIONS).await);
}
