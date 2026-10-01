//! The framework commit this deck was built against, read from
//! `Cargo.lock`.
//!
//! Shared with `build.rs` by path, so the function that embeds the
//! revision at build time is the one the tests exercise.

/// The full commit of `github.com/openquanter/openquanter` that
/// `Cargo.lock` resolved the framework crates to.
///
/// `Cargo.toml` pins a short rev; the lock records the full one after
/// the `#`, which is the one worth comparing.
///
/// # Errors
/// The lock names no framework commit, or more than one: either way
/// there is no single answer to "what does this deck run", and the
/// deck says so rather than picking one.
pub fn framework_rev(lock: &str) -> Result<String, String> {
    const SOURCE: &str = "git+https://github.com/openquanter/openquanter";
    let mut found: Vec<&str> = Vec::new();
    for line in lock.lines() {
        let Some(value) = line
            .trim()
            .strip_prefix("source = \"")
            .and_then(|v| v.strip_suffix('"'))
        else {
            continue;
        };
        let Some(rest) = value.strip_prefix(SOURCE) else {
            continue;
        };
        // The repository's name ends here; `openquanter-fork` does not.
        if !(rest.starts_with('?') || rest.starts_with('#') || rest.starts_with(".git")) {
            continue;
        }
        let Some((_, sha)) = rest.rsplit_once('#') else {
            continue;
        };
        if sha.len() == 40 && sha.bytes().all(|b| b.is_ascii_hexdigit()) && !found.contains(&sha) {
            found.push(sha);
        }
    }
    match found.as_slice() {
        [one] => Ok((*one).to_ascii_lowercase()),
        [] => Err("Cargo.lock names no openquanter commit".to_owned()),
        many => Err(format!(
            "Cargo.lock names more than one openquanter commit: {}",
            many.join(", ")
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::framework_rev;

    const SHA: &str = "bbdcb4e10d5b5b8a32f3ba51e4f353d6a9850a7b";

    #[test]
    fn the_full_commit_after_the_hash() {
        let lock = format!(
            "[[package]]\nname = \"oq-data\"\nversion = \"2.0.0\"\n\
             source = \"git+https://github.com/openquanter/openquanter?rev=bbdcb4e#{SHA}\"\n\
             [[package]]\nname = \"oq-types\"\n\
             source = \"git+https://github.com/openquanter/openquanter?rev=bbdcb4e#{SHA}\"\n\
             [[package]]\nname = \"serde\"\n\
             source = \"registry+https://github.com/rust-lang/crates.io-index\"\n"
        );
        assert_eq!(framework_rev(&lock).as_deref(), Ok(SHA));
    }

    #[test]
    fn a_tag_or_branch_source_reads_the_same() {
        let lock = format!(
            "source = \"git+https://github.com/openquanter/openquanter?tag=v2.0.1#{SHA}\"\n"
        );
        assert_eq!(framework_rev(&lock).as_deref(), Ok(SHA));
    }

    #[test]
    fn two_commits_are_no_answer() {
        let lock = format!(
            "source = \"git+https://github.com/openquanter/openquanter?rev=bbdcb4e#{SHA}\"\n\
             source = \"git+https://github.com/openquanter/openquanter?rev=1234567#\
             1234567890123456789012345678901234567890\"\n"
        );
        assert!(framework_rev(&lock).unwrap_err().contains("more than one"));
    }

    #[test]
    fn no_framework_is_no_answer() {
        assert!(framework_rev("source = \"registry+https://x\"\n").is_err());
        // Another repository whose name starts the same is not this one.
        let fork = format!(
            "source = \"git+https://github.com/openquanter/openquanter-fork?rev=1#{SHA}\"\n"
        );
        assert!(framework_rev(&fork).is_err());
    }
}
