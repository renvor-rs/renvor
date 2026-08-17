#!/usr/bin/env node
/**
 * Positive and negative controls for `check-image-inputs.mjs` (ADR-0009).
 *
 * WHY THIS EXISTS
 * ---------------
 * CI ran the guard only against the real tree, which contains no images, so the guard
 * could only ever print "ok" — and "ok" is exactly what a broken guard prints too. An
 * edit narrowing the scanned set, or one making the check return early, would have left
 * the pipeline green. Two advisory reviews raised this independently, after both had
 * already found real gaps in the guard's coverage; a fix shipping without a regression
 * test would have been the same mistake one layer down.
 *
 * The guard is the ONLY control that can fail a build over an image input — the vendored
 * replacement throws, but mdx-loader catches and continues — so every gap in it is a gap
 * in the whole enforcement. That is why the positive controls below are enumerated per
 * form rather than sampled.
 *
 * HOW IT WORKS
 * ------------
 * The guard derives its scan root from its own location (`new URL('..', import.meta.url)`),
 * so each case copies the guard into `<fixture>/scripts/` and runs it there. The real
 * `docs/` tree is never modified.
 *
 * Run: `npm test` (in docs/), or `node scripts/check-image-inputs.test.mjs`.
 */

import {execFileSync} from 'node:child_process';
import {mkdirSync, mkdtempSync, rmSync, writeFileSync, copyFileSync, symlinkSync, chmodSync} from 'node:fs';
import {join, dirname} from 'node:path';
import {tmpdir} from 'node:os';
import {fileURLToPath} from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const GUARD = join(HERE, 'check-image-inputs.mjs');

// A real 1x1 PNG. Byte content matters for nothing here — the guard reads names and text,
// never image bytes — but a real file keeps the fixtures honest.
const PNG = Buffer.from(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAACklEQVR4nGMAAQAABQABDQottAAAAABJRU5ErkJggg==',
  'base64',
);

let failures = 0;
let passes = 0;

function fixture(build) {
  const dir = mkdtempSync(join(tmpdir(), 'guard-fixture-'));
  mkdirSync(join(dir, 'scripts'), {recursive: true});
  copyFileSync(GUARD, join(dir, 'scripts', 'check-image-inputs.mjs'));
  build({
    dir,
    file(rel, contents) {
      const full = join(dir, rel);
      mkdirSync(dirname(full), {recursive: true});
      writeFileSync(full, contents);
      return full;
    },
    dirAt(rel) {
      const full = join(dir, rel);
      mkdirSync(full, {recursive: true});
      return full;
    },
  });
  return dir;
}

