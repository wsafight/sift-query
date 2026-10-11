# SiftQuery 文档站点

[English](README.md) | 简体中文

UI 直接沿用 app-struct 的 Astro 站点：首页布局、黑绿配色、括号按钮、导航、文档侧栏、搜索、主题、语言切换和移动菜单。来源与许可见 [NOTICE.md](NOTICE.md)。

## 本地运行

需要 Node.js 24 或更新版本、pnpm 11.25.0。在仓库根目录执行：

```bash
pnpm --dir site install --frozen-lockfile
pnpm --dir site dev
```

访问 <http://127.0.0.1:4321/?lang=zh>。

文档直接读取仓库 Markdown。中文参考页收录完整规范；英文参考页提供精简指南，不宣称逐行完整翻译。安装、快速开始、查询示例和数据库选择均提供中英文版本。

## 内容维护

`site/scripts/catalog.mjs` 管理文档分组、路由、中英文源文件和元信息。新手指南与英文参考位于根目录 `docs/guides/`；现有中文规范仍在 `docs/` 中维护。

`prepare.mjs` 生成渲染用 Markdown 和可下载的原文，改写站内链接并复制被引用的仓库文件。Bean Searcher 引用指向上游文档，不依赖本地被忽略的参考目录。中文标题在构建时生成独立 ID，避免双语正文锚点冲突。`search.mjs` 为两种语言分别建立全文索引。

生成目录 `.generated/`、`dist/`、`public/source/`、`public/version.json` 均不提交 Git。

## 验证

检查与构建都生成文档，应按顺序执行：

```bash
pnpm --dir site check
pnpm --dir site build
pnpm --dir site exec playwright install chromium
pnpm --dir site test
pnpm --dir site test:example
```

`test:example` 需要 Rust 1.99.0，会对比中英文示例，再用 crates.io 依赖在临时项目中编译并执行文档里的完整代码。全文索引由 `build` 生成，`dev` 模式使用目录搜索。

构建检查站内链接、资源、标题锚点和重复 ID。自动化测试覆盖桌面与移动端导航、中英文正文与偏好持久化、搜索、代码复制和水平溢出。

验证 GitHub Pages 子路径时，构建与测试使用相同环境：

```bash
DOCS_SITE=https://wsafight.github.io DOCS_BASE=/sift-query/ pnpm --dir site build
DOCS_SITE=https://wsafight.github.io DOCS_BASE=/sift-query/ pnpm --dir site test
```

`pnpm --dir site preview` 在 4321 端口预览当前构建。

## GitHub Pages

`.github/workflows/docs.yml` 在 PR 上检查、构建、测试；在 main 推送或手动执行时部署。仓库 Pages 的 Source 需要选择 **GitHub Actions**。工作流根据 `GITHUB_REPOSITORY` 推导部署地址，当前仓库对应 <https://wsafight.github.io/sift-query/>。自定义域名或路径可设置仓库变量 `DOCS_SITE`、`DOCS_BASE`。

站点构建与运行不需要数据库服务。
