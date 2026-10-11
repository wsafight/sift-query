import {defineConfig} from 'astro/config';
import sitemap from '@astrojs/sitemap';
import {satteri} from '@astrojs/markdown-satteri';
import {localizedAnchors} from './scripts/anchors.mjs';
import {deployment} from './scripts/deployment.mjs';

export default defineConfig({
  ...deployment(),
  output: 'static',
  trailingSlash: 'always',
  integrations: [sitemap()],
  markdown: {
    processor: satteri({hastPlugins: [localizedAnchors]}),
    shikiConfig: {
      theme: 'github-dark-default',
      wrap: false,
      langAlias: {ebnf: 'text', promql: 'text', rhai: 'javascript', rns: 'text'},
    },
  },
});
