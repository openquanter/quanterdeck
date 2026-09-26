//! Enrolling a browser by proving an SSH key.
//!
//! The other way to get a device credential is to sign in with the
//! password on that machine. This is for a machine the operator has an
//! SSH key on and no password yet — and it is the same mechanism the
//! host agent uses to trust a release: a public key in an
//! `allowed_signers` file, and a signature made with a private key that
//! never leaves the operator's machine.
//!
//! Two steps, because the browser and the command line are different
//! processes. The command line proves the key and is handed a **claim
//! code**; the operator opens a link carrying it, and the browser
//! exchanges it for the cookie. Neither half is worth anything alone: a
//! challenge is not a credential, and a claim code is spent by the first
//! browser that presents it.
//!
//! Both are unauthenticated, so both are bounded: a fixed number
//! outstanding, a short life, and single use. Whoever can reach the port
//! can ask for challenges all day and get eight of them.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long a challenge is good for, and how long a claim code is.
pub const CHALLENGE_LIFE: Duration = Duration::from_secs(120);
pub const CLAIM_LIFE: Duration = Duration::from_secs(120);
/// How many of each may be outstanding at once.
pub const MAX_OUTSTANDING: usize = 8;

#[derive(Default)]
struct Inner {
    /// Challenge text to when it was issued.
    challenges: HashMap<String, Instant>,
    /// Claim code to when it was minted, and what to call the browser
    /// it is about to become.
    claims: HashMap<String, (Instant, String)>,
}

/// Challenges handed out, and claim codes waiting to be spent.
#[derive(Default)]
pub struct Enrolments {
    inner: Mutex<Inner>,
}

impl Enrolments {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Hand out a challenge for a client to sign.
    ///
    /// # Errors
    /// The system CSPRNG was unavailable.
    pub fn challenge(&self, now: Instant) -> Result<String, String> {
        let text = oq_deck_core::auth::new_token().map_err(|e| e.to_string())?;
        let mut inner = self.lock();
        inner.challenges.retain(|_, at| now - *at < CHALLENGE_LIFE);
        inner.claims.retain(|_, (at, _)| now - *at < CLAIM_LIFE);
        if inner.challenges.len() >= MAX_OUTSTANDING {
            // Oldest out. Refusing instead would let anyone who can reach
            // the port stop the operator enrolling a machine.
            if let Some(oldest) = inner
                .challenges
                .iter()
                .min_by_key(|(_, at)| **at)
                .map(|(c, _)| c.clone())
            {
                inner.challenges.remove(&oldest);
            }
        }
        inner.challenges.insert(text.clone(), now);
        Ok(text)
    }

    /// Spend a challenge, and mint the claim code that replaces it.
    ///
    /// The challenge is taken whether or not the caller's signature turns
    /// out to be good — a challenge is spent by being offered, or a
    /// caller could try signatures against it for two minutes.
    ///
    /// `label` is carried here rather than looked up later because the
    /// claim code is the only thing that survives the trip through the
    /// browser, and a device called "a browser" is one the operator
    /// cannot tell from the next one on the page they revoke from.
    ///
    /// # Errors
    /// No such challenge, an expired one, or no entropy for the code.
    pub fn spend(&self, challenge: &str, label: &str, now: Instant) -> Result<String, String> {
        let mut inner = self.lock();
        let taken = inner.challenges.remove(challenge);
        match taken {
            Some(at) if now - at < CHALLENGE_LIFE => {}
            _ => return Err("no such challenge, or it has expired".to_owned()),
        }
        let code = oq_deck_core::auth::new_token().map_err(|e| e.to_string())?;
        inner.claims.insert(code.clone(), (now, label.to_owned()));
        Ok(code)
    }

    /// Spend a claim code, and say what the browser is to be called. The
    /// first time only.
    #[must_use]
    pub fn claim(&self, code: &str, now: Instant) -> Option<String> {
        let mut inner = self.lock();
        match inner.claims.remove(code) {
            Some((at, label)) if now - at < CLAIM_LIFE => Some(label),
            _ => None,
        }
    }
}

/// The namespace a signature must have been made in.
///
/// The host agent verifies release signatures under its own namespace;
/// this one is separate, so a signature made to authorise a release
/// cannot be replayed here, and one made here cannot authorise a
/// release. The signature itself covers only the challenge, which is a
/// nonce — the namespace is what makes it *this* question.
pub const NAMESPACE: &str = "oq-deck-enrol";

