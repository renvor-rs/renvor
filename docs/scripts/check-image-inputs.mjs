#!/usr/bin/env node
/**
 * No-image-input guard for the `image-size` isolation (T108, ADR-0009).
 *
 * WHY THIS EXISTS
 * ---------------
 * `image-size` reaches this project only as a build-time transitive dependency of
 * `@docusaurus/mdx-loader`, which calls it to measure images referenced from MDX.
 * Two high-severity advisories (GHSA-w3rx-r6r6-pgpr, GHSA-5p2g-fcmc-qvqq — infinite
 * loops in the ICNS, JXL and HEIF parsers) have NO fixed version at any release, and
 * the upstream repository is archived, so no fix is coming from that direction.
 *
 * The advisories are now closed by removal rather than by upgrade: an npm `overrides`
 * entry redirects `image-size` to `vendor/image-size-disabled`, a local no-op, so the
 * vulnerable parsers are never installed. The security premise no longer depends on
 * this script.
 *
 * What the script protects is the consequence of that removal. The replacement cannot
 * measure anything — it throws — so an image added to this site would build without
 * width/height attributes and shift layout as it loads.
 *
 * THIS IS THE ONLY FAIL-CLOSED CONTROL, AND THAT IS MEASURED, NOT ASSUMED
 * ----------------------------------------------------------------------
 * The stub's throw does NOT fail a build. `transformImage/index.js` wraps the call in
 * `try { … } catch (err) { console.error(err); logger.warn(…) }` and continues. Measured
 * end-to-end with this guard bypassed: `npx docusaurus build` prints the stub's error and
 * a warning, then exits **0**, emitting `<img …>` with no width and no height. Throwing
 * and returning `{}` differ only in console output.
 *
 * So this script is not a belt beside the throw's braces. It is the whole belt. Every
 * gap in it is a gap in the only enforcement that exists, which is why the coverage rules
 * below are deliberately broad and fail closed.
 *
 * FAIL-CLOSED BY CONSTRUCTION
 * ---------------------------
 * Wired as `prebuild` in package.json, so `npm run build` cannot proceed unless this
 * exits 0. It also exits non-zero on its own internal errors (unreadable directory,
 * unexpected exception) rather than skipping — a check that cannot run is a failure,
 * never a skip, matching the project's verification contract.
 *
 * Its behaviour is asserted by `scripts/check-image-inputs.test.mjs`, which plants each
 * caught form and requires exit 1, and asserts exit 0 on a clean tree. Without that, an
 * edit narrowing this file would leave CI green — the guard would still run, and would
 * still print "ok", because "ok" is what a broken guard prints too.
 *
 * COVERAGE RULES, AND WHY EACH ONE IS SHAPED THIS WAY
 * --------------------------------------------------
 * 1. SCAN BY EXCLUSION, NOT BY INCLUSION. An earlier version listed `docs`, `src`, `blog`.
 *    That silently missed `i18n/` and `versioned_docs/` — the two trees `docusaurus
 *    write-translations` and `docusaurus docs:version` create, both compiled by the same
 *    mdx-loader. A new content root must be covered by default rather than by somebody
 *    remembering to add it here.
 * 2. A SYMLINK IS A VIOLATION. `Dirent.isFile()` and `isDirectory()` are both false for a
 *    symlink, so an earlier version walked past symlinked trees entirely: a symlinked
 *    directory of documentation was invisible to this guard and fully visible to
 *    mdx-loader, proven through a real `npm run build`. Rather than resolve links and
 *    reason about cycles, any symlink under a scanned root is refused and must be
 *    justified by a human.
 * 3. REFERENCE-STYLE IMAGES COUNT. `![alt][ref]` with a separate `[ref]: ./x.png`
 *    definition has no parenthesised destination, so an inline-only regex misses it.
 * 4. BARE ESM IMPORTS COUNT. `import logo from "./logo.png"` is not a call expression, so
 *    a `require(`/`import(` pattern misses it.
 * 5. ANY JSX ELEMENT WITH AN IMAGE `src` COUNTS, not only lowercase `<img`. `<Image
 *    src="./x.png" />` reaches the same loader chain.
 * 6. THE EXTENSION SET IS BROAD. The exception rests on "no image input at all", not on
 *    avoiding the three advisory formats.
 */

import {readdirSync, readFileSync, lstatSync} from 'node:fs';
import {join, extname, relative, sep} from 'node:path';
import {fileURLToPath} from 'node:url';

const ROOT = fileURLToPath(new URL('..', import.meta.url));

// Everything under the site root is scanned EXCEPT these. Inverted deliberately — see
// coverage rule 1. `vendor/` holds the no-op replacement itself and ships no content, but
// it is scanned anyway: an image committed there would be as much of an input as any other.
const EXCLUDED_DIRS = new Set(['node_modules', 'build', '.docusaurus', '.git', '.cache']);

