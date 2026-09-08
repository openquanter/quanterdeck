//! Passwords, second factors, and tokens.

use oq_deck_core::auth::{
    hash_password, new_token, new_totp_secret, password_complaint, secrets_match, totp_code,
    verify_password, verify_totp,
};

#[test]
fn a_password_verifies_against_its_own_hash() {
    let hash = hash_password("a long enough passphrase").expect("hashes");
    assert!(verify_password("a long enough passphrase", &hash).is_ok());
}

#[test]
fn a_wrong_password_does_not() {
    let hash = hash_password("a long enough passphrase").expect("hashes");
    assert!(verify_password("a long enough passphrasf", &hash).is_err());
}

#[test]
fn the_same_password_hashes_differently_every_time() {
    // A shared salt would make two operators with the same password
    // visible to each other in the stored hashes.
    let first = hash_password("a long enough passphrase").expect("hashes");
    let second = hash_password("a long enough passphrase").expect("hashes");
    assert_ne!(first, second);
}

#[test]
fn a_corrupt_stored_hash_is_not_reported_as_a_wrong_password() {
    // Told apart deliberately: one means the operator mistyped, the
    // other means the deployment is broken, and treating the second as
    // the first sends someone to reset a password that was fine.
    let error = verify_password("anything", "not-a-hash").unwrap_err();
    assert!(matches!(error, oq_deck_core::auth::AuthError::Corrupt(_)));
}

#[test]
fn tokens_are_long_and_never_repeat() {
    let first = new_token().expect("entropy");
    let second = new_token().expect("entropy");
    assert_eq!(first.len(), 64, "256 bits, hex encoded");
    assert_ne!(first, second);
}

#[test]
fn secret_comparison_rejects_a_prefix() {
    let token = new_token().expect("entropy");
    assert!(secrets_match(&token, &token.clone()));
    assert!(!secrets_match(&token, &token[..63]));
    assert!(!secrets_match(&token, ""));
}

#[test]
fn a_totp_code_verifies_within_the_window_and_not_outside_it() {
    let secret = new_totp_secret().expect("entropy");
    // The code for one specific step, so the window either side of it is
    // the thing under test rather than whichever code happened to pass.
    let now = 1_700_000_010_i64; // ten seconds into a step
    let code = totp_code(&secret, now).expect("a code");

    assert!(verify_totp(&secret, &code, now).is_ok());
    // One step either side is accepted, two is not: ninety seconds of
    // tolerance, not an afternoon. Each extra step is another window in
    // which a code read over someone's shoulder still works.
    assert!(verify_totp(&secret, &code, now + 30).is_ok());
    assert!(verify_totp(&secret, &code, now - 30).is_ok());
    assert!(verify_totp(&secret, &code, now + 60).is_err());
    assert!(verify_totp(&secret, &code, now - 60).is_err());
}

#[test]
fn a_malformed_totp_secret_is_a_corruption_not_a_rejection() {
    let error = verify_totp("not base32 !!!", "000000", 0).unwrap_err();
    assert!(matches!(error, oq_deck_core::auth::AuthError::Corrupt(_)));
}

#[test]
fn short_passwords_are_refused_with_a_reason() {
    let complaint = password_complaint("short").expect("too short");
    assert!(complaint.contains("12"));
    assert!(
        password_complaint("123456789012345").is_some(),
        "all digits"
    );
    assert!(password_complaint("a long enough passphrase").is_none());
}
