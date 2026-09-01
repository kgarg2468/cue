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
    // A marker is a user-placed moment inside a session's timeline; the index serves the only
    // read path, which walks one session chronologically.
    "CREATE TABLE markers (
        id TEXT PRIMARY KEY,
        session_id TEXT NOT NULL REFERENCES sessions(id),
        at_ms INTEGER NOT NULL,
        kind TEXT NOT NULL,
        note TEXT
    );
    CREATE INDEX markers_by_session_and_at ON markers (session_id, at_ms, id)",
    // A session's note is optional and editable, so sessions written before this step stay
    // valid with a NULL note.
    "ALTER TABLE sessions ADD COLUMN note TEXT",
    // A transcript segment is one span of spoken text inside a session's timeline; the index
    // serves the only read path, which walks one session chronologically.
    "CREATE TABLE transcript_segments (
        id TEXT PRIMARY KEY,
        session_id TEXT NOT NULL REFERENCES sessions(id),
        start_ms INTEGER NOT NULL,
        end_ms INTEGER NOT NULL,
        speaker TEXT,
        text TEXT NOT NULL
    );
    CREATE INDEX transcript_by_session_and_start ON transcript_segments (session_id, start_ms, id)",
    // A run event is one backend-authored moment in a run record's life. Its sequence number
    // orders the trail independently of the clock, so a stepped wall clock cannot reorder a
    // run's own history; the index serves the only read path, which walks one record in order.
    "CREATE TABLE run_events (
        id TEXT PRIMARY KEY,
        record_id TEXT NOT NULL REFERENCES runs(id),
        seq INTEGER NOT NULL,
        at_ms INTEGER NOT NULL,
        kind TEXT NOT NULL
    );
    CREATE INDEX run_events_by_record_and_seq ON run_events (record_id, seq)",
    // An action is one unit of delegable work, drafted before anything runs it. A session link
    // is optional, so an action minted by hand stands alone; the index serves the only read
    // path, which walks every action newest-first.
    "CREATE TABLE actions (
        id TEXT PRIMARY KEY,
        session_id TEXT REFERENCES sessions(id),
        kind TEXT NOT NULL,
        title TEXT NOT NULL,
        status TEXT NOT NULL,
        created_at_ms INTEGER NOT NULL,
        updated_at_ms INTEGER NOT NULL
    );
    CREATE INDEX actions_by_creation ON actions (created_at_ms, id)",
    // A project is a durable container of work that outlives any one session, and a session may
    // belong to more than one. Membership is its own table so a link carries its own stamp
    // instead of rewriting either side; the primary key makes a pair statable exactly once, and
    // the index serves the only read path, which walks one session's projects in link order.
    "CREATE TABLE projects (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        created_at_ms INTEGER NOT NULL,
        updated_at_ms INTEGER NOT NULL
    );
    CREATE TABLE session_projects (
        session_id TEXT NOT NULL REFERENCES sessions(id),
        project_id TEXT NOT NULL REFERENCES projects(id),
        linked_at_ms INTEGER NOT NULL,
        PRIMARY KEY (session_id, project_id)
    );
    CREATE INDEX session_projects_by_link
        ON session_projects (session_id, linked_at_ms, project_id)",
    // A task packet is the provider-neutral document one action is delegated with. The document
    // is stored in canonical form — the parsed value re-serialized, so duplicate members
    // collapse last-wins and numbers take their shortest round-trip spelling — and only the
    // version it declares is lifted out of it into a column, so a packet shaped for one
    // provider stays readable to a
    // build that knows nothing about that provider's keys; the index serves the only read path,
    // which walks one action's packets in the order they were written.
    "CREATE TABLE task_packets (
        id TEXT PRIMARY KEY,
        action_id TEXT NOT NULL REFERENCES actions(id),
        packet_version INTEGER NOT NULL,
        body TEXT NOT NULL,
        created_at_ms INTEGER NOT NULL
    );
    CREATE INDEX task_packets_by_action ON task_packets (action_id, created_at_ms, id)",
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
    /// Absent for a session without a human note; the protocol omits the field entirely.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) note: Option<String>,
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

/// One span of spoken text inside a session's timeline, as the transcript recorded it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct TranscriptSegment {
    pub(crate) id: String,
    pub(crate) session_id: String,
    pub(crate) start_ms: i64,
    pub(crate) end_ms: i64,
    /// Absent when the speaker is unattributed; the protocol omits the field entirely.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) speaker: Option<String>,
    pub(crate) text: String,
}

/// A user-placed moment inside one session's timeline, with the kind of attention it deserves.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct Marker {
    pub(crate) id: String,
    pub(crate) session_id: String,
    pub(crate) at_ms: i64,
    pub(crate) kind: String,
    /// Absent when the marker carries no note; the protocol omits the field entirely.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) note: Option<String>,
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

/// One backend-authored moment in a run record's life, written by the same store call that
/// writes the run row it describes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct RunEvent {
    pub(crate) id: String,
    pub(crate) record_id: String,
    pub(crate) seq: i64,
    pub(crate) at_ms: i64,
    pub(crate) kind: String,
}

/// One unit of delegable work, drafted before anything runs it and durable across restarts so a
/// draft outlives the app that wrote it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct ActionRecord {
    pub(crate) id: String,
    /// Absent for an action that is not linked to a session; the protocol omits the field entirely.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) session_id: Option<String>,
    pub(crate) kind: String,
    pub(crate) title: String,
    pub(crate) status: String,
    pub(crate) created_at_ms: i64,
    pub(crate) updated_at_ms: i64,
}

/// A durable container of work that outlives any one session, so the sessions that touch it can
/// come and go while the project they belong to stays.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct Project {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) created_at_ms: i64,
    pub(crate) updated_at_ms: i64,
}

/// The provider-neutral document one action is delegated with. The domain layer keeps the
/// document exactly as it arrived and reads only the version it declares, so every other key
/// belongs to whichever provider the packet was shaped for.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct TaskPacket {
    pub(crate) id: String,
    pub(crate) action_id: String,
    pub(crate) packet_version: i64,
    pub(crate) body: serde_json::Value,
    pub(crate) created_at_ms: i64,
}

