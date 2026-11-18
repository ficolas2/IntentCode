// intentcode: ../../intent/repository.icode
// spec-hash: ef4e5e7dd7ede71c  (sha256 prefix of: repository.icode, utils.icode, types.icode)
// generated: claude-sonnet-5-5, by hand-driven compile, not reproducible
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use rusqlite::{Connection, ErrorCode, OptionalExtension, params};

use crate::utils::{exponential, retry};

// intent: repository::Link
#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    pub slug: String,
    pub target: String,
    pub created: DateTime<Utc>,
    pub expires: Option<DateTime<Utc>>,
    /// soft delete marker, set by ExpiryJob when the link has expired
    pub deleted: Option<DateTime<Utc>>,
}

// intent: repository::RepositoryError
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositoryError {
    /// a link with the same slug already exists
    Duplicate,
    /// no link with that slug exists
    NotFound,
    /// the database failed and the retries were exhausted
    Unavailable,
}

/// What a single database attempt can fail with. Only `Transient` is retried.
#[derive(Debug, Clone, Copy)]
enum Failure {
    Duplicate,
    NotFound,
    Transient,
    Permanent,
}

impl Failure {
    fn from_sqlite(e: &rusqlite::Error) -> Failure {
        match e {
            rusqlite::Error::SqliteFailure(f, _) => match f.code {
                ErrorCode::ConstraintViolation => Failure::Duplicate,
                ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked => Failure::Transient,
                _ => Failure::Permanent,
            },
            _ => Failure::Permanent,
        }
    }

    fn into_error(self) -> RepositoryError {
        match self {
            Failure::Duplicate => RepositoryError::Duplicate,
            Failure::NotFound => RepositoryError::NotFound,
            Failure::Transient | Failure::Permanent => RepositoryError::Unavailable,
        }
    }
}

// intent: repository::Repository
// db: sqlite. Link.slug is the unique key (PRIMARY KEY), enforced by the database itself.
// Soft deleted links keep their row, so their slug stays taken.
#[derive(Clone)]
pub struct Repository {
    conn: Arc<Mutex<Connection>>,
    /// test seam: makes every database call fail with a transient error
    unavailable: Arc<AtomicBool>,
}

fn millis(t: DateTime<Utc>) -> i64 {
    t.timestamp_millis()
}

fn from_millis(ms: i64) -> DateTime<Utc> {
    DateTime::from_timestamp_millis(ms).expect("timestamp in range")
}

fn row_to_link(row: &rusqlite::Row) -> rusqlite::Result<Link> {
    Ok(Link {
        slug: row.get(0)?,
        target: row.get(1)?,
        created: from_millis(row.get(2)?),
        expires: row.get::<_, Option<i64>>(3)?.map(from_millis),
        deleted: row.get::<_, Option<i64>>(4)?.map(from_millis),
    })
}

impl Repository {
    pub fn open(path: &str) -> rusqlite::Result<Repository> {
        Self::from_connection(Connection::open(path)?)
    }

