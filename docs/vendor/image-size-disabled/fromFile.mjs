// ESM view of ./fromFile.cjs. Nothing in this project imports it — @docusaurus/mdx-loader
// is CommonJS and takes the `require` condition — but the real package exposes both, so
// the stand-in does too. See fromFile.cjs for why this throws.

import mod from './fromFile.cjs';

export const {imageSizeFromFile, setConcurrency} = mod;
export default mod;
