//! Passwords, second factors, and the tokens that carry a session.
//!
//! # Why this exists at all on a loopback service
//!
//! An earlier version of this console required a password only when it
//! listened off the loopback interface, on the reasoning that the
//! operating system already keeps strangers off `127.0.0.1`. That
//! reasoning is wrong in four ways, and each of them is ordinary rather
//! than exotic:
//!
//! * **Loopback is not a user boundary.** Every process and every other
//!   account on the machine can open a loopback port. Trading hosts run
//!   several of both.
//! * **A browser on that machine can be made to send requests to it.**
//!   DNS rebinding turns any page the operator visits into a client of
//!   this console. Defeating it needs a `Host` check and a session, not
//!   an interface binding.
//! * **The binding does not survive the first tunnel.** `ssh -L` is how
//!   people will reach it, and after that "loopback" describes the
//!   server's socket and nothing about who is on the other end.
//! * **Once writes are on, this places orders.** An unauthenticated
//!   service that can move money is not made safe by where it listens.
//!
//! So authentication is unconditional. What the binding changes is how
//! much more is required on top: off the loopback interface a second
//! factor becomes mandatory as well.

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};
use hmac::{Hmac, Mac};
use rand::TryRngCore;
use sha1::Sha1;
use subtle::ConstantTimeEq;

/// Length of a session or setup token, in bytes before encoding.
///
/// 256 bits. These are bearer tokens: whoever holds one is the operator
/// until it expires, so they are generated from the operating system's
/// CSPRNG and never from anything seeded by a clock.
pub const TOKEN_BYTES: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthError {
    /// The credential did not verify. Deliberately one variant for both
    /// "no such user" and "wrong password": telling them apart is how an
    /// attacker enumerates.
    Rejected,
    /// The stored hash is not one this build can read.
    Corrupt(String),
    /// Something that must not fail did.
    Internal(String),
}

impl core::fmt::Display for AuthError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Rejected => f.write_str("rejected"),
            Self::Corrupt(why) | Self::Internal(why) => f.write_str(why),
        }
    }
}

impl core::error::Error for AuthError {}

/// Argon2id, at parameters chosen for an interactive login.
///
/// 19 MiB and one pass is the OWASP low-memory profile. The console runs
/// beside a trading process on the same host, so a login must not take
/// memory that the thing being watched needs; this is the trade being
/// made deliberately rather than by accepting a default.
fn argon2() -> Result<Argon2<'static>, AuthError> {
    let params = Params::new(19 * 1024, 2, 1, None)
        .map_err(|e| AuthError::Internal(format!("argon2 parameters: {e}")))?;
    Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
}

/// Hash a password for storage.
///
/// # Errors
/// The hasher rejected the parameters or the salt.
pub fn hash_password(password: &str) -> Result<String, AuthError> {
    let mut bytes = [0u8; 16];
    rand::rngs::OsRng
        .try_fill_bytes(&mut bytes)
        .map_err(|e| AuthError::Internal(format!("no entropy for a salt: {e}")))?;
    let salt =
        SaltString::encode_b64(&bytes).map_err(|e| AuthError::Internal(format!("salt: {e}")))?;
    argon2()?
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|e| AuthError::Internal(format!("hashing: {e}")))
}

/// Check a password against a stored hash.
///
/// # Errors
/// `Rejected` when it does not match; `Corrupt` when the stored hash
/// cannot be parsed, which is a different problem and must not be
/// reported to the caller as a wrong password.
pub fn verify_password(password: &str, stored: &str) -> Result<(), AuthError> {
    let parsed =
        PasswordHash::new(stored).map_err(|e| AuthError::Corrupt(format!("stored hash: {e}")))?;
    argon2()?
        .verify_password(password.as_bytes(), &parsed)
        .map_err(|_| AuthError::Rejected)
}

