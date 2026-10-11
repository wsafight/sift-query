import {expect, test} from '@playwright/test';
import {deployment} from '../scripts/deployment.mjs';

test.afterEach(async ({page}) => {
  expect(await page.pageErrors()).toEqual([]);
});

const {base} = deployment();
const home = base;
const docs = `${base}docs/`;
const overview = `${base}start/overview/`;
const quickStart = `${base}start/quick-start/`;

test('homepage uses the query model and three-backend content', async ({page}) => {
  await page.goto(home);
  await expect(page.getByRole('heading', {level: 1, name: 'SiftQuery'})).toBeVisible();
  await expect(page.locator('#compiler')).toContainText('Describe the boundary');
  await expect(page.locator('#runtime')).toContainText('Useful for the first list');
  await expect(page.locator('.app-preview')).toBeVisible();
});

test('navigation opens the documentation catalog', async ({page}, info) => {
  await page.goto(home);
  if (info.project.name === 'mobile') {
    await page.getByRole('button', {name: 'Open menu'}).click();
    await page.getByRole('navigation', {name: 'Mobile navigation'}).getByRole('link', {name: 'Docs', exact: true}).click();
  } else {
    await page.getByRole('navigation', {name: 'Primary navigation'}).getByRole('link', {name: 'Docs', exact: true}).click();
  }
  await expect(page).toHaveURL(docs);
  await expect(page.getByRole('heading', {level: 1, name: 'From your first query to production'})).toBeVisible();
});

test('documentation renders source, contents, and adjacent pages', async ({page}, info) => {
  await page.goto(overview);
  await expect(page.getByRole('heading', {level: 1, name: 'Overview', exact: true})).toBeVisible();
  await expect(page.locator('.prose.i18n-en p').first()).toContainText('typed, parameter-driven, read-only query library');
  await expect(page.getByRole('link', {name: /View source/})).toBeVisible();
  if (info.project.name !== 'mobile') {
    await expect(page.getByRole('navigation', {name: 'On this page'}).first()).toBeVisible();
    await expect(page.locator('.prose.i18n-en h2 .heading-anchor').first()).toBeAttached();
  } else {
    await page.locator('.docs-nav-panel summary').click();
    await expect(page.getByRole('navigation', {name: 'Documentation'}).getByRole('link', {name: 'Installation', exact: true})).toBeVisible();
  }
  await expect(page.getByRole('navigation', {name: 'Adjacent pages'}).getByRole('link', {name: /Installation/})).toBeVisible();
});

test('documentation header stays available while reading', async ({page}) => {
  await page.goto(quickStart);
  await page.evaluate(() => window.scrollTo(0, 1400));
  await expect(page.locator('#site-header')).not.toHaveClass(/is-hidden/);
  await expect(page.getByRole('button', {name: 'Search', exact: true})).toBeVisible();
});

test('full-text search finds English body content and a Chinese result after switching', async ({page}) => {
  await page.goto(home);
  await page.getByRole('button', {name: 'Search', exact: true}).click();
  const dialog = page.getByRole('dialog', {name: 'Search docs'});
  await dialog.getByRole('searchbox').fill('SearchParams');
  await expect(dialog.locator('a[href*="/production/transport-and-cache/"]')).toBeVisible();
  await page.keyboard.press('Escape');
  await page.getByRole('button', {name: 'Switch to Chinese'}).click();
  await page.getByRole('button', {name: '搜索', exact: true}).click();
  await page.getByRole('searchbox').fill('注册额度');
  await expect(page.locator('[data-search-results] a[href*="/rust/server-capabilities/"]')).toBeVisible();
  const result = page.locator('[data-search-results] a[href*="/rust/server-capabilities/"]');
  await expect(result).toHaveAttribute('href', `${base}rust/server-capabilities/?lang=zh`);
  await result.click();
  await expect(page).toHaveURL(`${base}rust/server-capabilities/?lang=zh`);
});

test('search shows an empty state', async ({page}) => {
  await page.goto(home);
  await page.getByRole('button', {name: 'Search', exact: true}).click();
  await page.getByRole('searchbox').fill('xyz-no-such-query-847291');
  await expect(page.getByText('No matching documentation.', {exact: true})).toBeVisible();
});

test('theme and language persist through navigation and reload', async ({page}) => {
  await page.goto(home);
  const root = page.locator('html');
  const before = await root.getAttribute('data-theme');
  await page.getByRole('button', {name: 'Toggle theme'}).click();
  await expect(root).not.toHaveAttribute('data-theme', before || '');
  await page.getByRole('button', {name: 'Switch to Chinese'}).click();
  await expect(root).toHaveAttribute('data-lang', 'zh');
  await expect(page.locator('.hero-statement')).toContainText('声明查询模型');
  await page.goto(quickStart);
  await expect(root).toHaveAttribute('data-lang', 'zh');
  await page.reload();
  await expect(root).toHaveAttribute('data-lang', 'zh');
  await expect(root).not.toHaveAttribute('data-theme', before || '');
});

test('Chinese guides and the complete reference render', async ({page}) => {
  await page.goto(`${quickStart}?lang=zh`);
  await expect(page.getByRole('heading', {level: 1, name: '快速开始', exact: true})).toBeVisible();
  await expect(page.locator('.prose.i18n-zh')).toContainText('本例使用内存 SQLite');
  await page.goto(`${base}start/installation/?lang=zh`);
  await expect(page.locator('.prose.i18n-zh')).toContainText('Rust 1.99.0');
  await page.goto(`${base}query/protocol/?lang=zh`);
  await expect(page.locator('.prose.i18n-zh')).toContainText('查询预算');
});

test('English guide links open the complete Chinese contract', async ({page}) => {
  await page.goto(`${base}query/protocol/?lang=en`);
  await page.locator('.prose.i18n-en').getByRole('link', {name: 'complete Chinese contract'}).click();
  await expect(page.locator('html')).toHaveAttribute('data-lang', 'zh');
  await expect(page.getByRole('heading', {level: 1, name: '参数协议', exact: true})).toBeVisible();
});

test('Chinese anchors exist in static HTML without JavaScript', async ({browser}) => {
  const context = await browser.newContext({javaScriptEnabled: false});
  const page = await context.newPage();
  await page.goto(quickStart);
  const ids = await page.locator('[id]').evaluateAll(nodes => nodes.map(node => node.id));
  expect(new Set(ids).size).toBe(ids.length);
  const links = await page.locator('.docs-toc.i18n-zh a').evaluateAll(nodes => nodes.map(node => node.getAttribute('href')?.slice(1)));
  expect(links.length).toBeGreaterThan(0);
  for (const id of links) expect(ids).toContain(decodeURIComponent(id || ''));
  await context.close();
});

test('code copying copies the executable example', async ({page, context}) => {
  await context.grantPermissions(['clipboard-read', 'clipboard-write']);
  await page.goto(quickStart);
  await page.locator('.prose.i18n-en pre').first().getByRole('button', {name: 'Copy', exact: true}).click();
  await expect(page.locator('.prose.i18n-en pre').first().getByRole('button', {name: 'Copied', exact: true})).toBeVisible();
  expect(await page.evaluate(() => navigator.clipboard.readText())).toContain('async fn main()');
});

test('layouts do not overflow horizontally', async ({page}) => {
  for (const url of [home, docs, overview, quickStart, `${base}query/protocol/?lang=zh`, `${base}rust/api/?lang=zh`]) {
    await page.goto(url);
    const overflow = await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
    expect(overflow).toBeLessThanOrEqual(1);
  }
});
