//! What the agent checks before doing anything: freshness, replay, and
//! for the risky requests a one-time code only a person can produce.

use std::collections::BTreeMap;

use oq_deck_core::auth;
use oq_deck_core::lang::Said;

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
    pub fn admit(&mut self, nonce: &str, expires_ms: i64, now_ms: i64) -> Result<(), Said> {
        self.seen.retain(|_, exp| *exp >= now_ms);
        if nonce.len() < 16 {
            return Err(Said::new(
                "请求没有可用的 nonce。",
                "the request carries no usable nonce",
            ));
        }
        if expires_ms < now_ms {
            return Err(Said::new("请求已过期。", "the request has expired"));
        }
        if expires_ms > now_ms + MAX_TTL_MS {
            return Err(Said::new(
                "请求声明的有效期过长。",
                "the request claims to be valid for too long",
            ));
        }
        if self.seen.insert(nonce.to_string(), expires_ms).is_some() {
            return Err(Said::new(
                "这个请求已经执行过了。",
                "this request was already answered",
            ));
        }
        Ok(())
    }
}

/// Wrong step-up codes before the door closes, and for how long.
///
/// The login has had this from the start and the step-up did not, which
/// had it the wrong way round: a six-digit code is a million guesses,
/// and what sits behind this one is stopping the trader, promoting a
/// release, or writing a strategy's configuration.
const MAX_FAILURES: u32 = 5;
const LOCKOUT_S: i64 = 15 * 60;

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
    /// Wrong codes since the last one that worked, and when the door
    /// reopens. Kept here rather than per caller because there is one
    /// person behind this agent and the guesses are theirs.
    failures: u32,
    locked_until_s: Option<i64>,
}

impl StepUp {
    #[must_use]
    pub fn new(secret: Option<String>) -> Self {
        Self {
            secret,
            last_step: i64::MIN,
            failures: 0,
            locked_until_s: None,
        }
    }

    #[must_use]
    pub fn configured(&self) -> bool {
        self.secret.is_some()
    }

    /// # Errors
    /// No secret configured, no code, a wrong one, or one already used.
    pub fn verify(&mut self, code: Option<&str>, now_seconds: i64) -> Result<(), Said> {
        // Checked before the code is looked at, so a locked-out caller
        // learns nothing about whether this guess was closer than the
        // last one.
        if let Some(until) = self.locked_until_s {
            if now_seconds < until {
                let wait = until - now_seconds;
                return Err(Said::new(
                    format!("二次验证码连续输错，请在 {wait} 秒后再试。"),
                    format!("too many wrong step-up codes; try again in {wait} seconds"),
                ));
            }
            self.locked_until_s = None;
            self.failures = 0;
        }
        let secret = self.secret.as_deref().ok_or(Said::new(
            "这台主机没有配置二次验证密钥，高风险操作已关闭。",
            "no step-up secret is configured on this host; high-risk actions are off",
        ))?;
        let code = code
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .ok_or(Said::new("需要二次验证码。", "a step-up code is required"))?;
        let step = now_seconds.div_euclid(30);
        for s in (step - auth::TOTP_SKEW_STEPS)..=(step + auth::TOTP_SKEW_STEPS) {
            let expected =
                auth::totp_code(secret, s * 30).map_err(|e| Said::same(e.to_string()))?;
            if auth::secrets_match(&expected, code) {
                if s <= self.last_step {
                    return Err(Said::new(
                        "该验证码已经用过了，请等下一个。",
                        "that code was already used; wait for the next one",
                    ));
                }
                self.last_step = s;
                self.failures = 0;
                return Ok(());
            }
        }
        // Counted here and not on the earlier refusals: a request with
        // no code is a client that forgot one, and this is a guess.
        self.failures += 1;
        if self.failures >= MAX_FAILURES {
            self.failures = 0;
            self.locked_until_s = Some(now_seconds + LOCKOUT_S);
        }
        Err(Said::new("二次验证码不正确。", "the step-up code is wrong"))
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

    /// A six-digit code is a million guesses and what sits behind it is
    /// stopping the trader, promoting a release, or writing a strategy's
    /// configuration. The login has counted failures from the start and
    /// this had not, which had it the wrong way round.
    #[test]
    fn wrong_codes_close_the_door_for_a_while() {
        let secret = auth::new_totp_secret().expect("secret");
        let now = 1_800_000_000;
        let code = auth::totp_code(&secret, now).expect("code");
        // A code that is certainly not this one, so the test does not
        // depend on a million-to-one.
        let wrong = if code == "000000" { "111111" } else { "000000" };
        let mut s = StepUp::new(Some(secret.clone()));

        for i in 0..MAX_FAILURES {
            assert!(s.verify(Some(wrong), now).is_err(), "guess {i}");
        }

        // The door is shut: the right code is refused too, and the
        // refusal says why rather than pretending the code was wrong.
        let refused = s.verify(Some(&code), now).expect_err("locked out");
        assert!(refused.en.contains("too many wrong"), "{}", refused.en);

        // And it opens again once the wait is over.
        let later = now + LOCKOUT_S;
        let then = auth::totp_code(&secret, later).expect("code");
        assert!(s.verify(Some(&then), later).is_ok());
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
                .en
                .contains("already used")
        );
        assert!(s.verify(None, now).is_err());
        assert!(
            StepUp::new(None).verify(Some(&code), now).is_err(),
            "no secret, no high-risk"
        );
    }
}
