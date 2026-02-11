# TODO

## Project-as-Environment Cleanup

- Remove legacy `environment` field from run models and runtime payloads in `src/run.rs`.
- Remove legacy `environment` persistence from SQLite and TSV history paths in `src/run_store.rs` and `src/history.rs`.
- Remove legacy `environment` field from template schema/TSV serialization in `src/job_template.rs`.
- Replace dashboard "Environment Breakdown" semantics with explicit project naming where needed in `src/ui.rs`.
- Add a one-time migration note for existing local run history so users understand compatibility and any data-shape changes.
