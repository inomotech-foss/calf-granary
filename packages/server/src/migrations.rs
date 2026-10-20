//! Schema migrations embedded in the binary.

use std::collections::BTreeMap;

use tokio_postgres::{Client, Transaction};

/// Every migration, in the order it is applied.
const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    name: "access",
    sql: include_str!("../migrations/0001_access.sql"),
}];

/// The advisory lock that serializes concurrent migration runs.
const LOCK: i64 = 0x0067_7261_6E61_7279;

/// Creates the ledger of applied migrations.
const CREATE_LEDGER: &str = "CREATE TABLE IF NOT EXISTS schema_migration (
    version integer PRIMARY KEY,
    name text NOT NULL,
    sql text NOT NULL,
    applied_at timestamptz NOT NULL DEFAULT now()
)";

/// A schema migration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Migration {
    /// Position in the sequence, starting at 1.
    version: i32,
    /// Name from the file name, without the version and extension.
    name: &'static str,
    /// The SQL that the migration runs.
    sql: &'static str,
}

impl Migration {
    /// Runs the migration and records it in the ledger.
    async fn apply(&self, transaction: &Transaction<'_>) -> Result<(), MigrationError> {
        transaction
            .batch_execute(self.sql)
            .await
            .map_err(|source| MigrationError::Apply {
                version: self.version,
                source,
            })?;
        transaction
            .execute(
                "INSERT INTO schema_migration (version, name, sql) VALUES ($1, $2, $3)",
                &[&self.version, &self.name, &self.sql],
            )
            .await
            .map_err(MigrationError::Ledger)?;
        tracing::info!(
            version = self.version,
            name = self.name,
            "applied migration"
        );
        Ok(())
    }
}

/// A migration as recorded in the ledger.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Applied {
    /// Position in the sequence.
    version: i32,
    /// Name recorded when the migration was applied.
    name: String,
    /// SQL recorded when the migration was applied.
    sql: String,
}

impl Applied {
    /// Reads the ledger, creating it first if it doesn't exist.
    async fn read(transaction: &Transaction<'_>) -> Result<Vec<Self>, MigrationError> {
        transaction
            .batch_execute(CREATE_LEDGER)
            .await
            .map_err(MigrationError::Ledger)?;
        let rows = transaction
            .query("SELECT version, name, sql FROM schema_migration", &[])
            .await
            .map_err(MigrationError::Ledger)?;
        rows.iter()
            .map(|row| {
                Ok(Self {
                    version: row.try_get("version")?,
                    name: row.try_get("name")?,
                    sql: row.try_get("sql")?,
                })
            })
            .collect::<Result<_, _>>()
            .map_err(MigrationError::Ledger)
    }
}

/// Errors from applying migrations.
#[derive(Debug, thiserror::Error)]
pub(crate) enum MigrationError {
    /// A migration failed.
    #[error("failed to apply migration {version}")]
    Apply {
        /// The migration that failed.
        version: i32,
        /// The error from Postgres.
        #[source]
        source: tokio_postgres::Error,
    },
    /// Reading or writing the ledger failed.
    #[error("failed to access the migration ledger")]
    Ledger(#[source] tokio_postgres::Error),
    /// An applied migration differs from the embedded one.
    #[error("applied migration {version} differs from the embedded one")]
    Modified {
        /// The migration that differs.
        version: i32,
    },
    /// The database has a migration that this binary doesn't know.
    #[error("the database has migration {version}, which this binary doesn't know")]
    Unknown {
        /// The unknown migration.
        version: i32,
    },
}

/// Applies the pending migrations and returns how many there were.
///
/// All migrations run in one transaction that holds an advisory lock, so
/// concurrent runs wait for each other and a failure leaves the schema as it
/// was.
pub(crate) async fn migrate(client: &mut Client) -> Result<usize, MigrationError> {
    let transaction = client.transaction().await.map_err(MigrationError::Ledger)?;
    transaction
        .execute("SELECT pg_advisory_xact_lock($1)", &[&LOCK])
        .await
        .map_err(MigrationError::Ledger)?;
    let applied = Applied::read(&transaction).await?;
    let pending = pending(MIGRATIONS, &applied)?;
    for migration in &pending {
        migration.apply(&transaction).await?;
    }
    transaction.commit().await.map_err(MigrationError::Ledger)?;
    Ok(pending.len())
}

/// Returns the migrations that the ledger doesn't list yet.
///
/// Fails if the ledger lists a migration that differs from the embedded one
/// or that isn't embedded at all.
fn pending<'a>(
    migrations: &'a [Migration],
    applied: &[Applied],
) -> Result<Vec<&'a Migration>, MigrationError> {
    let mut embedded: BTreeMap<i32, &Migration> = migrations
        .iter()
        .map(|migration| (migration.version, migration))
        .collect();
    for record in applied {
        let migration = embedded
            .remove(&record.version)
            .ok_or(MigrationError::Unknown {
                version: record.version,
            })?;
        if migration.name != record.name || migration.sql != record.sql {
            return Err(MigrationError::Modified {
                version: record.version,
            });
        }
    }
    Ok(embedded.into_values().collect())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::*;

    const FIRST: Migration = Migration {
        version: 1,
        name: "first",
        sql: "SELECT 1",
    };
    const SECOND: Migration = Migration {
        version: 2,
        name: "second",
        sql: "SELECT 2",
    };

    fn applied(migration: &Migration) -> Applied {
        Applied {
            version: migration.version,
            name: migration.name.to_owned(),
            sql: migration.sql.to_owned(),
        }
    }

    #[test]
    fn embedded_list_matches_directory() {
        let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
        let mut files: Vec<String> = fs::read_dir(directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        files.sort();
        let expected: Vec<String> = MIGRATIONS
            .iter()
            .map(|migration| format!("{:04}_{}.sql", migration.version, migration.name))
            .collect();
        assert_eq!(files, expected, "every migration file must be embedded");
        for (index, migration) in MIGRATIONS.iter().enumerate() {
            assert_eq!(
                usize::try_from(migration.version).unwrap(),
                index + 1,
                "versions must be contiguous from 1"
            );
        }
    }

    #[test]
    fn pending_skips_applied() {
        let pending = pending(&[FIRST, SECOND], &[applied(&FIRST)]).unwrap();
        assert_eq!(pending, [&SECOND], "only the second migration is pending");
    }

    #[test]
    fn pending_rejects_modified() {
        let mut record = applied(&FIRST);
        record.sql = "SELECT 3".to_owned();
        let result = pending(&[FIRST, SECOND], &[record]);
        assert!(
            matches!(result, Err(MigrationError::Modified { version: 1 })),
            "a changed migration is rejected"
        );
    }

    #[test]
    fn pending_rejects_unknown() {
        let result = pending(&[FIRST], &[applied(&FIRST), applied(&SECOND)]);
        assert!(
            matches!(result, Err(MigrationError::Unknown { version: 2 })),
            "a migration from a newer binary is rejected"
        );
    }
}
