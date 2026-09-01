use rusqlite::{Connection, params};
use serde::Serialize;
use std::fs;
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Forward-only schema steps; a step's index plus one is the `user_version` it installs.
const MIGRATIONS: &[&str] = &[
    "CREATE TABLE sessions (
        id TEXT PRIMARY KEY,
        title TEXT NOT NULL,
        created_at_ms INTEGER NOT NULL,
        updated_at_ms INTEGER NOT NULL
    )",
    // Categorization is optional, so sessions written before this step stay valid with a NULL kind.
    "ALTER TABLE sessions ADD COLUMN kind TEXT",
    // A source reference is a stable pointer back into a session's timeline; the index
    // serves the only read path, which walks one session chronologically.
    "CREATE TABLE sources (
        id TEXT PRIMARY KEY,
        session_id TEXT NOT NULL REFERENCES sessions(id),
        start_ms INTEGER NOT NULL,
        end_ms INTEGER NOT NULL,
        speaker TEXT,
        text TEXT NOT NULL
    );
    CREATE INDEX sources_by_session_and_start ON sources (session_id, start_ms, id)",
    // A run record is one execution of a process. Client-chosen run ids are reusable, so the
    // primary key is a fresh record id and run_id is only a label; the index serves the single
    // read path, which walks every run newest-first.
    "CREATE TABLE runs (
        id TEXT PRIMARY KEY,
        run_id TEXT NOT NULL,
        session_id TEXT REFERENCES sessions(id),
        executable TEXT NOT NULL,
        status TEXT NOT NULL,
        exit_code INTEGER,
        error_code TEXT,
        started_at_ms INTEGER NOT NULL,
        ended_at_ms INTEGER
    );
    CREATE INDEX runs_by_start ON runs (started_at_ms, id)",
];

/// How long a write waits for another process holding the store's write lock.
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);
const BUSY_RETRY_INTERVAL: Duration = Duration::from_millis(20);

static RECORD_ID_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct Session {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) created_at_ms: i64,
    pub(crate) updated_at_ms: i64,
    /// Absent for an uncategorized session; the protocol omits the field entirely.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) kind: Option<String>,
}

/// A stable pointer into one session's timeline, with the exact text it refers to.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct Source {
    pub(crate) id: String,
    pub(crate) session_id: String,
    pub(crate) start_ms: i64,
    pub(crate) end_ms: i64,
    /// Absent when the speaker is unattributed; the protocol omits the field entirely.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) speaker: Option<String>,
    pub(crate) text: String,
}

/// One execution of a process, durable across restarts so a run outlives the connection that
/// asked for it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct RunRecord {
    pub(crate) id: String,
    pub(crate) run_id: String,
    /// Absent for a run that is not linked to a session; the protocol omits the field entirely.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) session_id: Option<String>,
    pub(crate) executable: String,
    pub(crate) status: String,
    /// Always stated, null included, mirroring the terminal run_exit frame.
    pub(crate) exit_code: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error_code: Option<String>,
    pub(crate) started_at_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) ended_at_ms: Option<i64>,
}

/// Durable state shared by every connection thread; SQLite serializes writes behind the mutex.
#[derive(Clone, Debug)]
pub(crate) struct Store {
    connection: Arc<Mutex<Connection>>,
}

impl Session {
    /// A fully formed record that has not been persisted yet, so callers can
    /// bound-check the response frame before anything durable happens.
    pub(crate) fn draft(title: &str, kind: Option<&str>) -> Session {
        let created_at_ms = now_milliseconds();
        Session {
            id: session_id(),
            title: title.to_owned(),
            created_at_ms,
            updated_at_ms: created_at_ms,
            kind: kind.map(str::to_owned),
        }
    }
}

impl Source {
    /// A fully formed record that has not been persisted yet, so callers can
    /// bound-check the response frame before anything durable happens.
    pub(crate) fn draft(
        session_id: &str,
        start_ms: i64,
        end_ms: i64,
        speaker: Option<&str>,
        text: &str,
    ) -> Source {
        Source {
            id: record_id("source"),
            session_id: session_id.to_owned(),
            start_ms,
            end_ms,
            speaker: speaker.map(str::to_owned),
            text: text.to_owned(),
        }
    }
}

