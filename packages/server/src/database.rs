//! Postgres connections.

use core::error::Error;
use std::path::PathBuf;
use std::sync::Arc;

use clap::Args;
use deadpool_postgres::{BuildError, Manager, Pool};
use rustls::crypto::aws_lc_rs;
use rustls::{ClientConfig, RootCertStore};
use rustls_pki_types::CertificateDer;
use rustls_pki_types::pem::{self, PemObject as _};
use rustls_platform_verifier::Verifier;
use tokio::sync::Notify;
use tokio_postgres::{Client, Config};
use tokio_postgres_rustls::MakeRustlsConnect;

use crate::notifications;

/// Command-line arguments that locate Postgres.
///
/// The `sslmode` of the URL decides whether TLS is used. With `prefer`, the
/// default, and `require`, the server certificate is always verified.
#[derive(Debug, Args)]
pub(crate) struct DatabaseArgs {
    /// Postgres connection URL.
    #[arg(long, env = "GRANARY_DATABASE_URL", hide_env_values = true)]
    database_url: Config,
    /// PEM file with the CA certificates that sign the server certificate.
    ///
    /// Without it, the system trust store is used.
    #[arg(long, env = "GRANARY_DATABASE_CA")]
    database_ca: Option<PathBuf>,
}

impl DatabaseArgs {
    /// Prepares connections with the configured TLS trust.
    pub(crate) fn open(&self) -> Result<Database, TlsError> {
        let provider = Arc::new(aws_lc_rs::default_provider());
        let builder = ClientConfig::builder_with_provider(Arc::clone(&provider))
            .with_safe_default_protocol_versions()
            .map_err(TlsError::Config)?;
        let config = match &self.database_ca {
            Some(path) => {
                let mut roots = RootCertStore::empty();
                for certificate in
                    CertificateDer::pem_file_iter(path).map_err(|source| TlsError::Ca {
                        path: path.clone(),
                        source,
                    })?
                {
                    let certificate = certificate.map_err(|source| TlsError::Ca {
                        path: path.clone(),
                        source,
                    })?;
                    roots
                        .add(certificate)
                        .map_err(|source| TlsError::Certificate {
                            path: path.clone(),
                            source,
                        })?;
                }
                if roots.is_empty() {
                    return Err(TlsError::NoCertificates { path: path.clone() });
                }
                builder.with_root_certificates(roots)
            }
            None => builder
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(
                    Verifier::new(provider).map_err(TlsError::Config)?,
                )),
        }
        .with_no_client_auth();
        Ok(Database {
            config: self.database_url.clone(),
            tls: MakeRustlsConnect::new(config),
        })
    }
}

/// Opens connections to Postgres.
#[derive(Clone)]
pub(crate) struct Database {
    /// Where and how to connect.
    config: Config,
    /// Negotiates TLS.
    tls: MakeRustlsConnect,
}

impl Database {
    /// Opens a single connection and drives it in the background.
    pub(crate) async fn connect(&self) -> Result<Client, tokio_postgres::Error> {
        let (client, connection) = self.config.connect(self.tls.clone()).await?;
        tokio::spawn(async move {
            if let Err(err) = connection.await {
                tracing::error!(error = &err as &dyn Error, "database connection failed");
            }
        });
        Ok(client)
    }

    /// Creates a connection pool. Connections open on first use.
    pub(crate) fn pool(&self) -> Result<Pool, BuildError> {
        Pool::builder(Manager::new(self.config.clone(), self.tls.clone())).build()
    }

    /// Listens on a channel in the background on a dedicated connection.
    ///
    /// The returned handle is woken for every notification.
    pub(crate) fn listen(&self, channel: &'static str) -> Arc<Notify> {
        let notify = Arc::new(Notify::new());
        tokio::spawn(notifications::listen(
            self.clone(),
            channel,
            Arc::clone(&notify),
        ));
        notify
    }

    /// The connection settings.
    pub(crate) const fn config(&self) -> &Config {
        &self.config
    }

    /// The TLS connector.
    pub(crate) fn tls(&self) -> MakeRustlsConnect {
        self.tls.clone()
    }
}

/// Errors from setting up TLS for Postgres.
#[derive(Debug, thiserror::Error)]
pub(crate) enum TlsError {
    /// The CA file can't be read or parsed.
    #[error("failed to read the database CA from {}", path.display())]
    Ca {
        /// The CA file.
        path: PathBuf,
        /// The error from the PEM parser.
        #[source]
        source: pem::Error,
    },
    /// A CA certificate isn't valid.
    #[error("the database CA in {} contains an invalid certificate", path.display())]
    Certificate {
        /// The CA file.
        path: PathBuf,
        /// The error from rustls.
        #[source]
        source: rustls::Error,
    },
    /// The TLS configuration can't be built.
    #[error("failed to set up TLS for the database")]
    Config(#[source] rustls::Error),
    /// The CA file has no certificates.
    #[error("the database CA in {} contains no certificates", path.display())]
    NoCertificates {
        /// The CA file.
        path: PathBuf,
    },
}
