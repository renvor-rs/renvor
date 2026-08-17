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

/// Loads a file and returns the rendered error, failing if it somehow succeeded.
fn load_error(path: &std::path::Path) -> String {
    LayeredResolverBuilder::new()
        .with_file(FileLayer::required(path))
        .build::<Settings>()
        .resolve()
        .err()
        .map(|error| error.to_string())
        .expect("a hostile document must not resolve")
}

// ── Bounded diagnostics (T146) ──────────────────────────────────────────────────────────────

/// Resolves two files whose shapes disagree at `key`, returning the rendered conflict.
///
/// The conflict path rather than the decode path, and the reason is worth recording: a giant
/// **unknown** key never reaches a diagnostic at all. `PartialSettings` does not deny unknown
/// fields, so `<giant> = 1` is silently ignored and the resolver fails with `missing field
/// "port"` — a message naming a four-character key. The first version of this test asserted
/// against that message and would have "proved" the bound while never exercising it.
///
/// A shape conflict is different: `merge.rs` compares the shape of **every** key across layers
/// before the schema is consulted, so an unknown key that is an integer in one file and a string
/// in another is named in full.
fn conflict_error(label: &str, key: &str) -> String {
    let first = write(&format!("{label}-first.toml"), &format!("{key} = 1\n"));
    let second = write(&format!("{label}-second.toml"), &format!("{key} = \"x\"\n"));

    LayeredResolverBuilder::new()
        .with_file(FileLayer::required(&first))
        .with_file(FileLayer::required(&second))
        .build::<Settings>()
        .resolve()
        .err()
        .map(|error| error.to_string())
        .expect("two layers disagreeing about a key's shape must fail")
}

#[test]
fn a_gigantic_key_produces_a_bounded_error_through_the_real_stack() {
    // T146, measured end to end rather than at the constructor. Before this commit a 9,999,999
    // byte key produced a 10,000,269 byte message — the key, verbatim, wrapped in a sentence.
    //
    // Two sizes an order of magnitude apart, because the property is not "small" but "does not
    // grow with the input". One size alone cannot tell a bound from a coincidence.
    let mut lengths = Vec::new();
    for size in [100_000_usize, 1_000_000] {
        let rendered = conflict_error(&format!("giant-key-{size}"), &"k".repeat(size));

        assert!(
            rendered.len() < 2_048,
            "a {size}-byte key produced a {}-byte message",
            rendered.len()
        );
        assert!(
            rendered.contains("truncated"),
            "the message does not say it was truncated: {rendered}"
        );
        lengths.push(rendered.len());
    }

    let growth = lengths[1].abs_diff(lengths[0]);
    assert!(
        growth <= 8,
        "the message grew by {growth} bytes when the key grew 10-fold, so it is not bounded"
    );
}

#[test]
fn a_gigantic_astral_key_does_not_amplify_through_the_real_stack() {
    // A TOML key may contain any Unicode, and a key made of 4-byte characters is the file-layer
    // analogue of the environment layer's 3x lossy expansion: its byte length is four times its
    // character count, so a ceiling expressed in characters would be wrong by 4x here. The bound
    // is stated in bytes for exactly this reason.
    let key = "𝄞".repeat(250_000); // 1,000,000 bytes; 250,000 characters
    assert_eq!(key.len(), 1_000_000);

    let rendered = conflict_error("giant-astral", &format!("\"{key}\""));

    assert!(
        rendered.len() < 2_048,
        "a 1 MB astral-plane key produced a {}-byte message",
        rendered.len()
    );
    assert!(
        rendered.contains("1000000"),
        "the original byte length is missing: {rendered}"
    );
}

#[test]
fn a_gigantic_environment_variable_name_produces_a_bounded_error() {
    // The `configuration` constructor, through the real stack, on the layer where an operator is
    // most likely to be handed a name by somebody else. `with_environment_map` supplies the
    // variables directly because `std::env::set_var` is `unsafe` in edition 2024 and this
    // workspace forbids `unsafe`.
    //
    // The value is deliberately undecodable so the failure names the KEY: a variable that decodes
    // cleanly produces no diagnostic to measure.
    let mut variables = BTreeMap::new();
    variables.insert(
        format!("RENVOR_PORT{}", "X".repeat(500_000)),
        "1".to_owned(),
    );
    variables.insert("RENVOR_PORT".to_owned(), "not-a-number".to_owned());

    let rendered = LayeredResolverBuilder::new()
        .with_environment_map("RENVOR_", variables)
        .build::<Settings>()
        .resolve()
        .err()
        .map(|error| error.to_string())
        .expect("an undecodable port must fail");

    assert!(
        rendered.len() < 2_048,
        "a 500 KB variable name produced a {}-byte message",
        rendered.len()
    );
}