/// Whether `signature` is `challenge`, signed with a key
/// `trusted_keys` lists.
///
/// Shelling out to `ssh-keygen -Y verify`, which is the only thing on
/// the host that can check an SSH signature, and the same command the
/// host agent trusts a release with. The signature is a file because
/// that is what `-s` takes; the signed text goes in on stdin.
///
/// # Errors
/// A sentence for the operator: no such identity in the file, a
/// signature that does not check out, or `ssh-keygen` not being there.
pub fn verify(
    trusted_keys: &std::path::Path,
    identity: &str,
    challenge: &str,
    signature: &str,
) -> Result<(), String> {
    // Refused here rather than left to `ssh-keygen`: an identity that is
    // empty or names a flag is not an identity, and `-I` is allowed to
    // start with a dash.
    if identity.trim().is_empty() || identity.starts_with('-') || identity.contains('\n') {
        return Err("the identity is not a name ssh-keygen would accept".to_owned());
    }
    let dir = std::env::temp_dir();
    // Private to the machine and to this process's mount namespace: the
    // unit sets `PrivateTmp=yes`. Not a secret either way — a signature
    // is a public artefact — but the name must not be guessable, or two
    // callers would write over each other's.
    let path = dir.join(format!(
        "oq-deck-enrol-{}.sig",
        oq_deck_core::auth::new_token().map_err(|e| e.to_string())?
    ));
    {
        use std::io::Write as _;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| format!("could not write the signature: {e}"))?;
        file.write_all(signature.as_bytes())
            .map_err(|e| format!("could not write the signature: {e}"))?;
    }

    let run = || -> Result<std::process::Output, String> {
        use std::io::Write as _;
        let mut child = std::process::Command::new("ssh-keygen")
            .args(["-Y", "verify", "-f"])
            .arg(trusted_keys)
            .args(["-I", identity, "-n", NAMESPACE, "-s"])
            .arg(&path)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| format!("could not run ssh-keygen: {e}"))?;
        child
            .stdin
            .take()
            .ok_or("could not reach ssh-keygen")?
            .write_all(challenge.as_bytes())
            .map_err(|e| format!("could not send the challenge: {e}"))?;
        child
            .wait_with_output()
            .map_err(|e| format!("ssh-keygen did not finish: {e}"))
    };
    let output = run();
    let _ = std::fs::remove_file(&path);
    let output = output?;
    if output.status.success() {
        return Ok(());
    }
    // ssh-keygen's own words, which distinguish "no such key" from "bad
    // signature" — both are refusals, and neither is worth hiding from
    // the operator who is holding the key.
    let said = String::from_utf8_lossy(&output.stderr);
    Err(format!(
        "the signature was not accepted: {}",
        said.lines().next().unwrap_or("no reason given").trim()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_claim_code_is_spent_once() {
        let e = Enrolments::default();
        let now = Instant::now();
        let challenge = e.challenge(now).expect("challenge");
        let code = e.spend(&challenge, "a laptop", now).expect("code");
        assert_eq!(
            e.claim(&code, now).as_deref(),
            Some("a laptop"),
            "the first browser gets in, under the name it was given"
        );
        assert!(e.claim(&code, now).is_none(), "and the second does not");
    }

    #[test]
    fn a_challenge_is_spent_by_being_offered() {
        let e = Enrolments::default();
        let now = Instant::now();
        let challenge = e.challenge(now).expect("challenge");
        assert!(e.spend(&challenge, "a laptop", now).is_ok());
        assert!(
            e.spend(&challenge, "a laptop", now).is_err(),
            "a signature tried against it twice must not be possible"
        );
    }

    #[test]
    fn an_expired_one_is_not_a_credential() {
        let e = Enrolments::default();
        let now = Instant::now();
        let challenge = e.challenge(now).expect("challenge");
        let code = e.spend(&challenge, "a laptop", now).expect("code");
        let later = now + CLAIM_LIFE + Duration::from_secs(1);
        assert!(e.claim(&code, later).is_none());

        let challenge = e.challenge(now).expect("challenge");
        let later = now + CHALLENGE_LIFE + Duration::from_secs(1);
        assert!(e.spend(&challenge, "a laptop", later).is_err());
    }

    /// Anyone who can reach the port can ask; the list is bounded anyway.
    #[test]
    fn asking_for_challenges_does_not_grow_without_a_bound() {
        let e = Enrolments::default();
        let now = Instant::now();
        for _ in 0..MAX_OUTSTANDING * 4 {
            e.challenge(now).expect("challenge");
        }
        assert!(e.lock().challenges.len() <= MAX_OUTSTANDING);
    }

    /// The mechanism itself, against the real `ssh-keygen`: a key, a
    /// signature over a challenge, and the two ways it must fail.
    #[test]
    fn a_signature_from_a_listed_key_is_accepted() {
        use std::process::Command;

        let dir = tempfile::tempdir().expect("dir");
        let key = dir.path().join("id");
        let made = Command::new("ssh-keygen")
            .args(["-t", "ed25519", "-N", "", "-C", "enrol-test", "-f"])
            .arg(&key)
            .output()
            .expect("ssh-keygen is on any host that runs an sshd");
        assert!(
            made.status.success(),
            "{}",
            String::from_utf8_lossy(&made.stderr)
        );

        let public = std::fs::read_to_string(key.with_extension("pub")).expect("public key");
        let signers = dir.path().join("allowed_signers");
        std::fs::write(&signers, format!("laptop {public}")).expect("write");

        let challenge = "a challenge the deck just made up";
        let signature = sign(challenge, &key);

        assert!(verify(&signers, "laptop", challenge, &signature).is_ok());

        // A challenge that is not the one that was signed.
        assert!(verify(&signers, "laptop", "a different challenge", &signature).is_err());
        // An identity the file does not list.
        assert!(verify(&signers, "someone-else", challenge, &signature).is_err());
        // And nothing an attacker can put in the identity reaches a flag.
        assert!(verify(&signers, "-f /etc/shadow", challenge, &signature).is_err());
    }

    /// Sign a challenge the way the operator's script does.
    fn sign(challenge: &str, key: &std::path::Path) -> String {
        use std::io::Write as _;
        let mut child = std::process::Command::new("ssh-keygen")
            .args(["-Y", "sign", "-n", NAMESPACE, "-f"])
            .arg(key)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("spawn");
        child
            .stdin
            .take()
            .expect("stdin")
            .write_all(challenge.as_bytes())
            .expect("write");
        let out = child.wait_with_output().expect("wait");
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).expect("utf8")
    }
}
