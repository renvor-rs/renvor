'use strict';

/**
 * No-op replacement for `image-size/fromFile` (T108).
 *
 * WHY THIS EXISTS
 * ---------------
 * Every published version of `image-size` is covered by two unfixed high-severity
 * advisories — GHSA-w3rx-r6r6-pgpr (ICNS parser infinite loop) and
 * GHSA-5p2g-fcmc-qvqq (JXL and HEIF parser infinite loops). Both name the range
 * `<=2.0.2`, and 2.0.2 is the latest release, so there is no version to upgrade to.
 * Upstream is archived, so no fix is coming.
 *
 * `image-size` reaches this project through exactly one call site:
 * `@docusaurus/mdx-loader/lib/remark/transformImage/index.js`, which does
 * `require('image-size/fromFile')` and awaits `imageSizeFromFile(path)` to stamp
 * width/height onto images referenced from MDX. An npm `overrides` entry redirects
 * that specifier here, so the vulnerable parsers are never installed at all — this
 * is removal from the dependency graph, not suppression of a warning about it.
 *
 * WHY THROWING IS THE CORRECT BEHAVIOUR
 * -------------------------------------
 * The site embeds no images, and `scripts/check-image-inputs.mjs` runs as `prebuild`
 * to keep it that way, so this function is unreachable in any build the project can
 * currently produce. Returning empty dimensions instead of throwing would make a
 * future image silently lose its width/height attributes; throwing surfaces the
 * situation loudly (mdx-loader logs the error and warns) instead of degrading in
 * silence. The guard script remains the hard gate; this is the backstop behind it.
 *
 * Remove this package, the override, and the guard together if a real image pipeline
 * is ever needed — see the note in package.json's `comments.overrides`.
 */

const REASON = [
  'image-size is disabled in this project.',
  '',
  'The real package is replaced by vendor/image-size-disabled via an npm override,',
  'because every published version carries unfixed high-severity DoS advisories',
  '(GHSA-w3rx-r6r6-pgpr, GHSA-5p2g-fcmc-qvqq) and upstream is archived.',
  '',
  'Reaching this code means an image is being measured, which this site does not',
  'support. To add images you must restore a real image-size implementation, remove',
  'the override in package.json, and retire scripts/check-image-inputs.mjs — do not',
  'silence this error.',
].join('\n');

/**
 * Mirrors `imageSizeFromFile` from the real package, which resolves to `{width, height}`.
 * Rejects instead, so the caller cannot mistake "not measured" for "no dimensions".
 */
async function imageSizeFromFile(filePath) {
  throw new Error(`${REASON}\n\nAttempted to measure: ${filePath}`);
}

/**
 * Mirrors the real package's concurrency setter. It only sizes an internal file-handle
 * queue and has no observable result, so a no-op is a faithful stand-in rather than a
 * fallback that hides anything — no measuring ever happens here to be throttled.
 */
function setConcurrency() {}

module.exports = { imageSizeFromFile, setConcurrency };
