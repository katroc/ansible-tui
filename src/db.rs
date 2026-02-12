use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

const STORE_DIR: &str = ".ansible-tui";

pub fn open_db(path: &Path) -> io::Result<Connection> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path).map_err(sqlite_to_io)?;
    conn.execute_batch("PRAGMA journal_mode = WAL;")
        .map_err(sqlite_to_io)?;
    Ok(conn)
}

pub fn global_db_path(cwd: &Path) -> PathBuf {
    cwd.join(STORE_DIR).join("global.db")
}

pub fn project_db_path(project_root: &Path) -> PathBuf {
    project_root.join(STORE_DIR).join("project.db")
}

pub fn sqlite_to_io(err: rusqlite::Error) -> io::Error {
    io::Error::other(err.to_string())
}
