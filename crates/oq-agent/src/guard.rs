//! What the agent checks before doing anything: freshness, replay, and
//! for the risky requests a one-time code only a person can produce.

use std::collections::BTreeMap;

use oq_deck_core::auth;

/// Longest a request may live, in milliseconds.
pub const MAX_TTL_MS: i64 = 60_000;

/// Remembers nonces for as long as their requests could still be valid.
#[derive(Debug, Default)]
pub struct Replay {
    seen: BTreeMap<String, i64>,
}

impl Replay {
    /// Accept a nonce once, within its expiry.
    ///
    /// # Errors
    /// Expired, too far in the future, empty, or seen before.
    pub fn admit(&mut self, nonce: &str, expires_ms: i64, now_ms: i64) -> Result<(), String> {
        self.seen.retain(|_, exp| *exp >= now_ms);
        if nonce.len() < 16 {
            return Err("the request carries no usable nonce".into());
        }
        if expires_ms < now_ms {
            return Err("the request has expired".into());
        }
        if expires_ms > now_ms + MAX_TTL_MS {
            return Err("the request claims to be valid for too long".into());
        }
        if self.seen.insert(nonce.to_string(), expires_ms).is_some() {
            return Err("this request was already answered".into());
        }
        Ok(())
    }
}

/// The step-up check: a TOTP code from a secret only this agent holds.
///
/// Not the deck's login secret: a deck that is compromised has that one,
/// and could mint codes. Each code is accepted once — the step it matched
/// is remembered and nothing at or before it passes again — so a code
/// seen in transit cannot be replayed onto a second request.
#[derive(Debug)]
pub struct StepUp {
    secret: Option<String>,
    last_step: i64,
}

impl StepUp {
    #[must_use]
    pub fn new(secret: Option<String>) -> Self {
        Self {
            secret,
            last_step: i64::MIN,
        }
    }

    #[must_use]
    pub fn configured(&self) -> bool {
        self.secret.is_some()
    }

    /// # Errors
    /// No secret configured, no code, a wrong one, or one already used.
    pub fn verify(&mut self, code: Option<&str>, now_seconds: i64) -> Result<(), String> {
        let secret = self
            .secret
            .as_deref()
            .ok_or("no step-up secret is configured on this host; high-risk actions are off")?;
        let code = code
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .ok_or("a step-up code is required")?;
        let step = now_seconds.div_euclid(30);
        for s in (step - auth::TOTP_SKEW_STEPS)..=(step + auth::TOTP_SKEW_STEPS) {
            let expected = auth::totp_code(secret, s * 30).map_err(|e| e.to_string())?;
            if auth::secrets_match(&expected, code) {
                if s <= self.last_step {
                    return Err("that code was already used; wait for the next one".into());
                }
                self.last_step = s;
                return Ok(());
            }
        }
        Err("the step-up code is wrong".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_nonce_is_admitted_once_and_only_while_fresh() {
        let mut r = Replay::default();
        let n = "0123456789abcdef";
        assert!(r.admit(n, 1_000 + 30_000, 1_000).is_ok());
        assert!(r.admit(n, 1_000 + 30_000, 1_001).is_err(), "replayed");
        assert!(r.admit("fedcba9876543210", 500, 1_000).is_err(), "expired");
        assert!(
            r.admit("fedcba9876543211", 1_000 + 120_000, 1_000).is_err(),
            "too long"
        );
        assert!(r.admit("short", 2_000, 1_000).is_err(), "no real nonce");
    }

    #[test]
    fn a_code_works_once() {
        let secret = auth::new_totp_secret().expect("secret");
        let now = 1_800_000_000;
        let code = auth::totp_code(&secret, now).expect("code");
        let mut s = StepUp::new(Some(secret));
        assert!(s.verify(Some("000000"), now).is_err() || code == "000000");
        assert!(s.verify(Some(&code), now).is_ok());
        assert!(
            s.verify(Some(&code), now)
                .unwrap_err()
                .contains("already used")
        );
        assert!(s.verify(None, now).is_err());
        assert!(
            StepUp::new(None).verify(Some(&code), now).is_err(),
            "no secret, no high-risk"
        );
    }
}
