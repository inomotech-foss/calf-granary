//! The `worker` command.

use core::error::Error;
use core::time::Duration;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;

use calf_granary_server_db::queries::task::{
    claim_task, complete_task, enqueue_due_schedules, fail_task, release_task, renew_lease,
};
use clap::Args;
use deadpool_postgres::{BuildError, Pool, PoolError};
use tokio::signal::ctrl_c;
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::watch;
use tokio::time::{self, MissedTickBehavior};

use crate::config::{Config, ConfigError};
use crate::database::DatabaseArgs;
use crate::migrations::{self, MigrationError};
use crate::tasks::{Runner, TaskKind, WorkerId};

/// How long the worker waits when no task is due.
const POLL_INTERVAL: Duration = Duration::from_secs(15);

/// How long a claimed task stays with the worker without a renewal.
const LEASE: Duration = Duration::from_mins(5);

/// How often a running task's lease is renewed.
const LEASE_RENEWAL: Duration = Duration::from_mins(1);

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
        let caches = Config::load(&self.config)?.caches()?;
        let pool = self.database.pool().map_err(WorkerError::Pool)?;
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

        let worker = Worker {
            id: WorkerId::current(),
            runner: Runner::new(pool.clone(), Arc::new(caches)),
            pool,
        };
        tracing::info!(worker = worker.id.as_ref(), "worker started");
        while !*stop.borrow() {
            let ran = worker.step(&mut stop).await.unwrap_or_else(|err| {
                tracing::error!(error = &err as &dyn Error, "failed to run tasks");
                false
            });
            if !ran {
                tokio::select! {
                    () = time::sleep(POLL_INTERVAL) => {}
                    _ = stop.changed() => {}
                }
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
        let worker = self.id.as_ref();
        let Some(task) = claim_task()
            .bind(&client, &worker, &LEASE.as_secs_f64())
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
        tokio::pin!(run);
        let mut renewal = time::interval(LEASE_RENEWAL);
        renewal.reset();
        renewal.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let result = loop {
            tokio::select! {
                result = &mut run => break Some(result),
                _ = renewal.tick() => {
                    if let Err(err) = renew_lease()
                        .bind(&client, &LEASE.as_secs_f64(), &task.id, &worker)
                        .await
                    {
                        tracing::warn!(error = &err as &dyn Error, task = task.id, "failed to renew a lease");
                    }
                }
                _ = stop.changed() => break None,
            }
        };

        match result {
            Some(Ok(())) => {
                complete_task().bind(&client, &task.id, &worker).await?;
                tracing::info!(task = task.id, kind = task.kind, "completed a task");
            }
            Some(Err(err)) => {
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
            None => {
                release_task().bind(&client, &task.id, &worker).await?;
                tracing::info!(task = task.id, "released a task");
            }
        }
        Ok(true)
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
}
