//! Domain logic for the console.

pub mod attribution;
pub mod auth;
pub mod capabilities;
pub mod gate;
pub mod live;
pub mod markout;
pub mod ops;
pub mod runs;

/// A file named exactly `file_name` in `dir`'s own listing, if there is
/// one and it is a regular file.
///
/// Matched against the listing and never joined onto the directory, as
/// invariant 7 says; and not a symbolic link, which a join would have
/// followed out of the directory it was meant to stay in. The id arrives
/// from a URL.
pub(crate) fn listed(dir: &std::path::Path, file_name: &str) -> Option<std::path::PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .find_map(|entry| {
            let regular = entry.file_type().is_ok_and(|t| t.is_file());
            (regular && entry.file_name().to_str() == Some(file_name)).then(|| entry.path())
        })
}
