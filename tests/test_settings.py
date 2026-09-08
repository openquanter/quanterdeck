import pytest

from oq_deck.settings import ConfigurationError, Settings


def test_loopback_needs_no_password():
    Settings(host="127.0.0.1").validate()


def test_public_bind_without_a_password_is_refused():
    with pytest.raises(ConfigurationError) as caught:
        Settings(host="0.0.0.0").validate()
    assert "without a password" in str(caught.value)


def test_public_bind_still_needs_a_second_factor():
    with pytest.raises(ConfigurationError) as caught:
        Settings(host="0.0.0.0", password_hash="x").validate()
    assert "second factor" in str(caught.value)


def test_a_fully_configured_public_bind_is_allowed():
    Settings(host="0.0.0.0", password_hash="x", totp_secret="y").validate()