/// The event a run's admission records: a run is launched exactly once, at sequence zero.
const RUN_LAUNCHED_SEQUENCE: i64 = 0;
/// The event a run's close records: a run reaches its single terminal moment at sequence one,
/// whether a live backend closes it or a later startup sweep does.
const RUN_TERMINAL_SEQUENCE: i64 = 1;

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
            // A note is written by a later update, never at creation.
            note: None,
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

impl TranscriptSegment {
    /// A fully formed record that has not been persisted yet, so callers can
    /// bound-check the response frame before anything durable happens.
    pub(crate) fn draft(
        session_id: &str,
        start_ms: i64,
        end_ms: i64,
        speaker: Option<&str>,
        text: &str,
    ) -> TranscriptSegment {
        TranscriptSegment {
            id: record_id("segment"),
            session_id: session_id.to_owned(),
            start_ms,
            end_ms,
            speaker: speaker.map(str::to_owned),
            text: text.to_owned(),
        }
    }
}

impl Marker {
    /// A fully formed record that has not been persisted yet, so callers can
    /// bound-check the response frame before anything durable happens.
    pub(crate) fn draft(session_id: &str, at_ms: i64, kind: &str, note: Option<&str>) -> Marker {
        Marker {
            id: record_id("marker"),
            session_id: session_id.to_owned(),
            at_ms,
            kind: kind.to_owned(),
            note: note.map(str::to_owned),
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

impl RunEvent {
    /// A fully formed event that has not been persisted yet. Every field is backend-authored:
    /// the kind comes from a fixed set of lifecycle words and the stamp is the one already
    /// written to the run row, so no caller text can widen an event.
    pub(crate) fn draft(record_id: &str, seq: i64, at_ms: i64, kind: &str) -> RunEvent {
        RunEvent {
            // Module-qualified because the record id parameter shadows the id minter here.
            id: self::record_id("run-event"),
            record_id: record_id.to_owned(),
            seq,
            at_ms,
            kind: kind.to_owned(),
        }
    }
}

impl ActionRecord {
    /// A fully formed record that has not been persisted yet, so callers can
    /// bound-check the response frame before anything durable happens. The status is
    /// backend-authored: every action begins as a draft, and no request can mint one
    /// in any other status.
    pub(crate) fn draft(session_id: Option<&str>, kind: &str, title: &str) -> ActionRecord {
        let created_at_ms = now_milliseconds();
        ActionRecord {
            id: record_id("action"),
            session_id: session_id.map(str::to_owned),
            kind: kind.to_owned(),
            title: title.to_owned(),
            status: "draft".to_owned(),
            created_at_ms,
            updated_at_ms: created_at_ms,
        }
    }
}

impl Project {
    /// A fully formed record that has not been persisted yet, so callers can
    /// bound-check the response frame before anything durable happens.
    pub(crate) fn draft(name: &str) -> Project {
        let created_at_ms = now_milliseconds();
        Project {
            id: record_id("project"),
            name: name.to_owned(),
            created_at_ms,
            updated_at_ms: created_at_ms,
        }
    }
}

impl TaskPacket {
    /// A fully formed packet that has not been persisted yet, so callers can bound-check the
    /// response frame before anything durable happens. The version is the one the document
    /// itself declares, lifted out by the caller that read it.
    pub(crate) fn draft(
        action_id: &str,
        packet_version: i64,
        body: serde_json::Value,
    ) -> TaskPacket {
        TaskPacket {
            id: record_id("task-packet"),
            action_id: action_id.to_owned(),
            packet_version,
            body,
            created_at_ms: now_milliseconds(),
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
                "INSERT INTO sessions (id, title, created_at_ms, updated_at_ms, kind, note)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    session.id,
                    session.title,
                    session.created_at_ms,
                    session.updated_at_ms,
                    session.kind,
                    session.note
                ],
            )
            .map_err(io::Error::other)?;
        Ok(())
    }

    pub(crate) fn list_sessions(&self, limit: usize) -> io::Result<Vec<Session>> {
        let connection = self.connection();
        let mut statement = connection
            .prepare(
                "SELECT id, title, created_at_ms, updated_at_ms, kind, note FROM sessions
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
                    note: row.get(5)?,
                })
            })
            .map_err(io::Error::other)?
            .collect::<rusqlite::Result<Vec<Session>>>()
            .map_err(io::Error::other)?;
        Ok(sessions)
    }

    /// One session by id, or `None` when no such session exists.
    pub(crate) fn get_session(&self, id: &str) -> io::Result<Option<Session>> {
        self.connection()
            .query_row(
                "SELECT id, title, created_at_ms, updated_at_ms, kind, note FROM sessions
                 WHERE id = ?1",
                [id],
                |row| {
                    Ok(Session {
                        id: row.get(0)?,
                        title: row.get(1)?,
                        created_at_ms: row.get(2)?,
                        updated_at_ms: row.get(3)?,
                        kind: row.get(4)?,
                        note: row.get(5)?,
                    })
                },
            )
            .map(Some)
            .or_else(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                error => Err(io::Error::other(error)),
            })
    }

    /// Replaces one session's note wholesale — `None` clears it — and answers with the
    /// stored record, or `None` when no such session exists.
    pub(crate) fn update_session_note(
        &self,
        id: &str,
        note: Option<&str>,
    ) -> io::Result<Option<Session>> {
        let changed = self
            .connection()
            .execute(
                "UPDATE sessions SET note = ?2, updated_at_ms = ?3 WHERE id = ?1",
                params![id, note, now_milliseconds()],
            )
            .map_err(io::Error::other)?;
        if changed == 0 {
            return Ok(None);
        }
        self.get_session(id)
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

    pub(crate) fn insert_transcript_segment(&self, segment: &TranscriptSegment) -> io::Result<()> {
        self.connection()
            .execute(
                "INSERT INTO transcript_segments (id, session_id, start_ms, end_ms, speaker, text)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    segment.id,
                    segment.session_id,
                    segment.start_ms,
                    segment.end_ms,
                    segment.speaker,
                    segment.text
                ],
            )
            .map_err(io::Error::other)?;
        Ok(())
    }

    pub(crate) fn list_transcript(
        &self,
        session_id: &str,
        limit: usize,
    ) -> io::Result<Vec<TranscriptSegment>> {
        let connection = self.connection();
        let mut statement = connection
            .prepare(
                "SELECT id, session_id, start_ms, end_ms, speaker, text FROM transcript_segments
                 WHERE session_id = ?1 ORDER BY start_ms ASC, id ASC LIMIT ?2",
            )
            .map_err(io::Error::other)?;
        let segments = statement
            .query_map(params![session_id, limit as i64], |row| {
                Ok(TranscriptSegment {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    start_ms: row.get(2)?,
                    end_ms: row.get(3)?,
                    speaker: row.get(4)?,
                    text: row.get(5)?,
                })
            })
            .map_err(io::Error::other)?
            .collect::<rusqlite::Result<Vec<TranscriptSegment>>>()
            .map_err(io::Error::other)?;
        Ok(segments)
    }

    pub(crate) fn insert_marker(&self, marker: &Marker) -> io::Result<()> {
        self.connection()
            .execute(
                "INSERT INTO markers (id, session_id, at_ms, kind, note)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    marker.id,
                    marker.session_id,
                    marker.at_ms,
                    marker.kind,
                    marker.note
                ],
            )
            .map_err(io::Error::other)?;
        Ok(())
    }

    pub(crate) fn list_markers(&self, session_id: &str, limit: usize) -> io::Result<Vec<Marker>> {
        let connection = self.connection();
        let mut statement = connection
            .prepare(
                "SELECT id, session_id, at_ms, kind, note FROM markers
                 WHERE session_id = ?1 ORDER BY at_ms ASC, id ASC LIMIT ?2",
            )
            .map_err(io::Error::other)?;
        let markers = statement
            .query_map(params![session_id, limit as i64], |row| {
                Ok(Marker {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    at_ms: row.get(2)?,
                    kind: row.get(3)?,
                    note: row.get(4)?,
                })
            })
            .map_err(io::Error::other)?
            .collect::<rusqlite::Result<Vec<Marker>>>()
            .map_err(io::Error::other)?;
        Ok(markers)
    }

    /// Admits one run record and opens its event trail in the same transaction, so a record can
    /// never exist without the launch that produced it.
    pub(crate) fn insert_run(&self, run: &RunRecord) -> io::Result<()> {
        let mut connection = self.connection();
        let transaction = connection.transaction().map_err(io::Error::other)?;
        transaction
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
        // The event carries the row's own start stamp rather than a second reading of the
        // clock, so the trail can never disagree with the record it describes.
        insert_run_event_row(
            &transaction,
            &RunEvent::draft(
                &run.id,
                RUN_LAUNCHED_SEQUENCE,
                run.started_at_ms,
                "launched",
            ),
        )?;
        transaction.commit().map_err(io::Error::other)
    }

    /// Closes one run record at its terminal frame, and closes its event trail with it.
    pub(crate) fn finish_run(
        &self,
        id: &str,
        exit_code: Option<i64>,
        error_code: Option<&str>,
    ) -> io::Result<()> {
        let ended_at_ms = now_milliseconds();
        let mut connection = self.connection();
        let transaction = connection.transaction().map_err(io::Error::other)?;
        transaction
            .execute(
                "UPDATE runs SET status = 'exited', exit_code = ?2, error_code = ?3,
                     ended_at_ms = ?4 WHERE id = ?1",
                params![id, exit_code, error_code, ended_at_ms],
            )
            .map_err(io::Error::other)?;
        // Every close writes the same row status, so the error code is the more specific word
        // for how the run ended; a run that ended on its own terms has none.
        insert_run_event_row(
            &transaction,
            &RunEvent::draft(
                id,
                RUN_TERMINAL_SEQUENCE,
                ended_at_ms,
                error_code.unwrap_or("exited"),
            ),
        )?;
        transaction.commit().map_err(io::Error::other)
    }

    /// A record still marked running at open time belongs to a backend that died mid-run:
    /// nothing will ever close it, so the crash is recorded instead — in the row and in the
    /// record's event trail, which the dead backend left open too.
    pub(crate) fn mark_dangling_runs_interrupted(&self) -> io::Result<usize> {
        let ended_at_ms = now_milliseconds();
        let mut connection = self.connection();
        let transaction = connection.transaction().map_err(io::Error::other)?;
        let dangling = {
            let mut statement = transaction
                .prepare("SELECT id FROM runs WHERE status = 'running'")
                .map_err(io::Error::other)?;
            statement
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(io::Error::other)?
                .collect::<rusqlite::Result<Vec<String>>>()
                .map_err(io::Error::other)?
        };
        let swept = transaction
            .execute(
                "UPDATE runs SET status = 'interrupted', ended_at_ms = ?1 WHERE status = 'running'",
                params![ended_at_ms],
            )
            .map_err(io::Error::other)?;
        for id in &dangling {
            insert_run_event_row(
                &transaction,
                &RunEvent::draft(id, RUN_TERMINAL_SEQUENCE, ended_at_ms, "interrupted"),
            )?;
        }
        transaction.commit().map_err(io::Error::other)?;
        Ok(swept)
    }

    /// One run event, for the paths that write a run row and its trail separately.
    #[cfg(test)]
    pub(crate) fn insert_run_event(&self, event: &RunEvent) -> io::Result<()> {
        insert_run_event_row(&self.connection(), event)
    }

    /// Whether a run record exists under this record id — the run row's own primary key, not
    /// the reusable client-chosen run id.
    pub(crate) fn run_record_exists(&self, id: &str) -> io::Result<bool> {
        self.connection()
            .query_row("SELECT 1 FROM runs WHERE id = ?1", [id], |_| Ok(()))
            .map(|()| true)
            .or_else(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => Ok(false),
                error => Err(io::Error::other(error)),
            })
    }

    /// One run record's events, oldest first.
    pub(crate) fn list_run_events(
        &self,
        record_id: &str,
        limit: usize,
    ) -> io::Result<Vec<RunEvent>> {
        let connection = self.connection();
        let mut statement = connection
            .prepare(
                "SELECT id, record_id, seq, at_ms, kind FROM run_events
                 WHERE record_id = ?1 ORDER BY seq ASC, id ASC LIMIT ?2",
            )
            .map_err(io::Error::other)?;
        let events = statement
            .query_map(params![record_id, limit as i64], |row| {
                Ok(RunEvent {
                    id: row.get(0)?,
                    record_id: row.get(1)?,
                    seq: row.get(2)?,
                    at_ms: row.get(3)?,
                    kind: row.get(4)?,
                })
            })
            .map_err(io::Error::other)?
            .collect::<rusqlite::Result<Vec<RunEvent>>>()
            .map_err(io::Error::other)?;
        Ok(events)
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

    pub(crate) fn insert_action(&self, action: &ActionRecord) -> io::Result<()> {
        self.connection()
            .execute(
                "INSERT INTO actions (id, session_id, kind, title, status, created_at_ms,
                     updated_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    action.id,
                    action.session_id,
                    action.kind,
                    action.title,
                    action.status,
                    action.created_at_ms,
                    action.updated_at_ms
                ],
            )
            .map_err(io::Error::other)?;
        Ok(())
    }

    /// Every action, newest-first, so the most recent drafts lead the page.
    pub(crate) fn list_actions(&self, limit: usize) -> io::Result<Vec<ActionRecord>> {
        let connection = self.connection();
        let mut statement = connection
            .prepare(
                "SELECT id, session_id, kind, title, status, created_at_ms, updated_at_ms
                 FROM actions ORDER BY created_at_ms DESC, id DESC LIMIT ?1",
            )
            .map_err(io::Error::other)?;
        let actions = statement
            .query_map([limit as i64], |row| {
                Ok(ActionRecord {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    kind: row.get(2)?,
                    title: row.get(3)?,
                    status: row.get(4)?,
                    created_at_ms: row.get(5)?,
                    updated_at_ms: row.get(6)?,
                })
            })
            .map_err(io::Error::other)?
            .collect::<rusqlite::Result<Vec<ActionRecord>>>()
            .map_err(io::Error::other)?;
        Ok(actions)
    }

    pub(crate) fn insert_project(&self, project: &Project) -> io::Result<()> {
        self.connection()
            .execute(
                "INSERT INTO projects (id, name, created_at_ms, updated_at_ms)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    project.id,
                    project.name,
                    project.created_at_ms,
                    project.updated_at_ms
                ],
            )
            .map_err(io::Error::other)?;
        Ok(())
    }

    pub(crate) fn project_exists(&self, id: &str) -> io::Result<bool> {
        self.connection()
            .query_row("SELECT 1 FROM projects WHERE id = ?1", [id], |_| Ok(()))
            .map(|()| true)
            .or_else(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => Ok(false),
                error => Err(io::Error::other(error)),
            })
    }

    /// States that one session belongs to one project, stamped with the moment it was stated.
    /// Answers `true` when this call is what created the membership and `false` when the pair
    /// was already linked: the primary key admits a pair exactly once, so the ignored insert
    /// changes no row the second time and the caller can tell the two apart without racing a
    /// separate existence query against another writer.
    pub(crate) fn insert_session_project_link(
        &self,
        session_id: &str,
        project_id: &str,
    ) -> io::Result<bool> {
        let changed = self
            .connection()
            .execute(
                "INSERT OR IGNORE INTO session_projects (session_id, project_id, linked_at_ms)
                 VALUES (?1, ?2, ?3)",
                params![session_id, project_id, now_milliseconds()],
            )
            .map_err(io::Error::other)?;
        Ok(changed > 0)
    }

    /// One session's projects, in the order their memberships were stated. Links written inside
    /// the same millisecond fall back to the row's own insertion order rather than to the
    /// project id, which would smuggle project creation order back into a link-ordered page.
    pub(crate) fn list_session_projects(
        &self,
        session_id: &str,
        limit: usize,
    ) -> io::Result<Vec<Project>> {
        let connection = self.connection();
        let mut statement = connection
            .prepare(
                "SELECT projects.id, projects.name, projects.created_at_ms,
                     projects.updated_at_ms
                 FROM session_projects
                 JOIN projects ON projects.id = session_projects.project_id
                 WHERE session_projects.session_id = ?1
                 ORDER BY session_projects.linked_at_ms ASC, session_projects.rowid ASC
                 LIMIT ?2",
            )
            .map_err(io::Error::other)?;
        let projects = statement
            .query_map(params![session_id, limit as i64], |row| {
                Ok(Project {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    created_at_ms: row.get(2)?,
                    updated_at_ms: row.get(3)?,
                })
            })
            .map_err(io::Error::other)?
            .collect::<rusqlite::Result<Vec<Project>>>()
            .map_err(io::Error::other)?;
        Ok(projects)
    }

    pub(crate) fn action_exists(&self, id: &str) -> io::Result<bool> {
        self.connection()
            .query_row("SELECT 1 FROM actions WHERE id = ?1", [id], |_| Ok(()))
            .map(|()| true)
            .or_else(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => Ok(false),
                error => Err(io::Error::other(error)),
            })
    }

    /// Writes one packet, keeping its document as the canonical JSON text of the value the
    /// domain read, so the row states exactly the document the domain admitted.
    pub(crate) fn insert_task_packet(&self, packet: &TaskPacket) -> io::Result<()> {
        let body = serde_json::to_string(&packet.body)?;
        self.connection()
            .execute(
                "INSERT INTO task_packets (id, action_id, packet_version, body, created_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    packet.id,
                    packet.action_id,
                    packet.packet_version,
                    body,
                    packet.created_at_ms
                ],
            )
            .map_err(io::Error::other)?;
        Ok(())
    }

    /// One action's packets, oldest-first, so a page reads as the delegation history it is.
    /// Packets written inside the same millisecond fall back to the row's own insertion order
    /// rather than to the packet id, which carries the minting process into the ordering.
    pub(crate) fn list_task_packets(
        &self,
        action_id: &str,
        limit: usize,
    ) -> io::Result<Vec<TaskPacket>> {
        let connection = self.connection();
        let mut statement = connection
            .prepare(
                "SELECT id, action_id, packet_version, body, created_at_ms
                 FROM task_packets WHERE action_id = ?1
                 ORDER BY created_at_ms ASC, rowid ASC LIMIT ?2",
            )
            .map_err(io::Error::other)?;
        let rows = statement
            .query_map(params![action_id, limit as i64], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            })
            .map_err(io::Error::other)?
            .collect::<rusqlite::Result<Vec<(String, String, i64, String, i64)>>>()
            .map_err(io::Error::other)?;
        let mut packets = Vec::with_capacity(rows.len());
        for (id, action_id, packet_version, body, created_at_ms) in rows {
            packets.push(TaskPacket {
                id,
                action_id,
                packet_version,
                // The column holds the document the write serialized; a row that cannot be
                // read back as one is a corrupt store, not an empty packet.
                body: serde_json::from_str(&body)?,
                created_at_ms,
            });
        }
        Ok(packets)
    }

    fn connection(&self) -> MutexGuard<'_, Connection> {
        self.connection
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Writes one event, on whatever connection (or open transaction) the run row was written on.
fn insert_run_event_row(connection: &Connection, event: &RunEvent) -> io::Result<()> {
    connection
        .execute(
            "INSERT INTO run_events (id, record_id, seq, at_ms, kind)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                event.id,
                event.record_id,
                event.seq,
                event.at_ms,
                event.kind
            ],
        )
        .map_err(io::Error::other)?;
    Ok(())
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

