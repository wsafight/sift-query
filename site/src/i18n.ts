export type Lang = 'en' | 'zh';

export const ui = {
  en: {
    skip: 'Skip to main content', home: 'SiftQuery home', primaryNav: 'Primary navigation',
    mobileNav: 'Mobile navigation', footerNav: 'Footer navigation', docsNav: 'Documentation',
    toc: 'On this page', pager: 'Adjacent pages', previous: 'Previous', next: 'Next',
    search: 'Search', searchDocs: 'Search docs', searchEmpty: 'No matching documentation.',
    theme: 'Toggle theme', menuOpen: 'Open menu', menuClose: 'Close menu', lang: 'Switch to Chinese',
    copy: 'Copy', copied: 'Copied', compiler: 'Query model', runtime: 'Execution', modules: 'Capabilities',
    delivery: 'Integration', docs: 'Docs', browseDocs: 'Browse docs', linkHeading: 'Link to this section',
    quickStart: 'Quick start', github: 'GitHub',
  },
  zh: {
    skip: '跳到主要内容', home: 'SiftQuery 首页', primaryNav: '主要导航', mobileNav: '移动端导航',
    footerNav: '页脚导航', docsNav: '文档目录', toc: '本页目录', pager: '相邻文档',
    previous: '上一篇', next: '下一篇', search: '搜索', searchDocs: '搜索文档',
    searchEmpty: '没有匹配的文档。', theme: '切换主题', menuOpen: '打开菜单', menuClose: '关闭菜单',
    lang: 'Switch to English', copy: '复制', copied: '已复制', compiler: '查询模型', runtime: '查询执行',
    modules: '能力', delivery: '接入', docs: '文档', browseDocs: '浏览文档', linkHeading: '链到本节',
    quickStart: '快速开始', github: 'GitHub',
  },
} as const;

export const githubRepo = 'https://github.com/wsafight/sift-query';

export const pagesMeta = {
  home: {
    title: {en: 'SiftQuery - typed queries for Rust and SQLx', zh: 'SiftQuery - Rust 与 SQLx 的类型化查询库'},
    description: {
      en: 'Declare your query model once. SiftQuery validates parameters and runs bounded, parameterized queries on PostgreSQL, MySQL/MariaDB and SQLite.',
      zh: '声明查询模型，校验请求参数，在 PostgreSQL、MySQL/MariaDB 与 SQLite 上执行有预算的参数化查询。',
    },
  },
  docs: {
    title: {en: 'Documentation - SiftQuery', zh: '文档 - SiftQuery'},
    description: {en: 'SiftQuery installation, quick start, query protocol, Rust API and backend guides.', zh: 'SiftQuery 安装、快速开始、查询协议、Rust API 与数据库后端指南。'},
  },
  notFound: {
    title: {en: 'Page not found - SiftQuery', zh: '页面不存在 - SiftQuery'},
    description: {en: 'The requested page could not be found.', zh: '找不到请求的页面。'},
  },
};

export function langFromStorage(value: string | null): Lang {
  return value === 'zh' ? 'zh' : 'en';
}
