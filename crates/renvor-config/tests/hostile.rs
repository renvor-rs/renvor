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
// Every use of this is inside a `#[cfg(unix)]` test, because every one of them asserts on the
// refusal of a FIFO. Ungated it is an unused import on Windows, which the `platform` job has been
// reporting as a warning.
#[cfg(unix)]
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
/// | flooding writer | memory until the process died, then refused at the ceiling | refused before any read |
/// | **no writer at all** | `File::open` blocked for ever — the ceiling was never reached | refused; the open cannot block |
/// | **slow writer holding the descriptor** | `read_to_end` waited for an EOF that never came | refused before any read |
///
/// The refusal is the file *type*. **Since T143 that type is decided on `fstat` of a descriptor
/// that is already open**, not on a `stat` of the pathname — the path IS opened, with `O_NONBLOCK`
/// precisely so that opening a FIFO cannot block before the type is known. An earlier version of
/// this comment said the refusal happened "on a `stat` that never opens the path", which described
/// the code T143 replaced. `TIMEOUT` is what makes the claim testable: a hang is not an assertion
/// failure, it is a test that never returns, so each variant runs on a worker and the main thread
/// refuses to wait longer than the bound.
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

/// The check-then-open race (T143) — proven by case analysis, not sampled from a scheduler.
///
/// # What the race is
///
/// Until T143, `FileLayer::read()` called `std::fs::metadata(path)` and then, separately,
/// `std::fs::File::open(path)`. Two independent resolutions of one name. An attacker who can write
/// the containing directory swaps a regular file for a FIFO between them: `metadata` reports a
/// regular file, the type check passes, and `open` then blocks for ever on the FIFO that arrived
/// in its place. The race was won in **101 attempts, 1,517 ms** against the old code, and it was
/// reachable on the public `FileLayer::read()`, which no outer deadline covers.
///
/// # Why this test was rewritten
///
/// The first version ran an adversarial swapper thread and then asserted that the victim had
/// observed **both** file types — `parsed > 0 && refused > 0`. That is an assertion about thread
/// scheduling rather than about the code under test, and it failed on this workspace's `main` with
/// `0 attempts saw a regular file and 400 saw the fifo`, on more than one commit and more than one
/// toolchain. The swapper is a tight loop of three syscalls; the victim opens, `fstat`s, reads and
/// parses TOML. On a contended CI runner the victim only ever sampled the swapper's resting state,
/// so a "positive control" reported a defect that was not there.
///
/// It is deliberately **not** repaired by retrying, sleeping, raising the attempt count, ignoring
/// the result, or letting the job fail. Every one of those leaves the assertion probabilistic and
/// merely weights the coin — and a required check that fails on scheduling teaches people to press
/// *re-run*, which is how a real regression eventually gets waved through. It is repaired by
/// removing the need to win a race at all.
///
/// # The argument made instead
///
/// `rename(2)` over an existing path is atomic, so at the instant `read()` resolves `target` there
/// are exactly two states it can be in:
///
/// | State | What `read()` must do |
/// |---|---|
/// | **R** — `target` is the regular file | parse it |
/// | **F** — `target` is the FIFO | refuse it **and return** |
///
/// Both are checked below, deterministically, with no racing thread involved at all. `read()`
/// performs exactly **one** pathname resolution — that is T143's fix — so its behaviour is fully
/// determined by whichever of those two states it observed. Safety in both states is therefore
/// safety under every interleaving. That is a case analysis over a two-element space, which is a
/// stronger claim than a sample of a race, and it does not depend on a scheduler to hold.
///
/// A swapping phase still runs afterwards, over real interleavings. What it asserts is **safety** —
/// every attempt returned, no foreign bytes were ever parsed, every refusal was a fail-closed
/// configuration error — and never a particular mix of outcomes, which is the assertion that made
/// this test flaky.
///
/// # Why the argument is not circular
///
/// "Both states are safe" is worth nothing unless this fixture could have detected an unsafe one.
/// The last section is a negative control: the pre-T143 shape is reconstructed locally and driven
/// through the same FIFO, the same atomic `rename`, and the same timeout detection. It is shown to
/// block, and shown to be blocked *inside `open`* rather than merely unscheduled, by attaching a
/// writer and observing that this releases it. If this fixture ever loses the ability to catch
/// that, the control fails and takes the test with it.
///
/// # Why the substitution is a hard link rather than a symlink
///
/// The first version re-pointed a **symlink** with `rename`. Measured while rewriting this test,
/// that fixture produces a third outcome on macOS: `open` intermittently fails with `EINVAL` —
/// 13 to 24 times in 4,000 attempts — while a symlink's directory entry is being renamed over.
/// Zero on Linux. That is an artefact of the fixture, not of the code under test, and it would
/// have become a new platform-specific flake.
///
/// Renaming a **hard link** to one of two fixed inodes has neither problem: nothing resolves a
/// symlink body, the path is never absent, and there are only ever the two inodes. Measured at
/// 6,000 attempts per platform, on macOS and Linux, with zero errors and both states well
/// represented. It is also the closer model of the attack — the attacker replaces the file, not a
/// pointer to it.
#[cfg(unix)]
#[test]
fn replacing_the_path_between_check_and_open_cannot_block() {
    use std::os::unix::fs::OpenOptionsExt as _;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, mpsc};
    use std::time::Duration;

    /// A return inside this is a return. A hang is unbounded, so any finite bound separates the
    /// two; this one is far longer than the work and far shorter than a CI timeout.
    const RETURNS_WITHIN: Duration = Duration::from_secs(10);

    /// How long the negative control waits before calling the reference blocked. This bound is
    /// **not** load-bearing on its own — the probe that follows proves the block independently, so
    /// a slow machine cannot make the control vacuous without that probe also failing.
    const BLOCKED_AFTER: Duration = Duration::from_secs(5);

    /// Interleavings sampled by the swapping phase. Not a confidence parameter: nothing is
    /// asserted about how many of each outcome occur, so raising it would buy nothing.
    const SWAPS: usize = 400;

    let directory = std::env::temp_dir().join(format!("renvor-toctou-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("a temporary directory");

    // The two inodes, created once. Everything below only ever renames a hard link to one of them
    // over `target`; no third file is ever involved.
    let regular = directory.join("regular.toml");
    std::fs::write(&regular, "port = 8080\n").expect("a regular file");

    let fifo = directory.join("pipe.fifo");
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .map(|status| status.success())
            .unwrap_or(false),
        "mkfifo(1) is POSIX and required on this platform; a missing one is a broken test \
         environment, not a reason to report a pass"
    );

    let target = directory.join("target.toml");

    // Installs one of the two inodes at `target`, atomically. `hard_link` then `rename` rather
    // than remove-then-create: `rename(2)` over an existing path never leaves the path absent, so
    // the victim can never observe a state that is neither R nor F.
    let install = |source: &std::path::Path, staging_name: &str| {
        let staging = directory.join(staging_name);
        let _ = std::fs::remove_file(&staging);
        std::fs::hard_link(source, &staging).expect("a hard link to one of the two inodes");
        std::fs::rename(&staging, &target).expect("rename over an existing path");
    };

    // `read()` runs on a worker so that a block shows up as a timeout rather than as a suite that
    // never returns. The worker is never joined on the timeout path: a thread blocked in `open`
    // cannot be interrupted by any Rust API, and joining would reproduce the hang in the harness.
    let read_within_bound = |label: &'static str| {
        let (sender, receiver) = mpsc::channel();
        let path = target.clone();
        std::thread::spawn(move || {
            let outcome = FileLayer::required(&path).with_max_bytes(4096).read();
            // `KernelError` does not cross the channel, so the assertion's inputs travel with the
            // message instead.
            let _ = sender.send(
                outcome
                    .map(|table| table.map(|table| table.keys().cloned().collect::<Vec<_>>()))
                    .map_err(|error| (error.category(), error.to_string())),
            );
        });
        receiver.recv_timeout(RETURNS_WITHIN).unwrap_or_else(|_| {
            panic!(
                "{label}: read() did not return within {RETURNS_WITHIN:?} — the check-then-open \
                 race is back, and a direct FileLayer::read() caller can be blocked for ever by a \
                 FIFO at the path it opens"
            )
        })
    };

    // ── State R ──────────────────────────────────────────────────────────────────────────────
    //
    // The benign state, and the positive control for everything after it: a `read()` that refused
    // unconditionally would satisfy every refusal assertion in this test and be worthless.
    install(&regular, "install.regular");
    let keys = read_within_bound("state R")
        .expect("a regular file at the path is still readable")
        .expect("and it is present");
    assert_eq!(
        keys,
        vec!["port".to_owned()],
        "state R must actually parse the regular file, or the fixture is not reading what it \
         believes it is"
    );

    // ── State F ──────────────────────────────────────────────────────────────────────────────
    //
    // The state the attacker is trying to arrange, and the whole of the danger: a blocking
    // `open(2)` here waits for a writer that never arrives. `RETURNS_WITHIN` is what makes "cannot
    // block" testable at all — a hang is not an assertion failure, it is a test that never
    // returns.
    install(&fifo, "install.fifo");
    let (category, message) =
        read_within_bound("state F").expect_err("a FIFO is not a configuration file");
    assert_eq!(category, ErrorCategory::Configuration, "state F");
    assert!(
        message.contains("regular file"),
        "state F must be refused by TYPE. A refusal for any other reason — a parse failure, a byte \
         ceiling — would not show that the type was taken from the open descriptor: {message}"
    );

    // ── Real interleavings ───────────────────────────────────────────────────────────────────
    //
    // The case analysis above is the proof; this phase is a fuzz pass over interleavings a
    // scheduler actually produces. Every assertion here is a safety property that holds for any
    // mix of outcomes. Nothing is asserted about the mix itself — that assertion is what made this
    // test flaky, and re-adding it in any form re-adds the flake.
    let stop = Arc::new(AtomicBool::new(false));
    let swapper = std::thread::spawn({
        let directory = directory.clone();
        let regular = regular.clone();
        let fifo = fifo.clone();
        let target = target.clone();
        let stop = Arc::clone(&stop);
        move || {
            let staging = directory.join("swap.link");
            let mut point_at_fifo = true;
            while !stop.load(Ordering::Relaxed) {
                let _ = std::fs::remove_file(&staging);
                let source = if point_at_fifo { &fifo } else { &regular };
                if std::fs::hard_link(source, &staging).is_ok() {
                    let _ = std::fs::rename(&staging, &target);
                }
                point_at_fifo = !point_at_fifo;
            }
        }
    });

    let (sender, receiver) = mpsc::channel();
    std::thread::spawn({
        let target = target.clone();
        move || {
            let mut outcomes = Vec::with_capacity(SWAPS);
            for _ in 0..SWAPS {
                outcomes.push(
                    FileLayer::required(&target)
                        .with_max_bytes(4096)
                        .read()
                        .map(|table| {
                            table.map(|table| table.keys().cloned().collect::<Vec<String>>())
                        })
                        .map_err(|error| (error.category(), error.to_string())),
                );
            }
            let _ = sender.send(outcomes);
        }
    });
    let outcomes = receiver.recv_timeout(RETURNS_WITHIN).unwrap_or_else(|_| {
        panic!(
            "read() did not return within {RETURNS_WITHIN:?} while the path was being swapped — \
             the check-then-open race is back"
        )
    });
    stop.store(true, Ordering::Relaxed);
    let _ = swapper.join();

    assert_eq!(
        outcomes.len(),
        SWAPS,
        "the victim did not complete every attempt"
    );
    for outcome in &outcomes {
        match outcome {
            // Whatever the interleaving, a success must be the regular file's own content. A
            // `read()` that ever returned a FIFO's bytes would show up here and nowhere else.
            Ok(Some(keys)) => assert_eq!(
                keys,
                &vec!["port".to_owned()],
                "a successful read under swapping returned something other than the regular \
                 file's content"
            ),
            Ok(None) => panic!(
                "a REQUIRED layer reported the file absent, but `rename` is atomic and `target` is \
                 never unlinked"
            ),
            // Every refusal must fail closed as a configuration error. The refusal REASON is not
            // asserted: which of the two inodes a given attempt happened to catch is scheduling.
            Err((category, message)) => assert_eq!(
                *category,
                ErrorCategory::Configuration,
                "a refusal under swapping was not a fail-closed configuration error: {message}"
            ),
        }
    }

    // ── NEGATIVE CONTROL ─────────────────────────────────────────────────────────────────────
    //
    // Everything above asserts a refusal or a return. A fixture that could not produce a hang in
    // the first place would satisfy all of it. So the pre-T143 shape is reconstructed here —
    // `metadata(path)`, a type check, then a SEPARATE blocking `File::open(path)` — and driven
    // through this same FIFO, this same atomic rename, and this same timeout detection.
    //
    // The swap is choreographed rather than raced, because this reference is ours to instrument:
    // it reports when its type check has passed, waits, and opens only once the FIFO is in place.
    // The window is therefore 100% wide, and whether the control fires depends on a scheduler no
    // more than the sections above do.
    install(&regular, "control.regular");

    let (checked_sender, checked) = mpsc::channel();
    let (proceed, wait_for_swap) = mpsc::channel();
    let (opened_sender, opened) = mpsc::channel();
    std::thread::spawn({
        let target = target.clone();
        move || {
            // Resolution 1, on the pathname. This is the lookup T143 deleted.
            let is_regular_file = std::fs::metadata(&target)
                .map(|metadata| metadata.is_file())
                .unwrap_or(false);
            let _ = checked_sender.send(is_regular_file);
            let _ = wait_for_swap.recv();
            // Resolution 2, on the same pathname, now naming a different inode. No `O_NONBLOCK`:
            // this is the old code, and this is the call that never comes back.
            let _ = opened_sender.send(std::fs::File::open(&target).is_ok());
        }
    });

    assert!(
        checked
            .recv_timeout(RETURNS_WITHIN)
            .expect("the reference must reach its type check"),
        "the reference must have seen the REGULAR file, or it never passed the check that the race \
         is about"
    );
    install(&fifo, "control.fifo");
    let _ = proceed.send(());

    assert!(
        opened.recv_timeout(BLOCKED_AFTER).is_err(),
        "NEGATIVE CONTROL FAILED: the check-then-open reference returned from a blocking open() on \
         a FIFO with no writer. This fixture can no longer produce the hang it exists to detect, so \
         nothing asserted above is evidence of anything"
    );

    // …and it is BLOCKED, not merely unscheduled. A thread waiting inside `open(O_RDONLY)` on a
    // FIFO counts as an attached reader, so a non-blocking `O_WRONLY` open succeeds instead of
    // failing with `ENXIO` — and that same open completes the rendezvous and releases it. Both
    // halves were measured on Linux 6.12 and Darwin 25 before this test was written to rely on
    // them; if either stops holding, the assertions below say so rather than passing quietly.
    let release = std::fs::OpenOptions::new()
        .write(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(&fifo)
        .expect(
            "opening the FIFO for writing without blocking must succeed, which is only true while \
             a reader is attached — and the only candidate reader is the reference, inside open()",
        );
    assert!(
        opened.recv_timeout(RETURNS_WITHIN).expect(
            "attaching a writer must release a reader blocked in open(); if it does not, the \
             reference was never in open() and the control above proved nothing",
        ),
        "the released open must have succeeded"
    );
    drop(release);

    let _ = std::fs::remove_dir_all(&directory);
}

