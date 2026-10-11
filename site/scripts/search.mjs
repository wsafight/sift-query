import {readFile} from 'node:fs/promises';
import {fileURLToPath} from 'node:url';
import {load} from 'cheerio';
import {createIndex, close} from 'pagefind';
import {pages} from './catalog.mjs';

const segmenter = new Intl.Segmenter('zh', {granularity: 'word'});
const dist = fileURLToPath(new URL('../dist/', import.meta.url));
function checked(result) {
  if (result.errors.length) throw new Error(result.errors.join('\n'));
  return result;
}

try {
  const {index} = checked(await createIndex());
  if (!index) throw new Error('Pagefind did not create an index');
  for (const page of pages) {
    const $ = load(await readFile(`${dist}${page.slug}/index.html`, 'utf8'));
    for (const lang of ['en', 'zh']) {
      const document = load('<html><head></head><body data-pagefind-body></body></html>');
      document('html').attr('lang', lang);
      document('head').append(document('<title>').text(page.title[lang]));
      document('body').append(document('<h1 data-pagefind-meta="title">').text(page.title[lang]));
      document('body').append(document('<p>').text(page.description[lang]));
      document('body').append($(`.prose.i18n-${lang}`).html() || '');
      if (lang === 'zh') {
        // Match the browser's Intl word segmentation before Pagefind tokenizes text.
        document('*').contents().each((_, node) => {
          if (node.type === 'text' && /\p{Script=Han}/u.test(node.data)) {
            node.data = [...segmenter.segment(node.data)].map(item => item.segment).join(' ');
          }
        });
      }
      checked(await index.addHTMLFile({
        url: `/${page.slug}/?lang=${lang}`,
        content: document.html(),
      }));
    }
  }
  checked(await index.writeFiles({outputPath: `${dist}pagefind`}));
  console.log(`Indexed ${pages.length} documentation pages in English and Chinese.`);
} finally {
  await close();
}
