//! T072 and T073 — hostile input fails closed (C-C10, FR-038), and the TOML boundary gets
//! generated input rather than only hand-picked examples (constitution principle IX).
//!
//! # Why the generator is written here rather than pulled in
//!
//! Principle IX asks for property or fuzz testing at this boundary. `cargo-fuzz` needs a nightly
//! toolchain, which this workspace's fixed 1.94.0 floor rules out; `proptest` and `arbitrary` are
//! new production-adjacent dependencies for a phase whose dependency inventory is a recorded gate.
//!
//! So the generator below is about forty lines, has **no** dependency, and is **deterministic** —
//! a fixed seed, an integer step function, and no clock. A property test that fails only on
//! Tuesdays is a test people learn to re-run rather than read, and a suite that cannot be replayed
//! exactly cannot be used to reproduce a defect. The trade is real: this explores far less of the
//! space than a coverage-guided fuzzer would. That is recorded rather than glossed.
//!
//! # What "fails closed" is asserted to mean
//!
//! For every generated input, exactly one of two things happens: it parses, or it produces an
//! error. **0** inputs may panic, hang, or start an application. The harness cannot assert "did
//! not panic" from inside — a panic fails the test by itself, which is the point.

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::PathBuf;

use renvor_config::{ConfigSchema, FileLayer, LayeredResolverBuilder};
use renvor_core::ErrorCategory;
use renvor_core::config_port::ConfigResolver as _;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Settings {
    #[allow(dead_code)]
    port: u16,
}

#[allow(dead_code)]
#[derive(Debug, Default, Deserialize)]
struct PartialSettings {
    port: Option<u16>,
}

impl ConfigSchema for Settings {
    type Partial = PartialSettings;
}

fn scratch() -> PathBuf {
    let directory = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("hostile");
    std::fs::create_dir_all(&directory).expect("the scratch directory is writable");
    directory
}

fn write(name: &str, contents: &str) -> PathBuf {
    let path = scratch().join(name);
    let mut file = std::fs::File::create(&path).expect("writable");
    file.write_all(contents.as_bytes()).expect("written");
    path
}

/// Loads a file through the whole stack and reports only whether it succeeded.
///
/// A panic anywhere inside fails the test by itself, which is the assertion C-C10 needs.
fn load(path: &std::path::Path) -> bool {
    LayeredResolverBuilder::new()
        .with_file(FileLayer::required(path))
        .build::<Settings>()
        .resolve()
        .is_ok()
}

// ── Hand-picked hostile examples (T072) ─────────────────────────────────────────────────────

#[test]
fn malformed_input_produces_an_error_rather_than_a_panic() {
    let cases = [
        ("double-equals", "port = = 8080"),
        ("unterminated-string", "name = \"unterminated"),
        ("unterminated-table", "[server"),
        ("bare-value", "8080"),
        ("duplicate-key", "port = 1\nport = 2"),
        ("null-byte", "port = 1\u{0}\n"),
        ("lone-bracket", "]"),
        ("nested-unclosed", "a = { b = { c = "),
    ];

    for (name, contents) in cases {
        let path = write(&format!("{name}.toml"), contents);
        assert!(
            !load(&path),
            "`{name}` was accepted, which C-C10 forbids for malformed input"
        );
    }
}

#[test]
fn truncated_input_at_every_prefix_length_fails_closed() {
    // Truncation is the failure mode a parser is most likely to mishandle, because every prefix is
    // a different partial state. Every prefix of a valid document is tried.
    let complete = "[server]\nhost = \"localhost\"\nport = 8080\ntags = [\"a\", \"b\"]\n";

    for length in 0..complete.len() {
        let path = write("truncated.toml", &complete[..length]);
        // Either it parses (some prefixes are valid documents that simply lack `port`, which the
        // schema then rejects) or it errors. Both are fine; neither may panic or hang.
        let _ = load(&path);
    }

    // POSITIVE CONTROL: the untruncated document, given the key the schema needs, does load — so
    // the loop above is exercising a real parser rather than one that rejects everything.
    let path = write("truncated-full.toml", "port = 8080\n");
    assert!(load(&path), "the complete document must load");
}