#[test]
fn an_ordinary_key_is_still_named_in_full() {
    // POSITIVE CONTROL for both tests above, and the one that matters most in practice. A bound
    // that truncated ordinary keys would satisfy every size assertion here and make every real
    // diagnostic useless.
    let path = write("ordinary-key.toml", "port = \"not-a-number\"\n");
    let rendered = load_error(&path);

    assert!(
        rendered.contains("port"),
        "the key is not named: {rendered}"
    );
    assert!(
        !rendered.contains("truncated"),
        "an ordinary key was truncated: {rendered}"
    );
}

#[test]
fn a_gigantic_file_path_produces_a_bounded_error() {
    // Source attribution, not the key. The layer label is a *path*, chosen by whoever launched the
    // process, and it reaches every diagnostic and every attribution row. The file need not exist
    // — a required file that is missing names the path, which is the shortest route to the label.
    let long = "d".repeat(200_000);
    let path = scratch().join(format!("{long}.toml"));

    let rendered = LayeredResolverBuilder::new()
        .with_file(FileLayer::required(&path))
        .build::<Settings>()
        .resolve()
        .err()
        .map(|error| error.to_string())
        .expect("a missing required file must fail");

    assert!(
        rendered.len() < 2_048,
        "a 200 KB path produced a {}-byte message",
        rendered.len()
    );
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

/// The measured check-then-open race (T143), run against the fix.
///
/// # What this reproduces
///
/// Until T143, `FileLayer::read()` called `std::fs::metadata(path)` and then, separately,
/// `std::fs::File::open(path)`. Two independent resolutions of one name. An attacker who can write
/// the containing directory swaps a regular file for a FIFO between them: `metadata` reports a
/// regular file, the type check passes, and `open` then blocks for ever on the FIFO that arrived
/// in its place. The race was won in **101 attempts, 1,517 ms** against the old code, and it was
/// reachable on the public `FileLayer::read()`, which no outer deadline covers.
///
/// The path is a **symlink** and the attacker re-points it with `rename`, which is atomic. That is
/// a cleaner reproduction than deleting and recreating a file: there is no instant at which the
/// path is absent, so a failure here is the race and not a missing file.
#[cfg(unix)]
#[test]
fn replacing_the_path_between_check_and_open_cannot_block() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, mpsc};

    const ATTEMPTS: usize = 400;
    const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

    let directory = std::env::temp_dir().join(format!("renvor-toctou-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("a temporary directory");

    let regular = directory.join("regular.toml");
    std::fs::write(&regular, "port = 8080\n").expect("a regular file");

    let fifo = directory.join("pipe.fifo");
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .map(|status| status.success())
            .unwrap_or(false),
        "mkfifo(1) is POSIX and required on this platform"
    );

    let target = directory.join("target.toml");
    std::os::unix::fs::symlink(&regular, &target).expect("the target symlink");

    let stop = Arc::new(AtomicBool::new(false));

    // The attacker. Flips `target` between the regular file and the FIFO as fast as it can.
    let swapper = std::thread::spawn({
        let directory = directory.clone();
        let regular = regular.clone();
        let fifo = fifo.clone();
        let target = target.clone();
        let stop = Arc::clone(&stop);
        move || {
            let staging = directory.join("staging.link");
            let mut point_at_fifo = true;
            while !stop.load(Ordering::Relaxed) {
                let _ = std::fs::remove_file(&staging);
                let destination = if point_at_fifo { &fifo } else { &regular };
                if std::os::unix::fs::symlink(destination, &staging).is_ok() {
                    // `rename` over an existing path is atomic: the victim always sees one or the
                    // other, never neither.
                    let _ = std::fs::rename(&staging, &target);
                }
                point_at_fifo = !point_at_fifo;
            }
        }
    });

    // The victim, on a worker so that a block shows up as a timeout rather than as a test that
    // never finishes. It is not joined on the timeout path: a thread blocked in `open` cannot be
    // interrupted by any Rust API, and joining would reproduce the hang inside the harness.
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn({
        let target = target.clone();
        move || {
            let (mut parsed, mut refused) = (0_usize, 0_usize);
            for _ in 0..ATTEMPTS {
                // Only that it RETURNS matters here. `Ok` means the call caught the regular file,
                // `Err` means it caught the FIFO and refused it by type; both are bounded.
                match FileLayer::required(&target).with_max_bytes(4096).read() {
                    Ok(_) => parsed += 1,
                    Err(_) => refused += 1,
                }
            }
            let _ = sender.send((parsed, refused));
        }
    });

    let outcome = receiver.recv_timeout(TIMEOUT);
    stop.store(true, Ordering::Relaxed);
    let _ = swapper.join();

    let (parsed, refused) = match outcome {
        Ok(counts) => counts,
        Err(_) => panic!(
            "read() did not return within {TIMEOUT:?} while the path was being swapped — the \
             check-then-open race is back, and a direct FileLayer::read() caller can be blocked \
             for ever by a FIFO substituted after the type check"
        ),
    };

    // POSITIVE CONTROL: the race was actually exercised. If the swapper never won, every attempt
    // would land on the regular file and this test would "pass" having reproduced nothing at all
    // — which is precisely how the original defect survived a green suite.
    assert_eq!(
        parsed + refused,
        ATTEMPTS,
        "the victim did not complete every attempt"
    );
    assert!(
        parsed > 0 && refused > 0,
        "the swap never took effect: {parsed} attempts saw a regular file and {refused} saw the \
         fifo, so this run did not reproduce the race it exists to test"
    );

    let _ = std::fs::remove_dir_all(&directory);
}

