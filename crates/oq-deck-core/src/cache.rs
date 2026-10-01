//! Remembering what a file parsed to, for as long as the file is the same.
//!
//! A listing reads every file in its directory, and the journal listing
//! replays every journal in full to say what each one believed. On a
//! directory of a thousand runs, or of a few long journals, that is the
//! whole cost of the page, paid again on every refresh for files that
//! have not changed since the last one.
//!
//! The cache does not change what a listing says, only how often it is
//! worked out:
//!
//! * A file is the same file only while its device, inode, length,
//!   modification time and change time all are. The change time is the
//!   one no ordinary tool can set back, so a rewrite that restores the
//!   old modification time (`cp -p`, `rsync -t`) still reads as a change,
//!   and so does a `chmod`. A file replaced by a rename is a new inode.
//! * A file modified within the last [`RACY`] is parsed but not kept. A
//!   timestamp is only as fine as the filesystem's clock; a same-length
//!   rewrite inside one tick of it would otherwise keep the old stamp and
//!   the old answer. This is git's "racily clean" rule.
//! * Only a successful parse is kept. A file that would not read is read
//!   again next time, so a transient failure is not remembered as a fact,
//!   and an unreadable file is listed with its reason exactly as before.
//! * The file is examined *before* it is read. A write between the two
//!   leaves an older stamp beside newer content, which only means the
//!   next listing parses it again; the other order could keep a stale
//!   answer under a fresh stamp.
//! * A listing drops every entry it did not see, so the cache holds at
//!   most what the directory holds and a removed file is forgotten.
//!
//! The console is an observer: nothing here writes to the directories it
//! reads.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, SystemTime};

/// How recently a file may have been modified and still be kept.
pub const RACY: Duration = Duration::from_secs(2);

/// What identifies one version of a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stamp {
    dev: u64,
    ino: u64,
    len: u64,
    mtime: (i64, i64),
    ctime: (i64, i64),
}

impl Stamp {
    fn of(meta: &fs::Metadata) -> Self {
        Self {
            dev: meta.dev(),
            ino: meta.ino(),
            len: meta.len(),
            mtime: (meta.mtime(), meta.mtime_nsec()),
            ctime: (meta.ctime(), meta.ctime_nsec()),
        }
    }
}

/// Whether a file was modified too recently for its stamp to be trusted.
///
/// A modification time in the future counts as recent: a clock that
/// disagrees with the filesystem is not one to reason about.
fn racy(meta: &fs::Metadata) -> bool {
    meta.modified().map_or(true, |modified| {
        SystemTime::now()
            .duration_since(modified)
            .map_or(true, |age| age < RACY)
    })
}

/// Parsed files, keyed by path and valid for one version of each.
#[derive(Debug)]
pub struct ParseCache<T> {
    entries: Mutex<HashMap<PathBuf, (Stamp, T)>>,
}

impl<T> Default for ParseCache<T> {
    fn default() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
        }
    }
}

impl<T: Clone> ParseCache<T> {
    /// What `path` parses to, from the cache while the file is unchanged.
    ///
    /// The lock is not held while parsing: two listings at once may both
    /// parse a file, which costs time, not correctness.
    ///
    /// # Errors
    /// Whatever `parse` returns, which is never kept.
    pub fn get_or_parse(
        &self,
        path: &Path,
        parse: impl FnOnce(&Path) -> Result<T, String>,
    ) -> Result<T, String> {
        // A file that cannot be examined is not cached, and is left to
        // `parse` to fail on with its own words.
        let Ok(meta) = fs::metadata(path) else {
            return parse(path);
        };
        let stamp = Stamp::of(&meta);
        if let Some((kept, value)) = self.lock().get(path)
            && *kept == stamp
        {
            return Ok(value.clone());
        }
        let value = parse(path)?;
        if !racy(&meta) {
            self.lock()
                .insert(path.to_path_buf(), (stamp, value.clone()));
        } else {
            // An older version is worse than none at all.
            self.lock().remove(path);
        }
        Ok(value)
    }

    /// Forget every file not in `present`.
    pub fn retain(&self, present: &[PathBuf]) {
        let present: HashSet<&PathBuf> = present.iter().collect();
        self.lock().retain(|path, _| present.contains(path));
    }

    /// How many files are remembered.
    #[must_use]
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    /// Whether nothing is remembered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    // The map is only ever changed by a single insert, remove or retain,
    // so a panic elsewhere while it was locked cannot have left it half
    // updated; a poisoned lock is still a consistent map.
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<PathBuf, (Stamp, T)>> {
        self.entries.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn age(path: &Path, secs: u64) {
        let f = fs::File::options().write(true).open(path).expect("open");
        f.set_modified(SystemTime::now() - Duration::from_secs(secs))
            .expect("set mtime");
    }

    fn read(path: &Path) -> Result<String, String> {
        fs::read_to_string(path).map_err(|e| e.to_string())
    }

    #[test]
    fn an_unchanged_file_is_not_parsed_again() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("a");
        fs::write(&path, "one").expect("write");
        age(&path, 60);
        let cache = ParseCache::default();
        let parses = Cell::new(0);
        let counted = |p: &Path| {
            parses.set(parses.get() + 1);
            read(p)
        };
        assert_eq!(cache.get_or_parse(&path, counted).as_deref(), Ok("one"));
        assert_eq!(cache.get_or_parse(&path, counted).as_deref(), Ok("one"));
        assert_eq!(parses.get(), 1);
    }

    #[test]
    fn a_changed_file_is_parsed_again_even_at_the_same_length_and_time() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("a");
        fs::write(&path, "one").expect("write");
        age(&path, 60);
        let modified = fs::metadata(&path).unwrap().modified().unwrap();
        let cache = ParseCache::default();
        assert_eq!(cache.get_or_parse(&path, read).as_deref(), Ok("one"));

        // Same length, and the modification time put back: only the
        // change time is left to tell, and it does.
        std::thread::sleep(Duration::from_millis(20));
        fs::write(&path, "two").expect("rewrite");
        fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(modified)
            .unwrap();
        assert_eq!(cache.get_or_parse(&path, read).as_deref(), Ok("two"));
    }

    #[test]
    fn a_fresh_file_and_a_failure_are_not_kept() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("a");
        fs::write(&path, "one").expect("write");
        let cache: ParseCache<String> = ParseCache::default();
        assert_eq!(cache.get_or_parse(&path, read).as_deref(), Ok("one"));
        assert!(cache.is_empty(), "modified just now: not trusted yet");

        age(&path, 60);
        let refused = cache.get_or_parse(&path, |_| Err("no".to_owned()));
        assert_eq!(refused, Err("no".to_owned()));
        assert!(cache.is_empty(), "a failure is read again next time");
    }

    #[test]
    fn retain_forgets_what_is_gone() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (a, b) = (dir.path().join("a"), dir.path().join("b"));
        for p in [&a, &b] {
            fs::write(p, "x").expect("write");
            age(p, 60);
        }
        let cache = ParseCache::default();
        cache.get_or_parse(&a, read).unwrap();
        cache.get_or_parse(&b, read).unwrap();
        assert_eq!(cache.len(), 2);
        cache.retain(std::slice::from_ref(&a));
        assert_eq!(cache.len(), 1);
    }
}
