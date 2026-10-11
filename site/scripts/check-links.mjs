import {readFile, readdir, stat} from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {load} from 'cheerio';
import {deployment} from './deployment.mjs';

const dist = fileURLToPath(new URL('../dist', import.meta.url));
const {base, site} = deployment();
const files = [];
const errors = [];
const idCache = new Map();

async function walk(directory) {
  for (const entry of await readdir(directory, {withFileTypes: true})) {
    const target = path.join(directory, entry.name);
    if (entry.isDirectory()) await walk(target);
    else if (entry.name.endsWith('.html')) files.push(target);
  }
}

async function fileExists(file) {
  try { return (await stat(file)).isFile(); } catch { return false; }
}

async function resolveFile(pathname, from) {
  const relative = pathname.startsWith('/') ? pathname.slice(base === '/' ? 1 : base.length) : pathname;
  const absolute = pathname.startsWith('/') ? path.join(dist, relative) : path.resolve(path.dirname(from), relative);
  for (const candidate of [absolute, `${absolute}.html`, path.join(absolute, 'index.html')]) {
    if (await fileExists(candidate)) return candidate;
  }
  return null;
}

async function idsIn(file) {
  if (!idCache.has(file)) {
    const $ = load(await readFile(file, 'utf8'));
    idCache.set(file, new Set([...$('[id]')].map(node => $(node).attr('id')).filter(Boolean)));
  }
  return idCache.get(file);
}

await walk(dist);
for (const file of files) {
  const $ = load(await readFile(file, 'utf8'));
  const ids = [...$('[id]')].map(node => $(node).attr('id')).filter(Boolean);
  const localIds = new Set();
  for (const id of ids) {
    if (localIds.has(id)) errors.push(`${path.relative(dist, file)} -> duplicate id: ${id}`);
    localIds.add(id);
  }
  for (const node of [...$('a[href], img[src], link[href]:not([rel="canonical"]), script[src]')]) {
    const url = $(node).attr('href') || $(node).attr('src') || '';
    if (!url || /^(?:[a-z][a-z\d+.-]*:|\/\/)/i.test(url) || url.startsWith('data:')) continue;
    const [withoutHash, hash] = url.split('#');
    const pathname = withoutHash.split('?')[0];
    if (!pathname) {
      if (hash && !localIds.has(decodeURIComponent(hash))) errors.push(`${path.relative(dist, file)} -> ${url}`);
      continue;
    }
    if (pathname.startsWith('/') && base !== '/' && !pathname.startsWith(base)) {
      errors.push(`${path.relative(dist, file)} -> outside deployment base: ${url}`);
      continue;
    }
    const target = await resolveFile(pathname, file);
    if (!target) { errors.push(`${path.relative(dist, file)} -> ${url}`); continue; }
    if (hash && target.endsWith('.html') && !(await idsIn(target)).has(decodeURIComponent(hash))) {
      errors.push(`${path.relative(dist, file)} -> ${url}`);
    }
  }
}

// Verify generated SEO resources against the same deployment path as the pages.
const robots = await readFile(path.join(dist, 'robots.txt'), 'utf8');
const sitemapUrl = robots.match(/^Sitemap:\s*(.+)$/m)?.[1];
if (!sitemapUrl) errors.push('robots.txt -> missing sitemap');
const sitemapFiles = new Set(['sitemap-index.xml']);
for (const name of sitemapFiles) {
  const $ = load(await readFile(path.join(dist, name), 'utf8'), {xml: true});
  for (const node of $('loc')) {
    const url = new URL($(node).text(), site);
    if (!url.pathname.startsWith(base) || !await resolveFile(url.pathname, path.join(dist, name))) {
      errors.push(`${name} -> ${url.href}`);
    }
    if (url.pathname.endsWith('.xml')) sitemapFiles.add(url.pathname.slice(base.length));
  }
}
if (sitemapUrl && new URL(sitemapUrl).pathname !== `${base}sitemap-index.xml`) {
  errors.push(`robots.txt -> incorrect deployment path: ${sitemapUrl}`);
}

if (errors.length) {
  console.error(`Broken links (${errors.length}):\n${errors.join('\n')}`);
  process.exit(1);
}
console.log(`Checked ${files.length} HTML files, no broken internal links.`);