/// Exclusive, lifetime ownership of a store by one backend process. Held for as long as the
/// returned handle lives; the kernel releases it when the process exits, however it exits.
/// Binding a socket proves nothing about the store, so the sweep of dangling runs (and every
/// later write) is only safe once this lock is held. The lock is keyed by the canonical store
/// path — a symlink alias must contend on the same lock, not mint its own — which requires the
/// store file to already exist (call after `open_store`).
pub(crate) fn acquire_store_ownership(path: &Path) -> io::Result<fs::File> {
    let mut owner_path = path.canonicalize()?.into_os_string();
    owner_path.push(".owner");
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(&owner_path)?;
    // SAFETY: `file` owns a valid descriptor for the flock call.
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
        return Ok(file);
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::EWOULDBLOCK) {
        return Err(io::Error::new(
            io::ErrorKind::AddrInUse,
            format!(
                "store {} is already owned by another backend",
                path.display()
            ),
        ));
    }
    Err(error)
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
                note: None,
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
    fn opening_a_version_four_store_adds_markers_without_disturbing_its_rows() {
        let fixture = StoreFixture::new();
        let store_path = fixture.store_path();
        {
            // A store written before user markers existed.
            let connection = Connection::open(&store_path).expect("store should open");
            for migration in &MIGRATIONS[..4] {
                connection
                    .execute_batch(migration)
                    .expect("schema should apply");
            }
            connection
                .execute_batch(
                    "INSERT INTO sessions (id, title, created_at_ms, updated_at_ms)
                     VALUES ('session-legacy', 'Before markers', 7, 7);",
                )
                .expect("legacy session should be inserted");
            connection
                .pragma_update(None, "user_version", 4)
                .expect("user_version should be writable");
        }

        let store = open_store(&store_path).expect("store should open");

        assert_eq!(user_version(&store_path), MIGRATIONS.len() as i64);
        assert!(
            store
                .list_markers("session-legacy", 1)
                .expect("markers should list")
                .is_empty(),
            "a session written before markers existed must survive with no markers"
        );
        let marker = super::Marker::draft("session-legacy", 872_000, "decision", Some("Ship it"));
        store.insert_marker(&marker).expect("marker should insert");
        assert_eq!(
            store
                .list_markers("session-legacy", 2)
                .expect("markers should list"),
            vec![marker],
            "a marker written after the upgrade must round-trip"
        );
    }

    #[test]
    fn opening_a_version_five_store_adds_notes_without_disturbing_its_rows() {
        let fixture = StoreFixture::new();
        let store_path = fixture.store_path();
        {
            // A store written before session notes existed.
            let connection = Connection::open(&store_path).expect("store should open");
            for migration in &MIGRATIONS[..5] {
                connection
                    .execute_batch(migration)
                    .expect("schema should apply");
            }
            connection
                .execute_batch(
                    "INSERT INTO sessions (id, title, created_at_ms, updated_at_ms)
                     VALUES ('session-legacy', 'Before notes', 7, 7);",
                )
                .expect("legacy session should be inserted");
            connection
                .pragma_update(None, "user_version", 5)
                .expect("user_version should be writable");
        }

        let store = open_store(&store_path).expect("store should open");

        assert_eq!(user_version(&store_path), MIGRATIONS.len() as i64);
        let sessions = store.list_sessions(2).expect("sessions should list");
        assert_eq!(
            sessions.first().and_then(|session| session.note.clone()),
            None,
            "a session written before notes existed must survive without one"
        );
        let updated = store
            .update_session_note("session-legacy", Some("Follow up"))
            .expect("note should update")
            .expect("the legacy session should still exist");
        assert_eq!(updated.note.as_deref(), Some("Follow up"));
        assert_eq!(
            store
                .list_sessions(2)
                .expect("sessions should list")
                .first()
                .and_then(|session| session.note.clone()),
            Some("Follow up".to_owned()),
            "a note written after the upgrade must round-trip"
        );
    }

    #[test]
    fn transcript_segments_list_chronologically_for_their_own_session() {
        let fixture = StoreFixture::new();
        let store = open_store(&fixture.store_path()).expect("store should open");
        let session = store
            .create_session("Sprint planning")
            .expect("session should be created");
        let other = store
            .create_session("Unrelated session")
            .expect("session should be created");

        // Inserted out of order to pin chronological listing.
        let late = super::TranscriptSegment::draft(
            &session.id,
            872_000,
            884_000,
            Some("Sarah"),
            "Can you check PR 482?",
        );
        store
            .insert_transcript_segment(&late)
            .expect("segment should insert");
        let early = super::TranscriptSegment::draft(&session.id, 1_000, 1_000, None, "Zero-length");
        store
            .insert_transcript_segment(&early)
            .expect("segment should insert");
        let elsewhere =
            super::TranscriptSegment::draft(&other.id, 5, 6, None, "Belongs to the other session");
        store
            .insert_transcript_segment(&elsewhere)
            .expect("segment should insert");

        assert_eq!(
            store
                .list_transcript(&session.id, 10)
                .expect("transcript should list"),
            vec![early, late],
            "a transcript lists chronologically for its own session only"
        );
        assert_eq!(
            store
                .list_transcript(&session.id, 1)
                .expect("transcript should list")
                .len(),
            1,
            "the limit bounds the page"
        );
        assert_eq!(
            store
                .list_transcript(&other.id, 10)
                .expect("transcript should list"),
            vec![elsewhere]
        );
    }

    #[test]
    fn opening_a_version_six_store_adds_transcripts_without_disturbing_its_rows() {
        let fixture = StoreFixture::new();
        let store_path = fixture.store_path();
        {
            // A store written before transcript segments existed.
            let connection = Connection::open(&store_path).expect("store should open");
            for migration in &MIGRATIONS[..6] {
                connection
                    .execute_batch(migration)
                    .expect("schema should apply");
            }
            connection
                .execute_batch(
                    "INSERT INTO sessions (id, title, created_at_ms, updated_at_ms)
                     VALUES ('session-legacy', 'Before transcripts', 7, 7);",
                )
                .expect("legacy session should be inserted");
            connection
                .pragma_update(None, "user_version", 6)
                .expect("user_version should be writable");
        }

        let store = open_store(&store_path).expect("store should open");

        assert_eq!(user_version(&store_path), MIGRATIONS.len() as i64);
        assert_eq!(
            store
                .list_sessions(2)
                .expect("sessions should list")
                .first()
                .map(|session| session.title.clone()),
            Some("Before transcripts".to_owned()),
            "a session written before transcripts existed must survive untouched"
        );
        assert!(
            store
                .list_transcript("session-legacy", 1)
                .expect("transcript should list")
                .is_empty(),
            "a session written before transcripts existed must survive with no segments"
        );
        let segment = super::TranscriptSegment::draft(
            "session-legacy",
            872_000,
            884_000,
            Some("Sarah"),
            "Written after the upgrade",
        );
        store
            .insert_transcript_segment(&segment)
            .expect("segment should insert");
        assert_eq!(
            store
                .list_transcript("session-legacy", 2)
                .expect("transcript should list"),
            vec![segment],
            "a segment written after the upgrade must round-trip"
        );
    }

    #[test]
    fn run_events_list_in_sequence_for_their_own_record() {
        let fixture = StoreFixture::new();
        let store = open_store(&fixture.store_path()).expect("store should open");
        let run = super::RunRecord::draft("build", None, "/usr/bin/true");
        store.insert_run(&run).expect("run should insert");
        let other = super::RunRecord::draft("lint", None, "/usr/bin/true");
        store.insert_run(&other).expect("run should insert");

        // Written out of order, and with a stamp older than the launch it follows, to pin
        // that the sequence — not the clock or the insertion order — orders the trail.
        let terminal = super::RunEvent::draft(&run.id, 2, run.started_at_ms - 5_000, "exited");
        store
            .insert_run_event(&terminal)
            .expect("event should insert");
        let middle = super::RunEvent::draft(&run.id, 1, run.started_at_ms, "cancelled");
        store
            .insert_run_event(&middle)
            .expect("event should insert");

        let events = store
            .list_run_events(&run.id, 10)
            .expect("events should list");
        assert_eq!(
            events.iter().map(|event| event.seq).collect::<Vec<i64>>(),
            vec![0, 1, 2],
            "a record's events list in sequence order"
        );
        assert_eq!(
            events[0].kind, "launched",
            "admission opens the trail with the launch"
        );
        assert_eq!(events[0].at_ms, run.started_at_ms);
        assert_eq!(events[1..], [middle, terminal]);
        assert_eq!(
            store
                .list_run_events(&run.id, 2)
                .expect("events should list")
                .len(),
            2,
            "the limit bounds the page"
        );
        assert_eq!(
            store
                .list_run_events(&other.id, 10)
                .expect("events should list")
                .iter()
                .map(|event| event.kind.clone())
                .collect::<Vec<String>>(),
            vec!["launched".to_owned()],
            "a trail carries only its own record's events"
        );
    }

    #[test]
    fn closing_a_run_records_how_it_ended() {
        let fixture = StoreFixture::new();
        let store = open_store(&fixture.store_path()).expect("store should open");
        let clean = super::RunRecord::draft("clean", None, "/usr/bin/true");
        store.insert_run(&clean).expect("run should insert");
        let failed = super::RunRecord::draft("failed", None, "/nonexistent/probe");
        store.insert_run(&failed).expect("run should insert");

        store
            .finish_run(&clean.id, Some(0), None)
            .expect("run should close");
        store
            .finish_run(&failed.id, None, Some("spawn_failed"))
            .expect("run should close");

        for (record, kind) in [(&clean, "exited"), (&failed, "spawn_failed")] {
            let events = store
                .list_run_events(&record.id, 10)
                .expect("events should list");
            assert_eq!(events.len(), 2, "a closed run has a launch and a close");
            assert_eq!(events[1].kind, kind);
            let stamp = store
                .list_runs(10)
                .expect("runs should list")
                .into_iter()
                .find(|run| run.id == record.id)
                .and_then(|run| run.ended_at_ms)
                .expect("a closed run has an end stamp");
            assert_eq!(
                events[1].at_ms, stamp,
                "the terminal event carries the row's own end stamp"
            );
        }
    }

    #[test]
    fn the_interruption_sweep_closes_every_dangling_trail() {
        let fixture = StoreFixture::new();
        let store = open_store(&fixture.store_path()).expect("store should open");
        let dangling = super::RunRecord::draft("dangling", None, "/bin/sleep");
        store.insert_run(&dangling).expect("run should insert");
        let closed = super::RunRecord::draft("closed", None, "/usr/bin/true");
        store.insert_run(&closed).expect("run should insert");
        store
            .finish_run(&closed.id, Some(0), None)
            .expect("run should close");

        assert_eq!(
            store
                .mark_dangling_runs_interrupted()
                .expect("sweep should run"),
            1,
            "only the run left running is swept"
        );

        let swept = store
            .list_runs(10)
            .expect("runs should list")
            .into_iter()
            .find(|run| run.id == dangling.id)
            .expect("the dangling run should still exist");
        let events = store
            .list_run_events(&dangling.id, 10)
            .expect("events should list");
        assert_eq!(
            events
                .iter()
                .map(|event| event.kind.clone())
                .collect::<Vec<String>>(),
            vec!["launched".to_owned(), "interrupted".to_owned()],
            "the sweep closes the trail it found open"
        );
        assert_eq!(
            Some(events[1].at_ms),
            swept.ended_at_ms,
            "the interruption event carries the sweep's own end stamp"
        );
        assert_eq!(
            store
                .list_run_events(&closed.id, 10)
                .expect("events should list")
                .len(),
            2,
            "a run the sweep did not touch keeps its own trail"
        );
    }

    #[test]
    fn opening_a_version_seven_store_adds_run_events_without_disturbing_its_rows() {
        let fixture = StoreFixture::new();
        let store_path = fixture.store_path();
        {
            // A store written before run lifecycle events existed.
            let connection = Connection::open(&store_path).expect("store should open");
            for migration in &MIGRATIONS[..7] {
                connection
                    .execute_batch(migration)
                    .expect("schema should apply");
            }
            connection
                .execute_batch(
                    "INSERT INTO runs (id, run_id, executable, status, started_at_ms, ended_at_ms)
                     VALUES ('run-legacy', 'before-events', '/usr/bin/true', 'exited', 7, 9);",
                )
                .expect("legacy run should be inserted");
            connection
                .pragma_update(None, "user_version", 7)
                .expect("user_version should be writable");
        }

        let store = open_store(&store_path).expect("store should open");

        assert_eq!(user_version(&store_path), MIGRATIONS.len() as i64);
        assert_eq!(
            store
                .list_runs(2)
                .expect("runs should list")
                .first()
                .map(|run| run.run_id.clone()),
            Some("before-events".to_owned()),
            "a run written before events existed must survive untouched"
        );
        assert!(
            store
                .list_run_events("run-legacy", 1)
                .expect("events should list")
                .is_empty(),
            "a run written before events existed must survive with no trail"
        );
        let event = super::RunEvent::draft("run-legacy", 0, 7, "launched");
        store.insert_run_event(&event).expect("event should insert");
        assert_eq!(
            store
                .list_run_events("run-legacy", 2)
                .expect("events should list"),
            vec![event],
            "an event written after the upgrade must round-trip"
        );
    }

    #[test]
    fn actions_list_newest_first_and_keep_their_session_link() {
        let fixture = StoreFixture::new();
        let store = open_store(&fixture.store_path()).expect("store should open");
        let session = store
            .create_session("Sprint planning")
            .expect("session should be created");

        // Inserted oldest-first, with hand-set stamps, to pin newest-first listing.
        let older = super::ActionRecord {
            created_at_ms: 10,
            updated_at_ms: 10,
            ..super::ActionRecord::draft(
                Some(&session.id),
                "review_pull_request",
                "Check PR 482 for token refresh breakage",
            )
        };
        store.insert_action(&older).expect("action should insert");
        let newer = super::ActionRecord {
            created_at_ms: 20,
            updated_at_ms: 20,
            ..super::ActionRecord::draft(None, "custom", "Follow up with the team")
        };
        store.insert_action(&newer).expect("action should insert");

        assert_eq!(
            store.list_actions(10).expect("actions should list"),
            vec![newer.clone(), older],
            "actions list newest-first, and a linked action keeps its session"
        );
        assert_eq!(
            store.list_actions(1).expect("actions should list"),
            vec![newer],
            "the limit bounds the page"
        );
        assert!(
            store
                .list_actions(10)
                .expect("actions should list")
                .iter()
                .all(|action| action.status == "draft"),
            "every action begins as a draft"
        );
    }

    #[test]
    fn opening_a_version_eight_store_adds_actions_without_disturbing_its_rows() {
        let fixture = StoreFixture::new();
        let store_path = fixture.store_path();
        {
            // A store written before action drafts existed.
            let connection = Connection::open(&store_path).expect("store should open");
            for migration in &MIGRATIONS[..8] {
                connection
                    .execute_batch(migration)
                    .expect("schema should apply");
            }
            connection
                .execute_batch(
                    "INSERT INTO sessions (id, title, created_at_ms, updated_at_ms)
                     VALUES ('session-legacy', 'Before actions', 7, 7);",
                )
                .expect("legacy session should be inserted");
            connection
                .pragma_update(None, "user_version", 8)
                .expect("user_version should be writable");
        }

        let store = open_store(&store_path).expect("store should open");

        assert_eq!(user_version(&store_path), MIGRATIONS.len() as i64);
        assert_eq!(
            store
                .list_sessions(2)
                .expect("sessions should list")
                .first()
                .map(|session| session.title.clone()),
            Some("Before actions".to_owned()),
            "a session written before actions existed must survive untouched"
        );
        assert!(
            store
                .list_actions(1)
                .expect("actions should list")
                .is_empty(),
            "a store written before actions existed must survive with no drafts"
        );
        let action =
            super::ActionRecord::draft(Some("session-legacy"), "plan_change", "After the upgrade");
        store.insert_action(&action).expect("action should insert");
        assert_eq!(
            store.list_actions(2).expect("actions should list"),
            vec![action],
            "an action written after the upgrade must round-trip"
        );
    }

    #[test]
    fn a_session_lists_the_projects_it_was_linked_to_in_link_order() {
        let fixture = StoreFixture::new();
        let store = open_store(&fixture.store_path()).expect("store should open");
        let session = store
            .create_session("Kickoff")
            .expect("session should be created");
        let other = store
            .create_session("Unrelated session")
            .expect("session should be created");

        // Created brief-first and linked tool-first, to pin that link order — not project
        // creation order — drives the page.
        let brief = super::Project::draft("Hackathon Brief");
        store.insert_project(&brief).expect("project should insert");
        let tool = super::Project::draft("Capture Tool");
        store.insert_project(&tool).expect("project should insert");

        assert!(
            store
                .insert_session_project_link(&session.id, &tool.id)
                .expect("link should insert"),
            "a pair stated for the first time is a new membership"
        );
        assert!(
            store
                .insert_session_project_link(&session.id, &brief.id)
                .expect("link should insert")
        );
        assert!(
            store
                .insert_session_project_link(&other.id, &brief.id)
                .expect("link should insert")
        );
        assert!(
            !store
                .insert_session_project_link(&session.id, &tool.id)
                .expect("link should insert"),
            "relinking an existing pair states no new membership"
        );

        assert_eq!(
            store
                .list_session_projects(&session.id, 10)
                .expect("projects should list"),
            vec![tool.clone(), brief.clone()],
            "a session's projects list in the order their memberships were stated"
        );
        assert_eq!(
            store
                .list_session_projects(&session.id, 1)
                .expect("projects should list"),
            vec![tool],
            "the limit bounds the page"
        );
        assert_eq!(
            store
                .list_session_projects(&other.id, 10)
                .expect("projects should list"),
            vec![brief],
            "a link stays scoped to its own session"
        );
    }

    #[test]
    fn opening_a_version_nine_store_adds_projects_without_disturbing_its_rows() {
        let fixture = StoreFixture::new();
        let store_path = fixture.store_path();
        {
            // A store written before projects existed.
            let connection = Connection::open(&store_path).expect("store should open");
            for migration in &MIGRATIONS[..9] {
                connection
                    .execute_batch(migration)
                    .expect("schema should apply");
            }
            connection
                .execute_batch(
                    "INSERT INTO sessions (id, title, created_at_ms, updated_at_ms)
                     VALUES ('session-legacy', 'Before projects', 7, 7);",
                )
                .expect("legacy session should be inserted");
            connection
                .pragma_update(None, "user_version", 9)
                .expect("user_version should be writable");
        }

        let store = open_store(&store_path).expect("store should open");

        assert_eq!(user_version(&store_path), MIGRATIONS.len() as i64);
        assert_eq!(
            store
                .list_sessions(2)
                .expect("sessions should list")
                .first()
                .map(|session| session.title.clone()),
            Some("Before projects".to_owned()),
            "a session written before projects existed must survive untouched"
        );
        assert!(
            store
                .list_session_projects("session-legacy", 1)
                .expect("projects should list")
                .is_empty(),
            "a session written before projects existed must survive with no memberships"
        );
        let project = super::Project::draft("After the upgrade");
        store
            .insert_project(&project)
            .expect("project should insert");
        assert!(
            store
                .insert_session_project_link("session-legacy", &project.id)
                .expect("link should insert")
        );
        assert_eq!(
            store
                .list_session_projects("session-legacy", 2)
                .expect("projects should list"),
            vec![project],
            "a project linked after the upgrade must round-trip"
        );
    }

    #[test]
    fn task_packets_list_in_write_order_for_their_own_action() {
        let fixture = StoreFixture::new();
        let store = open_store(&fixture.store_path()).expect("store should open");
        let action = super::ActionRecord::draft(None, "custom", "Review PR 482");
        store.insert_action(&action).expect("action should insert");
        let other = super::ActionRecord::draft(None, "custom", "Unrelated");
        store.insert_action(&other).expect("action should insert");

        // Every packet is stamped with the same millisecond, so only the order the rows were
        // written in can decide the page. Minted brief-first and written revision-first, to pin
        // that write order — not the order the ids happened to be minted in — drives it.
        let mut brief = super::TaskPacket::draft(
            &action.id,
            1,
            serde_json::json!({"task_packet_version": 1, "action": {"objective": "Read it."}}),
        );
        brief.created_at_ms = 7;
        let mut revision = super::TaskPacket::draft(
            &action.id,
            1,
            serde_json::json!({"task_packet_version": 1, "revised": true}),
        );
        revision.created_at_ms = 7;
        let mut unrelated =
            super::TaskPacket::draft(&other.id, 1, serde_json::json!({"task_packet_version": 1}));
        unrelated.created_at_ms = 7;

        store
            .insert_task_packet(&revision)
            .expect("packet should insert");
        store
            .insert_task_packet(&brief)
            .expect("packet should insert");
        store
            .insert_task_packet(&unrelated)
            .expect("packet should insert");

        assert_eq!(
            store
                .list_task_packets(&action.id, 10)
                .expect("packets should list"),
            vec![revision.clone(), brief],
            "an action's packets list in the order they were written, documents intact"
        );
        assert_eq!(
            store
                .list_task_packets(&action.id, 1)
                .expect("packets should list"),
            vec![revision],
            "the limit bounds the page"
        );
        assert_eq!(
            store
                .list_task_packets(&other.id, 10)
                .expect("packets should list"),
            vec![unrelated],
            "a packet stays scoped to its own action"
        );
    }

    #[test]
    fn opening_a_version_ten_store_adds_task_packets_without_disturbing_its_rows() {
        let fixture = StoreFixture::new();
        let store_path = fixture.store_path();
        {
            // A store written before task packets existed.
            let connection = Connection::open(&store_path).expect("store should open");
            for migration in &MIGRATIONS[..10] {
                connection
                    .execute_batch(migration)
                    .expect("schema should apply");
            }
            connection
                .execute_batch(
                    "INSERT INTO actions (id, kind, title, status, created_at_ms, updated_at_ms)
                     VALUES ('action-legacy', 'custom', 'Before packets', 'draft', 7, 7);",
                )
                .expect("legacy action should be inserted");
            connection
                .pragma_update(None, "user_version", 10)
                .expect("user_version should be writable");
        }

        let store = open_store(&store_path).expect("store should open");

        assert_eq!(user_version(&store_path), MIGRATIONS.len() as i64);
        assert_eq!(
            store
                .list_actions(2)
                .expect("actions should list")
                .first()
                .map(|action| action.title.clone()),
            Some("Before packets".to_owned()),
            "an action written before task packets existed must survive untouched"
        );
        assert!(
            store
                .list_task_packets("action-legacy", 1)
                .expect("packets should list")
                .is_empty(),
            "an action written before task packets existed must survive with no packets"
        );
        let packet = super::TaskPacket::draft(
            "action-legacy",
            1,
            serde_json::json!({"task_packet_version": 1, "note": "After the upgrade"}),
        );
        store
            .insert_task_packet(&packet)
            .expect("packet should insert");
        assert_eq!(
            store
                .list_task_packets("action-legacy", 2)
                .expect("packets should list"),
            vec![packet],
            "a packet written after the upgrade must round-trip"
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
