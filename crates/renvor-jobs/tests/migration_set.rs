//! The README describes the migration set this crate ships, and the description must agree
//! with the directory it describes.
//!
//! # The defect this exists for
//!
//! On 2026-09-04 the fifth pair (`20260904000005_create_job_queue`) was added for the depth-bound
//! lock row, and `README.md` kept saying *"four `up`/`down` pairs … `20260904000001`–
//! `20260904000004`"*. Contract C-J10 and the directory said five. Nothing compared the two, so the
//! sentence stayed wrong through a phase closure and was found by a reader of the cheat sheet.
//!
//! A count written in prose is a claim. This test is what makes it a checked one: the directory is
//! the source, and the README must say what the directory says.

use std::path::Path;

const README: &str = include_str!("../README.md");

/// The number of `up`/`down` pairs one engine's directory holds, and its highest version.
fn pairs_in(engine: &str) -> (usize, String) {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("migrations")
        .join(engine);
    let names: Vec<String> = std::fs::read_dir(&directory)
        .unwrap_or_else(|error| panic!("`migrations/{engine}` is readable: {error}"))
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    let mut ups: Vec<&String> = names
        .iter()
        .filter(|name| name.ends_with(".up.sql"))
        .collect();
    let downs = names
        .iter()
        .filter(|name| name.ends_with(".down.sql"))
        .count();
    assert_eq!(
        ups.len(),
        downs,
        "`migrations/{engine}` has an `up` without its `down`, or the reverse; the set is \
         declared reversible"
    );
    assert!(
        !ups.is_empty(),
        "`migrations/{engine}` holds no migration at all"
    );
    ups.sort();
    let last = ups
        .last()
        .expect("checked non-empty")
        .split('_')
        .next()
        .expect("a migration file name starts with its version")
        .to_owned();
    (ups.len(), last)
}

/// The count as the README spells it. Small on purpose: a set that outgrows this table should
/// make somebody extend the table, not silently pass.
fn in_words(count: usize) -> &'static str {
    match count {
        1 => "one",
        2 => "two",
        3 => "three",
        4 => "four",
        5 => "five",
        6 => "six",
        7 => "seven",
        8 => "eight",
        9 => "nine",
        other => panic!("extend `in_words` for a set of {other} pairs"),
    }
}

#[test]
fn the_readme_states_the_number_of_pairs_the_directory_holds() {
    let (postgres, last_postgres) = pairs_in("postgres");
    let (mysql, last_mysql) = pairs_in("mysql");
    assert_eq!(
        postgres, mysql,
        "the two engines ship different numbers of pairs; the README describes one set"
    );
    assert_eq!(
        last_postgres, last_mysql,
        "the two engines' highest versions differ; the README names one"
    );

    let sentence = format!("{} `up`/`down` pairs", in_words(postgres));
    assert!(
        README.contains(&sentence),
        "README.md does not say {sentence:?}; the directory holds {postgres} pairs"
    );
    assert!(
        README.contains(&format!("`{last_postgres}`")),
        "README.md does not name the highest version `{last_postgres}`"
    );

    // NEGATIVE CONTROL on the README itself: it must not ALSO state a stale count somewhere else,
    // or a README that said both would pass the assertion above while still misleading a reader.
    for stale in (1..=9).filter(|count| *count != postgres) {
        let stale_sentence = format!("{} `up`/`down` pairs", in_words(stale));
        assert!(
            !README.contains(&stale_sentence),
            "README.md still says {stale_sentence:?} somewhere"
        );
    }
}

#[test]
fn the_word_table_spells_the_count_this_directory_holds() {
    // POSITIVE CONTROL for the spelling: a table that returned the wrong word for the real count
    // would make the assertion above fail for the wrong reason, or pass for no reason.
    let (count, _) = pairs_in("postgres");
    assert_eq!(in_words(count), "five", "the directory holds {count} pairs");
    assert_ne!(in_words(count), in_words(count - 1));
}
