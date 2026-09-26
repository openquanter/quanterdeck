//! Devices the operator has enrolled, so a browser they trust is not
//! asked for a password every morning.
//!
//! A session is a thing that ends — it idles out, and it dies with the
//! process. A device is a thing the operator *made*, deliberately, and
//! can take away: it is the difference between "you have been here
//! recently" and "this is my laptop".
//!
//! What that costs is stated where it is offered: the enrolled browser
//! holds one factor instead of two. So a device is listed, named and
//! revocable, and the token it holds is never written down — only its
//! SHA-256 is, so a copy of the file is a list of what exists rather
//! than a set of keys to use.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// One enrolled browser.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Device {
    /// Public. Names it for revocation; not a secret.
    pub id: String,
    /// SHA-256, hex, of the token the browser holds.
    pub hash: String,
    /// What the operator called it. A list of six hex ids is not a list
    /// anybody can revoke the right entry from.
    pub label: String,
    pub created_ms: i64,
}

/// The file the enrolled devices live in.
const FILE: &str = "devices.json";

/// How many devices may be enrolled at once. Nobody has thirty-two
/// browsers; a list that grows without a bound is one nobody reads, and
/// this one is read to decide what to revoke.
pub const MAX_DEVICES: usize = 32;

/// SHA-256 of a token, as hex.
#[must_use]
pub fn hash_token(token: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(token.as_bytes());
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// The enrolled devices, and the file they are kept in.
pub struct Devices {
    path: PathBuf,
    list: Vec<Device>,
}

impl Devices {
    /// Open the store beside the deck's other state.
    ///
    /// A file that will not parse is an error rather than an empty list:
    /// reading a broken file as "no devices enrolled" would lock out
    /// every device at once and look like the feature never worked.
    ///
    /// # Errors
    /// The file exists and cannot be read or parsed.
    pub fn open(state_dir: &Path) -> Result<Self, String> {
        let path = state_dir.join(FILE);
        let list = match std::fs::read_to_string(&path) {
            Ok(text) => {
                serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(format!("{}: {e}", path.display())),
        };
        Ok(Self { path, list })
    }

    /// Enrol a token, and return the device it became.
    ///
    /// # Errors
    /// The file could not be written. The token is not stored — the
    /// caller keeps it, and a failure here means nobody has it.
    pub fn issue(&mut self, label: &str, token: &str, now_ms: i64) -> Result<Device, String> {
        if self.list.len() >= MAX_DEVICES {
            return Err(format!(
                "already {MAX_DEVICES} devices enrolled; revoke one instead of adding another"
            ));
        }
        let device = Device {
            id: crate::auth::new_token().map_err(|e| e.to_string())?,
            hash: hash_token(token),
            label: label.trim().to_string(),
            created_ms: now_ms,
        };
        self.list.push(device.clone());
        self.save()?;
        Ok(device)
    }

    /// The device a token belongs to, if any.
    #[must_use]
    pub fn find(&self, token: &str) -> Option<&Device> {
        let hash = hash_token(token);
        self.list.iter().find(|d| d.hash == hash)
    }

    /// Take one away.
    ///
    /// # Errors
    /// The file could not be written.
    pub fn revoke(&mut self, id: &str) -> Result<bool, String> {
        let before = self.list.len();
        self.list.retain(|d| d.id != id);
        if self.list.len() == before {
            return Ok(false);
        }
        self.save()?;
        Ok(true)
    }

    #[must_use]
    pub fn list(&self) -> &[Device] {
        &self.list
    }

    fn save(&self) -> Result<(), String> {
        let text = serde_json::to_string_pretty(&self.list).map_err(|e| e.to_string())?;
        crate::write_private(&self.path, &text).map_err(|e| format!("{}: {e}", self.path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, Devices) {
        let dir = tempfile::tempdir().expect("dir");
        let d = Devices::open(dir.path()).expect("opens");
        (dir, d)
    }

    #[test]
    fn a_token_is_found_by_its_device_and_a_hash_is_all_that_is_stored() {
        use std::os::unix::fs::PermissionsExt;
        let (dir, mut d) = store();
        let device = d.issue("this laptop", "a-token", 1).expect("issued");
        assert_eq!(d.find("a-token").map(|f| f.id.clone()), Some(device.id));
        assert_eq!(d.find("another-token"), None);

        // What is on disk is the hash, not the token: a copy of the file
        // is a list of what exists, not a set of keys.
        let text = std::fs::read_to_string(dir.path().join("devices.json")).expect("read");
        assert!(!text.contains("a-token"), "{text}");
        assert!(text.contains(&hash_token("a-token")), "{text}");
        let mode = std::fs::metadata(dir.path().join("devices.json"))
            .expect("stat")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "{mode:o}");
    }

    #[test]
    fn revoking_one_leaves_the_others() {
        let (_d, mut d) = store();
        let first = d.issue("laptop", "one", 1).expect("issued");
        d.issue("phone", "two", 2).expect("issued");
        assert!(d.revoke(&first.id).expect("revoked"));
        assert_eq!(d.find("one"), None);
        assert!(d.find("two").is_some(), "the other device is untouched");
        assert!(!d.revoke("no-such-id").expect("no-op"), "and says so");
    }

    #[test]
    fn a_file_that_will_not_parse_is_an_error_not_an_empty_list() {
        let dir = tempfile::tempdir().expect("dir");
        std::fs::write(dir.path().join("devices.json"), "{ not json").expect("write");
        assert!(
            Devices::open(dir.path()).is_err(),
            "reading it as 'none enrolled' would lock out every device at once"
        );
    }
}
