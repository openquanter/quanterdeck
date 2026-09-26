//! The audit trail: every request that changed something, or tried to.
//!
//! One JSON line per entry, each carrying the hash of the one before, so
//! an edit or a deletion **in the middle** breaks the chain from there
//! on. A deletion at the end does not: what is left is a shorter trail
//! that still verifies, because there is nothing after it to disagree
//! with. What answers that is the copy posted off this host — every
//! entry goes to the alert channel — and nothing reads that copy back
//! yet, so the end of the trail is currently worth what the host is.
//!
//! It lives in the agent's own directory, which the deck cannot write: a
//! trail kept only where an intruder is has only their word for it.

use std::io::Write;
use std::path::{Path, PathBuf};

use oq_deck_core::lang::Said;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[derive(Debug)]
pub struct Audit {
    path: PathBuf,
    seq: u64,
    prev: String,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The hash an entry carries: of the previous hash and the entry without
/// its own hash, as written.
fn digest(prev: &str, body: &str) -> String {
    let mut h = Sha256::new();
    h.update(prev.as_bytes());
    h.update(b"\n");
    h.update(body.as_bytes());
    hex(&h.finalize())
}

/// Whether the chain in `text` holds, and where it first breaks.
///
/// # Errors
/// The first line that does not follow, or was altered.
pub fn verify(text: &str) -> Result<(u64, String), String> {
    let mut prev = "genesis".to_string();
    let mut seq = 0;
    for (i, line) in text.lines().enumerate() {
        let mut v: Value =
            serde_json::from_str(line).map_err(|e| format!("line {}: {e}", i + 1))?;
        let hash = v
            .as_object_mut()
            .and_then(|o| o.remove("hash"))
            .and_then(|h| h.as_str().map(str::to_string))
            .ok_or(format!("line {}: no hash", i + 1))?;
        if v.get("prev").and_then(Value::as_str) != Some(prev.as_str()) {
            return Err(format!(
                "line {}: does not follow the line before it",
                i + 1
            ));
        }
        let body = serde_json::to_string(&v).map_err(|e| e.to_string())?;
        if digest(&prev, &body) != hash {
            return Err(format!("line {}: altered after it was written", i + 1));
        }
        seq = v.get("seq").and_then(Value::as_u64).unwrap_or(seq);
        prev = hash;
    }
    Ok((seq, prev))
}

impl Audit {
    /// Open the trail in `dir`, continuing its chain.
    ///
    /// # Errors
    /// A trail whose chain does not hold: appending to it would bless
    /// whatever was done to it.
    pub fn open(dir: &Path) -> Result<Self, String> {
        let path = dir.join("audit.log");
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let (seq, prev) = verify(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(Self { path, seq, prev })
    }

    /// Append one entry and return it.
    ///
    /// Both sentences a reader sees are recorded in both languages, and
    /// both are inside the hash: the trail is read long after the night it
    /// was written, and a reader should not need the language of whoever
    /// was on call. What a person typed as the reason goes in the same
    /// both ways — it is their words, not a translation of them.
    ///
    /// # Errors
    /// The write failed. The caller refuses the request rather than acting
    /// unrecorded.
    pub fn append(
        &mut self,
        at_ms: i64,
        actor: &str,
        op: &str,
        reason: &Said,
        result: &Said,
    ) -> Result<Value, String> {
        let seq = self.seq + 1;
        let entry = json!({
            "seq": seq, "at_ms": at_ms, "actor": actor, "op": op,
            "reason": reason.zh, "reason_en": reason.en,
            "result": result.zh, "result_en": result.en,
            "prev": self.prev,
        });
        let body = serde_json::to_string(&entry).map_err(|e| e.to_string())?;
        let hash = digest(&self.prev, &body);
        let mut full = entry;
        full["hash"] = json!(hash);
        let line = serde_json::to_string(&full).map_err(|e| e.to_string())?;
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|e| format!("{}: {e}", self.path.display()))?;
        writeln!(f, "{line}").map_err(|e| e.to_string())?;
        f.sync_data().map_err(|e| e.to_string())?;
        self.seq = seq;
        self.prev = hash;
        Ok(full)
    }

