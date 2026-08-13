#!/usr/bin/env node
/**
 * Reachability guard for the `image-size` advisory exception (T108).
 *
 * WHY THIS EXISTS
 * ---------------
 * `image-size` reaches this project only as a build-time transitive dependency of
 * `@docusaurus/mdx-loader`, which calls it to measure images referenced from MDX.
 * Two high-severity advisories (GHSA-w3rx-r6r6-pgpr, GHSA-5p2g-fcmc-qvqq — infinite
 * loops in the ICNS, JXL and HEIF parsers) have NO fixed version at any release, and
 * the upstream repository is archived, so no fix is coming from that direction.
 *
 * The documentation site is currently safe from those parsers for one reason only:
 * it embeds no images, so `mdx-loader` never invokes them. That is a property of the
 * content, and content changes. This script turns that property into an enforced
 * precondition instead of an assumption someone has to remember.
 *
 * FAIL-CLOSED BY CONSTRUCTION
 * ---------------------------
 * Wired as `prebuild` in package.json, so `npm run build` cannot proceed unless this
 * exits 0. It also exits non-zero on its own internal errors (unreadable directory,
 * unexpected exception) rather than skipping — a check that cannot run is a failure,
 * never a skip, matching the project's verification contract.
 *
 * The exception this guards is time-bounded. When it lapses or is lifted, delete this
 * script and the `prebuild` hook together; leaving the guard without the exception, or
 * the exception without the guard, is worse than having neither.
 */

import {readdirSync, readFileSync, statSync} from 'node:fs';
import {join, extname, relative} from 'node:path';
import {fileURLToPath} from 'node:url';

const ROOT = fileURLToPath(new URL('..', import.meta.url));

// Directories whose contents are compiled by mdx-loader, plus the static root. `static/`
// is copied verbatim rather than parsed, but a raster image landing there is a strong
// signal that image content is arriving and the exception's premise is eroding.
const SCANNED = ['docs', 'src', 'blog'];
const STATIC_DIR = 'static';

const MDX_EXTENSIONS = new Set(['.md', '.mdx']);

// Raster formats. The advisories name ICNS, JXL and HEIF specifically, but the guard is
// deliberately broader: the exception rests on "no image input at all", not on avoiding
// three formats. SVG is excluded — it is markup, is not measured by image-size's binary
// parsers, and the site's own brand marks are SVG.
const RASTER_EXTENSIONS = new Set([
  '.png', '.jpg', '.jpeg', '.gif', '.webp', '.avif', '.bmp', '.tiff', '.tif',
  '.ico', '.icns', '.jxl', '.heic', '.heif', '.psd', '.cur', '.dds', '.ktx',
]);

// Markdown image syntax: ![alt](src) — the form mdx-loader resolves and measures.
const MARKDOWN_IMAGE = /!\[[^\]]*\]\(([^)]+)\)/g;
// JSX/HTML <img src="…">, including the MDX-idiomatic <img src={require(...)} />.
const HTML_IMAGE = /<img\b[^>]*\bsrc\s*=\s*(?:"([^"]*)"|'([^']*)'|\{([^}]*)\})/gi;
// require()/import of an image asset, which routes through the same loader chain.
const ASSET_REQUIRE = /(?:require|import)\s*\(\s*['"]([^'"]+\.(?:png|jpe?g|gif|webp|avif|bmp|tiff?|ico|icns|jxl|heif?|heic))['"]\s*\)/gi;

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
    if (entry.isDirectory()) {
      if (entry.name === 'node_modules' || entry.name.startsWith('.')) continue;
      walk(full);
    } else if (entry.isFile()) {
      inspect(full);
    }
  }
}

function inspect(file) {
  const rel = relative(ROOT, file);
  const ext = extname(file).toLowerCase();

  if (RASTER_EXTENSIONS.has(ext)) {
    violations.push({file: rel, reason: `raster image asset (${ext}) present in a scanned directory`});
    return;
  }
  if (!MDX_EXTENSIONS.has(ext)) return;

  const text = readFileSync(file, 'utf8');
  const lineOf = (index) => text.slice(0, index).split('\n').length;

  for (const m of text.matchAll(MARKDOWN_IMAGE)) {
    violations.push({file: rel, line: lineOf(m.index), reason: `Markdown image embed: ${m[0].slice(0, 60)}`});
  }
  for (const m of text.matchAll(HTML_IMAGE)) {
    const src = m[1] ?? m[2] ?? m[3] ?? '';
    violations.push({file: rel, line: lineOf(m.index), reason: `<img> element: src=${src.slice(0, 60)}`});
  }
  for (const m of text.matchAll(ASSET_REQUIRE)) {
    violations.push({file: rel, line: lineOf(m.index), reason: `image asset import: ${m[1]}`});
  }
}

function scanStatic() {
  const dir = join(ROOT, STATIC_DIR);
  let entries;
  try {
    entries = readdirSync(dir, {withFileTypes: true, recursive: true});
  } catch (error) {
    if (error.code === 'ENOENT') return;
    throw error;
  }
  for (const entry of entries) {
    if (!entry.isFile()) continue;
    const ext = extname(entry.name).toLowerCase();
    // favicon.ico is the one permitted exception: it is referenced from site config,
    // served as a static byte stream, and never parsed by mdx-loader.
    if (entry.name === 'favicon.ico') continue;
    if (RASTER_EXTENSIONS.has(ext)) {
      violations.push({
        file: join(STATIC_DIR, entry.parentPath ? relative(dir, join(entry.parentPath, entry.name)) : entry.name),
        reason: `raster image in static/ (${ext}) — the exception assumes no image input`,
      });
    }
  }
}

try {
  for (const dir of SCANNED) walk(join(ROOT, dir));
  scanStatic();
} catch (error) {
  console.error('[T108] image-input guard could not complete — treating as FAILURE, not skip.');
  console.error(`       ${error.message}`);
  process.exit(2);
}

if (violations.length > 0) {
  console.error('');
  console.error('[T108] BUILD REFUSED — image input found while the image-size advisory exception is active.');
  console.error('');
  for (const v of violations) {
    console.error(`  ${v.file}${v.line ? `:${v.line}` : ''}`);
    console.error(`      ${v.reason}`);
  }
  console.error('');
  console.error('  GHSA-w3rx-r6r6-pgpr and GHSA-5p2g-fcmc-qvqq (both high, CVSS 7.5) have no fixed');
  console.error('  version, and image-size upstream is archived. The exception recorded for T108');
  console.error('  rests on this site processing no images at all. Adding one voids it.');
  console.error('');
  console.error('  To proceed you must either remove the image input, or close T108 by resolving');
  console.error('  the advisories. Do not weaken or delete this check to make a build pass.');
  console.error('');
  process.exit(1);
}

console.log(`[T108] image-input guard: ok — no MDX image embeds, no raster assets, no image imports.`);