impl RunRecord {
    /// A live record for a run that has just been admitted; the terminal columns stay open
    /// until the run reaches its single terminal frame.
    pub(crate) fn draft(run_id: &str, session_id: Option<&str>, executable: &str) -> RunRecord {
        RunRecord {
            id: record_id("run"),
            run_id: run_id.to_owned(),
            session_id: session_id.map(str::to_owned),
            executable: executable.to_owned(),
            status: "running".to_owned(),
            exit_code: None,
            error_code: None,
            started_at_ms: now_milliseconds(),
            ended_at_ms: None,
        }
    }
}

impl Store {
    #[cfg(test)]
    pub(crate) fn create_session(&self, title: &str) -> io::Result<Session> {
        let session = Session::draft(title, None);
        self.insert_session(&session)?;
        Ok(session)
    }

    pub(crate) fn insert_session(&self, session: &Session) -> io::Result<()> {
        self.connection()
            .execute(
                "INSERT INTO sessions (id, title, created_at_ms, updated_at_ms, kind)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    session.id,
                    session.title,
                    session.created_at_ms,
                    session.updated_at_ms,
                    session.kind
                ],
            )
            .map_err(io::Error::other)?;
        Ok(())
    }

    pub(crate) fn list_sessions(&self, limit: usize) -> io::Result<Vec<Session>> {
        let connection = self.connection();
        let mut statement = connection
            .prepare(
                "SELECT id, title, created_at_ms, updated_at_ms, kind FROM sessions
                 ORDER BY created_at_ms DESC, id DESC LIMIT ?1",
            )
            .map_err(io::Error::other)?;
        let sessions = statement
            .query_map([limit as i64], |row| {
                Ok(Session {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    created_at_ms: row.get(2)?,
                    updated_at_ms: row.get(3)?,
                    kind: row.get(4)?,
                })
            })
            .map_err(io::Error::other)?
            .collect::<rusqlite::Result<Vec<Session>>>()
            .map_err(io::Error::other)?;
        Ok(sessions)
    }

    pub(crate) fn insert_source(&self, source: &Source) -> io::Result<()> {
        self.connection()
            .execute(
                "INSERT INTO sources (id, session_id, start_ms, end_ms, speaker, text)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    source.id,
                    source.session_id,
                    source.start_ms,
                    source.end_ms,
                    source.speaker,
                    source.text
                ],
            )
            .map_err(io::Error::other)?;
        Ok(())
    }

    pub(crate) fn session_exists(&self, id: &str) -> io::Result<bool> {
        self.connection()
            .query_row("SELECT 1 FROM sessions WHERE id = ?1", [id], |_| Ok(()))
            .map(|()| true)
            .or_else(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => Ok(false),
                error => Err(io::Error::other(error)),
            })
    }

    pub(crate) fn list_sources(&self, session_id: &str, limit: usize) -> io::Result<Vec<Source>> {
        let connection = self.connection();
        let mut statement = connection
            .prepare(
                "SELECT id, session_id, start_ms, end_ms, speaker, text FROM sources
                 WHERE session_id = ?1 ORDER BY start_ms ASC, id ASC LIMIT ?2",
            )
            .map_err(io::Error::other)?;
        let sources = statement
            .query_map(params![session_id, limit as i64], |row| {
                Ok(Source {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    start_ms: row.get(2)?,
                    end_ms: row.get(3)?,
                    speaker: row.get(4)?,
                    text: row.get(5)?,
                })
            })
            .map_err(io::Error::other)?
            .collect::<rusqlite::Result<Vec<Source>>>()
            .map_err(io::Error::other)?;
        Ok(sources)
    }

    pub(crate) fn insert_run(&self, run: &RunRecord) -> io::Result<()> {
        self.connection()
            .execute(
                "INSERT INTO runs (id, run_id, session_id, executable, status, exit_code,
                     error_code, started_at_ms, ended_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    run.id,
                    run.run_id,
                    run.session_id,
                    run.executable,
                    run.status,
                    run.exit_code,
                    run.error_code,
                    run.started_at_ms,
                    run.ended_at_ms
                ],
            )
            .map_err(io::Error::other)?;
        Ok(())
    }

    /// Closes one run record at its terminal frame.
    pub(crate) fn finish_run(
        &self,
        id: &str,
        exit_code: Option<i64>,
        error_code: Option<&str>,
    ) -> io::Result<()> {
        self.connection()
            .execute(
                "UPDATE runs SET status = 'exited', exit_code = ?2, error_code = ?3,
                     ended_at_ms = ?4 WHERE id = ?1",
                params![id, exit_code, error_code, now_milliseconds()],
            )
            .map_err(io::Error::other)?;
        Ok(())
    }

    /// A record still marked running at open time belongs to a backend that died mid-run:
    /// nothing will ever close it, so the crash is recorded instead.
    pub(crate) fn mark_dangling_runs_interrupted(&self) -> io::Result<usize> {
        self.connection()
            .execute(
                "UPDATE runs SET status = 'interrupted', ended_at_ms = ?1 WHERE status = 'running'",
                params![now_milliseconds()],
            )
            .map_err(io::Error::other)
    }

    pub(crate) fn list_runs(&self, limit: usize) -> io::Result<Vec<RunRecord>> {
        let connection = self.connection();
        let mut statement = connection
            .prepare(
                "SELECT id, run_id, session_id, executable, status, exit_code, error_code,
                     started_at_ms, ended_at_ms FROM runs
                 ORDER BY started_at_ms DESC, id DESC LIMIT ?1",
            )
            .map_err(io::Error::other)?;
        let runs = statement
            .query_map([limit as i64], |row| {
                Ok(RunRecord {
                    id: row.get(0)?,
                    run_id: row.get(1)?,
                    session_id: row.get(2)?,
                    executable: row.get(3)?,
                    status: row.get(4)?,
                    exit_code: row.get(5)?,
                    error_code: row.get(6)?,
                    started_at_ms: row.get(7)?,
                    ended_at_ms: row.get(8)?,
                })
            })
            .map_err(io::Error::other)?
            .collect::<rusqlite::Result<Vec<RunRecord>>>()
            .map_err(io::Error::other)?;
        Ok(runs)
    }

    fn connection(&self) -> MutexGuard<'_, Connection> {
        self.connection
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