#[test]
fn a_deeply_nested_document_is_bounded_rather_than_unbounded() {
    // Depth is the classic way to turn a recursive-descent parser into a stack overflow. The byte
    // ceiling bounds it before the parser sees it; this asserts the outcome is an error, not a
    // crash, at a depth far past anything a person would write.
    let depth = 5_000;
    let mut document = String::with_capacity(depth * 4);
    document.push_str("value = ");
    for _ in 0..depth {
        document.push('[');
    }
    for _ in 0..depth {
        document.push(']');
    }
    document.push('\n');

    let path = write("deep.toml", &document);
    assert!(!load(&path), "a 5000-deep document must not be accepted");
}

#[test]
fn an_oversized_file_is_refused_before_it_is_read() {
    // C-C10's unbounded-memory clause. The ceiling is checked against the file's metadata, so the
    // contents never enter the process.
    let path = write("oversized.toml", &"# padding\n".repeat(50_000));
    let refused = LayeredResolverBuilder::new()
        .with_file(FileLayer::required(&path).with_max_bytes(1024))
        .build::<Settings>()
        .resolve();
    assert!(refused.is_err(), "an oversized file must be refused");

    let error = refused.expect_err("checked above").to_string();
    assert!(error.contains("ceiling"), "and say why: {error}");
}

// ── Deterministic generated input (T073) ────────────────────────────────────────────────────

/// A deterministic 64-bit generator.
///
/// `SplitMix64`, written out: eleven lines, no dependency, and identical on every machine and
/// every run. A failing case can be reproduced by its index alone.
struct Rng(u64);

impl Rng {
    const fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn pick<'a>(&mut self, options: &[&'a str]) -> &'a str {
        let index = usize::try_from(self.next() % options.len() as u64).unwrap_or(0);
        options[index]
    }
}

/// Builds a document out of fragments that are individually plausible and jointly often invalid.
fn generate(rng: &mut Rng) -> String {
    const FRAGMENTS: &[&str] = &[
        "port = 8080\n",
        "port = \n",
        "[server]\n",
        "[server\n",
        "host = \"x\"\n",
        "host = \"\n",
        "tags = [\"a\",\n",
        "tags = []\n",
        "= 5\n",
        "\u{0}\n",
        "a.b.c = 1\n",
        "\"\"\"\n",
        "# comment\n",
        "[[array]]\n",
        "x = 1e999\n",
        "y = 0x\n",
    ];

    let count = usize::try_from(rng.next() % 12).unwrap_or(0) + 1;
    let mut document = String::new();
    for _ in 0..count {
        document.push_str(rng.pick(FRAGMENTS));
    }
    document
}

#[test]
fn generated_documents_never_panic_and_never_start_an_application() {
    // The property: for every generated input, resolution either succeeds or returns an error.
    // There is no third outcome. A panic fails this test directly; a hang fails it by timeout.
    const CASES: usize = 2_000;
    const SEED: u64 = 0x0002_0002; // Phase 002. Fixed, so a failure is reproducible by index.

    let mut rng = Rng::new(SEED);
    let mut accepted = 0_usize;

    for index in 0..CASES {
        let document = generate(&mut rng);
        let path = write("generated.toml", &document);
        if load(&path) {
            accepted += 1;
        }
        assert!(
            index < CASES,
            "unreachable, and present so the loop variable is used in the failure message"
        );
    }

    // POSITIVE CONTROL on the generator itself. If it only ever produced garbage, this test would
    // pass while proving nothing about the accepting path — so it must produce **some** documents
    // the stack accepts, and **not all** of them.
    assert!(
        accepted > 0,
        "the generator never produced an acceptable document, so the accepting path is untested"
    );
    assert!(
        accepted < CASES,
        "the generator never produced a rejected document, so the rejecting path is untested"
    );
}

