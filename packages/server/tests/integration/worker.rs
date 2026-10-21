//! Running scheduled and requested tasks.

use core::time::Duration;

use tokio::time;
use tokio_postgres::Client;

use crate::harness::{Stack, terminate};

const WAIT: Duration = Duration::from_secs(5);

async fn wait_done(database: &Client, id: i64) {
    let done = async {
        loop {
            let state: String = database
                .query_one("SELECT state FROM task WHERE id = $1", &[&id])
                .await
                .unwrap()
                .get(0);
            if state == "done" {
                return;
            }
            time::sleep(Duration::from_millis(50)).await;
        }
    };
    time::timeout(WAIT, done)
        .await
        .expect("the task is done in time");
}

#[tokio::test]
#[ignore = "needs Postgres and object storage"]
async fn runs_scheduled_and_requested_tasks() {
    let stack = Stack::new("caches = {}").await;
    stack.migrate().await;
    let database = stack.database().await;
    let mut worker = stack.worker();

    let scheduled = time::timeout(WAIT, async {
        loop {
            let row = database
                .query_opt("SELECT id FROM task WHERE requested_by = 'schedule'", &[])
                .await
                .unwrap();
            if let Some(row) = row {
                return row.get(0);
            }
            time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("the schedule creates a task");
    wait_done(&database, scheduled).await;

    let requested: i64 = database
        .query_one(
            "INSERT INTO task (kind, requested_by) VALUES ('prune', 'test') RETURNING id",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    wait_done(&database, requested).await;

    assert!(
        terminate(&mut worker).await.success(),
        "the worker stops cleanly"
    );
}
