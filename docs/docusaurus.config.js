// @ts-check
// Renvor documentation site.
//
// Search is provided by a LOCAL index (@easyops-cn/docusaurus-search-local) rather than
// a hosted service (FR-054). The index is built at `npm run build` and shipped with the
// site, so search works offline, adds no third-party runtime dependency, and sends no
// query data anywhere.

import { themes as prismThemes } from 'prism-react-renderer';

/** @type {import('@docusaurus/types').Config} */
const config = {
  title: 'Renvor',
  tagline: 'A Rust framework — pre-release, unpublished, and explicitly unstable',
  favicon: 'img/favicon.ico',

  url: 'https://renvor.dev',
  baseUrl: '/',

  organizationName: 'renvor-rs',
  projectName: 'renvor',

  // Broken links are a build FAILURE, never a warning. Verification step 8 must not
  // pass while the site contains a link that does not resolve.
  onBrokenLinks: 'throw',
  onBrokenAnchors: 'throw',

  markdown: {
    hooks: {
      // v4 location for this option; the top-level `onBrokenMarkdownLinks` is deprecated.
      onBrokenMarkdownLinks: 'throw',
    },
  },

  future: {
    v4: true,

    // ONE faster feature disabled, deliberately and narrowly.
    //
    // # What went wrong
    //
    // `v4: true` above is set for its v4 semantics. It ALSO sets `fasterByDefault: true`
    // (`@docusaurus/core/lib/server/configValidation.js`, `DEFAULT_FUTURE_V4_CONFIG_TRUE`), which
    // silently enables the whole faster suite — `rspackBundler`, `swcJsLoader`, `swcJsMinimizer`,
    // `swcHtmlMinimizer`, `lightningCssMinimizer`. Several of those are npm packages whose real
    // implementation is a **platform-specific native binary shipped as an OPTIONAL dependency**.
    //
    // npm omits optional dependencies non-deterministically, and on 2026-08-17 it did:
    // `verify (stable)` failed with `Cannot find module` inside `@swc/html/binding.js`, while
    // `verify (1.94.0)` built the SAME commit successfully ninety seconds earlier in a sibling
    // job. Same tree, same command, different outcome — a required check that fails on
    // environment rather than on content.
    //
    // # Reproduced, not assumed
    //
    // Moving `node_modules/@swc/html-<platform>` aside reproduces the exact CI error locally, and
    // restoring it makes the build pass again. That reproduction is what both candidate fixes
    // below were tested against.
    //
    // # Why this fix and not the broader one
    //
    // `faster: false` also works, and removes EVERY native binary from the build. It was rejected
    // after measurement: it switches the bundler from rspack back to webpack, which emits
    // `*.LICENSE.txt` sidecars carrying third-party licence banners. Those banners contain
    // `http://` URLs — lunrjs.com, tartarus.org, ricostacruz.com — and verification step 10 runs
    // `lychee --require-https`, so the broader fix traded a flaky build failure for **5
    // deterministic link-check failures**. A fix that moves the failure is not a fix.
    //
    // Disabling only `swcHtmlMinimizer` leaves the bundler and the emitted artifact set
    // byte-for-byte as they were — 0 `LICENSE.txt` sidecars, link check clean — and drops the one
    // dependency that actually broke. Measured cost: build unchanged at 11 s; `index.html`
    // 11,356 to 11,767 bytes, because HTML is minified by terser instead of swc.
    //
    // # What this does NOT fix
    //
    // `rspackBundler` and `lightningCssMinimizer` remain enabled and remain backed by native
    // optional binaries, so the same npm behaviour could break the build through a different
    // package. **The flake class is reduced, not eliminated**, and that is recorded here rather
    // than left for the next person to rediscover at 3 a.m. Eliminating it needs a deterministic
    // optional-dependency install, which is a separate change.
    faster: { swcHtmlMinimizer: false },
  },

  i18n: {
    defaultLocale: 'en',
    locales: ['en'],
  },

  presets: [
    [
      'classic',
      /** @type {import('@docusaurus/preset-classic').Options} */
      ({
        docs: {
          sidebarPath: './sidebars.js',
          editUrl: 'https://github.com/renvor-rs/renvor/tree/main/docs/',
        },
        // No blog. The project has nothing to announce yet, and an empty blog is a
        // maintenance surface with no reader.
        blog: false,
        theme: {
          customCss: './src/css/custom.css',
        },
      }),
    ],
  ],

  themes: [
    [
      '@easyops-cn/docusaurus-search-local',
      /** @type {import('@easyops-cn/docusaurus-search-local').PluginOptions} */
      ({
        hashed: true,
        indexBlog: false,
        docsRouteBasePath: '/docs',
        highlightSearchTermsOnTargetPage: true,
        explicitSearchResultPath: true,
      }),
    ],
  ],

  themeConfig:
    /** @type {import('@docusaurus/preset-classic').ThemeConfig} */
    ({
      announcementBar: {
        id: 'pre_release',
        content:
          'Renvor is pre-release, unpublished, and has no transport yet. Every API is unstable. Do not adopt it yet.',
        backgroundColor: '#8a1f11',
        textColor: '#ffffff',
        isCloseable: false,
      },
      navbar: {
        title: 'Renvor',
        items: [
          {
            type: 'docSidebar',
            sidebarId: 'docsSidebar',
            position: 'left',
            label: 'Documentation',
          },
          {
            href: 'https://github.com/renvor-rs/renvor',
            label: 'GitHub',
            position: 'right',
          },
        ],
      },
      footer: {
        style: 'dark',
        links: [
          {
            title: 'Documentation',
            items: [
              { label: 'Introduction', to: '/docs/intro' },
              { label: 'Support policy', to: '/docs/support-policy' },
              { label: 'Verification', to: '/docs/verification' },
            ],
          },
          {
            title: 'Project',
            items: [
              { label: 'GitHub', href: 'https://github.com/renvor-rs/renvor' },
              {
                label: 'Security policy',
                href: 'https://github.com/renvor-rs/renvor/blob/main/SECURITY.md',
              },
              {
                label: 'Governance',
                href: 'https://github.com/renvor-rs/renvor/blob/main/GOVERNANCE.md',
              },
            ],
          },
        ],
        copyright: `Copyright © ${new Date().getFullYear()} Ahmed Anbar and the Renvor contributors. Licensed MIT OR Apache-2.0.`,
      },
      prism: {
        theme: prismThemes.github,
        darkTheme: prismThemes.dracula,
        additionalLanguages: ['rust', 'toml', 'bash'],
      },
    }),
};

export default config;
