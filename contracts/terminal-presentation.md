---
description: "Contract C-8 — human terminal presentation: semantic roles, colour policy, layout, and prompt behaviour"
version: "1.0.0"
status: "normative — public contract from the first release that ships it; nothing has been published yet. first explicit version assigned to this contract text on 2026-08-21. This version identifies the contract text, not a stability promise"
---

# Contract C-8 — Human terminal presentation

**Status**: defined alongside the implementation it governs, per constitution principle V and FR-002.
**Everything in this file is a public contract from the first release that ships it.**

## What this contract does and does not govern

It governs **how the human-facing output looks**: which colour carries which meaning, when colour
is permitted at all, how a label and a value are laid out at a given terminal width, and what a
prompt does when the operator cancels.

It governs **nothing a machine consumes**. Command names, subcommands, flags, aliases, defaults,
argument semantics, exit codes, the JSON envelope, stream ownership, cancellation classification,
non-TTY behaviour, offline behaviour, generated project contents, filesystem safety, redaction, and
terminal-control-character neutralisation are defined by
[`command-surface.md`](command-surface.md), [`json-output.md`](json-output.md),
[`error-taxonomy.md`](error-taxonomy.md), and [`generation-transaction.md`](generation-transaction.md).
**This contract changes none of them and may not be read as amending any of them.**

Where this document and those documents appear to disagree, **they win**.

## Semantic roles

Output names a **meaning**. The appearance is resolved in exactly one place, and no command, no
test, and no template contains a colour name or an escape sequence.

| Role | Colour | Used for |
|---|---|---|
| accent | cyan | the active question, the selected choice, a command name the reader should type |
| success | green | `DONE`, `OK`, a completed operation |
| information | blue | `INFO` |
| warning | amber/yellow | `WARN` |
| error | red | `ERROR` |
| muted | dim bright-black | secondary text, hints, timestamps, inactive choices, leader dots |
| value | the terminal's own foreground | a value the reader came for |
| heading | the terminal's own foreground, bold | a section heading |

**`value` and `heading` are deliberately uncoloured.** They render in whatever foreground the
reader configured. Assigning them a colour would be wrong on half the world's terminal themes and
would override a choice the reader made on purpose.

`muted` is bright-black **with** a dim effect rather than a dimmed default foreground, because a
dimmed default is illegible on several light themes while bright-black is a palette entry the
reader's own theme defines.

### Colour is never the only signal

Every role that carries meaning also carries a **word**: `INFO`, `WARN`, `ERROR`, `DONE`, and — on
a key/value row — `OK`, `TOO OLD`, `MISSING`, `ABSENT`. A reader with no colour at all loses
decoration and loses nothing else.

This is a requirement, not a courtesy. It is what makes the output usable when piped, when captured
by CI, under `TERM=dumb`, by a screen reader, and by a reader who does not distinguish red from
green.

### No emoji

Not anywhere, in any mode. Emoji render at one, two, or zero columns depending on the terminal, the
font, and the platform, which makes a column-aligned layout unalignable — and screen readers
announce them by their CLDR names. The vocabulary is ASCII words, plus the box-drawing and
geometric characters the prompt library draws its own frame with.

## Colour policy

| Condition | Styling |
|---|---|
| `--output json` | **never** |
| the stream is not a terminal | **never** |
| `TERM=dumb` | **never** |
| `--no-color` | **never** |
| `NO_COLOR` present and non-empty | **never** |
| a human-format terminal with none of the above | enabled |

Four rules govern how those combine, and each of them is a rule rather than an implementation
detail:

1. **Every clause is a veto.** There is no clause that *enables* styling. Styling is what remains
   when nothing has forbidden it.
2. **An explicit refusal beats a force-colour environment.** `--no-color` and `NO_COLOR` win over
   `CLICOLOR_FORCE` and any other variable asking for colour. Renvor resolves the question once and
   **tells** its prompt library the answer rather than letting the library consult the environment
   itself, so one process cannot disagree with itself about a flag the operator set.
3. **`NO_COLOR` follows <https://no-color.org> exactly: present and *non-empty*.** `NO_COLOR=` is
   how a script cancels an inherited request, and treating it as set would make that inexpressible.
4. **The decision is per stream, not per process.** `renvor doctor > report.txt` leaves `stderr` a
   terminal while `stdout` is a file. One answer for both would put escape sequences in the file.

