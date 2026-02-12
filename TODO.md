# TODO

## Project-as-Environment Cleanup

- [x] Remove legacy `environment` field from run models and runtime payloads in `src/run.rs`.
- [x] Remove legacy `environment` persistence from SQLite and TSV history paths in `src/run_store.rs` and `src/history.rs`.
- [x] Remove legacy `environment` field from template schema/TSV serialization in `src/job_template.rs`.
- [x] Replace dashboard "Environment Breakdown" semantics with explicit project naming where needed in `src/ui.rs`.
- [x] Add a one-time migration note for existing local run history so users understand compatibility and any data-shape changes.