pub(crate) fn open_store(path: &Path) -> io::Result<Store> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        // Only a directory this store created is forced private; a caller-chosen
        // pre-existing directory keeps whatever permissions its owner gave it.
        if !parent.exists() {
            fs::create_dir_all(parent)?;
            fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
        }
    }

    // The schema check and the journal-mode conversion below cannot share one SQLite
    // transaction, so an advisory lock on the store file serializes the whole open
    // against other processes; without it a concurrent upgrade could land between
    // the check and the conversion.
    let _open_lock = acquire_open_lock(path)?;
    let connection = Connection::open(path).map_err(io::Error::other)?;
    connection
        .busy_timeout(BUSY_TIMEOUT)
        .map_err(io::Error::other)?;
    // A store from a newer build must be rejected before the journal mode, the
    // permissions, or any byte of the file is changed.
    supported_schema_version(&connection)?;
    // Tighten the freshly created database before WAL sidecars inherit its mode.
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .map_err(io::Error::other)?;
    enable_write_ahead_logging(&connection)?;
    migrate(&connection)?;

    Ok(Store {
        connection: Arc::new(Mutex::new(connection)),
    })
}

/// Advisory exclusive lock held across the whole open sequence; the kernel releases it when
/// the returned handle closes. It lives in a sidecar file because locking the store file
/// itself would collide with SQLite's own POSIX locks (and closing an extra descriptor on
/// the store would drop them).
fn acquire_open_lock(path: &Path) -> io::Result<fs::File> {
    let mut lock_path = path.as_os_str().to_owned();
    lock_path.push(".lock");
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(&lock_path)?;
    let deadline = Instant::now() + BUSY_TIMEOUT;
    loop {
        // SAFETY: `file` owns a valid descriptor for the flock call.
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
            return Ok(file);
        }
        let error = io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::EWOULDBLOCK) || Instant::now() >= deadline {
            return Err(error);
        }
        thread::sleep(BUSY_RETRY_INTERVAL);
    }
}