**Cursor visibility is not styling.** A prompt or a progress indicator hides the cursor whatever
this policy says, so it is restored whatever this policy says — including when the process is about
to exit from a panic, which runs no destructors.

## Stream ownership

Unchanged from [`command-surface.md`](command-surface.md) and restated only so that no reader has
to infer it from a presentation document:

| Stream | Carries |
|---|---|
| `stdout` | **the command's result, and nothing else.** With `--output json`, exactly one JSON document |
| `stderr` | prompts, progress, warnings, diagnostics, and error text |

In particular: a review screen, a progress indicator, and a human error message are **`stderr`**.

A **status label is not a stream** — it goes wherever the thing it labels goes. `DONE  Created 10
files` is part of `renvor new`'s result and is therefore on `stdout`, together with the dotted rows
beneath it; `ERROR  …` labels a diagnostic and is on `stderr`. An earlier version of this
paragraph said every status label was `stderr`, which was false for every command that has a
result — and which the test suite contradicted rather than enforced.

## JSON invariance

With `--output json`, this contract has **no observable effect at all**. The document is
byte-for-byte what it was before any of this existed: same schema, same values, same key order,
same framing, no escape sequences, no prompt, no progress.

That is not a promise about intent — it is a property of the structure. The human report and the
JSON document are built from separate values, and the reporter emits **exactly one of the two**.

## Layout

### Key/value rows

A label and a value are joined by leader dots and aligned to the right edge:

```
Project name ................................. demo
```

- **Display width is measured in columns, not in bytes and not in characters.** `日本語` is three
  characters and six columns; a combining mark is one character and zero columns. Both are laid out
  correctly, and both are tested.
- **Leaders are used only when a run of at least three dots fits** alongside the whole label and the
  whole value. Fewer than three reads as punctuation rather than as spacing.
- **The threshold is derived from the content, not from a column count.** A short label and a short
  value align happily at 40 columns; a long path with a long label does not fit at 100. There is no
  "80 columns" rule, because 80 is right in neither direction.
- **Below the threshold the row stacks** — label on its own line, value indented beneath it.
- **Nothing is ever truncated.** A value here may be a destination path, a version, or a tool name;
  a path shortened to fit is a path that looks like a *different* path. A stacked value that is
  still too long is left for the terminal to wrap, which is lossless.
- **No width panics.** Every width from zero upward produces output.

A row may end in a **mark**: a right-aligned state token, padded so a run of rows ends in one
straight edge.

```
cargo ..................................... 1.94.0        OK
docker .................................. optional        ABSENT
```

`OK`, `TOO OLD`, `MISSING`, and `ABSENT` are words. They mean the same thing with the colour
removed.

**The example shows two rows a real run can produce.** It previously showed a `MISSING` row, and
`renvor doctor` cannot emit one: a required tool that is absent or too old fails before the table
is built, and the optional tools declare no minimum version. `TOO OLD` and `MISSING` remain part
of the vocabulary — they are what a future optional tool with a minimum version would use — but a
contract should not publish an example of output the program does not produce.

### Status lines

```
INFO   Creating a Renvor application
DONE   Application created successfully
```

The label is padded to a fixed column, so every message starts in the same place whichever label
precedes it.

### Non-terminal width

When the stream is not a terminal the layout uses a **fixed** width rather than none, so piped
output is byte-identical between a developer's machine and CI.

### Maximum measure

However wide the terminal is, a leader run is capped. A 200-column terminal would otherwise put 180
dots between a label and its value, and a reader tracking a value back across that distance is
doing the work the leader was supposed to do for them.

## Prompts

### Framing

Questions are drawn as a connected sequence: a title, a rail joining the questions, and a close.
The live question is the accent colour; an answered question's rail and marker recede to muted
while **its answer stays readable**, because the reason to look back at the sequence is to check
what was answered.

**Nothing is validated at a prompt, and that is deliberate.** An answer is validated once, after
the sequence, by the same validator the flag surface uses — which is what makes "prompt and flag
inputs resolve to identical configuration" a property of the type graph rather than a test that
happens to pass. Validating here would put a second validator on one of the two paths, and an
operator would get a retry loop from the wizard and a refusal from the flags for the same input.

The consequence is that a validation *message* attached to its question is a capability the
drawing library has and this program does not use. An earlier version of this section described it
as behaviour, which it is not.

