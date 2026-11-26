import {href, pages} from '../catalog';
import {langFromStorage, ui, type Lang} from '../i18n';

const header = document.querySelector<HTMLElement>('#site-header');
const menu = document.querySelector<HTMLElement>('#mobile-menu');
const menuToggle = document.querySelector<HTMLButtonElement>('#menu-toggle');
const dialog = document.querySelector<HTMLDialogElement>('#search-dialog');
const searchInput = document.querySelector<HTMLInputElement>('[data-search-input]');
const results = document.querySelector<HTMLElement>('[data-search-results]');
const empty = document.querySelector<HTMLElement>('[data-search-empty]');

type SearchItem = {title: {en: string; zh: string}; description: {en: string; zh: string}; url: string};
type Pagefind = {init(): Promise<void>; search(query: string): Promise<{results: Array<{data(): Promise<{url: string; meta: {title?: string}; excerpt?: string}>}>}>};
const catalog: SearchItem[] = pages.map(page => ({title: page.title, description: page.description, url: href(page.slug)}));
type PagefindModule = {createInstance(): Pagefind};
const searchIndexes = new Map<Lang, Promise<Pagefind | undefined>>();
let pagefindModule: Promise<PagefindModule> | undefined;
let lastScroll = window.scrollY;
let searchRevision = 0;
let selectedSearchIndex = -1;
let tocObserver: IntersectionObserver | undefined;

function currentLang(): Lang {
  return langFromStorage(document.documentElement.dataset.lang || localStorage.getItem('siftquery-lang'));
}

function copy(lang = currentLang()) { return ui[lang]; }

function applyLang(lang: Lang, persist = true) {
  const root = document.documentElement;
  root.dataset.lang = lang;
  root.lang = lang === 'zh' ? 'zh-CN' : 'en';
  if (persist) {
    localStorage.setItem('siftquery-lang', lang);
    const url = new URL(location.href);
    if (lang === 'zh') url.searchParams.set('lang', 'zh');
    else url.searchParams.delete('lang');
    history.replaceState(null, '', `${url.pathname}${url.search}${url.hash}`);
  }
  const text = ui[lang];
  const title = root.getAttribute(lang === 'zh' ? 'data-title-zh' : 'data-title-en');
  const description = root.getAttribute(lang === 'zh' ? 'data-desc-zh' : 'data-desc-en');
  if (title) document.title = title;
  const meta = document.querySelector('meta[name="description"]');
  if (meta && description) meta.setAttribute('content', description);
  document.querySelectorAll<HTMLElement>('[data-i18n-aria]').forEach(node => {
    const key = node.dataset.i18nAria as keyof typeof text | undefined;
    if (key && text[key]) node.setAttribute('aria-label', text[key]);
  });
  document.querySelectorAll<HTMLInputElement>('[data-i18n-placeholder]').forEach(node => {
    const key = node.dataset.i18nPlaceholder as keyof typeof text | undefined;
    if (key && text[key]) node.setAttribute('placeholder', text[key]);
  });
  if (menuToggle && !menu?.classList.contains('is-open')) menuToggle.setAttribute('aria-label', text.menuOpen);
  document.querySelectorAll<HTMLButtonElement>('.copy-code').forEach(button => {
    if (button.dataset.copied !== 'true') button.textContent = text.copy;
  });
  document.querySelectorAll<HTMLAnchorElement>('.heading-anchor').forEach(anchor => {
    anchor.setAttribute('aria-label', text.linkHeading);
  });
  if (dialog?.open && searchInput?.value) void search(searchInput.value);
  watchToc();
}

function setMenu(open: boolean) {
  if (!menu || !menuToggle) return;
  menu.classList.toggle('is-open', open);
  menu.setAttribute('aria-hidden', String(!open));
  menu.inert = !open;
  menuToggle.setAttribute('aria-expanded', String(open));
  menuToggle.setAttribute('aria-label', open ? copy().menuClose : copy().menuOpen);
  header?.classList.toggle('menu-open', open);
  document.body.classList.toggle('menu-open', open);
}

