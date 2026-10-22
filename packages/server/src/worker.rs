//! The `worker` command.

use core::error::Error;
use core::time::Duration;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;

use calf_granary_server_db::queries::task::{
    claim_task, complete_task, enqueue_due_schedules, fail_abandoned_tasks, fail_task,
    next_deadline, release_task,
};
use clap::Args;
use deadpool_postgres::{BuildError, Pool, PoolError};
use tokio::signal::ctrl_c;
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::watch;
use tokio::time;

use crate::config::{Config, ConfigError};
use crate::database::{DatabaseArgs, TlsError};
use crate::migrations::{self, MigrationError};
use crate::tasks::{LEASE, Runner, TaskKind, WorkerId, renewing_lease};

/// The channel on which Postgres announces pending tasks.
const TASK_CHANNEL: &str = "task";

/// How long the worker waits after a database error.
const ERROR_DELAY: Duration = Duration::from_secs(15);

/// The shortest wait between checks for due tasks.
const MIN_IDLE: Duration = Duration::from_secs(1);

/// The longest wait between checks for due tasks.
const MAX_IDLE: Duration = Duration::from_mins(5);

/// How often a task is tried before it fails for good.
const MAX_ATTEMPTS: i32 = 3;

/// How long a failed task waits per attempt before it's tried again.
const RETRY_DELAY: Duration = Duration::from_mins(5);

/// Command-line arguments of the `worker` command.
#[derive(Debug, Args)]
pub(crate) struct WorkerArgs {
    /// Path to the configuration file.
    #[arg(long, env = "GRANARY_CONFIG")]
    config: PathBuf,
    /// Where to find Postgres.
    #[command(flatten)]
    database: DatabaseArgs,
}

impl WorkerArgs {
    /// Runs scheduled and requested tasks until the process receives SIGINT
    /// or SIGTERM.
    ///
    /// Refuses to start while migrations are pending.
    pub(crate) async fn run(self) -> Result<(), WorkerError> {
        let config = Config::load(&self.config)?;
        let caches = config.caches()?;
        let content = config.content()?;
        let database = self.database.open()?;
        let pool = database.pool().map_err(WorkerError::Pool)?;
        let client = pool.get().await.map_err(WorkerError::Connect)?;
        migrations::verify(&client).await?;
        drop(client);

        let mut terminate = signal(SignalKind::terminate()).map_err(WorkerError::Signal)?;
        let (stop_sender, mut stop) = watch::channel(false);
        tokio::spawn(async move {
            tokio::select! {
                _ = ctrl_c() => {}
                _ = terminate.recv() => {}
            }
            tracing::info!("shutting down");
            stop_sender.send_replace(true);
        });

        let wakeup = database.listen(TASK_CHANNEL);
        let worker = Worker {
            id: WorkerId::current(),
            runner: Runner::new(pool.clone(), Arc::new(caches), Arc::new(content)),
            pool,
        };
        tracing::info!(worker = worker.id.as_ref(), "worker started");
        while !*stop.borrow() {
            let idle = match worker.step(&mut stop).await {
                Ok(true) => continue,
                Ok(false) => worker.idle_time().await,
                Err(err) => Err(err),
            }
            .unwrap_or_else(|err| {
                tracing::error!(error = &err as &dyn Error, "failed to run tasks");
                ERROR_DELAY
            });
            tokio::select! {
                () = time::sleep(idle) => {}
                () = wakeup.notified() => {}
                _ = stop.changed() => {}
            }
        }
        Ok(())
    }
}

/// Claims and runs tasks.
#[derive(Debug)]
struct Worker {
    /// Identifies this worker in task leases.
    id: WorkerId,
    /// Runs the tasks.
    runner: Runner,
    /// Postgres connections.
    pool: Pool,
}

