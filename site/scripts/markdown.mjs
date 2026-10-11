import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {statSync} from 'node:fs';
import {visit} from 'unist-util-visit';
import {pages} from './catalog.mjs';
import {deployment} from './deployment.mjs';

const root = fileURLToPath(new URL('../../', import.meta.url)).replace(/[/\\]$/, '');
const pageBySource = new Map();
export const linkedSources = new Set();
for (const page of pages) {
  pageBySource.set(page.source, page);
  if (page.translation) pageBySource.set(page.translation, page);
}

export function resolveLink(url, file) {
  if (/^(?:[a-z][a-z\d+.-]*:|\/\/|#)/i.test(url)) return url;
  const [relative, hash = ''] = url.split('#');
  const target = path.relative(root, path.resolve(path.dirname(file), decodeURI(relative))).split(path.sep).join('/');
  if (target.startsWith('..') || path.isAbsolute(target)) throw new Error(`Link escapes repository: ${url}`);
  const base = deployment().base.replace(/\/$/, '');
  if (target === 'docs/README.md') return `${base}/docs/?lang=zh`;
  const page = pageBySource.get(target);
  if (page) {
    const lang = page.translation === target ? 'zh' : 'en';
    const suffix = hash ? `#${lang === 'zh' ? 'zh-' : ''}${hash}` : '';
    return `${base}/${page.slug}/?lang=${lang}${suffix}`;
  }
  const beanDocs = 'reference/bean-searcher-dev/bean-searcher-doc/docs/zh/';
  if (target.startsWith(beanDocs)) {
    return `https://www.bean-searcher.cn/${target.slice(beanDocs.length).replace(/\.md$/, '.html')}${hash ? `#${hash}` : ''}`;
  }
  const absolute = path.join(root, target);
  const info = statSync(absolute); // A missing source link must fail the build.
  if (info.isDirectory()) return `https://github.com/wsafight/sift-query/tree/main/${target}`;
  linkedSources.add(target);
  return `${base}/source/${target.split('/').map(encodeURIComponent).join('/')}${hash ? `#${hash}` : ''}`;
}

function nodeText(node) {
  if (node.type === 'text') return node.value || '';
  return (node.children || []).map(nodeText).join('');
}

export function docsMarkdown() {
  return (tree, file) => {
    tree.children = tree.children.filter(node => node.type !== 'paragraph' || !/^\s*(english|en)\s*\|\s*(简体中文|中文|chinese)\s*$/i.test(nodeText(node)));
    const firstHeading = tree.children.findIndex(node => node.type === 'heading');
    if (firstHeading >= 0 && tree.children[firstHeading].depth === 1) tree.children.splice(firstHeading, 1);
    visit(tree, node => {
      if (['link', 'image', 'definition'].includes(node.type)) node.url = resolveLink(node.url, file.path);
      if (node.type === 'code' && node.lang) node.lang = node.lang.split(',')[0];
    });
  };
}
