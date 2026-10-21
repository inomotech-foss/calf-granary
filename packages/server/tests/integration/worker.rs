//! Running scheduled and requested tasks.

use core::time::Duration;

use tokio::time;
use tokio_postgres::Client;

use crate::harness::{EMPTY, Stack, terminate};

const WAIT: Duration = Duration::from_secs(5);

async fn wait_for(database: &Client, id: i64, wanted: &str) {
    let reached = async {
        loop {
            let state: String = database
                .query_one("SELECT state::text FROM task WHERE id = $1", &[&id])
                .await
                .unwrap()
                .get(0);
            if state == wanted {
                return;
            }
            time::sleep(Duration::from_millis(50)).await;
        }
    };
    time::timeout(WAIT, reached)
        .await
        .unwrap_or_else(|_elapsed| panic!("the task is {wanted} in time"));
}

#[tokio::test]
#[ignore = "needs Postgres and object storage"]
async fn runs_scheduled_and_requested_tasks() {
    let stack = Stack::new(EMPTY).await;
    stack.migrate().await;
    let database = stack.database().await;
    let mut worker = stack.worker();

    let scheduled: Vec<i64> = time::timeout(WAIT, async {
        loop {
            let rows = database
                .query(
                    "SELECT id FROM task WHERE requested_by = 'schedule' ORDER BY kind",
                    &[],
                )
                .await
                .unwrap();
            if rows.len() == 2 {
                return rows.iter().map(|row| row.get(0)).collect();
            }
            time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("the prune and tier schedules create tasks");
    for id in scheduled {
        wait_for(&database, id, "done").await;
    }

    let requested: i64 = database
        .query_one(
            "INSERT INTO task (kind, requested_by) VALUES ('prune', 'test') RETURNING id",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    wait_for(&database, requested, "done").await;

    assert!(
        terminate(&mut worker).await.success(),
        "the worker stops cleanly"
    );
}

#[tokio::test]
#[ignore = "needs Postgres and object storage"]
async fn fails_tasks_that_keep_losing_their_worker() {
    let stack = Stack::new(EMPTY).await;
    stack.migrate().await;
    let database = stack.database().await;
    let abandoned: i64 = database
        .query_one(
            "INSERT INTO task (kind, requested_by, state, attempts, worker, lease_expires_at) \
             VALUES ('prune', 'test', 'running', 3, 'gone', now() - interval '1 minute') \
             RETURNING id",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    let mut worker = stack.worker();
    wait_for(&database, abandoned, "failed").await;
    assert!(
        terminate(&mut worker).await.success(),
        "the worker stops cleanly"
    );
}
