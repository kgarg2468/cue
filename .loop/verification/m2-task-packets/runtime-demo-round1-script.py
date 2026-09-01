#!/usr/bin/env python3
"""Runtime demo: v10->v11 migration + task packets on the real binary."""
import json, socket, subprocess, sys, time, os, sqlite3, signal

BINARY, WORKDIR = sys.argv[1], sys.argv[2]
SOCK = os.path.join(WORKDIR, "demo.sock")
STORE = os.path.join(WORKDIR, "store.sqlite")

# The v10 store this demo migrates is built here, by hand, independently of the
# backend's MIGRATIONS array — so a breaking change smuggled into an early
# migration cannot stay self-consistent with this check.
V10_FIXTURE = """
CREATE TABLE sessions (id TEXT PRIMARY KEY, title TEXT NOT NULL, created_at_ms INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL);
ALTER TABLE sessions ADD COLUMN kind TEXT;
CREATE TABLE sources (id TEXT PRIMARY KEY, session_id TEXT NOT NULL REFERENCES sessions(id), start_ms INTEGER NOT NULL, end_ms INTEGER NOT NULL, speaker TEXT, text TEXT NOT NULL);
CREATE INDEX sources_by_session_and_start ON sources (session_id, start_ms, id);
CREATE TABLE runs (id TEXT PRIMARY KEY, run_id TEXT NOT NULL, session_id TEXT REFERENCES sessions(id), executable TEXT NOT NULL, status TEXT NOT NULL, exit_code INTEGER, error_code TEXT, started_at_ms INTEGER NOT NULL, ended_at_ms INTEGER);
CREATE INDEX runs_by_start ON runs (started_at_ms, id);
CREATE TABLE markers (id TEXT PRIMARY KEY, session_id TEXT NOT NULL REFERENCES sessions(id), at_ms INTEGER NOT NULL, kind TEXT NOT NULL, note TEXT);
CREATE INDEX markers_by_session_and_at ON markers (session_id, at_ms, id);
ALTER TABLE sessions ADD COLUMN note TEXT;
CREATE TABLE transcript_segments (id TEXT PRIMARY KEY, session_id TEXT NOT NULL REFERENCES sessions(id), start_ms INTEGER NOT NULL, end_ms INTEGER NOT NULL, speaker TEXT, text TEXT NOT NULL);
CREATE INDEX transcript_by_session_and_start ON transcript_segments (session_id, start_ms, id);
CREATE TABLE run_events (id TEXT PRIMARY KEY, record_id TEXT NOT NULL REFERENCES runs(id), seq INTEGER NOT NULL, at_ms INTEGER NOT NULL, kind TEXT NOT NULL);
CREATE INDEX run_events_by_record_and_seq ON run_events (record_id, seq);
CREATE TABLE actions (id TEXT PRIMARY KEY, session_id TEXT REFERENCES sessions(id), kind TEXT NOT NULL, title TEXT NOT NULL, status TEXT NOT NULL, created_at_ms INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL);
CREATE INDEX actions_by_creation ON actions (created_at_ms, id);
CREATE TABLE projects (id TEXT PRIMARY KEY, name TEXT NOT NULL, created_at_ms INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL);
CREATE TABLE session_projects (session_id TEXT NOT NULL REFERENCES sessions(id), project_id TEXT NOT NULL REFERENCES projects(id), linked_at_ms INTEGER NOT NULL, PRIMARY KEY (session_id, project_id));
CREATE INDEX session_projects_by_link ON session_projects (session_id, linked_at_ms, project_id);
INSERT INTO sessions (id, title, created_at_ms, updated_at_ms) VALUES ('session-legacy', 'Before packets', 7, 7);
INSERT INTO actions (id, kind, title, status, created_at_ms, updated_at_ms) VALUES ('action-legacy', 'custom', 'Before packets', 'draft', 7, 7);
PRAGMA user_version = 10;
"""