### Cancellation

| Operator action | Classification | Exit |
|---|---|---|
| Ctrl-C | cancelled | `4` |
| Escape | cancelled | `4` |
| declining the review screen | cancelled | `4` |
| no terminal to prompt on | usage | `2` |
| any other prompt failure | internal — **a defect** | `1` |

Ctrl-C and Escape are different events with the same meaning, and distinguishing them in the exit
code would make every script handle two cases for one outcome. A missing terminal is a wrong
*invocation*, not a refusal, and its message names the flags to use instead. Exit `1` stays
reserved for defects: folding an unknown prompt failure into `cancelled` would hide a bug behind an
outcome that looks deliberate.

**The terminal and the cursor are restored on every one of these paths**, and on success, and on a
validation failure, and on a panic.

### Confirmations submit on the keypress

`y` answers yes and `n` answers no **without an Enter**; arrow keys move the selection and Enter
takes it. This is a **behavioural difference from Renvor's previous prompt library**, which read
`y`/`n` as text and waited for Enter, and it is recorded here rather than left to be discovered:
typing `y` then Enter sends one keystroke too many, and the stray Enter falls through to the next
question and accepts its default.

### What prompts may not do

- A prompt is **never** shown when `stdin` is not a terminal; the command exits naming the flags
  that would have supplied the answer. This is the case every automated consumer is in.
- A prompt is **never** shown when `stderr` is not a terminal either, because that is where it
  would be drawn. Same exit code, same message, and — this is the part worth stating — the refusal
  happens **before any chrome is written**, so a redirected `stderr` gets the diagnostic and
  nothing else.

  `stdin` decides eligibility; `stderr` decides drawability. They are checked separately on
  purpose: see [`command-surface.md`](command-surface.md) for why merging them would silently
  generate a project from defaults.
- A prompt **never touches `stdout`**, so it cannot appear inside a JSON document.

  This clause is written the way it is because the obvious stronger version — *"a prompt is never
  shown in JSON mode"* — **is not true**, and stating it would have been a claim nothing enforces.
  `renvor --output json new` on a terminal enters the wizard, exactly as the human-format run does:
  [`command-surface.md`](command-surface.md) makes the wizard conditional on `stdin` being a
  terminal and on nothing else, and a JSON consumer that has left a terminal on `stdin` has asked
  for that. The questions are drawn on `stderr` and `stdout` still carries exactly one document,
  which is what [`json-output.md`](json-output.md) actually requires.

  The behaviour predates this contract and is unchanged by it. It is recorded here because it was
  **measured** while checking a claim this document originally made and could not support.
- The prompt library's own logging and note helpers are **not** used for application data. They
  write straight to the terminal, bypassing redaction and control-character neutralisation. Every
  string handed to them as **chrome** is a literal, enforced by type.
- **A prompt's suggested default is redacted and neutralised before the library draws it.** It is
  the one string in a prompt that cannot be a literal — the project name comes from `argv` and the
  local domain is derived from it — so the type cannot enforce this and the wrapper does.

  This was **measured, not assumed**, and it found a real defect: a name containing an escape
  sequence rendered that sequence into the terminal, where it recoloured everything after it. The
  build before this contract existed had the same hole. A longer sequence clears the screen or
  moves the cursor, and a prompt is precisely where the reader is about to type.

  Escaping changes what pressing Enter returns — the escaped form rather than the raw one — and
  that is the intended consequence rather than a side effect: **what the reader sees is what they
  get.** A value that needed escaping would not have survived validation anyway.

- **The suggested default is always visible at the moment of the decision**, alongside any
  guidance rather than replaced by it. This is the same rule stated from the other side, and it
  was broken: the drawing library's placeholder overrides the default it would otherwise render,
  so the project-name question showed only *"ASCII letters, digits…"* while Enter silently
  returned `demo`. The operator committed to a value that was never on screen. Found by review;
  both are now shown, value first.

## Progress

- **`stderr`, always.**
- **Absent in JSON mode, and absent whenever `stderr` is not a terminal.** Not hidden — absent: the
  calls still happen and render nowhere, so no caller has an `if` to forget.
