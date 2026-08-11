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
  tagline: 'A Rust framework — pre-release, no runtime capability yet',
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
          'Renvor is pre-release and ships no runtime capability. Do not adopt it yet.',
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
