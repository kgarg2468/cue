# Cycle 17 resource-safety reviews

## Cross-family review — Opus 4.8

Verdict: **PASS**. No BLOCKER or MAJOR was introduced by the four-file slice.

The review confirmed atomic closed-first stdin-cap admission, compatibility of
one-shot callers, conservative PID liveness, private managed-root deletion
guards, startup ordering, bounded Git commands, typed Swift decoding, and the
real-runtime coverage. It recorded only non-blocking observations:

1. A reused `send_input` connection retains the ordinary 250 ms per-frame idle
   deadline; reuse is therefore for immediate retry, not human-paced input.
2. A client can hold one of eight bounded client slots while actively reusing a
   connection.
3. The reaped-PID test helper has a theoretical PID-reuse flake window.
4. Best-effort sidecar creation fails safely toward a leak and is silent.
5. Adding a public Swift enum case can affect exhaustive downstream switches.
6. Authenticated crash recovery intentionally mutates the owning Git repo.

## Initial Sol final review

Verdict: **CHANGES_REQUIRED** for one confirmed MAJOR.

The original `recover_worktree_repository` trusted the absolute Git admin path
in a candidate `.git` after checking only its lexical
`<repo>/.git/worktrees/<id>` shape. A forged managed-root candidate could point
to a stale worktree admin entry from another repository, after which cleanup
could prune and delete that foreign repository's managed-prefix branch.

## Fix and regression

Recovery now returns a repository only when the candidate `.git` is a real,
non-symlink regular file, the named admin path is absolute/canonical with the
expected structure, and the admin's own `gitdir` backlink is an absolute,
canonical path equal to that exact candidate `.git`. Failure returns `None`, so
only the guarded candidate-directory fallback runs and no Git command is built.

`orphan_cleanup_rejects_forged_repository_metadata` creates a real foreign
managed worktree/branch, copies its pointer into a dead-owned forged candidate,
and proves the candidate is removed while the foreign branch/admin registration
remain. Before the production fix, this test failed because the foreign branch
was actually deleted; after the fix it passes.

## Post-fix Sol disposition review

Verdict: **PASS**.

The reviewer re-read the original task, complete current diff, initial finding,
fix report, source, regression, targeted logs, and fresh parity log. It found the
prior MAJOR fully resolved and no new BLOCKER or MAJOR. It verified that every
authentication failure stays on the rechecked candidate-only deletion path,
while authentic valid recovery remains covered. Residual same-user TOCTOU, PID
reuse, per-command startup delay, and conservative leak cases were explicitly
classified as non-blocking within the specified private-root threat boundary.
