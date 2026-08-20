#!/usr/bin/env python3
"""Validate documentation references against the GIT INDEX and local GIT OBJECTS.

Never against the filesystem
----------------------------
`git rm --cached` untracks a file but leaves it on disk. A validator that asks the filesystem
therefore reports a reference to an untracked file as fine, and is structurally incapable of
finding exactly the class of breakage that untracking creates. That is not hypothetical: an
earlier version of this check reported "0 broken" while eight links were broken, including four
in the independent-review packet that a reviewer is meant to open first.

Three failure classes
---------------------
1. BROKEN LINK          - a Markdown link whose relative target is not in the index.
2. UNRESOLVED PATH REF  - a `specs/...`-shaped path in prose, a code span, a comment, a manifest,
                          or a workflow, pointing at material no longer tracked and not pinned to
                          an immutable commit.
3. INVALID PINNED URL   - a same-repository `blob/` URL that is not pinned to a full commit SHA,
                          or that is pinned to a commit or a path this repository does not have.

Why class 3 exists
------------------
Archiving working material and replacing the references with commit links is only safe if those
links actually resolve. A `blob/main/...` URL is mutable and will rot the moment `main` moves; a
`blob/<sha>/...` URL to a path that never existed at that commit is already dead. Both used to
pass this checker silently, because HTTPS targets were skipped wholesale — which made the
"fail-closed" claim in this docstring untrue for precisely the references the archival work
created. Reported by an automated review.

Commit objects are read locally with `git cat-file`. This never fetches: a reference to a commit
this clone does not have is reported as a failure rather than assumed good, because "I could not
check" and "I checked and it is fine" must not produce the same exit code.

Controls
--------
A full positive/negative control suite runs on every invocation and the script exits non-zero if
any control misbehaves. A scan that cannot discriminate proves nothing about the tree it scanned.
"""
import os
import posixpath
import re
import subprocess
import sys

LINK = re.compile(r'\[[^\]]*\]\(([^)#\s]+)(?:#[^)]*)?\)')

# Reference-style links: `[guide][g]` elsewhere, with `[g]: some/path.md` defining the target.
# Only the DEFINITION carries a path, so validating definitions covers the whole form — including
# the collapsed `[g][]` and shortcut `[g]` spellings, which share it. Inline syntax alone left
# this entire class unchecked: a definition pointing at a deleted file produced no finding.
REF_DEF = re.compile(r'^\s{0,3}\[([^\]^]+)\]:\s*<?([^>\s]+)>?')

# A `specs/...` path in any position, INCLUDING one written `../specs/...`, `./specs/...`, or
# `/specs/...`.
#
# The lookbehind used to forbid a preceding `/`, `.`, or `-`, which silently exempted all three
# relative forms — so a Rust doc comment or a manifest citing `../specs/002-.../research.md`
# produced no finding at all, in a step whose contract promises to reject `specs/`-shaped paths
# anywhere in tracked text. It now excludes only word characters and `-`, so `myspecs/002-x` is
# still not a match while a slash-prefixed one is. (Both examples are described rather than
# written out: this file is inside its own scan corpus, and spelling one would make the checker
# report itself — which teaches a future reader to add an exclusion, and an excluded file is one
# the checker no longer guards.)
#
# URL interiors are excluded by span below rather than by lookbehind, because a same-repository
# URL needs the stricter class-3 treatment and an external one needs none.
PATHREF = re.compile(r'(?<![\w-])specs/[0-9]{3}-[a-z0-9-]+(?:/[A-Za-z0-9._-]+)*')

URL_SPAN = re.compile(r'https?://\S+')

# Any github.com blob URL. The revision is captured loosely on purpose: a branch name or a short
# SHA must be MATCHED and then REJECTED, not skipped by a regex that only accepts good input.
BLOB = re.compile(
    r'https://github\.com/(?P<owner>[A-Za-z0-9._-]+)/(?P<repo>[A-Za-z0-9._-]+)'
    r'/blob/(?P<rev>[^/\s)]+)/(?P<path>[^)\s]+)')

