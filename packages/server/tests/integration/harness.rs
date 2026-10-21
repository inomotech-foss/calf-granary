//! A fresh database and key prefix per test, and the server binary.

use core::time::Duration;
use std::net::{Ipv4Addr, TcpListener};
use std::path::PathBuf;
use std::process::{ExitStatus, Output};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{env, fs, process, thread};

use reqwest::StatusCode;
use rustls::crypto::aws_lc_rs;
use rustls::{ClientConfig, RootCertStore};
use rustls_pki_types::CertificateDer;
use rustls_pki_types::pem::PemObject as _;
use tokio::process::{Child, Command};
use tokio::runtime::Builder;
use tokio::time;
use tokio_postgres::Client;
use tokio_postgres_rustls::MakeRustlsConnect;

/// The binary under test.
const SERVER: &str = env!("CARGO_BIN_EXE_calf-granary-server");

/// The token secret of every test.
const SECRET: &str = "0123456789abcdef0123456789abcdef";

/// How long processes may take to start or stop.
const TIMEOUT: Duration = Duration::from_secs(30);

/// Tells apart the stacks of one process.
static COUNTER: AtomicU32 = AtomicU32::new(0);

/// A database, a key prefix and a configuration file of one test.
///
/// Expects `GRANARY_DATABASE_URL` in the key=value form, with a role that may
/// create databases, `GRANARY_DATABASE_CA` for TLS, and the `AWS_*` variables
/// of the object storage. The bucket is `GRANARY_TEST_BUCKET`, `granary-test`
/// by default.
pub(crate) struct Stack {
    /// Names the database and the key prefix.
    id: String,
    /// Connects to the database that creates and drops test databases.
    admin_url: String,
    /// Connects to the test database.
    database_url: String,
    /// The bucket that holds the objects.
    bucket: String,
    /// Holds the configuration file.
    dir: PathBuf,
}

impl Stack {
    /// Creates the database and writes the configuration.
    ///
    /// `$bucket` and `$prefix` in `caches` are replaced with the test's
    /// bucket and unique prefix.
    pub(crate) async fn new(caches: &str) -> Self {
        let admin_url =
            env::var("GRANARY_DATABASE_URL").expect("GRANARY_DATABASE_URL locates Postgres");
        assert!(
            !admin_url.contains("://"),
            "GRANARY_DATABASE_URL must use the key=value form"
        );
        assert!(
            env::var_os("AWS_ENDPOINT").is_some(),
            "AWS_ENDPOINT locates the object storage"
        );
        let bucket = env::var("GRANARY_TEST_BUCKET").unwrap_or_else(|_| "granary-test".to_owned());
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .subsec_nanos();
        let id = format!(
            "t{}x{}x{nanos}",
            process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        );

        connect(&admin_url)
            .await
            .batch_execute(&format!("CREATE DATABASE {id}"))
            .await
            .unwrap();
        let dir = env::temp_dir().join(format!("granary-{id}"));
        fs::create_dir_all(&dir).unwrap();
        let config = caches.replace("$bucket", &bucket).replace("$prefix", &id);
        fs::write(dir.join("config.toml"), config).unwrap();
        Self {
            database_url: format!("{admin_url} dbname={id}"),
            id,
            admin_url,
            bucket,
            dir,
        }
    }

    /// The bucket that holds the objects.
    pub(crate) fn bucket(&self) -> &str {
        &self.bucket
    }

    /// The full object key of a path below the test's prefix.
    pub(crate) fn key(&self, path: &str) -> String {
        format!("{}/{path}", self.id)
    }

    /// Connects to the test database.
    pub(crate) async fn database(&self) -> Client {
        connect(&self.database_url).await
    }