/// Switching journal modes needs a brief exclusive lock that SQLite reports as busy instead of
/// honoring the busy timeout, so first-open races between processes are retried here.
fn enable_write_ahead_logging(connection: &Connection) -> io::Result<()> {
    let deadline = Instant::now() + BUSY_TIMEOUT;
    loop {
        let attempt = connection
            .pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get::<_, String>(0))
            .map_err(io::Error::other);
        let expired = Instant::now() >= deadline;
        match attempt {
            Ok(journal_mode) if journal_mode.eq_ignore_ascii_case("wal") => return Ok(()),
            Ok(journal_mode) if expired => {
                return Err(io::Error::other(format!(
                    "store journal mode must be WAL, got {journal_mode}"
                )));
            }
            Err(error) if expired => return Err(error),
            _ => thread::sleep(BUSY_RETRY_INTERVAL),
        }
    }
}

/// Migrated store that never touches the filesystem, for tests that only need the protocol surface.
#[cfg(test)]
pub(crate) fn in_memory_store() -> io::Result<Store> {
    let connection = Connection::open_in_memory().map_err(io::Error::other)?;
    migrate(&connection)?;
    Ok(Store {
        connection: Arc::new(Mutex::new(connection)),
    })
}

fn migrate(connection: &Connection) -> io::Result<()> {
    // The schema check runs inside the write transaction so that two processes opening the same
    // store concurrently cannot both decide to apply the same migration.
    connection
        .execute_batch("BEGIN IMMEDIATE")
        .map_err(io::Error::other)?;
    match apply_migrations(connection) {
        Ok(()) => connection
            .execute_batch("COMMIT")
            .map_err(io::Error::other)
            .inspect_err(|_| {
                let _ = connection.execute_batch("ROLLBACK");
            }),
        Err(error) => {
            let _ = connection.execute_batch("ROLLBACK");
            Err(error)
        }
    }
}

fn supported_schema_version(connection: &Connection) -> io::Result<usize> {
    let user_version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(io::Error::other)?;
    let Ok(applied) = usize::try_from(user_version) else {
        return Err(io::Error::other(format!(
            "store schema version {user_version} is not a known version"
        )));
    };
    if applied > MIGRATIONS.len() {
        return Err(io::Error::other(format!(
            "store schema version {applied} is newer than the supported version {}; \
             refusing to modify it",
            MIGRATIONS.len()
        )));
    }
    Ok(applied)
}

fn apply_migrations(connection: &Connection) -> io::Result<()> {
    // Re-checked inside the write transaction: another process may have migrated
    // (or upgraded) the store between the open-time check and this lock.
    let applied = supported_schema_version(connection)?;

    for (index, migration) in MIGRATIONS.iter().enumerate().skip(applied) {
        let next_version = index + 1;
        connection
            .execute_batch(&format!(
                "{migration}; PRAGMA user_version = {next_version};"
            ))
            .map_err(io::Error::other)?;
    }

    Ok(())
}

fn session_id() -> String {
    record_id("session")
}

/// Process, wall clock, and sequence together keep ids unique across restarts and threads.
fn record_id(prefix: &str) -> String {
    let nanoseconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let sequence = RECORD_ID_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!(
        "{prefix}-{:x}-{nanoseconds:x}-{sequence:x}",
        std::process::id()
    )
}

