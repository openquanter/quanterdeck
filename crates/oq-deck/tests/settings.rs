//! What the deck refuses to start as.

use std::net::{IpAddr, Ipv4Addr};

use oq_deck::settings::Settings;

fn on(host: &str) -> Settings {
    Settings {
        host: host.parse::<IpAddr>().unwrap(),
        ..Settings::default()
    }
}

#[test]
fn loopback_needs_no_password() {
    assert!(Settings::default().validate().is_ok());
    assert_eq!(Settings::default().host, IpAddr::V4(Ipv4Addr::LOCALHOST));
}

#[test]
fn a_public_bind_without_a_password_is_refused() {
    let error = on("0.0.0.0").validate().unwrap_err();
    assert!(error.0.contains("without a password"));
    assert!(
        error.0.contains("127.0.0.1"),
        "the refusal must say what to do instead"
    );
}

#[test]
fn a_public_bind_still_needs_a_second_factor() {
    let settings = Settings {
        password_hash: Some("x".into()),
        ..on("0.0.0.0")
    };
    assert!(settings.validate().unwrap_err().0.contains("second factor"));
}

#[test]
fn a_fully_configured_public_bind_is_allowed() {
    let settings = Settings {
        password_hash: Some("x".into()),
        totp_secret: Some("y".into()),
        ..on("0.0.0.0")
    };
    assert!(settings.validate().is_ok());
}
