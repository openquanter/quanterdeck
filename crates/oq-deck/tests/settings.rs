//! What the deck refuses to start as.

use std::net::{IpAddr, Ipv4Addr};

use oq_deck::settings::Settings;

fn hash() -> String {
    oq_deck_core::auth::hash_password("a long enough passphrase").expect("hashes")
}

fn secret() -> String {
    oq_deck_core::auth::new_totp_secret().expect("a secret")
}

fn on(host: &str) -> Settings {
    Settings {
        host: host.parse::<IpAddr>().unwrap(),
        ..Settings::default()
    }
}

#[test]
fn the_default_is_loopback_read_only_and_unconfigured() {
    let settings = Settings::default();
    assert_eq!(settings.host, IpAddr::V4(Ipv4Addr::LOCALHOST));
    assert!(!settings.allow_writes);
    assert!(!settings.configured(), "no password until setup runs");
    assert!(settings.validate().is_ok());
}

#[test]
fn loopback_still_requires_a_password_at_runtime() {
    // `validate` permits an unconfigured deck to *start* on loopback,
    // because that is how setup is reached. It does not permit it to
    // serve anything: see the security tests, where an unconfigured deck
    // returns 401 for every route but health and setup.
    assert!(Settings::default().validate().is_ok());
    assert!(!Settings::default().configured());
}

#[test]
fn a_public_bind_without_a_password_is_refused() {
    let error = on("0.0.0.0").validate().unwrap_err();
    assert!(error.0.contains("尚未设置密码"));
    assert!(
        error.0.contains("127.0.0.1"),
        "the refusal must say what to do instead"
    );
}

#[test]
fn a_public_bind_still_needs_a_second_factor() {
    let settings = Settings {
        password_hash: Some(hash()),
        ..on("0.0.0.0")
    };
    assert!(settings.validate().unwrap_err().0.contains("第二因素"));
}

#[test]
fn a_fully_configured_public_bind_is_allowed() {
    let settings = Settings {
        password_hash: Some(hash()),
        totp_secret: Some(secret()),
        ..on("0.0.0.0")
    };
    assert!(settings.validate().is_ok());
}

/// Published through a proxy, a deck bound to loopback is reachable from
/// the network, and needs what any exposed deck needs.
#[test]
fn a_loopback_deck_published_through_a_proxy_needs_a_second_factor() {
    for settings in [
        Settings {
            password_hash: Some(hash()),
            behind_tls: true,
            ..Settings::default()
        },
        Settings {
            password_hash: Some(hash()),
            extra_hosts: vec!["deck.example.org".into()],
            ..Settings::default()
        },
    ] {
        assert!(settings.validate().unwrap_err().0.contains("第二因素"));
    }
}

/// An empty or malformed secret is not a second factor: empty, its codes
/// are computable by anyone; malformed, every login fails and counts.
#[test]
fn a_secret_that_is_not_a_key_is_refused() {
    for bad in ["", "!!notbase32", "JBSWY3DP"] {
        let settings = Settings {
            password_hash: Some(hash()),
            totp_secret: Some(bad.into()),
            ..on("0.0.0.0")
        };
        assert!(
            settings.validate().unwrap_err().0.contains("TOTP"),
            "{bad:?} must be refused"
        );
    }
}

/// A hash that does not parse is refused at startup, not reported at
/// every login as a wrong password.
#[test]
fn a_hash_that_does_not_parse_is_refused_at_startup() {
    let settings = Settings {
        password_hash: Some("not a hash".into()),
        ..Settings::default()
    };
    assert!(
        settings
            .validate()
            .unwrap_err()
            .0
            .contains("OQ_DECK_PASSWORD_HASH")
    );
}