/// A fresh bearer token, hex encoded.
///
/// # Errors
/// The system CSPRNG was unavailable, which is not a condition to paper
/// over with a weaker source.
pub fn new_token() -> Result<String, AuthError> {
    let mut bytes = [0u8; TOKEN_BYTES];
    rand::rngs::OsRng
        .try_fill_bytes(&mut bytes)
        .map_err(|e| AuthError::Internal(format!("no entropy for a token: {e}")))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// Compare two secrets without leaking where they first differ.
#[must_use]
pub fn secrets_match(a: &str, b: &str) -> bool {
    // Lengths are compared in the clear because a token's length is not
    // a secret; the contents are not.
    a.len() == b.len() && a.as_bytes().ct_eq(b.as_bytes()).into()
}

/// How far either side of the current step a TOTP code is accepted.
///
/// One step, so a code is valid for at most ninety seconds. Wider
/// windows are common and each extra step is another code an attacker
/// who saw one over someone's shoulder can still use.
pub const TOTP_SKEW_STEPS: i64 = 1;
const TOTP_STEP_SECONDS: i64 = 30;
const TOTP_DIGITS: u32 = 6;

fn totp_at(secret: &[u8], counter: i64) -> Result<String, AuthError> {
    let mut mac = <Hmac<Sha1>>::new_from_slice(secret)
        .map_err(|e| AuthError::Internal(format!("totp key: {e}")))?;
    mac.update(&counter.to_be_bytes());
    let digest = mac.finalize().into_bytes();

    // RFC 4226 dynamic truncation.
    let offset = usize::from(digest[digest.len() - 1] & 0x0f);
    let binary = (u32::from(digest[offset] & 0x7f) << 24)
        | (u32::from(digest[offset + 1]) << 16)
        | (u32::from(digest[offset + 2]) << 8)
        | u32::from(digest[offset + 3]);
    let modulus = 10u32.pow(TOTP_DIGITS);
    Ok(format!(
        "{:0width$}",
        binary % modulus,
        width = TOTP_DIGITS as usize
    ))
}

/// Decode a base32 TOTP secret as an authenticator app writes it.
///
/// # Errors
/// The secret is not valid base32.
pub fn decode_totp_secret(secret: &str) -> Result<Vec<u8>, AuthError> {
    base32::decode(
        base32::Alphabet::Rfc4648 { padding: false },
        &secret.replace(' ', "").to_uppercase(),
    )
    .ok_or_else(|| AuthError::Corrupt("the TOTP secret is not base32".to_owned()))
}

/// The code a base32 secret produces at a given instant.
///
/// Exists so enrolment can be confirmed: after scanning the secret, the
/// operator compares what their authenticator shows against this, and
/// finds out immediately rather than at the next login. It is also the
/// only way to name the code for one specific step, which a test of the
/// acceptance window needs.
///
/// # Errors
/// The secret is not valid base32.
pub fn totp_code(secret: &str, now_seconds: i64) -> Result<String, AuthError> {
    let key = decode_totp_secret(secret)?;
    totp_at(&key, now_seconds.div_euclid(TOTP_STEP_SECONDS))
}

/// Check a six-digit code against a base32 secret, and name the step it
/// matched.
///
/// `now_seconds` is passed in rather than read here so the check is a
/// pure function and the window can be tested without waiting.
///
/// The step is returned rather than swallowed because a code that is
/// right is not yet a code that may be accepted: the same six digits
/// stay valid for the whole window, so whoever calls this must remember
/// the last step it let through and refuse anything at or before it.
/// That memory is state, and state belongs to the caller, not here.
///
/// # Errors
/// `Rejected` when no accepted step produces the code.
pub fn verify_totp(secret: &str, code: &str, now_seconds: i64) -> Result<i64, AuthError> {
    let key = decode_totp_secret(secret)?;
    let step = now_seconds.div_euclid(TOTP_STEP_SECONDS);
    for offset in -TOTP_SKEW_STEPS..=TOTP_SKEW_STEPS {
        let expected = totp_at(&key, step + offset)?;
        if secrets_match(&expected, code.trim()) {
            return Ok(step + offset);
        }
    }
    Err(AuthError::Rejected)
}

/// A new base32 TOTP secret for enrolment.
///
/// # Errors
/// The system CSPRNG was unavailable.
pub fn new_totp_secret() -> Result<String, AuthError> {
    let mut bytes = [0u8; 20];
    rand::rngs::OsRng
        .try_fill_bytes(&mut bytes)
        .map_err(|e| AuthError::Internal(format!("no entropy for a secret: {e}")))?;
    Ok(base32::encode(
        base32::Alphabet::Rfc4648 { padding: false },
        &bytes,
    ))
}

/// Why a password was refused, in words the operator can act on.
///
/// Length first and no composition rules: a long passphrase beats a
/// short one with a digit bolted on, and rules that force the second
/// produce written-down passwords.
#[must_use]
pub fn password_complaint(password: &str, lang: crate::lang::Lang) -> Option<String> {
    let length = password.chars().count();
    if length < 12 {
        return Some(lang.pick(
            format!(
                "密码至少 12 个字符，当前 {length} 个。这个控制台能下单，\
                 请用一句只有你记得的话，而不是一个词加几个数字。"
            ),
            format!(
                "A password needs at least 12 characters; this one has {length}. This console \
                 can place orders: use a sentence only you remember, not a word and some digits."
            ),
        ));
    }
    if password.chars().all(|c| c.is_ascii_digit()) {
        return Some(
            lang.pick(
                "全是数字的密码会被最先猜到。",
                "A password of only digits is the first kind guessed.",
            )
            .to_owned(),
        );
    }
    None
}
