//! The job-store migration set is embedded as constants that equal the files (Phase 011).
//!
//! Same argument as `renvor-auth/tests/embedded_migrations.rs`: the generator copies this set into
//! a generated project and cannot read it from a checkout at generation time.

use std::path::Path;

use renvor_jobs::migrations::{Engine, EngineSet, mysql, postgres};

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
                std::fs::read_to_string(entry.path()).expect("readable"),
            )
        })
        .collect();
    files.sort();
    files
}

fn assert_set_is_the_directory(set: &EngineSet, engine: &str) {
    let disk = on_disk(engine);
    assert!(!disk.is_empty());
    let embedded: Vec<(String, String)> = set
        .files()
        .iter()
        .map(|file| (file.name().to_owned(), file.contents().to_owned()))
        .collect();
    assert_eq!(
        embedded, disk,
        "the embedded {engine} set differs from `migrations/{engine}`"
    );
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
    // The count the README states (tests/migration_set.rs) is the count the constant carries.
    assert_eq!(ups, 5, "{engine}: five pairs, contract C-J10");
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
fn the_set_can_tell_a_changed_file_from_an_unchanged_one() {
    let mut edited = on_disk("mysql");
    edited[0].1.push_str("-- edited\n");
    let embedded: Vec<(String, String)> = mysql()
        .files()
        .iter()
        .map(|file| (file.name().to_owned(), file.contents().to_owned()))
        .collect();
    assert_ne!(embedded, edited);
}
