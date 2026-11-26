import type { APIRoute } from 'astro';
import {
  DEFAULT_LANG,
  type PageType,
  getAvailableLanguages,
  pageAbsoluteUrl,
} from '../lib/i18n';

const INDEXABLE_PAGES: Array<{
  type: Exclude<PageType, '404'>;
  changefreq: string;
  priority: string;
}> = [
  { type: 'home', changefreq: 'monthly', priority: '1.0' },
  { type: 'privacy', changefreq: 'yearly', priority: '0.3' },
];

export const GET: APIRoute = ({ site }) => {
  const origin = site ? site.toString().replace(/\/$/, '') : 'https://ocy.damt.xyz';
  const base = import.meta.env.BASE_URL.replace(/\/$/, '');
  const langs = getAvailableLanguages();
  const lastmod = new Date().toISOString().slice(0, 10);

  const urlBlocks: string[] = [];
  for (const page of INDEXABLE_PAGES) {
    const xDefaultUrl = pageAbsoluteUrl(origin, base, DEFAULT_LANG, page.type);
    const alternateLines = [
      ...langs.map(
        (alt) =>
          `    <xhtml:link rel="alternate" hreflang="${alt}" href="${pageAbsoluteUrl(origin, base, alt, page.type)}" />`
      ),
      `    <xhtml:link rel="alternate" hreflang="x-default" href="${xDefaultUrl}" />`,
    ].join('\n');

    for (const lang of langs) {
      const loc = pageAbsoluteUrl(origin, base, lang, page.type);
      urlBlocks.push(
        `  <url>
    <loc>${loc}</loc>
${alternateLines}
    <lastmod>${lastmod}</lastmod>
    <changefreq>${page.changefreq}</changefreq>
    <priority>${page.priority}</priority>
  </url>`
      );
    }
  }

  const body = `<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9"
        xmlns:xhtml="http://www.w3.org/1999/xhtml">
${urlBlocks.join('\n')}
</urlset>
`;

  return new Response(body, {
    headers: {
      'Content-Type': 'application/xml; charset=utf-8',
    },
  });
};
