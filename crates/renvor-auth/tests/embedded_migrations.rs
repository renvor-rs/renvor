//! The migration set this crate ships is embedded as constants, and the constants are the files.
//!
//! # Why the set is embedded at all
//!
//! Phase 011's generator copies the authentication migrations into a generated project. Contract
//! C-4 says every template is embedded in the executable, so the generator cannot read SQL from a
//! framework checkout at generation time — it depends on this crate and takes the set from here.
//! The constants are `include_str!`s of this crate's own files, so a migration edited on disk
//! changes the constant with it; this test is what proves nothing drifts between the two, and that
//! the set is complete rather than the subset somebody remembered to list.

use std::path::Path;

use renvor_auth::migrations::{Engine, EngineSet, mysql, postgres};

/// Every `.sql` file in one engine's directory, sorted by name.
fn on_disk(engine: &str) -> Vec<(String, String)> {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("migrations")
        .join(engine);
    let mut files: Vec<(String, String)> = std::fs::read_dir(&directory)
        .unwrap_or_else(|error| panic!("`migrations/{engine}` is readable: {error}"))
        .flatten()
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "sql")
        })
        .map(|entry| {
            (
                entry.file_name().to_string_lossy().into_owned(),
                std::fs::read_to_string(entry.path()).expect("a migration is readable"),
            )
        })
        .collect();
    files.sort();
    files
}

fn assert_set_is_the_directory(set: &EngineSet, engine: &str) {
    let disk = on_disk(engine);
    assert!(!disk.is_empty(), "`migrations/{engine}` holds no migration");
    let embedded: Vec<(String, String)> = set
        .files()
        .iter()
        .map(|file| (file.name().to_owned(), file.contents().to_owned()))
        .collect();
    assert_eq!(
        embedded, disk,
        "the embedded {engine} set differs from `migrations/{engine}` — a file was added, \
         removed, renamed, or edited without the constant following"
    );
    // Every file is one half of a reversible pair.
    for file in set.files() {
        assert!(
            file.name().ends_with(".up.sql") || file.name().ends_with(".down.sql"),
            "{} is neither an up nor a down migration",
            file.name()
        );
    }
    let ups = set
        .files()
        .iter()
        .filter(|file| file.name().ends_with(".up.sql"))
        .count();
    assert_eq!(
        ups * 2,
        set.files().len(),
        "{engine}: every up has its down"
    );
}

#[test]
fn the_postgres_set_is_exactly_the_postgres_directory() {
    assert_set_is_the_directory(postgres(), "postgres");
    assert_eq!(postgres().engine(), Engine::Postgres);
}

#[test]
fn the_mysql_set_is_exactly_the_mysql_directory() {
    assert_set_is_the_directory(mysql(), "mysql");
    assert_eq!(mysql().engine(), Engine::MySql);
}

#[test]
fn the_two_engines_differ_by_exactly_the_documented_index_migration() {
    // The sets differ in SQL, and — measured rather than assumed — in one file: PostgreSQL carries
    // `20260901000009_index_auth_refresh_family` as its own migration, MySQL declares that index
    // inline. A test asserting equal counts would assert a coincidence; this asserts the one
    // known difference and nothing else, so an unrelated divergence still fails.
    let versions = |set: &EngineSet| -> Vec<String> {
        let mut versions: Vec<String> = set
            .files()
            .iter()
            .map(|file| file.version().to_owned())
            .collect();
        versions.dedup();
        versions
    };
    let mut expected_mysql = versions(postgres());
    let removed = expected_mysql
        .iter()
        .position(|version| version == "20260901000009")
        .expect("the PostgreSQL set carries the index migration");
    expected_mysql.remove(removed);
    assert_eq!(versions(mysql()), expected_mysql);
}

#[test]
fn the_set_can_tell_a_changed_file_from_an_unchanged_one() {
    // POSITIVE CONTROL for the comparison above: a comparison that could not see a difference
    // would report every set as matching its directory.
    let mut edited = on_disk("postgres");
    edited[0].1.push_str("-- edited\n");
    let embedded: Vec<(String, String)> = postgres()
        .files()
        .iter()
        .map(|file| (file.name().to_owned(), file.contents().to_owned()))
        .collect();
    assert_ne!(embedded, edited);
}
