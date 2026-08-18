//! The command surface as **expected output** (contract C-1, research D14).
//!
//! # Why the contract is asserted as text rather than in code
//!
//! C-1 makes `--help`'s structure part of the public contract: usage line, description, arguments,
//! options grouped consistently, and exit codes documented. Asserting that with
//! `assert!(help.contains("--dry-run"))` would pass while the help became unreadable, and would
//! make a contract change invisible in review.
//!
//! The files in `tests/cmd/` are the contract. A change to the surface appears as a **diff in those
//! files**, which a reviewer sees and has to agree to. That is the whole reason for the indirection.
//!
//! # `[EXE]` in the usage lines is not a wildcard
//!
//! Windows renders the usage line as `renvor.exe`. `[EXE]` is trycmd's own portability
//! placeholder: it expands to `.exe` there and to nothing elsewhere, so the expectation stays
//! **exact on every platform** rather than being loosened to `[..]`. Caught by the Windows matrix
//! after the files were seeded on macOS, which is the second time this session that the advisory
//! platform jobs found something the required ones did not.
//!
//! Regenerate deliberately with `TRYCMD=overwrite cargo test -p renvor-cli --test cli`, and read
//! the resulting diff before committing it — an auto-updated expectation that nobody looked at is
//! not a contract.

#[test]
fn the_command_surface_matches_its_recorded_contract() {
    trycmd::TestCases::new()
        .case("tests/cmd/*.trycmd")
        .default_bin_name("renvor");
}
