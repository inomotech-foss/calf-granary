//! Postgres connections.

use core::error::Error;
use std::sync::Arc;

use clap::Args;
use deadpool_postgres::{BuildError, Manager, Pool};
use tokio::sync::Notify;
use tokio_postgres::{Client, Config, NoTls};

use crate::notifications;

/// Command-line arguments that locate Postgres.
#[derive(Debug, Args)]
pub(crate) struct DatabaseArgs {
    /// Postgres connection URL.
    #[arg(long, env = "GRANARY_DATABASE_URL", hide_env_values = true)]
    database_url: Config,
}

impl DatabaseArgs {
    /// Opens a single connection and drives it in the background.
    pub(crate) async fn connect(&self) -> Result<Client, tokio_postgres::Error> {
        let (client, connection) = self.database_url.connect(NoTls).await?;
        tokio::spawn(async move {
            if let Err(err) = connection.await {
                tracing::error!(error = &err as &dyn Error, "database connection failed");
            }
        });
        Ok(client)
    }

    /// Creates a connection pool. Connections open on first use.
    pub(crate) fn pool(&self) -> Result<Pool, BuildError> {
        Pool::builder(Manager::new(self.database_url.clone(), NoTls)).build()
    }

    /// Listens on a channel in the background on a dedicated connection.
    ///
    /// The returned handle is woken for every notification.
    pub(crate) fn listen(&self, channel: &'static str) -> Arc<Notify> {
        let notify = Arc::new(Notify::new());
        tokio::spawn(notifications::listen(
            self.database_url.clone(),
            channel,
            Arc::clone(&notify),
        ));
        notify
    }
}
