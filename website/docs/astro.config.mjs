// The openDJ docs, served at opendj.app/docs. Pages are Markdown files in
// src/content/docs; the sidebar below lists them in reading order. The
// controller pages and their sidebar list are written by
// ../tools/controllers.py, which runs here whenever Astro loads this config,
// so `astro build` and `astro dev` always have them.
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import { satteri } from '@astrojs/markdown-satteri';
import linksValidator from 'starlight-links-validator';
import mdLinks from './plugins/md-links.mjs';

const base = '/docs/';
execFileSync('python3', ['../tools/controllers.py', '../../res/controllers', '.'], {
  cwd: new URL('.', import.meta.url),
  stdio: 'inherit',
});
const controllers = JSON.parse(
  fs.readFileSync(new URL('./src/controllers.json', import.meta.url), 'utf8'),
);

export default defineConfig({
  site: 'https://opendj.app',
  base,
  // Built into the website's dist/ next to the landing pages (see ../build.sh).
  outDir: '../dist/docs',
  // Pages keep the .html addresses they had under mdBook, which the app's
  // tutorials and older links use (/docs/guide/looping.html).
  build: { format: 'file' },
  trailingSlash: 'never',
  redirects: {
    '/guide/library-health': '/docs/guide/collection-health.html',
    '/controllers/index': '/docs/controllers.html',
  },
  markdown: {
    processor: satteri({ mdastPlugins: [mdLinks({ base, contentDir: './src/content/docs' })] }),
  },
  vite: { server: { fs: { allow: ['..'] } } },
  integrations: [
    starlight({
      title: 'openDJ Docs',
      favicon: '/favicon.png',
      components: { SiteTitle: './src/components/SiteTitle.astro' },
      customCss: ['./src/styles/opendj.css'],
      social: [{ icon: 'github', label: 'GitHub', href: 'https://github.com/openwave-studio/opendj' }],
      editLink: { baseUrl: 'https://github.com/openwave-studio/opendj/edit/main/website/docs/' },
      plugins: [linksValidator({ errorOnRelativeLinks: false })],
      sidebar: [
        { label: 'Introduction', link: '/' },
        {
          label: 'User guide',
          items: [
            'guide/install',
            'guide/music',
            'guide/filtering',
            'guide/collection-health',
            'guide/usb-export',
            'guide/decks',
            'guide/pads',
            'guide/looping',
            'guide/fx',
            'guide/sampler',
            'guide/groove-box',
            'guide/stems',
            'guide/layouts',
            'guide/lyrics',
            'guide/automix',
            'guide/remote',
            'guide/broadcasting',
            'guide/mixed-set-editor',
            {
              label: 'Controllers',
              items: [{ label: 'Overview', slug: 'guide/controllers' }, 'guide/controllers/ddj-flx4'],
            },
            {
              label: 'Supported controllers',
              collapsed: true,
              items: [{ label: 'All controllers', link: '/controllers' }, ...controllers],
            },
            'guide/osc',
            'guide/link',
            'guide/midi-clock',
            'guide/deck-outputs',
            'guide/external-mixer',
            'guide/shortcuts',
            'guide/feedback',
          ],
        },
        { label: 'Developer guide', items: ['dev/build', 'dev/mapping-format'] },
      ],
    }),
  ],
});