assert not os.path.exists(STORE), "demo needs a fresh workdir"
fixture = sqlite3.connect(STORE)
fixture.executescript(V10_FIXTURE)
fixture.close()
os.chmod(STORE, 0o600)

def start():
    p = subprocess.Popen([BINARY, "--socket", SOCK, "--store", STORE],
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    for _ in range(100):
        if os.path.exists(SOCK):
            try:
                exchange({"version": 1, "type": "health"})
                return p
            except OSError:
                pass
        time.sleep(0.05)
    raise RuntimeError("backend did not come up")

def stop(p):
    p.send_signal(signal.SIGTERM); p.wait(timeout=10)

def exchange(req):
    s = socket.socket(socket.AF_UNIX); s.settimeout(5); s.connect(SOCK)
    s.sendall((json.dumps(req) + "\n").encode())
    buf = b""
    while not buf.endswith(b"\n"):
        chunk = s.recv(65536)
        if not chunk: break
        buf += chunk
    s.close()
    return json.loads(buf)

def show(label, value):
    print(f"--- {label}")
    print(json.dumps(value, indent=1)[:700])

print("pre-migration user_version:",
      sqlite3.connect(STORE).execute("PRAGMA user_version").fetchone()[0])
p = start()
print("post-migration user_version:",
      sqlite3.connect(STORE).execute("PRAGMA user_version").fetchone()[0])

action = exchange({"version": 1, "type": "create_action",
    "kind": "review_pull_request", "title": "Review PR 482"})["action"]
body = {"task_packet_version": 1,
        "origin": {"session_id": "ses_legacy01"},
        "action": {"type": "review_pull_request",
                   "objective": "Check token refresh"},
        "execution": {"preferred_agent": "codex", "capability": "read"}}
show("packet created", exchange({"version": 1, "type": "create_task_packet",
    "action_id": action["id"], "body": body}))
show("version 2 rejected", exchange({"version": 1, "type": "create_task_packet",
    "action_id": action["id"], "body": {"task_packet_version": 2}}))
show("non-object body rejected", exchange({"version": 1, "type": "create_task_packet",
    "action_id": action["id"], "body": "not an object"}))
show("unknown action rejected", exchange({"version": 1, "type": "create_task_packet",
    "action_id": "action-missing", "body": {"task_packet_version": 1}}))

def exchange_raw(req):
    s = socket.socket(socket.AF_UNIX); s.settimeout(5); s.connect(SOCK)
    s.sendall((json.dumps(req) + "\n").encode())
    buf = b""
    while not buf.endswith(b"\n"):
        chunk = s.recv(65536)
        if not chunk: break
        buf += chunk
    s.close()
    return buf.decode()

WITNESS = 3.0700532959020438e+87  # shortest-repr float that drifts under best-effort parsing
LITERAL = json.dumps(WITNESS)
assert LITERAL == "3.0700532959020438e+87", LITERAL
fid = exchange({"version": 1, "type": "create_action",
    "kind": "custom", "title": "Float fidelity"})["action"]
created_raw = exchange_raw({"version": 1, "type": "create_task_packet",
    "action_id": fid["id"], "body": {"task_packet_version": 1, "measurement": WITNESS}})
assert LITERAL in created_raw, f"created wire text drifted: {created_raw}"

stop(p); p = start()
show("packets after restart", exchange({"version": 1, "type": "list_task_packets",
    "action_id": action["id"]}))
listed_raw = exchange_raw({"version": 1, "type": "list_task_packets",
    "action_id": fid["id"]})
assert LITERAL in listed_raw, f"listed wire text drifted: {listed_raw}"
listed = json.loads(listed_raw)["packets"][0]["body"]["measurement"]
assert listed == WITNESS, f"float drifted across restart: {listed!r} != {WITNESS!r}"
print("float fidelity across restart: OK", repr(listed))
print("store mode:", oct(os.stat(STORE).st_mode & 0o777))
stop(p)
print("DEMO OK")
