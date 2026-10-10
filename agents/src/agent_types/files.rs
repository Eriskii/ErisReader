//! The agents folder, read under ErisAE's rules for `.ae/agents`: definitions are regular,
//! singly linked files, never followed through symlinks, and each read is bounded.
use anyhow::{Result, anyhow, bail, ensure};
use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

pub struct AgentFiles(PathBuf);

impl AgentFiles {
    /// The folder at `path`, created if missing.
    pub fn prepare(path: &Path) -> Result<Self> {
        fs::create_dir_all(path).map_err(|e| anyhow!("unable to create the agents folder: {e}"))?;
        Ok(Self(path.to_owned()))
    }

    #[cfg(test)]
    pub fn path(&self) -> &Path {
        &self.0
    }

    /// The names in the folder ending in `suffix`, sorted; more than `limit` is an error.
    pub fn names_with_suffix(&self, suffix: &str, limit: usize) -> Result<Vec<String>> {
        let mut names = Vec::new();
        for entry in fs::read_dir(&self.0)? {
            let name = entry?.file_name();
            let Some(name) = name.to_str() else { continue };
            if name.ends_with(suffix) {
                ensure!(names.len() < limit, "too many {suffix} files; maximum is {limit}");
                names.push(name.to_owned());
            }
        }
        names.sort();
        Ok(names)
    }

    /// A file's bytes, at most `limit` of them, or `None` when it does not exist.
    pub fn read_limit(&self, name: &str, limit: u64) -> Result<Option<Vec<u8>>> {
        let file = match self.open(name, OpenOptions::new().read(true)) {
            Ok(file) => file,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => bail!("unable to read agents/{name}: {error}"),
        };
        let metadata = file.metadata()?;
        ensure!(metadata.is_file() && metadata.nlink() == 1, "agents/{name} must be a regular, singly linked file");
        let mut bytes = Vec::new();
        file.take(limit + 1).read_to_end(&mut bytes)?;
        ensure!(bytes.len() as u64 <= limit, "agents/{name} exceeds {} KiB", limit / 1024);
        Ok(Some(bytes))
    }

    /// Writes a file whole, replacing what is there.
    #[cfg(test)]
    pub fn write(&self, name: &str, bytes: &[u8]) -> Result<()> {
        self.commit(name, bytes, true).map(|_| ())
    }

    /// Writes a file whole unless one exists; whether it wrote.
    pub fn write_if_missing(&self, name: &str, bytes: &[u8]) -> Result<bool> {
        self.commit(name, bytes, false)
    }

    fn commit(&self, name: &str, bytes: &[u8], replace: bool) -> Result<bool> {
        let target = self.0.join(component(name)?);
        let temporary = self.0.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| {
            let mut file = OpenOptions::new().write(true).create_new(true).mode(0o600).open(&temporary)?;
            file.write_all(bytes)?;
            file.sync_all()?;
            if replace {
                fs::rename(&temporary, &target)?;
                return Ok(true);
            }
            // A hard link fails where anything, a symlink included, already has the name.
            match fs::hard_link(&temporary, &target) {
                Ok(()) => Ok(true),
                Err(error) if error.kind() == ErrorKind::AlreadyExists => Ok(false),
                Err(error) => Err(anyhow::Error::from(error)),
            }
        })();
        let _ = fs::remove_file(&temporary);
        result
    }

    /// Opens a file in the folder without following a symlink, and without blocking on a FIFO.
    fn open(&self, name: &str, options: &mut OpenOptions) -> std::io::Result<File> {
        let name = component(name).map_err(|e| std::io::Error::new(ErrorKind::InvalidInput, e.to_string()))?;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC).open(self.0.join(name))
    }
}

fn component(name: &str) -> Result<&str> {
    ensure!(
        !(name.is_empty() || name == "." || name == ".." || name.contains('/')),
        "invalid agent file name {name:?}"
    );
    Ok(name)
}
