//! The host: systemd units, health, and the log directory.

use std::io::{Read, Seek, SeekFrom};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;
use std::process::Command;

use serde_json::{Value, json};

/// `systemctl show` for one unit, as a JSON object.
#[must_use]
pub fn unit_state(unit: &str) -> Value {
    let out = Command::new("systemctl")
        .args([
            "show",
            unit,
            "--property=ActiveState,SubState,Result,NRestarts,ExecMainStartTimestamp,\
             ExecMainPID,ExecMainStatus,UnitFileState,LoadState",
        ])
        .output();
    let mut obj = serde_json::Map::new();
    obj.insert("unit".into(), json!(unit));
    match out {
        Ok(o) if o.status.success() => {
            for line in String::from_utf8_lossy(&o.stdout).lines() {
                if let Some((k, v)) = line.split_once('=') {
                    obj.insert(k.to_string(), json!(v));
                }
            }
        }
        Ok(o) => {
            obj.insert(
                "error".into(),
                json!(String::from_utf8_lossy(&o.stderr).trim()),
            );
        }
        Err(e) => {
            obj.insert("error".into(), json!(e.to_string()));
        }
    }
    Value::Object(obj)
}

/// Whether a unit is running.
#[must_use]
pub fn is_active(unit: &str) -> bool {
    unit_state(unit).get("ActiveState").and_then(Value::as_str) == Some("active")
}

/// Start, stop or restart a unit. Polkit decides whether this user may.
///
/// # Errors
/// systemctl's own complaint.
pub fn unit_action(unit: &str, verb: &str) -> Result<(), String> {
    if !matches!(verb, "start" | "stop" | "restart") {
        return Err(format!("{verb} is not an action this agent takes"));
    }
    let out = Command::new("systemctl")
        .args(["--no-ask-password", verb, unit])
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Load, memory, disks, clock sync, uptime.
#[must_use]
pub fn host_health() -> Value {
    let loadavg = std::fs::read_to_string("/proc/loadavg").unwrap_or_default();
    let load: Vec<f64> = loadavg
        .split_whitespace()
        .take(3)
        .filter_map(|v| v.parse().ok())
        .collect();
    let meminfo = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let mem = |key: &str| {
        meminfo
            .lines()
            .find(|l| l.starts_with(key))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| v.parse::<u64>().ok())
            .map(|kb| kb * 1024)
    };
    let uptime = std::fs::read_to_string("/proc/uptime").ok().and_then(|u| {
        u.split_whitespace()
            .next()
            .and_then(|v| v.parse::<f64>().ok())
    });
    let disks: Vec<Value> = Command::new("df")
        .args(["-B1", "--output=target,size,used,avail", "/", "/var"])
        .output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .skip(1)
                .filter_map(|l| {
                    let f: Vec<&str> = l.split_whitespace().collect();
                    (f.len() == 4).then(|| {
                        json!({"mount": f[0], "size": f[1].parse::<u64>().ok(),
                               "used": f[2].parse::<u64>().ok(), "avail": f[3].parse::<u64>().ok()})
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let synced = Command::new("timedatectl")
        .args(["show", "-p", "NTPSynchronized", "--value"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "yes");
    json!({
        "load": load,
        "mem_total": mem("MemTotal:"),
        "mem_available": mem("MemAvailable:"),
        "uptime_s": uptime,
        "disks": disks,
        "clock_synced": synced,
        "now_ms": now_ms(),
    })
}

/// Unix milliseconds.
#[must_use]
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// The regular files in the log directory, newest first.
#[must_use]
pub fn log_files(dir: &Path) -> Value {
    let mut files: Vec<(i64, String, u64)> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .filter_map(|e| {
                    // symlink_metadata: a link is not followed and not listed.
                    let meta = std::fs::symlink_metadata(e.path()).ok()?;
                    meta.file_type().is_file().then(|| {
                        (
                            meta.mtime(),
                            e.file_name().to_string_lossy().to_string(),
                            meta.len(),
                        )
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    files.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    json!(
        files
            .into_iter()
            .map(|(mtime, name, size)| json!({"name": name, "mtime": mtime, "size": size}))
            .collect::<Vec<_>>()
    )
}

/// Longest tail read, in bytes, whatever was asked.
pub const MAX_TAIL_BYTES: u64 = 4 << 20;
/// Most lines returned.
pub const MAX_LINES: usize = 2000;

/// The last `lines` lines of `name` in `dir`, optionally only those
/// containing `grep`.
///
/// The name is one path component: no separators, no `..`. The file is
/// opened without following a link and must be a regular file, so nothing
/// placed in the directory can lead outside it or block the read.
///
/// # Errors
/// A name that is not a plain file name, or a file that is not a regular
/// file in the directory.
pub fn tail(dir: &Path, name: &str, lines: usize, grep: Option<&str>) -> Result<Value, String> {
    if name.is_empty() || name.contains('/') || name.contains('\\') || name == "." || name == ".." {
        return Err(format!("{name:?} is not a file name"));
    }
    let path = dir.join(name);
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(&path)
        .map_err(|e| format!("{name}: {e}"))?;
    let meta = file.metadata().map_err(|e| format!("{name}: {e}"))?;
    if !meta.file_type().is_file() {
        return Err(format!("{name} is not a regular file"));
    }
    let size = meta.len();
    let start = size.saturating_sub(MAX_TAIL_BYTES);
    file.seek(SeekFrom::Start(start))
        .map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    file.take(MAX_TAIL_BYTES)
        .read_to_end(&mut buf)
        .map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&buf);
    let mut all: Vec<&str> = text.lines().collect();
    if start > 0 && !all.is_empty() {
        // The first line was cut by where the read began.
        all.remove(0);
    }
    let filtered: Vec<&str> = match grep {
        Some(g) if !g.is_empty() => all.into_iter().filter(|l| l.contains(g)).collect(),
        _ => all,
    };
    let n = lines.clamp(1, MAX_LINES);
    let from = filtered.len().saturating_sub(n);
    Ok(json!({
        "name": name,
        "size": size,
        "truncated": start > 0,
        "lines": filtered[from..],
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tail_reads_the_end_and_filters() {
        let dir = tempfile::tempdir().expect("dir");
        let body: String = (0..50)
            .map(|i| format!("line {i}{}\n", if i % 10 == 0 { " HALT" } else { "" }))
            .collect();
        std::fs::write(dir.path().join("a.log"), body).expect("write");
        let t = tail(dir.path(), "a.log", 3, None).expect("tail");
        assert_eq!(t["lines"], json!(["line 47", "line 48", "line 49"]));
        let t = tail(dir.path(), "a.log", 10, Some("HALT")).expect("tail");
        assert_eq!(t["lines"].as_array().expect("lines").len(), 5);
    }

    #[test]
    fn names_that_leave_the_directory_or_links_are_refused() {
        let dir = tempfile::tempdir().expect("dir");
        let outside = tempfile::NamedTempFile::new().expect("outside");
        std::os::unix::fs::symlink(outside.path(), dir.path().join("link.log")).expect("link");
        for name in ["../etc/passwd", "/etc/passwd", "..", "", "a/b"] {
            assert!(tail(dir.path(), name, 5, None).is_err(), "{name}");
        }
        assert!(
            tail(dir.path(), "link.log", 5, None).is_err(),
            "a link is not followed"
        );
        std::fs::create_dir(dir.path().join("sub")).expect("sub");
        assert!(
            tail(dir.path(), "sub", 5, None).is_err(),
            "a directory is not a log"
        );
        let listed = log_files(dir.path());
        assert!(listed.as_array().expect("list").is_empty(), "{listed}");
    }
}