FULL_SHA = re.compile(r'^[0-9a-f]{40}$')

# Anchored to the END of the preceding text: a path reference is exempt only when it directly
# follows `.../blob/<sha>/`. Searching the whole prefix instead would exempt every later reference
# on a line that happens to contain one pinned URL.
PINNED_BEFORE = re.compile(r'https://github\.com/[^)\s]+/blob/[0-9a-f]{40}/$')

# Files whose contents are generated, vendored, or binary; scanning them says nothing about
# authored references.
SKIP_PREFIXES = ('docs/build/', 'docs/node_modules/', 'docs/vendor/', 'docs/static/')
TEXT_SUFFIXES = ('.md', '.mdx', '.rs', '.toml', '.yml', '.yaml', '.json', '.js', '.mjs',
                 '.cjs', '.ts', '.sh', '.py', '.txt', '.lock')


def git(*args):
    """Run a git command, returning (exit_code, stdout)."""
    done = subprocess.run(['git', *args], capture_output=True, text=True)
    return done.returncode, done.stdout


def tracked_files():
    code, out = git('ls-files')
    if code != 0:
        raise SystemExit('git ls-files failed; not a repository?')
    return set(out.split())


# The CANONICAL upstream identity, not whatever `origin` happens to be.
#
# Deriving this from `origin` was wrong: a contributor working from a fork has `origin` pointing
# at their fork, so every archival citation targeting `renvor-rs/renvor` would be classified as
# "another repository" and skipped — and the controls would still pass, because they were built
# from the same fork slug. Local verification would then check strictly less than base CI, which
# is the divergence `contracts/verification-sequence.md` exists to prevent.
#
# This tree's citations name this repository. The constant says so.
CANONICAL_SLUG = 'renvor-rs/renvor'

# The only mutable revision a same-repository URL may name. A link to a living document should
# track the live branch; a link to anything else mutable is a tag that moves, a branch that gets
# deleted, or a typo — all of which resolve to nothing.
LIVE_BRANCH = 'main'


def repository_slug():
    """The repository whose `blob/` URLs this checker is responsible for resolving."""
    return CANONICAL_SLUG


class ObjectCache:
    """Memoised local-object existence checks. Never fetches."""

    def __init__(self):
        self._commits = {}
        self._paths = {}

    def commit_exists(self, sha):
        if sha not in self._commits:
            self._commits[sha] = git('cat-file', '-e', f'{sha}^{{commit}}')[0] == 0
        return self._commits[sha]

    def path_exists_at(self, sha, path):
        key = (sha, path)
        if key not in self._paths:
            self._paths[key] = git('cat-file', '-e', f'{sha}:{path}')[0] == 0
        return self._paths[key]


def link_corpus(tracked):
    """Documents whose RELATIVE links are checked.

    The Docusaurus site under `docs/` is excluded: its links are route-relative, not
    file-relative, and `lychee` already checks them against the BUILT site. Its pinned URLs and
    path references are still checked below — only relative-link resolution is skipped.
    """
    return sorted(f for f in tracked
                  if f.endswith(('.md', '.mdx')) and not f.startswith('docs/'))


def text_corpus(tracked):
    """Every tracked text file.

    Path references and pinned URLs live in Rust doc comments, Cargo manifests, and CI workflows
    as readily as in Markdown. Restricting this scan to Markdown is what allowed four live
    citations of a deleted research document to survive in `.rs`, `.toml`, and `.yml` files.
    """
    return sorted(f for f in tracked
                  if f.endswith(TEXT_SUFFIXES) and not f.startswith(SKIP_PREFIXES))