- **Absent under `TERM=dumb`, and under `TERM` unset — but the operator is still told.** A live
  indicator redraws its own line, and neither of those terminals promises the cursor movement to
  redraw with, so the indicator is replaced by **one static line naming the operation**.

  This is a *different* condition from "not a terminal" — a dumb terminal is one — and it is
  stated separately because it was **measured**, and because the first version of this change got
  it wrong: it dropped the indicator on those terminals and put nothing in its place, turning a
  line into tens of seconds of silence through a cold five-check build. `TERM` unset is the state
  of cron, systemd units, and several embedded terminals, so that is not a rare corner.

  **The rule is that the operator is told, not that an indicator appears.**
- **Attached only to work measured in seconds whose output is captured.** Work that completes
  almost immediately gets no indicator, and work that already shows the reader its own output — a
  child process holding the terminal — gets none either.
- **Every dynamic label is redacted and neutralised**, because the indicator redraws its own line
  and a label that could move the cursor would not be confined to it.
- **It is cleared on success, on error, and on cancellation** — every exit that runs a
  destructor.
- **It is *not* cleared on panic**, and that is stated rather than promised away. The panic path
  calls `exit` without unwinding, so no destructor runs: the bar's last line survives and the
  panic message is printed over it. What *is* restored on that path is the **cursor**, which the
  prompt library hides and which would otherwise outlive the process — see *Cancellation* above.
  An earlier version of this list claimed the bar was cleared too.

## Safety

The ordering below is the contract, not an implementation note. Everything else here depends on it.

1. **Redact.** Credential-shaped values are replaced.
2. **Neutralise.** Every control character in every dynamic field is escaped — newline and tab
   included, because each field of a report is one line by construction and a value that ends the
   reader's line and starts its own is the whole forgery.
3. **Measure.** Display width is computed on the neutralised text, which is what will occupy
   columns.
4. **Style.** Escape sequences are added last, around text that is already safe.

**Styling trusted punctuation never makes an untrusted value trusted.** The value was made safe in
step 2, before anything decided what colour to paint the dots beside it.

Two independent mechanisms enforce the colour policy: the resolver declines to emit sequences, and
the writer strips them on every path that forbids styling. A defect in either one alone is not a
leak.

## `--help`

- The renderer stays the argument parser's. The **content** — usage, commands, options,
  descriptions, exit codes — is generated from the same declaration that parses the command line,
  so the documented surface cannot drift from the parsed one. It is not replaced by handwritten
  strings.
- Only the **palette** is Renvor's, and it is the same palette as everything else: a heading in
  `--help` is the heading role.
- The colour policy above applies in full, `--no-color` included — which the parser cannot know
  about on its own, so it is enforced where the help text is written.
- The help text is asserted as **expected output**, byte for byte, so a change to the surface
  appears as a diff a reviewer has to agree to.

## Accessibility

- Colour is never the only signal. See *Semantic roles*.
- No emoji.
- Labels are words in a fixed vocabulary, not symbols and not abbreviations.
- The progress indicator uses ASCII frames rather than braille, which several fixed-width fonts do
  not render and some screen readers announce character by character.
- `NO_COLOR` and `TERM=dumb` are honoured exactly.
- Nothing depends on the terminal being a particular width; the layout degrades to a stacked form
  and never truncates.

## Platform coverage

Linux, macOS, and Windows, **natively**. Windows is not reached through a compatibility layer and
is not excluded: escape sequences are translated to console API calls where virtual-terminal
processing is unavailable, and the prompt and terminal tests run against ConPTY on the Windows leg
of the matrix exactly as they run against a pty elsewhere.

The support scope itself is [`support-policy.md`](support-policy.md)'s, and this contract does not
widen or narrow it.

## Compatibility and versioning

This contract's version identifies **this text**. It is not a stability promise about the surface,
which [`api-stability.md`](api-stability.md) governs and currently declares unstable.

While the surface is unstable, the following are still true and are what a reader may rely on:

- **A change to the colour policy is a change to this contract** and requires a version bump here.
- **A change to a semantic role's meaning is a change to this contract.** A change to which colour
  renders a role is not, provided the meaning and the accompanying word are unchanged.
- **Human output is not a machine interface.** A script that parses it is relying on something no
  contract promises. `--output json` is the interface for that, and it is governed by
  [`json-output.md`](json-output.md).
- **Nothing in this contract may be satisfied by weakening one of the contracts listed at the top.**
  If honouring a presentation rule would require changing an exit code, a JSON document, a stream
  assignment, or a redaction guarantee, the presentation rule is the one that gives way.
