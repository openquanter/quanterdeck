//! Quanterdeck — the self-hosted console for OpenQuanter.
//!
//! A library as well as a binary so the routes can be exercised without
//! binding a port: an HTTP test that needs a listening socket is a test
//! that fails on a busy machine for a reason unrelated to the code.

pub mod app;
pub mod enrol;
pub mod guard;
pub mod lockfile;
pub mod session;
pub mod settings;
pub mod upstream;
