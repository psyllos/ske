use serde_json::{json, Value};
use ske_common::{Error, Result};
use std::{
    io::{IsTerminal, Read, Write},
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

/// Consent is scoped to the absolute repository and adapter content hash.
pub fn ensure(db: &Path, repo: &Path, adapter: &Path, yes: bool) -> Result<()> {
    let usage = |e: std::io::Error| Error::new("E_USAGE", e.to_string());
    let repo = repo.canonicalize().map_err(usage)?;
    let adapter = adapter.canonicalize().map_err(usage)?;
    if !adapter.is_file() {
        return Err(Error::new("E_USAGE", "adapter must be a regular file"));
    }
    let version =
        ske_common::sha256_reader(std::fs::File::open(&adapter).map_err(usage)?).map_err(usage)?;
    let dir = db
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .join(".ske");
    let path = dir.join("consent.json");
    if std::fs::symlink_metadata(&dir).is_ok_and(|m| m.file_type().is_symlink())
        || std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink())
    {
        return Err(Error::new("E_USAGE", "consent path must not be a symlink"));
    }
    std::fs::create_dir_all(&dir).map_err(usage)?;
    let lock = dir.join("consent.lock");
    let _file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock)
        .map_err(|_| {
            Error::new(
                "E_USAGE",
                "consent store locked; retry after checking consent.lock",
            )
        })?;
    struct Lock(std::path::PathBuf);
    impl Drop for Lock {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let _lock = Lock(lock);
    let read = || -> std::io::Result<Vec<u8>> {
        if !std::fs::metadata(&path)?.is_file() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "consent store must be a regular file",
            ));
        }
        let mut bytes = Vec::new();
        std::fs::File::open(&path)?
            .take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        Ok(bytes)
    };
    let mut entries: Vec<Value> = match read() {
        Ok(bytes) if bytes.len() > 1024 * 1024 => {
            return Err(Error::new("E_USAGE", "consent store exceeds 1 MiB"))
        }
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|e| Error::new("E_USAGE", format!("invalid consent.json: {e}")))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(usage(e)),
    };
    if entries.iter().any(|entry| {
        entry["repository"] == repo.to_string_lossy().as_ref()
            && entry["adapter"] == adapter.to_string_lossy().as_ref()
            && entry["version"] == version
    }) {
        return Ok(());
    }
    if !yes {
        if !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
            return Err(Error::new(
                "E_USAGE",
                "adapter execution needs --yes-run-adapter without a terminal",
            ));
        }
        eprint!(
            "Run adapter {} on {}? [y/N] ",
            adapter.display(),
            repo.display()
        );
        std::io::stderr().flush().map_err(usage)?;
        let mut answer = String::new();
        std::io::stdin().read_line(&mut answer).map_err(usage)?;
        if !matches!(answer.trim(), "y" | "Y" | "yes") {
            return Err(Error::new("E_USAGE", "adapter execution declined"));
        }
    }
    entries.push(json!({"repository":repo,"adapter":adapter,"version":version,
        "date":SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| Error::new("E_USAGE",e.to_string()))?.as_secs()}));
    let bytes =
        serde_json::to_vec_pretty(&entries).map_err(|e| Error::new("E_USAGE", e.to_string()))?;
    if bytes.len() > 1024 * 1024 {
        return Err(Error::new("E_USAGE", "consent store exceeds 1 MiB"));
    }
    let temp = dir.join(format!(
        "consent.{}.{}.tmp",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| Error::new("E_USAGE", e.to_string()))?
            .as_nanos()
    ));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| {
        let mut file = options.open(&temp).map_err(usage)?;
        file.write_all(&bytes).map_err(usage)?;
        file.sync_all().map_err(usage)?;
        std::fs::rename(&temp, &path).map_err(usage)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consent_is_scoped_to_repository_adapter_path_and_content() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir(&repo).unwrap();
        let adapter = dir.path().join("rust-adapter");
        std::fs::write(&adapter, b"version one").unwrap();
        let db = dir.path().join("db");
        ensure(&db, &repo, &adapter, true).unwrap();
        ensure(&db, &repo, &adapter, true).unwrap();
        let entries = || -> Vec<Value> {
            serde_json::from_slice(&std::fs::read(dir.path().join(".ske/consent.json")).unwrap())
                .unwrap()
        };
        assert_eq!(entries().len(), 1);
        std::fs::write(&adapter, b"version two").unwrap();
        ensure(&db, &repo, &adapter, true).unwrap();
        assert_eq!(entries().len(), 2);
        let java = dir.path().join("java-adapter");
        std::fs::copy(&adapter, &java).unwrap();
        ensure(&db, &repo, &java, true).unwrap();
        assert_eq!(entries().len(), 3);
        assert_ne!(entries()[0]["version"], entries()[1]["version"]);
    }
}