function run(dir) {
  try {
    const stdout = execFileSync(process.execPath, [join(dir, 'scripts', 'check-image-inputs.mjs')], {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    return {code: 0, output: stdout};
  } catch (error) {
    return {code: error.status ?? -1, output: `${error.stdout ?? ''}${error.stderr ?? ''}`};
  }
}

/** The guard MUST refuse this input. A case that passes here is a hole in the only enforcement. */
function refuses(name, build, {expect = 1} = {}) {
  const dir = fixture(build);
  try {
    const {code, output} = run(dir);
    if (code === expect) {
      passes += 1;
      console.log(`  ok    refuses ${name} (exit ${code})`);
    } else {
      failures += 1;
      console.error(`  FAIL  ${name}: expected exit ${expect}, got ${code}`);
      console.error(output.split('\n').map((l) => `        ${l}`).join('\n'));
    }
  } finally {
    rmSync(dir, {recursive: true, force: true});
  }
}

/** The guard MUST accept this input. Without these, "refuse everything" would score 100%. */
function accepts(name, build) {
  const dir = fixture(build);
  try {
    const {code, output} = run(dir);
    if (code === 0) {
      passes += 1;
      console.log(`  ok    accepts ${name}`);
    } else {
      failures += 1;
      console.error(`  FAIL  ${name}: expected exit 0, got ${code}`);
      console.error(output.split('\n').map((l) => `        ${l}`).join('\n'));
    }
  } finally {
    rmSync(dir, {recursive: true, force: true});
  }
}

console.log('[T108] guard controls — every caught form, plus the cases that must NOT fire');
console.log('');
console.log('POSITIVE CONTROLS — the guard must exit non-zero:');

refuses('a raster asset in a content directory', ({file}) => file('docs/a.png', PNG));

refuses('a Markdown inline image', ({file}) => {
  file('docs/page.mdx', '# P\n\n![alt](./a.png)\n');
  file('docs/a.png', PNG);
});

// The reference form has no parenthesised destination, so an inline-only regex misses it.
refuses('a Markdown reference-style image definition', ({file}) =>
  file('docs/page.mdx', '# P\n\n![alt][ref]\n\n[ref]: ../outside/a.png\n'),
);

refuses('an <img> element in MDX', ({file}) => file('docs/page.mdx', '<img src="../outside/a.png" />\n'));

// Not only lowercase <img>: a capitalised wrapper reaches the same loader chain.
refuses('a capitalised <Image> element in MDX', ({file}) =>
  file('docs/page.mdx', '<Image src="../outside/a.png" />\n'),
);

refuses('a require() of an image', ({file}) => file('docs/page.mdx', 'const x = require("../outside/a.png");\n'));

// The statement form has no parentheses, so a call-only regex misses it.
refuses('a bare ESM import of an image in a component', ({file}) =>
  file('src/Logo.jsx', 'import logo from "../outside/a.png";\nexport default logo;\n'),
);

refuses('an image in the i18n tree', ({file}) =>
  file('i18n/fr/docusaurus-plugin-content-docs/current/a.png', PNG),
);

refuses('an image in the versioned_docs tree', ({file}) => file('versioned_docs/version-1.0/a.png', PNG));

refuses('an exotic raster extension (.tga)', ({file}) => file('docs/a.tga', PNG));

// Dirent.isFile() and isDirectory() are both false for a symlink, so an earlier guard
// walked past symlinked trees in silence — proven through a real `npm run build`.
refuses('a symlinked directory inside a scanned tree', ({dir, dirAt, file}) => {
  const outside = mkdtempSync(join(tmpdir(), 'guard-outside-'));
  writeFileSync(join(outside, 'hidden.png'), PNG);
  dirAt('docs');
  file('docs/keep.md', '# keep\n');
  symlinkSync(outside, join(dir, 'docs', 'sub'), 'dir');
});

refuses('a symlinked image file inside a scanned tree', ({dir, dirAt}) => {
  const outside = mkdtempSync(join(tmpdir(), 'guard-outside-'));
  const target = join(outside, 'hidden.png');
  writeFileSync(target, PNG);
  dirAt('docs');
  symlinkSync(target, join(dir, 'docs', 'linked.png'), 'file');
});

// A check that cannot run is a failure, never a skip.
refuses(
  'an unreadable directory (exit 2, not a skip)',
  ({dirAt}) => chmodSync(dirAt('docs/locked'), 0o000),
  {expect: 2},
);

// The favicon permission is scoped to static/. Without this, "name is favicon.ico" would be
// a rename away from a universal bypass.
refuses('a favicon.ico smuggled into a content directory', ({file}) => file('docs/favicon.ico', PNG));

// scripts/ is exempt from TEXT scanning only. Exempting a directory from one rule must not
// quietly exempt it from all of them.
refuses('a raster asset inside the exempt scripts/ directory', ({file}) => file('scripts/a.png', PNG));

console.log('');
console.log('NEGATIVE CONTROLS — the guard must exit 0, or "refuse everything" would score 100%:');

accepts('a clean tree', ({file}) => file('docs/page.mdx', '# Just words\n'));

accepts('the permitted favicon', ({file}) => file('static/favicon.ico', PNG));

// A src that is not an image must not fire, or the guard becomes noisy enough to switch off.
accepts('a non-image src attribute', ({file}) => file('docs/page.mdx', '<script src="./app.js"></script>\n'));

// SVG files are permitted as static assets; an SVG *embed* is still caught, because
// mdx-loader passes every local image path to image-size regardless of extension.
accepts('an SVG file used as a static asset', ({file}) => file('static/img/brand.svg', '<svg/>'));

refuses('an SVG embedded in MDX', ({file}) => file('docs/page.mdx', '![mark](./brand.svg)\n'));

console.log('');
if (failures > 0) {
  console.error(`[T108] guard controls FAILED — ${failures} failing, ${passes} passing.`);
  process.exit(1);
}
console.log(`[T108] guard controls: ok — ${passes} controls passing, 0 failing.`);
