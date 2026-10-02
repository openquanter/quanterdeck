//! Domain logic for the console.

pub mod attribution;
pub mod auth;
pub mod cache;
pub mod capabilities;
pub mod devices;
pub mod gate;
pub mod lang;
pub mod live;
pub mod markout;

/// Writes `text` where only this user can read it.
///
/// The mode is set when the file is *created*, not afterwards: a file
/// that exists briefly under the umask is a file somebody else may have
/// opened in the meantime.
///
/// # Errors
/// The file could not be created or written.
pub fn write_private(path: &std::path::Path, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    f.write_all(text.as_bytes())?;
    f.write_all(b"\n")
}
pub mod ops;
pub mod report;
pub mod runs;
pub mod sweeps;
pub mod upstream;

/// Whether a directory entry is a regular file with this extension.
///
/// Judged by the entry's own type, not by `Path::is_file`: that follows
/// a symbolic link. These directories are written by another account —
/// the trader's, not the console's — so a link planted in one would be
/// read, parsed and reported as though it belonged here. The listings
/// and the resolvers also have to agree about what is in there, or the
/// page offers a file that cannot be opened.
pub(crate) fn regular_with(entry: &std::fs::DirEntry, extension: &str) -> bool {
    entry.file_type().is_ok_and(|t| t.is_file())
        && entry.path().extension().is_some_and(|e| e == extension)
}

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
