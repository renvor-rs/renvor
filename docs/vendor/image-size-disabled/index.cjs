'use strict';

// Root entry of the image-size stand-in. Nothing in this tree imports it —
// @docusaurus/mdx-loader only ever requires the `./fromFile` subpath — but the real
// package exports these names, so the replacement keeps the same shape rather than
// letting a future consumer fail with a confusing "undefined is not a function".
//
// See fromFile.cjs for the full rationale (T108).

const {imageSizeFromFile, setConcurrency} = require('./fromFile.cjs');

/** Sync counterpart of imageSizeFromFile in the real package; throws for the same reason. */
function imageSize(input) {
  throw new Error(
    'image-size is disabled in this project (see vendor/image-size-disabled/fromFile.cjs). ' +
      `Attempted to measure a ${input && input.length ? `${input.length}-byte ` : ''}buffer.`,
  );
}

/** The real package lists the formats it can detect. This one detects none, truthfully. */
const types = [];

/** Real signature narrows the enabled detectors. With no detectors, this is a no-op. */
function disableTypes() {}

module.exports = {default: imageSize, imageSize, imageSizeFromFile, setConcurrency, types, disableTypes};
