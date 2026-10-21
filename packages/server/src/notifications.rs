//! Waking up on Postgres notifications.

use core::error::Error;
use core::future;
use core::time::Duration;
use std::sync::Arc;

use futures::{StreamExt as _, stream};
use tokio::sync::Notify;
use tokio::time;
use tokio_postgres::AsyncMessage;

use crate::database::Database;

/// How long to wait before reconnecting after the connection was lost.
const RECONNECT_DELAY: Duration = Duration::from_secs(5);

/// Wakes `notify` for every notification on the channel.
///
/// Reconnects when the connection is lost. Also wakes `notify` after every
/// connect, since notifications may have been missed in the meantime.
pub(crate) async fn listen(database: Database, channel: &'static str, notify: Arc<Notify>) -> ! {
    loop {
        match listen_once(&database, channel, &notify).await {
            Ok(()) => tracing::warn!(channel, "the notification connection closed"),
            Err(err) => tracing::warn!(
                error = &err as &dyn Error,
                channel,
                "lost the notification connection"
            ),
        }
        time::sleep(RECONNECT_DELAY).await;
    }
}

/// Listens on one connection until it closes or fails.
async fn listen_once(
    database: &Database,
    channel: &str,
    notify: &Notify,
) -> Result<(), tokio_postgres::Error> {
    let (client, mut connection) = database.config().connect(database.tls()).await?;
    let mut messages = stream::poll_fn(move |cx| connection.poll_message(cx));
    let forward = async {
        while let Some(message) = messages.next().await {
            if let AsyncMessage::Notification(_) = message? {
                notify.notify_one();
            }
        }
        Ok(())
    };
    let subscribe = async {
        client.batch_execute(&format!("LISTEN {channel}")).await?;
        tracing::debug!(channel, "listening for notifications");
        notify.notify_one();
        future::pending().await
    };
    tokio::select! {
        result = forward => result,
        result = subscribe => result,
    }
}