fn now_milliseconds() -> i64 {
    let milliseconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    i64::try_from(milliseconds).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::{MIGRATIONS, open_store};
    use rusqlite::Connection;
    use std::fs;
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::thread;
    use std::time::Duration;

    static STORE_TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    struct StoreFixture {
        directory: PathBuf,
    }

    impl Drop for StoreFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.directory);
        }
    }

    impl StoreFixture {
        fn new() -> Self {
            let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .expect("backend should be inside repository")
                .join("target")
                .join(format!(
                    "cs-{}-{}",
                    std::process::id(),
                    STORE_TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
                ));
            fs::create_dir_all(&directory).expect("store fixture directory should be created");
            Self { directory }
        }

        fn store_path(&self) -> PathBuf {
            self.directory.join("store.sqlite")
        }
    }

    fn user_version(path: &std::path::Path) -> i64 {
        Connection::open(path)
            .expect("store should open")
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("user_version should be readable")
    }

    #[test]
    fn opening_a_new_store_applies_every_migration() {
        let fixture = StoreFixture::new();
        let store = open_store(&fixture.store_path()).expect("store should open");

        assert_eq!(user_version(&fixture.store_path()), MIGRATIONS.len() as i64);
        assert!(
            store
                .list_sessions(1)
                .expect("sessions should list")
                .is_empty()
        );
    }

    #[test]
    fn reopening_a_store_does_not_re_run_migrations() {
        let fixture = StoreFixture::new();
        let session = open_store(&fixture.store_path())
            .expect("store should open")
            .create_session("Existing session")
            .expect("session should be created");

        let reopened = open_store(&fixture.store_path()).expect("store should reopen");

        assert_eq!(user_version(&fixture.store_path()), MIGRATIONS.len() as i64);
        assert_eq!(
            reopened.list_sessions(2).expect("sessions should list"),
            vec![session]
        );
    }

    #[test]
    fn opening_a_version_one_store_adds_kinds_without_disturbing_its_rows() {
        let fixture = StoreFixture::new();
        let store_path = fixture.store_path();
        {
            // A store written before session kinds existed.
            let connection = Connection::open(&store_path).expect("store should open");
            connection
                .execute_batch(MIGRATIONS[0])
                .expect("schema should apply");
            connection
                .execute_batch(
                    "INSERT INTO sessions (id, title, created_at_ms, updated_at_ms)
                     VALUES ('session-legacy', 'Before kinds', 7, 7);",
                )
                .expect("legacy session should be inserted");
            connection
                .pragma_update(None, "user_version", 1)
                .expect("user_version should be writable");
        }

        let store = open_store(&store_path).expect("store should open");

        assert_eq!(user_version(&store_path), MIGRATIONS.len() as i64);
        let sessions = store.list_sessions(2).expect("sessions should list");
        assert_eq!(
            sessions,
            vec![super::Session {
                id: "session-legacy".to_owned(),
                title: "Before kinds".to_owned(),
                created_at_ms: 7,
                updated_at_ms: 7,
                kind: None,
            }],
            "a session written before kinds existed must survive as uncategorized"
        );
    }

    #[test]
    fn opening_a_version_two_store_adds_sources_without_disturbing_its_rows() {
        let fixture = StoreFixture::new();
        let store_path = fixture.store_path();
        {
            // A store written before source references existed.
            let connection = Connection::open(&store_path).expect("store should open");
            for migration in &MIGRATIONS[..2] {
                connection
                    .execute_batch(migration)
                    .expect("schema should apply");
            }
            connection
                .execute_batch(
                    "INSERT INTO sessions (id, title, created_at_ms, updated_at_ms)
                     VALUES ('session-legacy', 'Before sources', 7, 7);",
                )
                .expect("legacy session should be inserted");
            connection
                .pragma_update(None, "user_version", 2)
                .expect("user_version should be writable");
        }

        let store = open_store(&store_path).expect("store should open");

        assert_eq!(user_version(&store_path), MIGRATIONS.len() as i64);
        assert!(
            store
                .list_sources("session-legacy", 1)
                .expect("sources should list")
                .is_empty(),
            "a session written before sources existed must survive with no sources"
        );
    }

    #[test]
    fn opening_a_version_three_store_adds_runs_without_disturbing_its_rows() {
        let fixture = StoreFixture::new();
        let store_path = fixture.store_path();
        {
            // A store written before run records existed.
            let connection = Connection::open(&store_path).expect("store should open");
            for migration in &MIGRATIONS[..3] {
                connection
                    .execute_batch(migration)
                    .expect("schema should apply");
            }
            connection
                .execute_batch(
                    "INSERT INTO sessions (id, title, created_at_ms, updated_at_ms)
                     VALUES ('session-legacy', 'Before runs', 7, 7);",
                )
                .expect("legacy session should be inserted");
            connection
                .pragma_update(None, "user_version", 3)
                .expect("user_version should be writable");
        }

        let store = open_store(&store_path).expect("store should open");

        assert_eq!(user_version(&store_path), MIGRATIONS.len() as i64);
        assert!(
            store.list_runs(1).expect("runs should list").is_empty(),
            "a store written before runs existed must survive with no run records"
        );
    }

    #[test]
    fn opening_a_newer_store_fails_without_touching_the_data() {
        let fixture = StoreFixture::new();
        let store_path = fixture.store_path();
        let future_version = MIGRATIONS.len() as i64 + 1;
        {
            // A future store in the default DELETE journal mode, never touched by open_store.
            let connection = Connection::open(&store_path).expect("store should open");
            connection
                .execute_batch(
                    "CREATE TABLE future_data (payload TEXT NOT NULL);
                     INSERT INTO future_data (payload) VALUES ('from the future');",
                )
                .expect("future store should be prepared");
            connection
                .pragma_update(None, "user_version", future_version)
                .expect("user_version should be writable");
        }
        let original_bytes = fs::read(&store_path).expect("store bytes should be readable");
        let original_mode = file_mode(&store_path);

        let error = open_store(&store_path).expect_err("newer store should not open");

        assert!(
            error.to_string().contains("newer"),
            "error should explain the version mismatch, got {error}"
        );
        assert_eq!(user_version(&store_path), future_version);
        assert_eq!(
            fs::read(&store_path).expect("store bytes should be readable"),
            original_bytes,
            "a rejected store's bytes must be untouched"
        );
        assert_eq!(
            file_mode(&store_path),
            original_mode,
            "a rejected store's permissions must be untouched"
        );
        for suffix in ["-wal", "-shm"] {
            let mut sidecar = store_path.clone().into_os_string();
            sidecar.push(suffix);
            assert!(
                !Path::new(&sidecar).exists(),
                "a rejected store must not gain a {suffix} sidecar"
            );
        }
    }

    fn file_mode(path: &Path) -> u32 {
        fs::symlink_metadata(path)
            .expect("metadata should be readable")
            .permissions()
            .mode()
            & 0o777
    }

    #[test]
    fn a_version_bump_during_a_blocked_open_is_rejected_without_conversion() {
        let fixture = StoreFixture::new();
        let store_path = fixture.store_path();
        {
            // A current-version store in the default DELETE journal mode.
            let connection = Connection::open(&store_path).expect("store should open");
            for migration in MIGRATIONS {
                connection
                    .execute_batch(migration)
                    .expect("schema should apply");
            }
            connection
                .pragma_update(None, "user_version", MIGRATIONS.len() as i64)
                .expect("user_version should be writable");
        }
        let mut lock_path = store_path.clone().into_os_string();
        lock_path.push(".lock");
        let lock_file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .expect("open-lock sidecar should open for locking");
        // SAFETY: lock_file owns a valid descriptor for the exclusive flock.
        assert_eq!(
            unsafe { libc::flock(lock_file.as_raw_fd(), libc::LOCK_EX) },
            0
        );

        let opener = {
            let store_path = store_path.clone();
            thread::spawn(move || open_store(&store_path))
        };
        // Give the opener time to reach (and block on) the open lock, then upgrade
        // the store the way a newer build would.
        thread::sleep(Duration::from_millis(150));
        Connection::open(&store_path)
            .expect("store should open")
            .pragma_update(None, "user_version", MIGRATIONS.len() as i64 + 1)
            .expect("user_version should be writable");
        // SAFETY: lock_file still owns the locked descriptor.
        assert_eq!(
            unsafe { libc::flock(lock_file.as_raw_fd(), libc::LOCK_UN) },
            0
        );

        let error = opener
            .join()
            .expect("opener thread should finish")
            .expect_err("a store upgraded during the open must be rejected");
        assert!(
            error.to_string().contains("newer"),
            "error should explain the version mismatch, got {error}"
        );
        let journal_mode: String = Connection::open(&store_path)
            .expect("store should open")
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .expect("journal_mode should be readable");
        assert!(
            journal_mode.eq_ignore_ascii_case("delete"),
            "a rejected store must keep its journal mode, got {journal_mode}"
        );
    }

    #[test]
    fn a_pre_existing_parent_directory_keeps_its_permissions() {
        let fixture = StoreFixture::new();
        let parent = fixture.directory.join("custom");
        fs::create_dir(&parent).expect("parent should be created");
        fs::set_permissions(&parent, fs::Permissions::from_mode(0o755))
            .expect("parent permissions should be settable");

        open_store(&parent.join("store.sqlite")).expect("store should open");

        assert_eq!(
            file_mode(&parent),
            0o755,
            "a parent directory the store did not create must keep its permissions"
        );
    }
}