#[test]
fn the_generator_is_deterministic() {
    // A property test that cannot be replayed exactly cannot be used to reproduce a defect. Two
    // runs from the same seed must produce the same sequence, on any machine.
    let mut first = Rng::new(0x0002_0002);
    let mut second = Rng::new(0x0002_0002);
    let a: Vec<String> = (0..50).map(|_| generate(&mut first)).collect();
    let b: Vec<String> = (0..50).map(|_| generate(&mut second)).collect();
    assert_eq!(a, b, "the same seed must produce the same sequence");

    // POSITIVE CONTROL: a different seed produces a different sequence, so equality above is a
    // property of the seed rather than of a generator that always emits the same thing.
    let mut third = Rng::new(0x0002_0003);
    let c: Vec<String> = (0..50).map(|_| generate(&mut third)).collect();
    assert_ne!(a, c);
}

#[test]
fn generated_environments_never_panic() {
    // The other hostile boundary: environment text. Every value is attacker-influenced in a
    // container, and the two-candidate decode path parses text as TOML.
    let mut rng = Rng::new(0x0002_00E0);

    for _ in 0..500 {
        let value = rng
            .pick(&[
                "",
                "0",
                "-1",
                "999999999999999999999999",
                "true",
                "[",
                "\"",
                "\u{0}",
                "1e999",
                "0x",
                "inf",
                "nan",
                "  ",
                "\n",
            ])
            .to_owned();

        let mut variables = BTreeMap::new();
        variables.insert("RENVOR_PORT".to_owned(), value);

        // Succeeds or errors. Never panics — a panic fails this test on its own.
        let _ = LayeredResolverBuilder::new()
            .with_environment_map("RENVOR_", variables)
            .build::<Settings>()
            .resolve();
    }
}