    /// The last `n` entries and whether the whole chain still holds.
    #[must_use]
    pub fn tail(&self, n: usize) -> Value {
        let text = std::fs::read_to_string(&self.path).unwrap_or_default();
        let chain = match verify(&text) {
            Ok(_) => json!({"intact": true}),
            Err(e) => json!({"intact": false, "problem": e}),
        };
        let lines: Vec<Value> = text
            .lines()
            .rev()
            .take(n.clamp(1, 1000))
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        json!({"chain": chain, "entries": lines})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_chain_continues_across_opens_and_catches_an_edit() {
        let dir = tempfile::tempdir().expect("dir");
        let mut a = Audit::open(dir.path()).expect("open");
        a.append(
            1,
            "deck:x",
            "halt",
            &Said::same("why"),
            &Said::new("已停机", "halted"),
        )
        .expect("one");
        drop(a);
        let mut a = Audit::open(dir.path()).expect("reopen");
        a.append(
            2,
            "deck:x",
            "resume",
            &Said::same("ok now"),
            &Said::new("已恢复", "resumed"),
        )
        .expect("two");
        // Both renderings are in the entry, and so inside the hash: the
        // trail reads in either language without being rewritten.
        let first = a.tail(10)["entries"][0].clone();
        assert_eq!(first["result"], "已停机");
        assert_eq!(first["result_en"], "halted");
        // A person's own reason is not translated, and says so.
        assert_eq!(first["reason"], first["reason_en"]);
        assert_eq!(a.tail(10)["chain"]["intact"], true);
        assert_eq!(a.tail(10)["entries"].as_array().expect("entries").len(), 2);

        let path = dir.path().join("audit.log");
        let text = std::fs::read_to_string(&path).expect("read");
        std::fs::write(&path, text.replacen("\"why\"", "\"nothing to see\"", 1)).expect("edit");
        assert!(
            Audit::open(dir.path()).is_err(),
            "an edited trail is not continued"
        );

        let second = text.lines().nth(1).expect("line").to_string();
        std::fs::write(&path, second + "\n").expect("drop the first");
        assert!(
            Audit::open(dir.path()).is_err(),
            "a deleted entry breaks the chain"
        );
    }

    /// A trail written before an entry carried English verifies, and can be
    /// continued: the hash is over the bytes the entry has, not over the
    /// shape the code happens to write now. A host's trail is years long,
    /// and one that stopped verifying at an upgrade would be read as an
    /// incident rather than as a change of format.
    #[test]
    fn an_entry_with_one_language_still_verifies() {
        let dir = tempfile::tempdir().expect("dir");
        // The entry as it was written before there were two: `reason` and
        // `result` alone, hashed the same way.
        let old = json!({
            "seq": 1, "at_ms": 7, "actor": "deck:x", "op": "halt",
            "reason": "检查时钟同步", "result": "已完成", "prev": "genesis",
        });
        let body = serde_json::to_string(&old).expect("body");
        let mut line = old;
        line["hash"] = json!(digest("genesis", &body));
        std::fs::write(
            dir.path().join("audit.log"),
            format!("{}\n", serde_json::to_string(&line).expect("line")),
        )
        .expect("write");

        let mut a = Audit::open(dir.path()).expect("an older trail opens");
        assert_eq!(a.seq, 1);
        a.append(
            2,
            "deck:x",
            "resume",
            &Said::same("好了"),
            &Said::new("已恢复", "resumed"),
        )
        .expect("and takes a new entry");
        let t = a.tail(10);
        assert_eq!(t["chain"]["intact"], true, "{t}");
        assert_eq!(t["entries"][0]["result"], "已完成");
        // The older entry has one rendering, and reads back as it was left.
        assert!(t["entries"][0].get("result_en").is_none());
    }
}