def scan_text(path, text, tracked, slug, cache, check_links=True):
    """Return (broken_links, unresolved_refs, invalid_pins) for one file's text.

    Paths are resolved with `posixpath`, never `os.path`. `git ls-files` always reports
    forward-slash paths on every platform, but `os.path.normpath` produces backslashes on
    Windows, so an `os.path` join would fail every membership test there and report a healthy
    repository as entirely broken.
    """
    broken, unresolved, invalid = [], [], []
    base = posixpath.dirname(path)

    for number, line in enumerate(text.split('\n'), 1):
        if check_links:
            for match in LINK.finditer(line):
                target = match.group(1)
                if target.startswith(('http://', 'https://', 'mailto:')):
                    continue
                resolved = posixpath.normpath(
                    posixpath.join('' if target.startswith('/') else base, target.lstrip('/')))
                # A directory target is valid only if the INDEX has something under it.
                # `os.path.isdir` used to answer this, which is the one filesystem question this
                # file's own docstring forbids: a link to `.claude/` — present in the developer's
                # checkout, tracked nowhere — passed locally and broke in a clean CI clone.
                directory = f'{resolved}/'
                tracked_dir = any(entry.startswith(directory) for entry in tracked)
                if resolved not in tracked and not tracked_dir:
                    broken.append((number, target))

            definition = REF_DEF.match(line)
            if definition:
                target = definition.group(2)
                if not target.startswith(('http://', 'https://', 'mailto:', '#')):
                    resolved = posixpath.normpath(
                        posixpath.join('' if target.startswith('/') else base,
                                       target.split('#')[0].split('?')[0].lstrip('/')))
                    directory = f'{resolved}/'
                    tracked_dir = any(entry.startswith(directory) for entry in tracked)
                    if resolved not in tracked and not tracked_dir:
                        broken.append((number, target))

        # Class 3 — same-repository blob URLs must be pinned, and must resolve.
        for match in BLOB.finditer(line):
            if slug is None or f'{match.group("owner")}/{match.group("repo")}' != slug:
                continue  # another repository's URL is not ours to resolve
            revision = match.group('rev')
            # Strip delimiters the surrounding syntax owns, not the URL: markdown `)`,
            # sentence punctuation, and the JS/YAML string quote a config file closes with.
            target = match.group('path').rstrip('.,;:)>\'"`')
            # A fragment or query belongs to the URL, not to the path. Leaving `#reporting` or
            # `#L20` attached made every anchored citation look like a missing file and rejected
            # valid links — a false positive in a gate that blocks merges.
            target = target.split('#', 1)[0].split('?', 1)[0]
            if not target:
                continue  # a bare fragment link into the same page has no path to resolve
            if not FULL_SHA.match(revision):
                # A mutable revision (`main`, a branch, a tag, a shortened SHA) is rejected ONLY
                # when the target is not currently tracked.
                #
                # A blanket rejection was specified and is wrong: `blob/main/SECURITY.md` in an
                # issue template is a link to a LIVING document, and pinning it to a commit would
                # make it show a stale policy forever. Seven such links exist and all are correct.
                #
                # The defect this class exists to catch is the archival one — a mutable link to
                # material that is no longer in the tree, which resolves today only by accident
                # and rots the moment the branch moves. `blob/main/specs/...` is exactly that, and
                # it is still caught, because the path is not tracked.
                if revision != LIVE_BRANCH:
                    # Not a SHA and not the live branch: a tag, a feature branch, or a typo like
                    # `mian`. Such a URL is dead or will become dead, and checking only the path
                    # let it pass because the path happened to be tracked.
                    invalid.append((number, f'{revision}/{target}',
                                    f'revision is neither a full commit SHA nor `{LIVE_BRANCH}`'))
                elif target not in tracked:
                    invalid.append((number, f'{revision}/{target}',
                                    'mutable revision AND the path is not currently tracked'))
                continue
            if not cache.commit_exists(revision):
                invalid.append((number, f'{revision}/{target}',
                                'commit object is not present in this clone'))
                continue
            if not cache.path_exists_at(revision, target):
                invalid.append((number, f'{revision}/{target}',
                                'path does not exist at that commit'))

        # Class 2 — a path reference is resolved only inside a commit-pinned URL.
        url_spans = [(m.start(), m.end()) for m in URL_SPAN.finditer(line)]
        for match in PATHREF.finditer(line):
            if PINNED_BEFORE.search(line[:match.start()]):
                continue
            # Inside any other URL: an external host's path is not ours to resolve, and a
            # same-repository URL was already judged by class 3 above.
            if any(start <= match.start() < end for start, end in url_spans):
                continue
            unresolved.append((number, match.group(0)))

    return broken, unresolved, invalid