    /// Prepares a server command with the test's configuration.
    pub(crate) fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(SERVER);
        command
            .args(args)
            .env("GRANARY_CONFIG", self.dir.join("config.toml"))
            .env("GRANARY_DATABASE_URL", &self.database_url)
            .env("GRANARY_TOKEN_SECRET", SECRET)
            .env_remove("OTEL_EXPORTER_OTLP_ENDPOINT")
            .kill_on_drop(true);
        command
    }

    /// Runs a server command to completion.
    pub(crate) async fn run(&self, args: &[&str]) -> Output {
        time::timeout(TIMEOUT, self.command(args).output())
            .await
            .expect("the command finishes")
            .unwrap()
    }

    /// Applies the migrations.
    pub(crate) async fn migrate(&self) {
        let output = self.run(&["migrate"]).await;
        assert!(output.status.success(), "migrate failed: {}", text(&output));
    }

    /// Signs a write token for the caches.
    pub(crate) async fn token(&self, caches: &[&str]) -> String {
        let mut args = vec!["token"];
        for cache in caches {
            args.extend(["--cache", cache]);
        }
        let output = self.run(&args).await;
        assert!(output.status.success(), "token failed: {}", text(&output));
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    /// Starts `serve` and waits until it's ready.
    pub(crate) async fn serve(&self) -> Server {
        let address = free_address();
        let child = self
            .command(&["serve"])
            .env("GRANARY_LISTEN", &address)
            .spawn()
            .unwrap();
        let mut server = Server {
            child,
            url: format!("http://{address}"),
            http: reqwest::Client::new(),
        };
        server.wait_ready().await;
        server
    }

    /// Starts `worker`.
    pub(crate) fn worker(&self) -> Child {
        self.command(&["worker"]).spawn().unwrap()
    }
}

impl Drop for Stack {
    fn drop(&mut self) {
        drop(fs::remove_dir_all(&self.dir));
        let admin_url = self.admin_url.clone();
        let statement = format!("DROP DATABASE IF EXISTS {} WITH (FORCE)", self.id);
        let dropped = thread::spawn(move || {
            Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async { connect(&admin_url).await.batch_execute(&statement).await })
        })
        .join();
        if !matches!(dropped, Ok(Ok(()))) {
            eprintln!("failed to drop the test database {}", self.id);
        }
    }
}

/// A running `serve` process.
pub(crate) struct Server {
    /// The process.
    child: Child,
    /// The base URL.
    url: String,
    /// Sends requests.
    http: reqwest::Client,
}

impl Server {
    /// The URL of a path.
    pub(crate) fn url(&self, path: &str) -> String {
        format!("{}/{path}", self.url)
    }

    /// Sends requests.
    pub(crate) const fn http(&self) -> &reqwest::Client {
        &self.http
    }

    /// Stops the server with SIGTERM.
    pub(crate) async fn stop(mut self) -> ExitStatus {
        terminate(&mut self.child).await
    }

    /// Waits until `/readyz` succeeds.
    async fn wait_ready(&mut self) {
        let ready = async {
            loop {
                if let Some(status) = self.child.try_wait().unwrap() {
                    panic!("serve exited with {status}");
                }
                let ready = self.http.get(self.url("readyz")).send().await;
                if ready.is_ok_and(|response| response.status() == StatusCode::OK) {
                    return;
                }
                time::sleep(Duration::from_millis(100)).await;
            }
        };
        time::timeout(TIMEOUT, ready)
            .await
            .expect("serve becomes ready");
    }
}

/// Stops a process with SIGTERM and waits for it.
pub(crate) async fn terminate(child: &mut Child) -> ExitStatus {
    let pid = child.id().expect("the process is running").to_string();
    let killed = Command::new("kill")
        .args(["-TERM", &pid])
        .status()
        .await
        .unwrap();
    assert!(killed.success(), "kill failed");
    time::timeout(TIMEOUT, child.wait())
        .await
        .expect("the process exits after SIGTERM")
        .unwrap()
}

/// The output of a command as text.
pub(crate) fn text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// Connects to Postgres and drives the connection in the background.
///
/// Trusts the CA in `GRANARY_DATABASE_CA`.
async fn connect(url: &str) -> Client {
    let mut roots = RootCertStore::empty();
    if let Some(path) = env::var_os("GRANARY_DATABASE_CA") {
        for certificate in CertificateDer::pem_file_iter(path).unwrap() {
            roots.add(certificate.unwrap()).unwrap();
        }
    }
    let config = ClientConfig::builder_with_provider(aws_lc_rs::default_provider().into())
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let (client, connection) = tokio_postgres::connect(url, MakeRustlsConnect::new(config))
        .await
        .unwrap();
    tokio::spawn(connection);
    client
}

/// A local address that was free a moment ago.
fn free_address() -> String {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    listener.local_addr().unwrap().to_string()
}