function setTheme(theme: 'light' | 'dark') {
  document.documentElement.dataset.theme = theme;
  localStorage.setItem('siftquery-theme', theme);
  const themeMeta = document.querySelector('meta[name="theme-color"]');
  if (themeMeta) themeMeta.setAttribute('content', theme === 'light' ? '#fafaf9' : '#090b10');
}

async function loadPagefind(lang = currentLang()): Promise<Pagefind | undefined> {
  const url = dialog?.dataset.pagefindUrl;
  if (!url) return undefined;
  const existing = searchIndexes.get(lang);
  if (existing) return existing;
  const loading = (async () => {
    try {
      // Keep one module so its worker instance IDs stay unique across languages.
      pagefindModule ??= import(/* @vite-ignore */ url) as Promise<PagefindModule>;
      const loaded = await Promise.race([
        pagefindModule,
        new Promise<never>((_, reject) => window.setTimeout(() => reject(new Error('Search index timeout')), 1500)),
      ]);
      if (currentLang() !== lang) return undefined;
      const index = loaded.createInstance();
      await index.init();
      return index;
    } catch { pagefindModule = undefined; return undefined; }
  })();
  searchIndexes.set(lang, loading);
  const result = await loading;
  if (!result) searchIndexes.delete(lang);
  return result;
}

function updateSearchSelection(index: number) {
  if (!results) return;
  const links = Array.from(results.querySelectorAll<HTMLAnchorElement>('a'));
  if (!links.length) { selectedSearchIndex = -1; return; }
  selectedSearchIndex = ((index % links.length) + links.length) % links.length;
  links.forEach((link, i) => {
    const active = i === selectedSearchIndex;
    link.classList.toggle('is-selected', active);
    if (active) link.scrollIntoView({block: 'nearest'});
  });
}

function renderItems(items: SearchItem[]) {
  if (!results || !empty) return;
  results.replaceChildren();
  selectedSearchIndex = -1;
  empty.hidden = items.length > 0 || !searchInput?.value.trim();
  for (const item of items.slice(0, 8)) {
    const link = document.createElement('a');
    const title = document.createElement('strong');
    const excerpt = document.createElement('small');
    const lang = currentLang();
    const url = new URL(item.url, location.href);
    url.searchParams.set('lang', lang);
    link.href = `${url.pathname}${url.search}${url.hash}`;
    title.textContent = item.title[lang];
    excerpt.textContent = item.description[lang];
    link.append(title, excerpt);
    results.append(link);
  }
  if (items.length) updateSearchSelection(0);
}

async function search(query: string) {
  const revision = ++searchRevision;
  const value = query.trim();
  if (!value) { renderItems([]); if (empty) empty.hidden = true; searchInput?.removeAttribute('aria-busy'); return; }
  searchInput?.setAttribute('aria-busy', 'true');
  try {
    const pagefind = await loadPagefind();
    if (revision !== searchRevision) return;
    if (pagefind) {
      const found = await pagefind.search(value);
      const items = await Promise.all(found.results.slice(0, 8).map(async result => {
        const data = await result.data();
        const pathname = new URL(data.url, location.href).pathname.replace(/\/$/, '');
        const localized = catalog.find(item => new URL(item.url, location.href).pathname.replace(/\/$/, '') === pathname);
        const excerpt = data.excerpt?.replace(/<[^>]+>/g, '').replace(/(\p{Script=Han})\s+(?=\p{Script=Han})/gu, '$1') || '';
        return {
          title: localized?.title ?? {en: data.meta.title || data.url, zh: data.meta.title || data.url},
          description: {en: excerpt || localized?.description.en || '', zh: excerpt || localized?.description.zh || ''},
          url: data.url,
        };
      }));
      if (revision !== searchRevision) return;
      if (items.length) { renderItems(items); return; }
    }
    const lower = value.toLowerCase();
    const items = catalog.filter(item => `${item.title.en} ${item.title.zh} ${item.description.en} ${item.description.zh}`.toLowerCase().includes(lower));
    if (revision === searchRevision) renderItems(items);
  } finally {
    if (revision === searchRevision) searchInput?.removeAttribute('aria-busy');
  }
}