// The one permitted raster: referenced from site config, served as a byte stream, never
// parsed by mdx-loader. Permitted only under `static/`, so a `favicon.ico` smuggled into a
// content directory is still refused.
const isPermittedFavicon = (rel) =>
  rel.split(sep)[0] === 'static' && rel.split(sep).at(-1) === 'favicon.ico';

// Node tooling run by npm scripts — NOT site content, and never reached by mdx-loader or
// webpack. Their TEXT is not pattern-scanned, because this guard and its own test file
// necessarily contain image specifiers as examples and fixtures, and a check that flags its
// own documentation is a check somebody deletes. They ARE still scanned for raster assets:
// excluding a directory from one rule must not exclude it from all of them.
const TOOLING_DIRS = new Set(['scripts']);

const MDX_EXTENSIONS = new Set(['.md', '.mdx']);
// JSX lives here. Deliberately excludes `.mjs`/`.cjs`: this file is `.mjs` and contains the
// literal `<img … src=…` pattern inside its own regex, which would self-match.
const JSX_EXTENSIONS = new Set(['.jsx', '.tsx']);
// Import/require of an image asset can appear in any of these.
const CODE_EXTENSIONS = new Set(['.js', '.ts', '.mjs', '.cjs', '.jsx', '.tsx']);

// Raster formats. The advisories name ICNS, JXL and HEIF specifically, but the guard is
// deliberately broader: the exception rests on "no image input at all", not on avoiding
// three formats. `.tga`, `.jfif`, `.apng`, `.jp2`, `.pnm`, `.qoi`, `.jpe`, `.pcx` and
// `.xbm` were added after a review measured each of them passing.
const RASTER_EXTENSIONS = new Set([
  '.png', '.jpg', '.jpeg', '.jpe', '.jfif', '.gif', '.webp', '.avif', '.apng',
  '.bmp', '.tiff', '.tif', '.tga', '.ico', '.icns', '.jxl', '.heic', '.heif',
  '.psd', '.cur', '.dds', '.ktx', '.jp2', '.jpx', '.pnm', '.pbm', '.pgm', '.ppm',
  '.qoi', '.pcx', '.xbm', '.xpm',
]);

// An image extension as it appears inside a quoted specifier.
const IMAGE_EXT_PATTERN = [...RASTER_EXTENSIONS, '.svg'].map((e) => e.slice(1)).join('|');

// Markdown inline image: ![alt](src) — the form mdx-loader resolves and measures.
const MARKDOWN_IMAGE = /!\[[^\]]*\]\(([^)]+)\)/g;
// Markdown reference definition: [ref]: ./x.png — the destination half of ![alt][ref].
// Matched on the definition rather than the use, because the definition carries the path.
const MARKDOWN_REFERENCE = new RegExp(
  String.raw`^[ \t]*\[[^\]]+\]:[ \t]*<?([^\s>]+\.(?:${IMAGE_EXT_PATTERN}))`,
  'gim',
);
// Any JSX/HTML element with a src=… — not only `<img>`, because `<Image src=…>` and other
// wrappers reach the same loader chain.
const ELEMENT_IMAGE = new RegExp(
  String.raw`<[A-Za-z][\w.-]*\b[^>]*\bsrc\s*=\s*(?:"([^"]*)"|'([^']*)'|\{([^}]*)\})`,
  'gi',
);
// require()/import() of an image asset — the call form.
const ASSET_CALL = new RegExp(
  String.raw`(?:require|import)\s*\(\s*['"]([^'"]+\.(?:${IMAGE_EXT_PATTERN}))['"]\s*\)`,
  'gi',
);
// Bare ESM import of an image asset — the statement form, which has no parentheses.
const ASSET_IMPORT = new RegExp(
  String.raw`\bimport\b[^;\n]*?\bfrom\s*['"]([^'"]+\.(?:${IMAGE_EXT_PATTERN}))['"]`,
  'gi',
);

const violations = [];

