//! Files only the user can read, written whole or not at all.
use anyhow::Result;
use serde::Serialize;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

pub fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    let partial = path.with_extension(format!("{}.partial", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&partial)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&partial, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&partial);
    }
    result
}

pub fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    write_private(path, &bytes)
}

/// Removes a file; one already gone is fine.
pub fn remove(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
        _ => Ok(()),
    }
}

/// Unix milliseconds.
pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}