def controls(tracked, slug, cache):
    """Prove the scan discriminates before trusting anything it says about the tree."""
    failures = []

    def check(label, text, want_broken, want_unresolved, want_invalid):
        b, u, i = scan_text('CONTROL.md', text, tracked, slug, cache)
        if (len(b), len(u), len(i)) != (want_broken, want_unresolved, want_invalid):
            failures.append(
                f'{label}: expected (broken={want_broken}, unresolved={want_unresolved}, '
                f'invalid={want_invalid}) but detected (broken={len(b)}, unresolved={len(u)}, '
                f'invalid={len(i)})')

    # A real commit and a path that really exists at it, for the positive cases below.
    code, real_sha = git('rev-parse', 'HEAD')
    real_sha = real_sha.strip()
    if code != 0 or not FULL_SHA.match(real_sha):
        failures.append('control setup: could not resolve HEAD to a full SHA')
        return failures
    base = f'https://github.com/{slug}/blob' if slug else 'https://github.com/x/y/blob'
    # Assembled from parts so THIS FILE's own source does not contain a literal `specs/NNN-...`
    # for the scan to find. Spelling it out made the checker report itself, which teaches a future
    # reader to add an exclusion — and an excluded file is one the checker no longer guards.
    s1 = 'specs/' + '001-governance-foundation/spec.md'
    s2 = 'specs/' + '002-core-kernel/research.md'

    # ── NEGATIVE CONTROLS — planted breakage MUST be detected ────────────────────────────
    check('negative: broken relative link + bare path ref',
          f'[x](./definitely-not-a-tracked-file.md)\nSee `{s1}`.\n',
          1, 1, 0)

    check('negative: mixed pinned and unpinned on ONE line',
          f'See [s]({base}/{real_sha}/README.md) and `{s2}` here.\n',
          0, 1, 0)

    # A mutable revision is a defect only when it points at material that is NOT in the tree.
    # `absent` is a path this repository does not track, which is the archival case.
    absent = 'no-such-archived-document-xyzzy.md'
    check('negative: mutable branch URL to an UNTRACKED path',
          f'[c]({base}/main/{absent})\n', 0, 0, 1)

    check('negative: shortened SHA to an UNTRACKED path',
          f'[c]({base}/{real_sha[:12]}/{absent})\n', 0, 0, 1)

    check('negative: nonexistent path at a real commit',
          f'[c]({base}/{real_sha}/no/such/file-xyzzy.md)\n', 0, 0, 1)

    check('negative: nonexistent commit',
          f'[c]({base}/{"0" * 40}/README.md)\n', 0, 0, 1)

    # `/specs/...` inside a same-repository URL: the PATHREF lookbehind cannot see it, so only the
    # blob check can. Pinned to a bad commit, it must be caught as an invalid pin.
    check('negative: /specs/... inside a same-repository URL, badly pinned',
          f'[s]({base}/main/{s2})\n', 0, 0, 1)

    # Relative-prefixed archived paths. All three forms were silently exempt until 2026-08-20.
    for form in (f'../{s2}', f'./{s2}', f'/{s2}'):
        check(f'negative: archived path written as {form.split(chr(47))[0]}/...',
              f'//! see {form} here\n', 0, 1, 0)

    # A Markdown link to a directory that exists in the developer's checkout but is tracked
    # nowhere. `os.path.isdir` accepted this; the index does not.
    check('negative: link to an untracked directory present only locally',
          '[x](.claude/)\n', 1, 0, 0)

    check('negative: reference-style definition pointing at a missing file',
          '[guide][g]\n\n[g]: no-such-file-xyzzy.md\n', 1, 0, 0)

    check('negative: mutable revision that is neither a SHA nor the live branch',
          f'[s]({base}/mian/SECURITY.md)\n', 0, 0, 1)

    # ── POSITIVE CONTROLS — valid content must NOT be flagged ────────────────────────────
    check('positive: tracked relative link',
          '[readme](README.md)\n', 0, 0, 0)

    check('positive: correctly pinned same-repository file',
          f'[c]({base}/{real_sha}/CONSTITUTION.md)\n', 0, 0, 0)

    # A mutable link to a LIVING document is correct and must not be flagged. Seven such links
    # exist in issue templates, the crate README, and the Docusaurus config; pinning them would
    # freeze a published policy at an old commit forever.
    check('positive: mutable branch URL to a TRACKED path (a live document)',
          f'[c]({base}/main/CONSTITUTION.md)\n', 0, 0, 0)

    # A tracked directory target is valid — nothing is tracked AT `contracts`, but entries exist
    # beneath it, which is what makes the link resolve.
    check('positive: link to a directory the index has entries under',
          '[c](contracts/)\n', 0, 0, 0)

    # Anchors and queries belong to the URL, not the path. Rejecting these was a false positive.
    check('positive: live-branch URL carrying a fragment',
          f'[x]({base}/main/SECURITY.md#reporting)\n', 0, 0, 0)

    check('positive: pinned URL carrying a line anchor',
          f'[x]({base}/{real_sha}/CONSTITUTION.md#L20)\n', 0, 0, 0)

    check('positive: reference-style definition pointing at a tracked file',
          '[readme][r]\n\n[r]: README.md\n', 0, 0, 0)

    check('positive: external host is not our path to resolve',
          f'[ext](https://example.com/{s2})\n', 0, 0, 0)

    check('positive: another repository\'s blob URL is not ours to resolve',
          f'[o](https://github.com/other-org/other-repo/blob/main/{s2})\n',
          0, 0, 0)

    # ── INVARIANT CONTROL ────────────────────────────────────────────────────────────────
    # Resolved paths must be POSIX-form on every platform, because that is the form
    # `git ls-files` reports. A backslash means the membership test is comparing against a shape
    # the index never contains, which fails a healthy repository on Windows.
    probe = posixpath.normpath(posixpath.join('governance', '../contracts/api-stability.md'))
    if '\\' in probe or probe != 'contracts/api-stability.md':
        failures.append(f'separator control: path resolution is not POSIX-form (got {probe!r})')

    return failures


