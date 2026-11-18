import { defineConfig } from 'astro/config';

export default defineConfig({
  site: 'https://danielmevit.github.io',
  base: '/ocy',
  build: {
    inlineStylesheets: 'always',
  },
});
