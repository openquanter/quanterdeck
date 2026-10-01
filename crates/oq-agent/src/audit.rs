//! The audit trail: every request that changed something, or tried to.
//!
//! One JSON line per entry, each carrying the hash of the one before, so
//! an edit or a deletion **in the middle** breaks the chain from there
//! on. A deletion at the end does not: what is left is a shorter trail
//! that still verifies, because there is nothing after it to disagree
//! with. What answers that is the copy posted off this host: every entry
//! goes to the alert channel carrying its number and the first twelve
//! characters of its hash, and the agent reads the channel back at
//! startup and reports an entry that is gone from here or no longer has
//! the hash that was posted.
//!
//! What that cannot see: whoever can write this trail can also post to
//! the channel, and could mirror a line to match. It catches the
//! tampering that did not think of the channel, and every accidental
//! truncation.
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

/// The bytes of a written line that its hash covers: the line with its
/// own `,"hash":"…"` cut out, and nothing else changed.
///
/// The hash is over these bytes and not over a re-serialisation of the
/// parsed line, because parsing forgets things a reader of the file
/// still sees. Two equal keys parse as the last of them, so a forged
/// `"reason"` slipped in before the real one left the parsed entry — and
/// a hash over it — exactly as it was, while `less` showed the forgery.
///
/// The writer emits compact JSON with sorted keys, so `hash` always
/// follows another key (`at_ms`) and is always preceded by a comma; were
/// keys ever kept in insertion order instead it would come last, still
/// after a comma. Cutting that segment out therefore gives back the very
/// body that was hashed, and trails written before this read raw bytes
/// verify unchanged. The segment must occur exactly once: the hash field
/// is the one part of a line its hash cannot cover, so a second copy of
/// it, or one somewhere the writer never puts it, is refused here.
fn hashed_bytes(line: &str, hash: &str) -> Option<String> {
    let segment = format!(",\"hash\":\"{hash}\"");
    let mut found = line.match_indices(&segment);
    let (Some((at, _)), None) = (found.next(), found.next()) else {
        return None;
    };
    Some(format!("{}{}", &line[..at], &line[at + segment.len()..]))
}

/// Whether the chain in `text` holds, and where it first breaks.
///
/// # Errors
/// The first line that does not follow, or was altered.
pub fn verify(text: &str) -> Result<(u64, String), String> {
    let mut prev = "genesis".to_string();
    let mut seq = 0;
    for (i, line) in text.lines().enumerate() {
        let v: Value = serde_json::from_str(line).map_err(|e| format!("line {}: {e}", i + 1))?;
        let hash = v
            .get("hash")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or(format!("line {}: no hash", i + 1))?;
        if v.get("prev").and_then(Value::as_str) != Some(prev.as_str()) {
            return Err(format!(
                "line {}: does not follow the line before it",
                i + 1
            ));
        }
        let altered = || format!("line {}: altered after it was written", i + 1);
        let body = hashed_bytes(line, &hash).ok_or_else(altered)?;
        if digest(&prev, &body) != hash {
            return Err(altered());
        }
        seq = v.get("seq").and_then(Value::as_u64).unwrap_or(seq);
        prev = hash;
    }
    Ok((seq, prev))
}