/// W-005 security finding 4.1, and its re-review follow-up SV-N2 — a file whose reported length is
/// a lie, and a read whose bytes are bounded while its **waiting** is not.
///
/// A FIFO reports `len() == 0` and then yields as many bytes as the reader takes. 4.1's fix bounded
/// the bytes with `Read::take`, which closed the memory half. The re-review then found two variants
/// that still waited for ever, and this test covers **all three**:
///
/// | Variant | Before | Now |
/// |---|---|---|
/// | flooding writer | memory until the process died, then refused at the ceiling | refused, no read |
/// | **no writer at all** | `File::open` blocked for ever — the ceiling was never reached | refused, no open |
/// | **slow writer holding the descriptor** | `read_to_end` waited for an EOF that never came | refused, no open |
///
/// The refusal is now the file *type*, decided on a `stat` that never opens the path — which is
/// why the two blocking variants terminate. `TIMEOUT` is what makes the claim testable: a hang is
/// not an assertion failure, it is a test that never returns, so each variant runs on a worker and
/// the main thread refuses to wait longer than the bound.
///
/// `#[cfg(unix)]` because a FIFO is a unix concept. Linux is the claimed platform and Ubuntu CI is
/// the authority; this also runs on the maintainer's macOS workstation.
#[cfg(unix)]
#[test]
fn a_fifo_is_refused_by_type_before_any_read_can_block() {
    use std::sync::mpsc;

    const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

    let directory = std::env::temp_dir().join(format!("renvor-fifo-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("a temporary directory");

    // Each variant gets its own FIFO. Reusing one would let a writer left over from a previous
    // variant satisfy the next one's open, which is exactly the confound being tested for.
    let make_fifo = |name: &str| {
        let path = directory.join(name);
        // `mkfifo` via the shell: creating one needs `libc`, and this workspace forbids `unsafe`.
        let made = std::process::Command::new("mkfifo")
            .arg(&path)
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
        // W-005 re-review RV-N15: this used to print "skipping: mkfifo unavailable" and return,
        // which reports a hostile-input test that never ran as a pass. `mkfifo(1)` is POSIX and
        // present on every platform this `cfg(unix)` block compiles for, so its absence is a
        // broken environment and is reported as one.
        assert!(
            made,
            "mkfifo(1) is POSIX and required on this platform; a missing one is a broken test \
             environment, not a reason to report a pass"
        );
        let reported = std::fs::metadata(&path).expect("the fifo exists").len();
        assert_eq!(
            reported, 0,
            "a fifo reports no length; that is the whole point"
        );
        path
    };

    // Runs `read()` on a worker so a hang shows up as a timeout rather than as a test that never
    // finishes. The worker is deliberately not joined on the timeout path: if it is blocked in
    // `open`, nothing can interrupt it, and joining would reproduce the hang in the harness.
    let read_within_timeout = |path: std::path::PathBuf, variant: &'static str| {
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let outcome = FileLayer::required(&path).with_max_bytes(4096).read();
            // The category travels with the message: `KernelError` is not sent across the
            // channel, so the assertion has to carry what it needs to check.
            let _ = sender.send(
                outcome
                    .map(|_| ())
                    .map_err(|error| (error.category(), error.to_string())),
            );
        });
        match receiver.recv_timeout(TIMEOUT) {
            Ok(outcome) => outcome,
            Err(_) => panic!("{variant}: read() did not return within {TIMEOUT:?} — it is waiting"),
        }
    };

    // Variant 1: no writer. `File::open` on this would block for ever.
    let no_writer = make_fifo("no-writer.toml");
    let error = read_within_timeout(no_writer, "no writer")
        .expect_err("a fifo is not a configuration file");

    // Variant 2: a slow writer that sends a little and never closes. `read_to_end` on this would
    // wait for an EOF that never arrives.
    let slow = make_fifo("slow-writer.toml");
    let slow_writer = std::thread::spawn({
        let path = slow.clone();
        move || {
            if let Ok(mut pipe) = std::fs::OpenOptions::new().write(true).open(&path) {
                let _ = pipe.write_all(b"a = 1\n");
                std::thread::sleep(TIMEOUT);
            }
        }
    });
    let slow_error =
        read_within_timeout(slow, "slow writer").expect_err("a fifo is not a configuration file");

    // Variant 3: the original flooding writer from 4.1.
    let flooding = make_fifo("flooding-writer.toml");
    let flooding_writer = std::thread::spawn({
        let path = flooding.clone();
        move || {
            if let Ok(mut pipe) = std::fs::OpenOptions::new().write(true).open(&path) {
                let block = vec![b'a'; 64 * 1024];
                while pipe.write_all(&block).is_ok() {}
            }
        }
    });
    let flooding_error = read_within_timeout(flooding, "flooding writer")
        .expect_err("an endless stream must be refused, not consumed");

    for (variant, (category, message)) in [
        ("no writer", &error),
        ("slow writer", &slow_error),
        ("flooding writer", &flooding_error),
    ] {
        assert_eq!(*category, ErrorCategory::Configuration, "{variant}");
        assert!(
            message.contains("regular file"),
            "{variant}: the refusal must name the file type, not the byte ceiling: {message}"
        );
    }

    // POSITIVE CONTROL. Every assertion above is about a refusal, and a `read()` that refused
    // everything unconditionally would satisfy all of them. A real regular file in the same
    // directory, read by the same call, must still succeed.
    let regular = directory.join("regular.toml");
    std::fs::write(&regular, b"a = 1\n").expect("a regular file");
    let table = FileLayer::required(&regular)
        .with_max_bytes(4096)
        .read()
        .expect("a regular file is still readable")
        .expect("and it is present");
    assert!(
        table.contains_key("a"),
        "the control must actually parse: {table:?}"
    );

    // Both writers exit on EPIPE or on their own sleep; neither is joined, because a writer
    // blocked on `open` for a reader that never came cannot be woken.
    drop(slow_writer);
    drop(flooding_writer);
    let _ = std::fs::remove_dir_all(&directory);
}