window.addEventListener('scroll', () => {
  if (!header) return;
  const current = window.scrollY;
  const reading = document.body.classList.contains('docs-body');
  header.classList.toggle('is-scrolled', current > 16);
  header.classList.toggle('is-hidden', !reading && current > lastScroll && current > 120 && !document.body.classList.contains('menu-open'));
  lastScroll = current;
}, {passive: true});

menuToggle?.addEventListener('click', () => setMenu(!menu?.classList.contains('is-open')));
menu?.querySelectorAll('a').forEach(link => link.addEventListener('click', () => setMenu(false)));
document.querySelectorAll<HTMLButtonElement>('[data-theme-toggle]').forEach(button => button.addEventListener('click', () => setTheme(document.documentElement.dataset.theme === 'dark' ? 'light' : 'dark')));
document.querySelectorAll<HTMLButtonElement>('[data-lang-toggle]').forEach(button => button.addEventListener('click', () => applyLang(currentLang() === 'zh' ? 'en' : 'zh')));
applyLang(currentLang(), false);

document.querySelectorAll<HTMLButtonElement>('[data-search-open]').forEach(button => button.addEventListener('click', () => {
  dialog?.showModal();
  searchInput?.focus();
  void loadPagefind();
}));

document.addEventListener('keydown', event => {
  if ((event.key === 'k' || event.key === 'K') && (event.metaKey || event.ctrlKey)) {
    event.preventDefault();
    document.querySelector<HTMLButtonElement>('[data-search-open]')?.click();
    return;
  }
  if (event.key === '/' && !(event.target instanceof HTMLInputElement) && !(event.target instanceof HTMLTextAreaElement)) {
    event.preventDefault();
    document.querySelector<HTMLButtonElement>('[data-search-open]')?.click();
  }
  if (event.key === 'Escape') {
    if (dialog?.open) { event.preventDefault(); dialog.close(); }
    setMenu(false);
  }
}, {capture: true});

searchInput?.addEventListener('input', () => void search(searchInput.value));
searchInput?.addEventListener('keydown', event => {
  if (!results) return;
  const links = results.querySelectorAll<HTMLAnchorElement>('a');
  if (!links.length) return;
  if (event.key === 'ArrowDown') {
    event.preventDefault();
    updateSearchSelection(selectedSearchIndex + 1);
  } else if (event.key === 'ArrowUp') {
    event.preventDefault();
    updateSearchSelection(selectedSearchIndex - 1);
  } else if (event.key === 'Enter' && selectedSearchIndex >= 0 && links[selectedSearchIndex]) {
    event.preventDefault();
    links[selectedSearchIndex].click();
  }
});
dialog?.addEventListener('click', event => { if (event.target === dialog) dialog.close(); });

document.querySelectorAll<HTMLElement>('.prose pre').forEach(block => {
  if (block.querySelector('.copy-code')) return;
  const button = document.createElement('button');
  button.type = 'button';
  button.className = 'copy-code';
  button.textContent = copy().copy;
  button.addEventListener('click', async () => {
    const code = block.querySelector('code')?.textContent || block.textContent || '';
    await navigator.clipboard.writeText(code);
    button.dataset.copied = 'true';
    button.textContent = copy().copied;
    window.setTimeout(() => { delete button.dataset.copied; button.textContent = copy().copy; }, 1600);
  });
  block.append(button);
});

document.querySelectorAll<HTMLElement>('.prose :is(h2,h3,h4)[id]').forEach(heading => {
  if (heading.querySelector('.heading-anchor')) return;
  const anchor = document.createElement('a');
  anchor.className = 'heading-anchor';
  anchor.href = `#${heading.id}`;
  anchor.dataset.i18nAria = 'linkHeading';
  anchor.setAttribute('aria-label', copy().linkHeading);
  heading.append(anchor);
});

function syncDocPanels() {
  const compactNav = window.matchMedia('(max-width: 780px)').matches;
  const compactToc = window.matchMedia('(max-width: 1080px)').matches;
  document.querySelectorAll<HTMLDetailsElement>('.docs-nav-panel').forEach(panel => {
    if (!compactNav) panel.open = true;
  });
  document.querySelectorAll<HTMLDetailsElement>('.docs-outline').forEach(panel => {
    if (!compactToc) panel.open = true;
  });
}
syncDocPanels();
window.matchMedia('(max-width: 780px)').addEventListener('change', syncDocPanels);
window.matchMedia('(max-width: 1080px)').addEventListener('change', syncDocPanels);