/// The structural half of T143 — the half a behavioural test provably cannot decide.
///
/// # Why this exists, stated plainly
///
/// The test above proves that `FileLayer::read()` cannot be *blocked* by either state a swapped
/// path can be in. It does **not** prove that the pathname is resolved only once, and while
/// rewriting it that gap was measured rather than assumed: `read()` was temporarily reverted to
/// the pre-T143 check-then-open shape, and the behavioural test **passed** — 400 swapped attempts,
/// five seconds, green.
///
/// It passed for a reason that no amount of tuning removes. A check-then-open implementation
/// `stat`s the pathname first, so on a path that *is* a FIFO it refuses exactly like the fixed one
/// does; the two differ only when a substitution lands inside a window of a few instructions. On
/// this workspace's CI the victim never observed the regular file at all — `0 attempts saw a
/// regular file and 400 saw the fifo` — so on that runner even a swapping test has no power. A
/// race is not something a black-box functional test decides; it is something it samples, and a
/// sample that can be empty is a gate that can be silently open.
///
/// So the discriminating check is made structurally, on the one property that actually implies the
/// safety: **`read()` resolves the pathname exactly once, and delegates that resolution.** Every
/// pathname use lives in `open_without_blocking`; the file type comes from `File::metadata`, which
/// is `fstat` on the descriptor already open; and `read_bounded` consumes that descriptor rather
/// than reopening the name. With one resolution there is no second lookup for a substitution to
/// win, so the race does not become unlikely — it stops existing.
///
/// This is a lint, and it is written like one: it is checked against the real source, and it is
/// itself checked against deliberately broken bodies at the end, so a checker that stopped
/// rejecting anything fails rather than reporting a pass. It runs on **every** platform, including
/// Windows, because the single-resolution property is platform-independent even though the
/// non-blocking open is not.
#[test]
fn read_resolves_the_pathname_exactly_once() {
    let source = source_of("src/layer/file.rs");

    // ── The real implementation ──────────────────────────────────────────────────────────────
    let read = method_body(
        &source,
        "    pub fn read(&self) -> Result<Option<Table>, KernelError> {",
    );
    resolutions_are_delegated(read)
        .expect("FileLayer::read must resolve the pathname exactly once (T143)");

    // The descriptor is consumed, never reopened from the name. A `read_bounded` that opened the
    // file itself would reintroduce the second resolution `read` just avoided.
    let bounded = method_body(
        &source,
        "    fn read_bounded(&self, file: std::fs::File) -> Result<String, KernelError> {",
    );
    assert!(
        pathname_use_in(bounded).is_none(),
        "read_bounded must consume the open descriptor, not the pathname: {:?}",
        pathname_use_in(bounded)
    );

    // ── The one function that IS allowed to name the path ────────────────────────────────────
    //
    // Two functions share this signature — the unix one and the fallback — and the `#[cfg(unix)]`
    // attribute is part of the pattern so this cannot silently match the wrong one.
    let opener = method_body(
        &source,
        "    #[cfg(unix)]\n    fn open_without_blocking(&self) -> std::io::Result<std::fs::File> {",
    );
    assert_eq!(
        code_only(opener).matches("self.path").count(),
        1,
        "the unix opener is the single point of pathname resolution and must name it exactly once"
    );
    assert!(
        code_only(opener).contains("custom_flags(libc::O_NONBLOCK)"),
        "opening without O_NONBLOCK reintroduces the blocking open a FIFO exploits, and the type \
         check placed after it would never run"
    );

    // ── CONTROLS ─────────────────────────────────────────────────────────────────────────────
    //
    // Everything above is an assertion that something is ABSENT. A checker that had stopped
    // recognising anything would satisfy all of it, which is the failure mode this whole rewrite
    // exists to remove. So the checker is run against bodies whose verdict is known.
    for (label, body, expected) in [
        (
            "the pre-T143 check-then-open shape",
            "let metadata = std::fs::metadata(&self.path)?;\n\
             if !metadata.is_file() { return Err(refuse()); }\n\
             let file = std::fs::File::open(&self.path)?;\n\
             self.read_bounded(file)",
            Some(
                "`std::fs::metadata(` — stats a pathname, which is the check half of check-then-open",
            ),
        ),
        (
            "a reopen hidden behind OpenOptions",
            "let file = self.open_without_blocking()?;\n\
             let metadata = file.metadata()?;\n\
             if !metadata.is_file() { return Err(refuse()); }\n\
             let again = std::fs::OpenOptions::new().read(true).open(&self.path)?;\n\
             self.read_bounded(again)",
            Some("`OpenOptions` — opens a pathname, and the one open is already delegated"),
        ),
        (
            "the type taken from the pathname rather than the descriptor",
            "let file = self.open_without_blocking()?;\n\
             let metadata = std::fs::symlink_metadata(&self.path)?;\n\
             if !metadata.is_file() { return Err(refuse()); }\n\
             self.read_bounded(file)",
            Some(
                "`std::fs::symlink_metadata(` — stats a pathname, which is the check half of check-then-open",
            ),
        ),
        (
            "a body that only MENTIONS the old call in a comment",
            "// The previous revision called `std::fs::metadata(&self.path)` here.\n\
             let file = self.open_without_blocking()?;\n\
             let metadata = file.metadata()?;\n\
             if !metadata.is_file() { return Err(refuse()); }\n\
             self.read_bounded(file)",
            None,
        ),
        (
            "a body that names the old call inside a diagnostic string",
            "let file = self.open_without_blocking()?;\n\
             let metadata = file.metadata()?;\n\
             if !metadata.is_file() { return Err(refuse(\"not std::fs::metadata(x)\")); }\n\
             self.read_bounded(file)",
            None,
        ),
        (
            "the real shape, restated",
            "let file = self.open_without_blocking()?;\n\
             let metadata = file.metadata()?;\n\
             if !metadata.is_file() { return Err(refuse()); }\n\
             self.read_bounded(file)",
            None,
        ),
    ] {
        assert_eq!(
            pathname_use_in(body).as_deref(),
            expected,
            "the pathname-use checker gave the wrong verdict on {label}; while it is wrong, the \
             assertions above prove nothing"
        );
    }

    // The ordering and delegation checks get controls of their own, for the same reason.
    for (label, body, must_fail_because) in [
        (
            "an open that never happens",
            "let metadata = file.metadata()?;\n\
             if !metadata.is_file() { return Err(refuse()); }",
            "resolves the pathname 0 times",
        ),
        (
            "two opens",
            "let file = self.open_without_blocking()?;\n\
             let spare = self.open_without_blocking()?;\n\
             let metadata = file.metadata()?;\n\
             if !metadata.is_file() { return Err(refuse()); }",
            "resolves the pathname 2 times",
        ),
        (
            "the type check before the open",
            "let metadata = file.metadata()?;\n\
             if !metadata.is_file() { return Err(refuse()); }\n\
             let file = self.open_without_blocking()?;",
            "checks the file type before the open that establishes it",
        ),
    ] {
        let verdict = resolutions_are_delegated(body)
            .expect_err(&format!("{label} must be rejected, and was not"));
        assert!(
            verdict.contains(must_fail_because),
            "{label} was rejected for the wrong reason: {verdict}"
        );
    }

    // ── CRLF CONTROL ─────────────────────────────────────────────────────────────────────────
    //
    // Runs on every platform, including the ones that never produce CRLF, so the Windows fix is
    // exercised rather than assumed. Verified to have power: with the normalisation removed, this
    // section fails.
    assert_eq!(normalise_line_endings("a\r\nb\r\n"), "a\nb\n");
    let crlf = source_of("src/layer/file.rs").replace('\n', "\r\n");
    assert!(
        crlf.contains("\r\n"),
        "the CRLF fixture is not actually CRLF"
    );
    let normalised = normalise_line_endings(&crlf);
    let read_from_crlf = method_body(
        &normalised,
        "    pub fn read(&self) -> Result<Option<Table>, KernelError> {",
    );
    resolutions_are_delegated(read_from_crlf)
        .expect("a CRLF checkout must reach the same verdict as an LF one");
}

