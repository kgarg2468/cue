#!/usr/bin/env python3
"""Runtime demo: v11->v12 migration + durable §14 audit events on the real binary."""
import json, socket, subprocess, sys, time, os, sqlite3, signal

BINARY = sys.argv[1]
WORKDIR = sys.argv[2]
SOCK = os.path.join(WORKDIR, "demo.sock")
STORE = os.path.join(WORKDIR, "store.sqlite")

# The v11 store this demo migrates is built here, by hand, independently of the
# backend's MIGRATIONS array — so a breaking change smuggled into an early
# migration cannot stay self-consistent with this check.
V11_FIXTURE = """
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
CREATE TABLE task_packets (id TEXT PRIMARY KEY, action_id TEXT NOT NULL REFERENCES actions(id), packet_version INTEGER NOT NULL, body TEXT NOT NULL, created_at_ms INTEGER NOT NULL);
CREATE INDEX task_packets_by_action ON task_packets (action_id, created_at_ms, id);
INSERT INTO runs (id, run_id, executable, status, exit_code, started_at_ms, ended_at_ms)
    VALUES ('run-legacy', 'legacy', '/usr/bin/true', 'exited', 0, 7, 8);
PRAGMA user_version = 11;
"""

assert not os.path.exists(STORE), "demo needs a fresh workdir"
fixture = sqlite3.connect(STORE)
fixture.executescript(V11_FIXTURE)
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

def stop(p, sig=signal.SIGTERM):
    p.send_signal(sig)
    try:
        p.wait(timeout=10)
    except subprocess.TimeoutExpired:
        p.kill()

def exchange(req, timeout=15):
    s = socket.socket(socket.AF_UNIX)
    s.settimeout(timeout)
    s.connect(SOCK)
    s.sendall((json.dumps(req) + "\n").encode())
    buf = b""
    while not buf.endswith(b"\n"):
        chunk = s.recv(65536)
        if not chunk:
            break
        buf += chunk
    s.close()
    return json.loads(buf.split(b"\n")[0])

def stream_run(req):
    s = socket.socket(socket.AF_UNIX)
    s.settimeout(20)
    s.connect(SOCK)
    s.sendall((json.dumps(req) + "\n").encode())
    f = s.makefile()
    for line in f:
        frame = json.loads(line)
        if frame.get("type") in ("run_exit", "error"):
            s.close()
            return frame

def show(label, value):
    print(f"--- {label}")
    print(json.dumps(value, indent=1)[:700])

AUDIT_KINDS = ["authorizer", "source", "packet_version", "permission_granted",
               "file_accessed", "service_accessed", "permission_requested",
               "user_response", "artifact", "final_status"]

print("pre-migration user_version:",
      sqlite3.connect(STORE).execute("PRAGMA user_version").fetchone()[0])
p = start()
print("post-migration user_version:",
      sqlite3.connect(STORE).execute("PRAGMA user_version").fetchone()[0])

# A run written before audit events existed gains a trail after the upgrade.
show("legacy run's empty trail", exchange({"version": 1,
    "type": "list_audit_events", "record_id": "run-legacy"}))
legacy_event = exchange({"version": 1, "type": "record_audit_event",
    "record_id": "run-legacy", "kind": "final_status", "detail": "exited 0"})
assert legacy_event["event"]["seq"] == 0, legacy_event

# A fresh run records the full §14 trail; the backend numbers it densely.
exit_frame = stream_run({"version": 1, "type": "start_process", "run_id": "audited",
    "executable": "/usr/bin/true", "arguments": [], "timeout_milliseconds": 5000})
print("run exit_code:", exit_frame.get("exit_code"))
runs = exchange({"version": 1, "type": "list_runs"})["runs"]
rec = next(r for r in runs if r["run_id"] == "audited")
for kind in AUDIT_KINDS:
    event = exchange({"version": 1, "type": "record_audit_event",
        "record_id": rec["id"], "kind": kind, "detail": f"demo {kind}"})
    assert event["type"] == "record_audit_event_response", event

# A forged sequence and stamp never leak into the trail.
forged = exchange({"version": 1, "type": "record_audit_event",
    "record_id": rec["id"], "kind": "user_response", "detail": "approved",
    "seq": 99, "at_ms": 7})
assert forged["event"]["seq"] == len(AUDIT_KINDS), forged
assert forged["event"]["at_ms"] > 7, forged
print("forged seq/at_ms ignored: OK")

show("unknown kind rejected", exchange({"version": 1, "type": "record_audit_event",
    "record_id": rec["id"], "kind": "custom", "detail": "x"}))
show("unknown run rejected", exchange({"version": 1, "type": "record_audit_event",
    "record_id": "run-nope", "kind": "authorizer", "detail": "x"}))
show("blank detail rejected", exchange({"version": 1, "type": "record_audit_event",
    "record_id": rec["id"], "kind": "authorizer", "detail": "  "}))

stop(p)
p = start()
page = exchange({"version": 1, "type": "list_audit_events", "record_id": rec["id"]})
events = page["events"]
assert [e["kind"] for e in events][:10] == AUDIT_KINDS, events
assert [e["seq"] for e in events] == list(range(len(events))), events
show("trail after restart (head)", {"count": len(events), "first": events[0],
    "last": events[-1], "truncated": page["truncated"]})
legacy = exchange({"version": 1, "type": "list_audit_events",
    "record_id": "run-legacy"})["events"]
assert len(legacy) == 1 and legacy[0]["detail"] == "exited 0", legacy
print("legacy trail after restart: OK")
print("store mode:", oct(os.stat(STORE).st_mode & 0o777))
stop(p)
print("DEMO OK")
