//! Strategy configuration files: read, changed with a backup, rolled back.
//!
//! The files live in a directory this agent owns and the trader reads
//! through its group, so a change is made here or nowhere. Every write
//! keeps the previous version, is refused if the file moved since the
//! operator read it, and is audited by the caller. A change takes effect
//! when the trader restarts — saying so is the caller's job, not a
//! restart this module does behind the operator's back.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

fn sha(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A config file name: one component, `.json`, nothing clever.
fn valid(name: &str) -> bool {
    name.ends_with(".json")
        && name.len() <= 100
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

fn backups(dir: &Path) -> PathBuf {
    dir.join("backups")
}

/// The config files there are.
#[must_use]
pub fn list(dir: &Path) -> Value {
    let mut out: Vec<Value> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    let meta = std::fs::symlink_metadata(e.path()).ok()?;
                    (valid(&name) && meta.file_type().is_file()).then(|| {
                        let bytes = std::fs::read(e.path()).unwrap_or_default();
                        json!({"name": name, "size": meta.len(), "sha": sha(&bytes)})
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    json!(out)
}

fn read(dir: &Path, name: &str) -> Result<Vec<u8>, String> {
    if !valid(name) {
        return Err(format!("{name:?} is not a config name"));
    }
    let path = dir.join(name);
    let meta = std::fs::symlink_metadata(&path).map_err(|e| format!("{name}: {e}"))?;
    if !meta.file_type().is_file() {
        return Err(format!("{name} is not a regular file"));
    }
    std::fs::read(&path).map_err(|e| format!("{name}: {e}"))
}

/// One file, its hash, and its backups newest first; or one backup's
/// content when `backup` names one.
///
/// # Errors
/// A bad name, or a file or backup that is not there.
pub fn get(dir: &Path, name: &str, backup: Option<&str>) -> Result<Value, String> {
    let current = read(dir, name)?;
    let mut kept: Vec<Value> = std::fs::read_dir(backups(dir))
        .map(|rd| {
            rd.filter_map(Result::ok)
                .map(|e| e.file_name().to_string_lossy().to_string())
                .filter(|b| b.starts_with(&format!("{name}.")))
                .map(|b| {
                    let at = b.rsplit('.').next().and_then(|t| t.parse::<i64>().ok());
                    json!({"id": b, "at_ms": at})
                })
                .collect()
        })
        .unwrap_or_default();
    kept.sort_by(|a, b| b["at_ms"].as_i64().cmp(&a["at_ms"].as_i64()));
    let backup_content = match backup {
        Some(id) => {
            if !id.starts_with(&format!("{name}.")) || id.contains('/') || id.contains("..") {
                return Err(format!("{id:?} is not a backup of {name}"));
            }
            let bytes = std::fs::read(backups(dir).join(id)).map_err(|e| format!("{id}: {e}"))?;
            Some(String::from_utf8_lossy(&bytes).to_string())
        }
        None => None,
    };
    Ok(json!({
        "name": name,
        "content": String::from_utf8_lossy(&current),
        "sha": sha(&current),
        "backups": kept,
        "backup_content": backup_content,
    }))
}

/// Replace `name` with `content`, if it still hashes to `base_sha`.
///
/// # Errors
/// Not JSON, changed since it was read, or a write that failed — in which
/// case the file is as it was.
pub fn put(
    dir: &Path,
    name: &str,
    content: &str,
    base_sha: &str,
    now_ms: i64,
) -> Result<Value, String> {
    let current = read(dir, name)?;
    if sha(&current) != base_sha {
        return Err(
            "the file changed since you read it; reload, look again, and redo the change".into(),
        );
    }
    serde_json::from_str::<Value>(content).map_err(|e| format!("not valid JSON: {e}"))?;
    if content.as_bytes() == current.as_slice() {
        return Err("nothing changed".into());
    }
    std::fs::create_dir_all(backups(dir)).map_err(|e| e.to_string())?;
    let backup = format!("{name}.{now_ms}");
    std::fs::write(backups(dir).join(&backup), &current).map_err(|e| format!("backup: {e}"))?;
    let tmp = dir.join(format!(".{name}.new"));
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o640)
            .open(&tmp)
            .map_err(|e| e.to_string())?;
        f.write_all(content.as_bytes()).map_err(|e| e.to_string())?;
        f.sync_all().map_err(|e| e.to_string())?;
    }
    std::fs::rename(&tmp, dir.join(name)).map_err(|e| e.to_string())?;
    Ok(json!({
        "name": name, "backup": backup,
        "from_sha": sha(&current), "to_sha": sha(content.as_bytes()),
        "takes_effect": "when the trader restarts",
    }))
}

/// Put a backup's content back, keeping the current one as a backup too.
///
/// # Errors
/// As [`put`], or a backup that is not there.
pub fn rollback(
    dir: &Path,
    name: &str,
    backup: &str,
    base_sha: &str,
    now_ms: i64,
) -> Result<Value, String> {
    let got = get(dir, name, Some(backup))?;
    let content = got["backup_content"]
        .as_str()
        .ok_or("backup unreadable")?
        .to_string();
    put(dir, name, &content, base_sha, now_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_change_keeps_the_old_version_and_refuses_a_stale_base() {
        let dir = tempfile::tempdir().expect("dir");
        std::fs::write(dir.path().join("s.json"), r#"{"a":1}"#).expect("seed");
        let first = get(dir.path(), "s.json", None).expect("get");
        let base = first["sha"].as_str().expect("sha").to_string();

        assert!(
            put(dir.path(), "s.json", "{not json", &base, 1)
                .unwrap_err()
                .contains("JSON")
        );
        let done = put(dir.path(), "s.json", r#"{"a":2}"#, &base, 10).expect("put");
        assert_eq!(done["backup"], "s.json.10");
        assert!(
            put(dir.path(), "s.json", r#"{"a":3}"#, &base, 11)
                .unwrap_err()
                .contains("changed since")
        );

        let now = get(dir.path(), "s.json", Some("s.json.10")).expect("get");
        assert_eq!(now["content"], r#"{"a":2}"#);
        assert_eq!(now["backup_content"], r#"{"a":1}"#);
        let back = rollback(
            dir.path(),
            "s.json",
            "s.json.10",
            now["sha"].as_str().expect("sha"),
            20,
        )
        .expect("rollback");
        assert_eq!(back["backup"], "s.json.20");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("s.json")).expect("read"),
            r#"{"a":1}"#
        );
    }

    #[test]
    fn names_are_plain_json_files_in_the_directory() {
        let dir = tempfile::tempdir().expect("dir");
        for name in ["../x.json", "x", ".x.json", "a/b.json"] {
            assert!(get(dir.path(), name, None).is_err(), "{name}");
        }
        std::fs::write(dir.path().join("s.json"), "{}").expect("seed");
        assert!(get(dir.path(), "s.json", Some("../../etc/passwd")).is_err());
    }
}