    pub fn open_in_memory() -> rusqlite::Result<Repository> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(conn: Connection) -> rusqlite::Result<Repository> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS links (
                slug    TEXT PRIMARY KEY,
                target  TEXT NOT NULL,
                created INTEGER NOT NULL,
                expires INTEGER,
                deleted INTEGER
            );
            CREATE INDEX IF NOT EXISTS links_target ON links (target);",
        )?;
        Ok(Repository {
            conn: Arc::new(Mutex::new(conn)),
            unavailable: Arc::new(AtomicBool::new(false)),
        })
    }

    #[doc(hidden)]
    pub fn set_unavailable(&self, unavailable: bool) {
        self.unavailable.store(unavailable, Ordering::SeqCst);
    }

    /// One attempt against the database, off the async runtime.
    async fn attempt<T: Send + 'static>(
        &self,
        f: impl FnOnce(&Connection) -> Result<T, Failure> + Send + 'static,
    ) -> Result<T, Failure> {
        let conn = self.conn.clone();
        let unavailable = self.unavailable.clone();
        tokio::task::spawn_blocking(move || {
            if unavailable.load(Ordering::SeqCst) {
                return Err(Failure::Transient);
            }
            let conn = conn.lock().map_err(|_| Failure::Permanent)?;
            f(&conn)
        })
        .await
        .map_err(|_| Failure::Permanent)?
    }

    /// `@retry(2^n*0.1s, 5 tries)`: retries transient failures only.
    async fn with_retries<T: Send + 'static>(
        &self,
        op: impl Fn(&Connection) -> Result<T, Failure> + Clone + Send + 'static,
    ) -> Result<T, RepositoryError> {
        retry(
            exponential(0.1),
            5,
            |f: &Failure| matches!(f, Failure::Transient),
            || self.attempt(op.clone()),
        )
        .await
        .map_err(Failure::into_error)
    }

    // intent: Repository::insert
    /// Atomic: `Duplicate` comes from the database rejecting the unique key, never from
    /// checking first and inserting after. `@retry` covers transient failures only; a retried
    /// insert can report `Duplicate` because an earlier attempt did go through, and the caller
    /// (POST /url) tells by querying again.
    pub async fn insert(&self, link: Link) -> Result<(), RepositoryError> {
        self.with_retries(move |c| {
            c.execute(
                "INSERT INTO links (slug, target, created, expires, deleted) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    link.slug,
                    link.target,
                    millis(link.created),
                    link.expires.map(millis),
                    link.deleted.map(millis)
                ],
            )
            .map(|_| ())
            .map_err(|e| Failure::from_sqlite(&e))
        })
        .await
    }

    // intent: Repository::get
    /// Also returns expired and soft deleted links, the caller decides what they mean.
    pub async fn get(&self, slug: &str) -> Result<Link, RepositoryError> {
        let slug = slug.to_string();
        self.with_retries(move |c| {
            c.query_row(
                "SELECT slug, target, created, expires, deleted FROM links WHERE slug = ?1",
                params![slug],
                row_to_link,
            )
            .optional()
            .map_err(|e| Failure::from_sqlite(&e))?
            .ok_or(Failure::NotFound)
        })
        .await
    }

    // intent: Repository::with_prefix
    /// Every link whose slug starts with `prefix`, in a single query, including expired and
    /// soft deleted ones (their slug is still taken).
    pub async fn with_prefix(&self, prefix: &str) -> Result<Vec<Link>, RepositoryError> {
        let prefix = prefix.to_string();
        self.with_retries(move |c| {
            // substr instead of LIKE: '_' is a legal slug character and a LIKE wildcard.
            let mut stmt = c
                .prepare(
                    "SELECT slug, target, created, expires, deleted FROM links
                     WHERE substr(slug, 1, length(?1)) = ?1",
                )
                .map_err(|e| Failure::from_sqlite(&e))?;
            let rows = stmt
                .query_map(params![prefix], row_to_link)
                .map_err(|e| Failure::from_sqlite(&e))?;
            rows.collect::<Result<Vec<_>, _>>().map_err(|e| Failure::from_sqlite(&e))
        })
        .await
    }

    // intent: Repository::soft_delete_expired
    /// Sets `deleted` on every non deleted link that has expired. A single statement, so it is
    /// idempotent: running it twice, or from two instances at once, gives the same result.
    pub async fn soft_delete_expired(&self, now: DateTime<Utc>) -> Result<(), RepositoryError> {
        self.with_retries(move |c| {
            c.execute(
                "UPDATE links SET deleted = ?1 WHERE deleted IS NULL AND expires IS NOT NULL AND expires <= ?1",
                params![millis(now)],
            )
            .map(|_| ())
            .map_err(|e| Failure::from_sqlite(&e))
        })
        .await
    }

    // intent: Repository::renew
    /// Clears `deleted` and sets `expires`, in a single statement. Idempotent, so it is retried.
    pub async fn renew(&self, slug: &str, expires: DateTime<Utc>) -> Result<(), RepositoryError> {
        let slug = slug.to_string();
        self.with_retries(move |c| {
            let changed = c
                .execute(
                    "UPDATE links SET deleted = NULL, expires = ?2 WHERE slug = ?1",
                    params![slug, millis(expires)],
                )
                .map_err(|e| Failure::from_sqlite(&e))?;
            if changed == 0 { Err(Failure::NotFound) } else { Ok(()) }
        })
        .await
    }
}