/// The newest audit entry a set of mirrored messages names, as
/// `(seq, short hash)`.
///
/// Every entry is posted off this host with its number and the first
/// twelve characters of its hash — that print is the anchor, and this is
/// the reader for it.
#[must_use]
pub fn anchored_in(bodies: &[String]) -> Option<(u64, String)> {
    let mut newest: Option<(u64, String)> = None;
    for body in bodies {
        let Some(rest) = body.split("审计 #").nth(1) else {
            continue;
        };
        let Some((digits, tail)) = rest.split_once(' ') else {
            continue;
        };
        let Ok(seq) = digits.parse::<u64>() else {
            continue;
        };
        let short: String = tail.chars().take_while(char::is_ascii_hexdigit).collect();
        if short.len() < 12 {
            continue;
        }
        if newest.as_ref().is_none_or(|(seen, _)| seq > *seen) {
            newest = Some((seq, short));
        }
    }
    newest
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

    /// Whether this trail still holds the entry the off-host copy names.
    ///
    /// `None` when it does. A reason otherwise: the entry is gone, or its
    /// hash is not the one that was posted — and neither is visible from
    /// the chain alone, which walks the entries that are there.
    #[must_use]
    pub fn anchor_mismatch(&self, seq: u64, short_hash: &str) -> Option<Said> {
        let text = std::fs::read_to_string(&self.path).unwrap_or_default();
        for line in text.lines() {
            let Ok(v) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            if v.get("seq").and_then(Value::as_u64) != Some(seq) {
                continue;
            }
            let hash = v.get("hash").and_then(Value::as_str).unwrap_or("");
            if !hash.starts_with(short_hash) {
                return Some(Said::new(
                    format!("审计的第 {seq} 条在本机被改过：哈希与告警频道里的不一致。"),
                    format!(
                        "audit entry {seq} was changed here: its hash is not the one in the \
                         alert channel"
                    ),
                ));
            }
            return None;
        }
        Some(Said::new(
            format!("审计的第 {seq} 条在本机的记录里不存在，但它在告警频道里。"),
            format!(
                "audit entry {seq} is not in this host's trail, but it is in the alert channel"
            ),
        ))
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
    /// The posted copy is the anchor, and this is the reader for it.
    /// Only the newest matters: an older one says nothing about what
    /// happened after it.
    #[test]
    fn the_newest_posted_entry_is_the_anchor() {
        let bodies: Vec<String> = [
            "deck：halt\n结果：halted\n审计 #1 aaaaaaaaaaaa",
            "deck：resume\n结果：resumed\n审计 #7 bbbbbbbbbbbb",
            "a message that is not an audit entry at all",
            "deck：go\n结果：went\n审计 #3 cccccccccccc",
        ]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
        assert_eq!(anchored_in(&bodies), Some((7, "bbbbbbbbbbbb".to_string())));
        assert_eq!(anchored_in(&[]), None);
        // A message that names a number but no hash is not an anchor.
        assert_eq!(anchored_in(&["审计 #9 ".to_string()]), None);
    }

    /// The deletion the chain cannot see: an entry gone from the end of
    /// the local trail, with the copy off this host still naming it.
    #[test]
    fn an_entry_the_channel_remembers_and_this_host_does_not_is_reported() {
        let dir = tempfile::tempdir().expect("dir");
        let mut a = Audit::open(dir.path()).expect("open");
        a.append(
            1,
            "deck:x",
            "halt",
            &Said::same("why"),
            &Said::same("halted"),
        )
        .expect("one");
        a.append(
            2,
            "deck:x",
            "resume",
            &Said::same("ok"),
            &Said::same("resumed"),
        )
        .expect("two");
        let second: String = a.tail(10)["entries"][1]["hash"]
            .as_str()
            .expect("hash")
            .chars()
            .take(12)
            .collect();
        assert_eq!(a.anchor_mismatch(2, &second), None, "it is here, unchanged");

        // The trail is cut back to the first entry: what is left
        // verifies, and only the anchor knows it used to be longer.
        let text = std::fs::read_to_string(dir.path().join("audit.log")).expect("read");
        let kept: String = text.lines().take(1).map(|l| format!("{l}\n")).collect();
        std::fs::write(dir.path().join("audit.log"), kept).expect("write");
        let a = Audit::open(dir.path()).expect("a shorter trail still opens");
        let why = a.anchor_mismatch(2, &second).expect("reported");
        assert!(why.en.contains("not in this host's trail"), "{}", why.en);

        // An entry that is here but not as posted is reported too.
        assert!(
            a.anchor_mismatch(1, "ffffffffffff").is_some(),
            "a different hash is a different entry"
        );
    }

    /// Two entries with the text a person might type as a reason: quotes,
    /// a backslash, Chinese, and the very bytes the verifier cuts out.
    fn a_trail_with_awkward_reasons(dir: &Path) -> String {
        let mut a = Audit::open(dir).expect("open");
        a.append(
            1,
            "deck:x",
            "halt",
            &Said::same(r#"he said "stop", path C:\oq"#),
            &Said::new("已停机", "halted"),
        )
        .expect("one");
        a.append(
            2,
            "deck:x",
            "resume",
            &Said::same(
                r#","hash":"0000000000000000000000000000000000000000000000000000000000000000""#,
            ),
            &Said::new("已恢复", "resumed"),
        )
        .expect("two");
        std::fs::read_to_string(dir.join("audit.log")).expect("read")
    }

    /// What the writer puts on disk verifies byte for byte: the hash sits
    /// between `at_ms` and `op` in a compact, key-sorted line, and cutting
    /// it out leaves exactly the body that was hashed. So a trail written
    /// before the verifier read raw bytes still opens, unmigrated.
    #[test]
    fn what_the_writer_wrote_verifies_from_its_bytes() {
        let dir = tempfile::tempdir().expect("dir");
        let text = a_trail_with_awkward_reasons(dir.path());
        let (seq, _) = verify(&text).expect("the writer's own trail verifies");
        assert_eq!(seq, 2);
        // The shape the cut depends on, checked rather than assumed.
        for line in text.lines() {
            let at = line.find(",\"hash\":\"").expect("a hash field");
            assert!(line[..at].ends_with(|c: char| c.is_ascii_digit()), "{line}");
            assert!(line[at..].starts_with(",\"hash\":\""), "{line}");
            assert_eq!(&line[at + 9 + 64..at + 9 + 64 + 7], "\",\"op\":", "{line}");
        }
    }

    /// The edit the old verifier could not see: a second `reason`, placed
    /// before the real one. A JSON parser keeps the last of two equal
    /// keys, so the parsed entry — and so a hash over its re-serialised
    /// form — is unchanged, while anyone reading the file with `less` or
    /// `grep` sees the forged one first.
    #[test]
    fn a_duplicate_key_breaks_the_chain() {
        let dir = tempfile::tempdir().expect("dir");
        let text = a_trail_with_awkward_reasons(dir.path());
        let forged = text.replacen(
            ",\"op\":\"halt\"",
            ",\"op\":\"halt\",\"reason\":\"routine\"",
            1,
        );
        assert_ne!(forged, text);
        // The parsed entry is the same: that is what made this invisible.
        let first = |t: &str| serde_json::from_str::<Value>(t.lines().next().unwrap()).unwrap();
        assert_eq!(first(&forged), first(&text));
        std::fs::write(dir.path().join("audit.log"), &forged).expect("forge");
        let error = Audit::open(dir.path()).expect_err("a forged trail is not continued");
        assert!(error.contains("line 1"), "{error}");
    }

    /// The hash field itself is not covered by the hash, so it is the one
    /// place a second copy could hide. Two of them is not a trail this
    /// agent wrote.
    #[test]
    fn a_second_hash_field_breaks_the_chain() {
        let dir = tempfile::tempdir().expect("dir");
        let text = a_trail_with_awkward_reasons(dir.path());
        let line = text.lines().next().expect("line");
        let at = line.find(",\"hash\":\"").expect("hash");
        let segment = &line[at..at + 9 + 64 + 1];
        let doubled = line.replacen(segment, &format!("{segment}{segment}"), 1);
        assert!(verify(&format!("{doubled}\n")).is_err());
        // Moved to the front, the one copy is not where the writer puts it.
        let moved = format!(
            "{{\"hash\":{},{}",
            &segment[8..],
            line[1..].replacen(segment, "", 1)
        );
        assert!(verify(&format!("{moved}\n")).is_err(), "{moved}");
    }

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