/// Reads one of this crate's own source files, with line endings normalised.
fn source_of(relative: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(relative);
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{} is unreadable: {error}", path.display()));
    normalise_line_endings(&source)
}

/// Converts CRLF to LF.
///
/// Load-bearing, and learned rather than anticipated: the first version of this test failed on
/// both `platform (windows-latest, …)` jobs with *"no rustfmt-shaped closing brace"*. A Windows
/// checkout converts `\n` to `\r\n`, so every `"\n    }\n"` and every multi-line signature
/// stopped matching. The gate failed closed, which is the right direction — but a property that is
/// platform-independent must also be CHECKABLE on every platform.
///
/// Named and separated from `source_of` so the control below can exercise it on a platform that
/// never produces CRLF, instead of leaving the fix to be believed.
fn normalise_line_endings(source: &str) -> String {
    source.replace("\r\n", "\n")
}

/// Extracts one `impl`-level method body.
///
/// Relies on rustfmt's fixed shape — a method inside an `impl` closes on a line that is exactly
/// four spaces and a brace — rather than on matching braces through strings and comments. That is
/// safe here because `cargo fmt --check` is a required gate in this workspace, so a body this
/// cannot find means the source stopped being formatted, which is itself worth failing on. It is
/// reported as such rather than skipped.
fn method_body<'a>(source: &'a str, signature: &str) -> &'a str {
    let start = source
        .find(signature)
        .unwrap_or_else(|| panic!("the source no longer contains:\n{signature}"));
    let after = &source[start + signature.len()..];
    let end = after
        .find("\n    }\n")
        .unwrap_or_else(|| panic!("no rustfmt-shaped closing brace after:\n{signature}"));
    let body = &after[..end];
    assert!(
        !body.trim().is_empty(),
        "extracted an empty body for:\n{signature}"
    );
    // `code_only` below models `//` comments and ordinary string literals, and nothing else.
    // Rather than mis-scan a construct it does not know, this fails and says so.
    for unmodelled in ["r\"", "r#\"", "/*"] {
        assert!(
            !body.contains(unmodelled),
            "the body of\n{signature}\nnow contains `{unmodelled}`, which `code_only` does not \
             model; extend the stripper rather than letting it guess"
        );
    }
    body
}

