# SiftQuery documentation site

English | [简体中文](README.zh-CN.md)

The UI directly reuses app-struct's Astro site: homepage sections, black/green palette, bracket links, header/footer, documentation sidebar, search dialog, language/theme controls and mobile navigation. Source and license details are in [NOTICE.md](NOTICE.md).

## Local development

Node.js 24 or newer and pnpm 11.25.0 are required. From the repository root:

```bash
pnpm --dir site install --frozen-lockfile
pnpm --dir site dev
```

Open <http://127.0.0.1:4321/>; append `?lang=zh` for Chinese. Documentation uses repository Markdown as its source. Chinese reference pages preserve the full specifications; English reference pages are concise guides, not complete translations. Installation, quick start, query cookbook and backend selection are available in both languages.

## Structure

- `src/`: shared UI, homepage, documentation rendering and interactions.
- `scripts/catalog.mjs`: documentation groups, routes, source files and bilingual metadata.
- `docs/guides/` in the repository root: onboarding and English reference guides.
- `scripts/prepare.mjs`: generated Markdown and downloadable source files.
- `scripts/search.mjs`: separate English/Chinese Pagefind full-text indexes.
- `.generated/`, `public/source/`, `public/version.json` and `dist/`: generated files, excluded from Git.

To add a page, edit the repository Markdown and register both language sources in the catalog. Linked repository files are copied into `/source/`; Bean Searcher references point to upstream documentation rather than the local ignored reference checkout. Chinese heading IDs are prefixed at build time to avoid duplicate IDs across the two bodies.

## Validation

Run check and build in sequence because both prepare generated content:

```bash
pnpm --dir site check
pnpm --dir site build
pnpm --dir site exec playwright install chromium
pnpm --dir site test
pnpm --dir site test:example
```

`test:example` requires Rust 1.99.0, compares both language snippets, then compiles and executes the exact quick-start code against crates.io in a temporary project. Full-text search is built during `build`; `dev` falls back to catalog search.

Build verifies local links, assets, heading anchors and duplicate IDs. Tests cover desktop/mobile navigation, language/theme persistence, translated content, search, code copying and layout overflow.

To test the production path, build and test with the same base:

```bash
DOCS_SITE=https://wsafight.github.io DOCS_BASE=/sift-query/ pnpm --dir site build
DOCS_SITE=https://wsafight.github.io DOCS_BASE=/sift-query/ pnpm --dir site test
```

`pnpm --dir site preview` serves the current build at port 4321.

## GitHub Pages

`.github/workflows/docs.yml` checks and builds the site on pull requests, and deploys on main pushes or manual dispatch. Select **GitHub Actions** as the Pages source in repository settings. With `GITHUB_REPOSITORY=wsafight/sift-query`, deployment targets <https://wsafight.github.io/sift-query/>. Repository variables `DOCS_SITE` and `DOCS_BASE` can override the origin and path for custom domains.

The workflow runs browser tests before deploying. Local development needs neither GitHub credentials nor a running database.