impl Worker {
    /// Enqueues tasks for due schedules and runs one task.
    ///
    /// Returns whether a task was claimed. When `stop` changes, the running
    /// task is released for another worker.
    async fn step(&self, stop: &mut watch::Receiver<bool>) -> Result<bool, WorkerError> {
        let client = self.pool.get().await.map_err(WorkerError::Connect)?;
        for task in enqueue_due_schedules().bind(&client).all().await? {
            tracing::info!(task = task.id, kind = task.kind, "scheduled a task");
        }
        for task in fail_abandoned_tasks()
            .bind(&client, &MAX_ATTEMPTS)
            .all()
            .await?
        {
            tracing::error!(task, "a task lost its worker too often");
        }
        let worker = self.id.as_ref();
        let kinds = TaskKind::ALL.map(TaskKind::name);
        let Some(task) = claim_task()
            .bind(&client, &worker, &LEASE.as_secs_f64(), &kinds.as_slice())
            .opt()
            .await?
        else {
            return Ok(false);
        };
        let Ok(kind) = task.kind.parse::<TaskKind>() else {
            tracing::error!(task = task.id, kind = task.kind, "unknown task kind");
            fail_task()
                .bind(&client, &0, &0.0, &"unknown task kind", &task.id, &worker)
                .await?;
            return Ok(true);
        };
        tracing::info!(
            task = task.id,
            kind = task.kind,
            attempt = task.attempts,
            "running a task"
        );

        let run = self.runner.run(task.id, kind);
        let result = tokio::select! {
            result = renewing_lease(&client, task.id, &self.id, run) => Some(result),
            _ = stop.changed() => None,
        };

        match result {
            Some(Ok(Ok(()))) => {
                complete_task().bind(&client, &task.id, &worker).await?;
                tracing::info!(task = task.id, kind = task.kind, "completed a task");
            }
            Some(Ok(Err(err))) => {
                let delay = RETRY_DELAY.as_secs_f64() * f64::from(task.attempts);
                fail_task()
                    .bind(
                        &client,
                        &MAX_ATTEMPTS,
                        &delay,
                        &err.describe(),
                        &task.id,
                        &worker,
                    )
                    .await?;
                tracing::error!(
                    error = &err as &dyn Error,
                    task = task.id,
                    attempt = task.attempts,
                    "task failed"
                );
            }
            Some(Err(lost)) => {
                tracing::warn!(error = &lost as &dyn Error, "stopped a task");
            }
            None => {
                release_task().bind(&client, &task.id, &worker).await?;
                tracing::info!(task = task.id, "released a task");
            }
        }
        Ok(true)
    }

    /// Returns how long to wait until a schedule, task or lease is due.
    async fn idle_time(&self) -> Result<Duration, WorkerError> {
        let client = self.pool.get().await.map_err(WorkerError::Connect)?;
        let seconds = next_deadline().bind(&client).one().await?;
        Ok(seconds
            .and_then(|seconds| Duration::try_from_secs_f64(seconds.max(0.0)).ok())
            .map_or(MAX_IDLE, |deadline| deadline.clamp(MIN_IDLE, MAX_IDLE)))
    }
}

/// Errors from the `worker` command.
#[derive(Debug, thiserror::Error)]
pub(crate) enum WorkerError {
    /// The configuration is invalid.
    #[error(transparent)]
    Config(#[from] ConfigError),
    /// Connecting to Postgres failed.
    #[error("failed to connect to the database")]
    Connect(#[source] PoolError),
    /// Updating tasks failed.
    #[error("failed to update tasks")]
    Database(#[from] tokio_postgres::Error),
    /// The schema doesn't match this binary.
    #[error(transparent)]
    Migrations(#[from] MigrationError),
    /// Creating the connection pool failed.
    #[error("failed to create the database pool")]
    Pool(#[source] BuildError),
    /// Installing the signal handler failed.
    #[error("failed to install the SIGTERM handler")]
    Signal(#[source] io::Error),
    /// TLS for Postgres can't be set up.
    #[error(transparent)]
    Tls(#[from] TlsError),
}
