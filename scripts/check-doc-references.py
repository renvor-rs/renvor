#!/usr/bin/env python3
"""Validate every reference in tracked Markdown against the GIT INDEX, not the filesystem.

Why the index and not `os.path.exists`
--------------------------------------
`git rm --cached` untracks a file but leaves it on disk. A validator that asks the filesystem
therefore reports a reference to an untracked file as fine, and is structurally incapable of
finding exactly the class of breakage that untracking creates. That is not hypothetical: an
earlier version of this check reported "0 broken" while eight links were broken, including four
in the independent-review packet that a reviewer is meant to open first.

Two failure classes
-------------------
1. BROKEN LINK           - a Markdown link whose relative target is not in the index.
2. UNRESOLVED PATH REF   - a `specs/...`-shaped path in prose, a code span, or a table cell,
                           pointing at material that is no longer tracked and is not pinned to
                           an immutable commit.

Controls
--------
Both a positive and a negative control run on every invocation. If either misbehaves the script
fails, because a scan that cannot discriminate proves nothing about the tree it scanned.
"""
import os
import posixpath
import re
import subprocess
import sys

LINK = re.compile(r'\[[^\]]*\]\(([^)#\s]+)(?:#[^)]*)?\)')
PATHREF = re.compile(r'(?<![\w/.-])specs/[0-9]{3}-[a-z0-9-]+(?:/[A-Za-z0-9._-]+)*')
# Anchored to the END of the text preceding a match: a path is resolved only when it directly
# follows `.../blob/<sha>/`. Searching the whole prefix instead would exempt every later
# reference on a line that happens to contain one pinned URL — a false negative in a checker
# whose entire purpose is to miss nothing. Found by an automated review; see the control below.
PINNED_BEFORE = re.compile(r'https://github\.com/[^)\s]+/blob/[0-9a-f]{40}/$')


def tracked_files():
    out = subprocess.run(['git', 'ls-files'], capture_output=True, text=True, check=True).stdout
    return set(out.split())


def markdown_corpus(tracked):
    return sorted(f for f in tracked
                  if f.endswith(('.md', '.mdx')) and not f.startswith('docs/'))


def scan_text(path, text, tracked):
    """Return (broken_links, unresolved_refs) for one document's text.

    Paths are resolved with `posixpath`, never `os.path`. `git ls-files` always reports
    forward-slash paths on every platform, but `os.path.normpath` produces backslashes on
    Windows — so an `os.path` join would make every membership test fail there and report a
    healthy repository as entirely broken. Markdown link targets are forward-slash by
    definition, so POSIX semantics are the correct ones on both sides of the comparison.
    """
    broken, unresolved = [], []
    base = posixpath.dirname(path)
    for number, line in enumerate(text.split('\n'), 1):
        for match in LINK.finditer(line):
            target = match.group(1)
            if target.startswith(('http://', 'https://', 'mailto:')):
                continue
            resolved = posixpath.normpath(
                posixpath.join('' if target.startswith('/') else base, target.lstrip('/')))
            if resolved not in tracked and not os.path.isdir(resolved):
                broken.append((number, target))
        # A path-like reference is resolved only when it sits inside a commit-pinned URL.
        for match in PATHREF.finditer(line):
            if PINNED_BEFORE.search(line[:match.start()]):
                continue
            unresolved.append((number, match.group(0)))
    return broken, unresolved


def controls(tracked):
    """Prove the scan discriminates before trusting anything it says about the tree."""
    failures = []

    # NEGATIVE CONTROL: planted breakage must be detected.
    bad = "[x](./definitely-not-a-tracked-file.md)\nSee `specs/001-governance-foundation/spec.md`.\n"
    b, u = scan_text('CONTROL.md', bad, tracked)
    if len(b) != 1:
        failures.append(f'negative control: expected 1 broken link, detected {len(b)}')
    if len(u) != 1:
        failures.append(f'negative control: expected 1 unresolved path ref, detected {len(u)}')

    # NEGATIVE CONTROL 2 — the mixed line. A pinned reference earlier on the same line must NOT
    # exempt an unpinned one after it. This exact false negative shipped once.
    sha = '0123456789abcdef0123456789abcdef01234567'
    mixed = (f'See [s](https://github.com/renvor-rs/renvor/blob/{sha}/specs/001-governance-foundation/spec.md) '
             'and `specs/002-core-kernel/research.md` locally.\n')
    _, u = scan_text('CONTROL.md', mixed, tracked)
    if len(u) != 1:
        failures.append(
            f'negative control (mixed pinned/unpinned line): expected 1 unresolved ref, detected {len(u)}')

    # POSITIVE CONTROL: valid content must NOT be flagged.
    pinned = ('https://github.com/renvor-rs/renvor/blob/'
              '0123456789abcdef0123456789abcdef01234567/specs/001-governance-foundation/spec.md')
    good = f'[README](README.md) and [pinned]({pinned}) and [ext](https://example.com/specs/002-x/y.md)\n'
    b, u = scan_text('CONTROL.md', good, tracked)
    if b:
        failures.append(f'positive control: valid links flagged as broken: {b}')
    if u:
        failures.append(f'positive control: pinned/external refs flagged as unresolved: {u}')

    # CONTROL: resolved paths must be POSIX-form on every platform, because that is the form
    # `git ls-files` reports. A backslash here means the membership test below is comparing
    # against a shape the index never contains, which fails a healthy repository on Windows.
    probe = posixpath.normpath(posixpath.join('governance', '../contracts/api-stability.md'))
    if '\\' in probe or probe != 'contracts/api-stability.md':
        failures.append(f'separator control: path resolution is not POSIX-form (got {probe!r})')

    return failures


def main():
    tracked = tracked_files()
    corpus = markdown_corpus(tracked)

    control_failures = controls(tracked)
    if control_failures:
        print('CONTROLS FAILED — this scan cannot be trusted:')
        for failure in control_failures:
            print(f'  {failure}')
        return 2
    print('controls: negative detects planted breakage, positive passes valid content — OK')

    broken_total = unresolved_total = 0
    for path in corpus:
        with open(path, encoding='utf-8', errors='replace') as handle:
            broken, unresolved = scan_text(path, handle.read(), tracked)
        for number, target in broken:
            print(f'BROKEN LINK        {path}:{number} -> {target}')
        for number, ref in unresolved:
            print(f'UNRESOLVED PATH    {path}:{number} -> {ref}')
        broken_total += len(broken)
        unresolved_total += len(unresolved)

    print(f'\ndocuments scanned:      {len(corpus)}')
    print(f'broken links:           {broken_total}')
    print(f'unresolved path refs:   {unresolved_total}')
    return 1 if (broken_total or unresolved_total) else 0


if __name__ == '__main__':
    sys.exit(main())
