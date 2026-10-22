//! Leases that keep running tasks with their worker.

use core::error::Error;
use core::time::Duration;

use calf_granary_server_db::queries::task::renew_lease;
use deadpool_postgres::Client;
use tokio::time::{self, MissedTickBehavior};

use super::WorkerId;

/// How long a running task stays with its worker without a renewal.
pub(crate) const LEASE: Duration = Duration::from_mins(5);

/// How often a running task's lease is renewed.
const RENEWAL: Duration = Duration::from_mins(1);

/// A task stopped because its lease went to another worker.
#[derive(Debug, thiserror::Error)]
#[error("task {task} lost its lease")]
pub(crate) struct LeaseLost {
    /// The task.
    pub(crate) task: i64,
}

/// Runs a task while renewing its lease.
///
/// Stops the task once its lease is gone, since another worker may run it.
pub(crate) async fn renewing<F: Future>(
    client: &Client,
    task: i64,
    worker: &WorkerId,
    run: F,
) -> Result<F::Output, LeaseLost> {
    tokio::pin!(run);
    let mut renewal = time::interval(RENEWAL);
    renewal.reset();
    renewal.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            result = &mut run => return Ok(result),
            _ = renewal.tick() => {
                match renew_lease()
                    .bind(client, &LEASE.as_secs_f64(), &task, &worker.as_ref())
                    .await
                {
                    Ok(0) => return Err(LeaseLost { task }),
                    Ok(_) => {}
                    Err(err) => {
                        tracing::warn!(error = &err as &dyn Error, task, "failed to renew a lease");
                    }
                }
            }
        }
    }
}
