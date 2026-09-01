1. MINOR — `list_runs` retains a pre-existing listability gap: `start_process` inserts without probing the list envelope, and terminal fields can enlarge a previously listable record until the single-record page is oversized and returns empty with `truncated:true`. Its fix must account for the worst-case terminal shape, so deferring it to a run-record follow-up is acceptable and does not block this PR.

The former MAJOR is closed for newly admitted markers, sources, and sessions. Each probe uses the exact generated record and ID before insertion; `truncated:false` is one byte larger than `true`; and multi-record truncation cannot drain a nonempty page because the remaining single record was already proven to fit. Tail records still require the previously recorded pagination follow-up.

Both recorded MINOR deferrals match the established sources-slice precedent and are acceptable. The remediation introduces no unintended regression: it is mutation-free before insertion and only rejects records that cannot fit their single-item list envelope.

VERDICT: PASS