/// Every way a body could resolve a pathname, and what each one would mean.
///
/// `file.metadata()` is deliberately absent: it is `fstat` on an open descriptor and takes no
/// pathname at all, which is the entire distinction T143 turns on.
const PATHNAME_USES: [(&str, &str); 6] = [
    (
        "self.path",
        "names the pathname directly instead of delegating the single resolution",
    ),
    (
        "std::fs::metadata(",
        "stats a pathname, which is the check half of check-then-open",
    ),
    (
        "std::fs::symlink_metadata(",
        "stats a pathname, which is the check half of check-then-open",
    ),
    (
        "std::fs::File::open(",
        "opens a pathname, and the one open is already delegated",
    ),
    (
        "OpenOptions",
        "opens a pathname, and the one open is already delegated",
    ),
    (
        "read_to_string(",
        "reads a pathname rather than the descriptor already open",
    ),
];

/// Strips comments and string literals, leaving only code.
///
/// This is not tidiness. The first version of this checker searched the raw body and rejected the
/// real, correct implementation — because `read()` documents the defect it fixed and names
/// `std::fs::metadata(&self.path)` in a comment while calling neither. A gate that reads
/// commentary is a gate that can be switched off by rewording a comment, or tripped by explaining
/// the very bug it guards against.
///
/// Rust's full lexical grammar is not reimplemented here. Raw strings and block comments are
/// rejected outright by `method_body` instead, so this handles what remains: `//` comments and
/// ordinary string literals with escapes.
fn code_only(body: &str) -> String {
    let mut code = String::with_capacity(body.len());
    let mut characters = body.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '/' if characters.peek() == Some(&'/') => {
                for next in characters.by_ref() {
                    if next == '\n' {
                        code.push('\n');
                        break;
                    }
                }
            }
            '"' => {
                while let Some(next) = characters.next() {
                    if next == '\\' {
                        characters.next();
                    } else if next == '"' {
                        break;
                    }
                }
                // A placeholder, so that two adjacent literals cannot fuse into a new token.
                code.push_str("\"\"");
            }
            _ => code.push(character),
        }
    }
    code
}

