import { defineConfig } from 'astro/config';

export default defineConfig({
  // Served at https://ocy.damt.xyz/ (GitHub Pages custom domain); the old
  // danielmevit.github.io/ocy/ address redirects here.
  site: 'https://ocy.damt.xyz',
  base: '/',
  build: {
    inlineStylesheets: 'always',
  },
});
