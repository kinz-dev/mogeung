//! Which commit this binary was built from. `R-J105`, feature 0049.
//!
//! Set by `build.rs` from `git rev-parse HEAD` at build time. The window builds
//! this crate as a dependency and reads the same constant, so the daemon and
//! the window it ships inside cannot disagree about what they are.

/// The full commit hash, or empty when the build had no repository to ask.
pub const COMMIT: &str = env!("MOGEUNG_COMMIT");

/// The first seven characters, or `unknown`.
pub fn short() -> &'static str {
    if COMMIT.len() >= 7 {
        &COMMIT[..7]
    } else {
        "unknown"
    }
}

/// `0.1.0 (abc1234)`, for `--version`.
pub const DESCRIBE: &str = concat!(env!("CARGO_PKG_VERSION"), " (", env!("MOGEUNG_COMMIT"), ")");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_build_is_stamped_with_a_commit_when_it_had_a_repository_to_ask() {
        // This test runs from a checkout, so there was one.
        assert_eq!(COMMIT.len(), 40, "a full hash: {COMMIT:?}");
        assert!(COMMIT.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(short(), &COMMIT[..7]);
        assert!(DESCRIBE.contains(COMMIT));
    }
}