function walk(dir) {
  let entries;
  try {
    entries = readdirSync(dir, {withFileTypes: true});
  } catch (error) {
    if (error.code === 'ENOENT') return; // an optional directory simply not present
    throw error; // anything else is a check that could not run — surface it
  }
  for (const entry of entries) {
    const full = join(dir, entry.name);
    const rel = relative(ROOT, full);

    // Coverage rule 2. A symlink is neither isFile() nor isDirectory(), so an earlier
    // version fell through both branches and skipped it in silence. Refuse instead of
    // resolving: following links means reasoning about cycles and about escapes outside
    // the site root, and this guard's whole job is to be the thing that cannot be walked
    // past. `lstat` rather than `stat` so the link itself is inspected, not its target.
    let stats;
    try {
      stats = lstatSync(full);
    } catch (error) {
      if (error.code === 'ENOENT') continue; // vanished between readdir and lstat
      throw error;
    }
    if (stats.isSymbolicLink()) {
      violations.push({
        file: rel,
        reason:
          'symlink inside a scanned tree — refused rather than followed, because a ' +
          'symlinked subtree is invisible to this guard and fully visible to mdx-loader',
      });
      continue;
    }

    if (stats.isDirectory()) {
      if (EXCLUDED_DIRS.has(entry.name) || entry.name.startsWith('.')) continue;
      walk(full);
    } else if (stats.isFile()) {
      inspect(full, rel);
    }
  }
}

function inspect(file, rel) {
  const ext = extname(file).toLowerCase();

  if (RASTER_EXTENSIONS.has(ext)) {
    if (!isPermittedFavicon(rel)) {
      violations.push({file: rel, reason: `raster image asset (${ext}) present under the site root`});
    }
    return;
  }

  // Tooling text is not pattern-scanned — see TOOLING_DIRS. The raster check above still ran.
  if (TOOLING_DIRS.has(rel.split(sep)[0])) return;

  const isMdx = MDX_EXTENSIONS.has(ext);
  const isJsx = JSX_EXTENSIONS.has(ext);
  const isCode = CODE_EXTENSIONS.has(ext);
  if (!isMdx && !isCode) return;

  const text = readFileSync(file, 'utf8');
  const lineOf = (index) => text.slice(0, index).split('\n').length;
  const add = (m, reason) => violations.push({file: rel, line: lineOf(m.index), reason});

  if (isMdx) {
    for (const m of text.matchAll(MARKDOWN_IMAGE)) {
      add(m, `Markdown image embed: ${m[0].slice(0, 60)}`);
    }
    for (const m of text.matchAll(MARKDOWN_REFERENCE)) {
      add(m, `Markdown image reference definition: ${m[0].trim().slice(0, 60)}`);
    }
  }
  if (isMdx || isJsx) {
    for (const m of text.matchAll(ELEMENT_IMAGE)) {
      const src = m[1] ?? m[2] ?? m[3] ?? '';
      // An element `src` is only an image input if it names one. A `<script src>` or a
      // `<Tabs src>` is not, and flagging those would make the guard noisy enough to be
      // switched off — which is the failure mode this whole file exists to prevent.
      if (!new RegExp(String.raw`\.(?:${IMAGE_EXT_PATTERN})\b`, 'i').test(src)) continue;
      add(m, `element with image src: ${m[0].slice(0, 60)}`);
    }
  }
  if (isCode || isMdx) {
    for (const m of text.matchAll(ASSET_CALL)) add(m, `image asset require/import(): ${m[1]}`);
    for (const m of text.matchAll(ASSET_IMPORT)) add(m, `image asset ESM import: ${m[1]}`);
  }
}

try {
  walk(ROOT);
} catch (error) {
  console.error('[T108] image-input guard could not complete — treating as FAILURE, not skip.');
  console.error(`       ${error.message}`);
  process.exit(2);
}

if (violations.length > 0) {
  violations.sort((a, b) => a.file.localeCompare(b.file) || (a.line ?? 0) - (b.line ?? 0));
  console.error('');
  console.error('[T108] BUILD REFUSED — image input found while image-size is disabled.');
  console.error('');
  for (const v of violations) {
    console.error(`  ${v.file}${v.line ? `:${v.line}` : ''}`);
    console.error(`      ${v.reason}`);
  }
  console.error('');
  console.error('  GHSA-w3rx-r6r6-pgpr and GHSA-5p2g-fcmc-qvqq (both high, CVSS 7.5) have no fixed');
  console.error('  version, and image-size upstream is archived, so T108 closed them by removing');
  console.error('  the package: an npm override points image-size at vendor/image-size-disabled,');
  console.error('  a no-op that throws instead of measuring. This site therefore cannot size an');
  console.error('  image, and one added here would render without width/height and shift layout.');
  console.error('');
  console.error('  This guard is the ONLY control that fails the build. The replacement throws,');
  console.error('  but mdx-loader catches that and continues, so a bypass here ships a');
  console.error('  dimensionless image in a build that reports success.');
  console.error('');
  console.error('  To proceed, either remove the image input, or restore a real image-size and');
  console.error('  retire this guard with it. Do not weaken or delete this check on its own to');
  console.error('  make a build pass.');
  console.error('');
  process.exit(1);
}

console.log(
  `[T108] image-input guard: ok — no image assets, embeds, references, or imports under ${relative(process.cwd(), ROOT) || '.'}${sep}`,
);