// ── R4-1: the diagnostic bound, on the schema shape the first fix missed ─────────────────────

/// A schema that **denies unknown fields**, which `Settings` above deliberately does not.
///
/// That difference is the entire reason R4-1 escaped T146. Every bounded-diagnostic test written
/// for T146 used a permissive schema, where an unknown key is silently ignored and never reaches a
/// message at all. Under `deny_unknown_fields` — the fail-closed shape the constitution pushes an
/// author toward — `serde` produces ``unknown field `<key>`, expected one of …``, and that message
/// travelled into `KernelError::Configuration`'s `constraint` field, which the first fix left
/// unbounded while carefully bounding `key` beside it.
///
/// Measured before the fix: a 1,000,000-byte key produced a **1,000,363-byte** message.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictSettings {
    #[allow(dead_code)]
    port: u16,
}

#[allow(dead_code)]
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictPartialSettings {
    port: Option<u16>,
}

impl ConfigSchema for StrictSettings {
    type Partial = StrictPartialSettings;
}

#[test]
fn a_rejected_unknown_key_produces_a_bounded_error() {
    let mut lengths = Vec::new();
    for size in [100_000_usize, 1_000_000] {
        let key = "k".repeat(size);
        let path = write(
            &format!("strict-unknown-{size}.toml"),
            &format!("port = 8080\n{key} = 1\n"),
        );

        let rendered = LayeredResolverBuilder::new()
            .with_file(FileLayer::required(&path))
            .build::<StrictSettings>()
            .resolve()
            .err()
            .map(|error| error.to_string())
            .expect("deny_unknown_fields must reject the extra key");

        assert!(
            rendered.len() < 2_048,
            "a {size}-byte unknown key produced a {}-byte message",
            rendered.len()
        );
        lengths.push(rendered.len());
    }

    // The property, not a magnitude: a ten-fold larger key must not make a larger message.
    let growth = lengths[1].abs_diff(lengths[0]);
    assert!(
        growth <= 8,
        "the message grew by {growth} bytes when the key grew 10-fold"
    );
}

#[test]
fn a_valid_document_still_resolves_under_the_strict_schema() {
    // POSITIVE CONTROL. Without it, a `StrictSettings` that rejected *everything* would satisfy
    // the test above perfectly while proving nothing about unknown keys specifically.
    let path = write("strict-valid.toml", "port = 8080\n");
    LayeredResolverBuilder::new()
        .with_file(FileLayer::required(&path))
        .build::<StrictSettings>()
        .resolve()
        .expect("a document matching the strict schema must resolve");
}