/// Reports the first pathname use in a body's CODE, or `None`.
fn pathname_use_in(body: &str) -> Option<String> {
    let code = code_only(body);
    PATHNAME_USES
        .iter()
        .filter_map(|(needle, reason)| code.find(needle).map(|at| (at, needle, reason)))
        .min_by_key(|(at, _, _)| *at)
        .map(|(_, needle, reason)| format!("`{needle}` — {reason}"))
}

/// The whole structural invariant, as one verdict, so the controls exercise the same code the
/// real assertion does.
fn resolutions_are_delegated(body: &str) -> Result<(), String> {
    let body = &code_only(body);
    if let Some(use_of_path) = pathname_use_in(body) {
        return Err(format!(
            "the body resolves a pathname itself: {use_of_path}. Every pathname use belongs in \
             `open_without_blocking`, so that exactly one lookup happens and there is no second \
             one for a substitution to win"
        ));
    }

    let opens = body.matches("self.open_without_blocking()").count();
    if opens != 1 {
        return Err(format!(
            "the body resolves the pathname {opens} times through `open_without_blocking`; T143 \
             requires exactly one"
        ));
    }

    let open_at = body
        .find("self.open_without_blocking()")
        .expect("counted above");
    let fstat_at = body.find("file.metadata()").ok_or_else(|| {
        "the body never takes metadata from the open descriptor; the file type must come from \
         `File::metadata`, which is `fstat`, and not from the name"
            .to_owned()
    })?;
    let check_at = body.find("metadata.is_file()").ok_or_else(|| {
        "the body never checks the file type; a configuration source that is not a regular file \
         must be refused as a class"
            .to_owned()
    })?;

    if !(open_at < fstat_at && fstat_at < check_at) {
        return Err(
            "the body checks the file type before the open that establishes it; a check that runs \
             before the open is checking a name rather than the thing that will be read"
                .to_owned(),
        );
    }

    Ok(())
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