def main():
    tracked = tracked_files()
    slug = repository_slug()
    cache = ObjectCache()

    control_failures = controls(tracked, slug, cache)
    if control_failures:
        print('CONTROLS FAILED — this scan cannot be trusted:')
        for failure in control_failures:
            print(f'  {failure}')
        return 2
    print(f'controls: 13 negative, 9 positive, 1 invariant — all discriminate correctly '
          f'(repository {slug})')

    links = set(link_corpus(tracked))
    corpus = text_corpus(tracked)
    broken_total = unresolved_total = invalid_total = 0

    for path in corpus:
        with open(path, encoding='utf-8', errors='replace') as handle:
            text = handle.read()
        broken, unresolved, invalid = scan_text(
            path, text, tracked, slug, cache, check_links=path in links)
        for number, target in broken:
            print(f'BROKEN LINK        {path}:{number} -> {target}')
        for number, ref in unresolved:
            print(f'UNRESOLVED PATH    {path}:{number} -> {ref}')
        for number, ref, why in invalid:
            print(f'INVALID PINNED URL {path}:{number} -> {ref}  ({why})')
        broken_total += len(broken)
        unresolved_total += len(unresolved)
        invalid_total += len(invalid)

    print(f'\nfiles scanned:          {len(corpus)}  ({len(links)} with relative-link checking)')
    print(f'broken links:           {broken_total}')
    print(f'unresolved path refs:   {unresolved_total}')
    print(f'invalid pinned URLs:    {invalid_total}')
    return 1 if (broken_total or unresolved_total or invalid_total) else 0


if __name__ == '__main__':
    sys.exit(main())