document.querySelectorAll<HTMLAnchorElement>('.docs-toc a').forEach(link => {
  link.addEventListener('click', () => {
    if (window.matchMedia('(max-width: 1080px)').matches) {
      const outline = link.closest<HTMLDetailsElement>('.docs-outline');
      if (outline) outline.open = false;
    }
  });
});

function watchToc() {
  tocObserver?.disconnect();
  const lang = currentLang();
  const tocLinks = Array.from(document.querySelectorAll<HTMLAnchorElement>(`.docs-toc.i18n-${lang} a`));
  if (!tocLinks.length) return;
  const pairs = tocLinks.map(link => {
    const id = decodeURIComponent(link.getAttribute('href')?.slice(1) || '');
    return {link, target: document.getElementById(id)};
  }).filter((item): item is {link: HTMLAnchorElement; target: HTMLElement} => Boolean(item.target));
  if (!pairs.length) return;
  const mark = (active: HTMLAnchorElement) => {
    tocLinks.forEach(link => link.classList.toggle('is-active', link === active));
  };
  mark(pairs[0].link);
  tocObserver = new IntersectionObserver(entries => {
    const visible = entries.filter(entry => entry.isIntersecting).sort((a, b) => a.boundingClientRect.top - b.boundingClientRect.top);
    const hit = visible[0] && pairs.find(item => item.target === visible[0].target);
    if (hit) mark(hit.link);
  }, {rootMargin: '-96px 0px -70% 0px'});
  pairs.forEach(item => tocObserver?.observe(item.target));
}
watchToc();

// Reading progress indicator on documentation pages
const progressBar = document.querySelector<HTMLElement>('#reading-progress');
if (progressBar) {
  const updateProgress = () => {
    const docHeight = document.documentElement.scrollHeight - window.innerHeight;
    const ratio = docHeight > 0 ? Math.min(1, Math.max(0, window.scrollY / docHeight)) : 0;
    progressBar.style.transform = `scaleX(${ratio})`;
  };
  updateProgress();
  window.addEventListener('scroll', updateProgress, {passive: true});
  window.addEventListener('resize', updateProgress, {passive: true});
}

// Generic tab switcher helper
function wireTabs(rootSelector: string, tabAttr: string, panelAttr: string, activeClass: string) {
  document.querySelectorAll<HTMLElement>(rootSelector).forEach(root => {
    const tabs = Array.from(root.querySelectorAll<HTMLButtonElement>(`[${tabAttr}]`));
    const panels = Array.from(root.querySelectorAll<HTMLElement>(`[${panelAttr}]`));
    tabs.forEach(tab => {
      tab.addEventListener('click', () => {
        const key = tab.getAttribute(tabAttr);
        tabs.forEach(item => {
          const active = item === tab;
          item.classList.toggle(activeClass, active);
          item.setAttribute('aria-selected', String(active));
        });
        panels.forEach(panel => {
          panel.hidden = panel.getAttribute(panelAttr) !== key;
        });
      });
    });
  });
}

wireTabs('[data-hero-demo]', 'data-demo-tab', 'data-demo-panel', 'is-active');
wireTabs('[data-workbench]', 'data-tab', 'data-panel', 'is-current');
wireTabs('[data-preview]', 'data-preview-tab', 'data-preview-view', 'active');

// Inline copy buttons (install bar & workbench)
document.querySelectorAll<HTMLButtonElement>('[data-copy-text]').forEach(button => {
  button.addEventListener('click', async () => {
    const text = button.dataset.copyText;
    if (!text) return;
    await navigator.clipboard.writeText(text);
    const original = button.innerHTML;
    button.dataset.copied = 'true';
    button.textContent = copy().copied;
    window.setTimeout(() => {
      delete button.dataset.copied;
      button.innerHTML = original;
    }, 1600);
  });
});
