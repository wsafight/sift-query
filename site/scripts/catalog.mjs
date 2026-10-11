export const groups = [
  {
    "id": "start",
    "number": "01",
    "label": {
      "en": "Getting started",
      "zh": "开始使用"
    }
  },
  {
    "id": "query",
    "number": "02",
    "label": {
      "en": "Query contracts",
      "zh": "查询合同"
    }
  },
  {
    "id": "rust",
    "number": "03",
    "label": {
      "en": "Rust integration",
      "zh": "Rust 接入"
    }
  },
  {
    "id": "backends",
    "number": "04",
    "label": {
      "en": "Database backends",
      "zh": "数据库后端"
    }
  },
  {
    "id": "production",
    "number": "05",
    "label": {
      "en": "Production use",
      "zh": "生产使用"
    }
  },
  {
    "id": "project",
    "number": "06",
    "label": {
      "en": "Project reference",
      "zh": "项目参考"
    }
  }
];

export const pages = [
  {
    "source": "docs/guides/en/overview.md",
    "translation": "README.md",
    "slug": "start/overview",
    "group": "start",
    "title": {
      "en": "Overview",
      "zh": "项目概览"
    },
    "description": {
      "en": "What SiftQuery does, its boundaries and where to begin.",
      "zh": "了解 SiftQuery 的用途、边界与阅读顺序。"
    }
  },
  {
    "source": "docs/guides/en/installation.md",
    "translation": "docs/guides/zh/installation.md",
    "slug": "start/installation",
    "group": "start",
    "title": {
      "en": "Installation",
      "zh": "安装"
    },
    "description": {
      "en": "Select features and install the published crates.",
      "zh": "选择 feature 并安装已经发布的 crate。"
    }
  },
  {
    "source": "docs/guides/en/quick-start.md",
    "translation": "docs/guides/zh/quick-start.md",
    "slug": "start/quick-start",
    "group": "start",
    "title": {
      "en": "Quick start",
      "zh": "快速开始"
    },
    "description": {
      "en": "Run a complete SQLite query without an external database.",
      "zh": "无需外部数据库，运行完整 SQLite 查询。"
    }
  },
  {
    "source": "docs/guides/en/query-cookbook.md",
    "translation": "docs/guides/zh/query-cookbook.md",
    "slug": "start/query-cookbook",
    "group": "start",
    "title": {
      "en": "Query cookbook",
      "zh": "查询示例"
    },
    "description": {
      "en": "Filtering, ordering, projection and paging examples.",
      "zh": "筛选、排序、投影和分页示例。"
    }
  },
  {
    "source": "docs/guides/en/product.md",
    "translation": "docs/01-PRODUCT.md",
    "slug": "query/product",
    "group": "query",
    "title": {
      "en": "Product boundaries",
      "zh": "产品边界"
    },
    "description": {
      "en": "Read-only querying, trusted metadata and compatibility principles.",
      "zh": "只读查询、可信元数据与兼容原则。"
    }
  },
  {
    "source": "docs/guides/en/protocol.md",
    "translation": "docs/02-PROTOCOL.md",
    "slug": "query/protocol",
    "group": "query",
    "title": {
      "en": "Parameter protocol",
      "zh": "参数协议"
    },
    "description": {
      "en": "Names, operators, groups, input limits and strict errors.",
      "zh": "参数名、运算符、逻辑分组、输入预算与严格错误。"
    }
  },
  {
    "source": "docs/guides/en/semantics.md",
    "translation": "docs/03-QUERY-SEMANTICS.md",
    "slug": "query/semantics",
    "group": "query",
    "title": {
      "en": "Query semantics",
      "zh": "查询语义"
    },
    "description": {
      "en": "WHERE/HAVING, DISTINCT, ordering, paging and aggregate contracts.",
      "zh": "WHERE/HAVING、DISTINCT、排序、分页与聚合合同。"
    }
  },
  {
    "source": "docs/guides/en/api.md",
    "translation": "docs/04-RUST-API.md",
    "slug": "rust/api",
    "group": "rust",
    "title": {
      "en": "Rust API",
      "zh": "Rust API"
    },
    "description": {
      "en": "Entity declarations, typed queries, scopes and execution entry points.",
      "zh": "实体声明、类型化查询、Scope 与执行入口。"
    }
  },
  {
    "source": "docs/guides/en/extensions.md",
    "translation": "docs/08-P0-EXTENSIONS.md",
    "slug": "rust/extensions",
    "group": "rust",
    "title": {
      "en": "Query extensions",
      "zh": "查询扩展"
    },
    "description": {
      "en": "Codecs, scopes, scan, CSV, labels and extension hooks.",
      "zh": "Codec、Scope、扫描、CSV、Label 与扩展钩子。"
    }
  },
  {
    "source": "docs/guides/en/rpc-and-routing.md",
    "translation": "docs/10-BEAN-SEARCHER-PRODUCT-ALIGNMENT.md",
    "slug": "rust/rpc-and-routing",
    "group": "rust",
    "title": {
      "en": "RPC and pool routing",
      "zh": "RPC 与连接池路由"
    },
    "description": {
      "en": "Query RPC, reusable mappings and trusted-context routing.",
      "zh": "查询 RPC、映射复用与可信上下文路由。"
    }
  },
  {
    "source": "docs/guides/en/server-capabilities.md",
    "translation": "docs/15-PRODUCT-ENHANCEMENTS.md",
    "slug": "rust/server-capabilities",
    "group": "rust",
    "title": {
      "en": "Server capabilities",
      "zh": "服务端增强"
    },
    "description": {
      "en": "Capability matrices, labels, converters and controlled SQL extensions.",
      "zh": "能力矩阵、Label、转换器与受控 SQL 扩展。"
    }
  },
  {
    "source": "docs/guides/en/ecosystem.md",
    "translation": "docs/13-ECOSYSTEM.md",
    "slug": "rust/ecosystem",
    "group": "rust",
    "title": {
      "en": "Metadata, RSQL and RLS",
      "zh": "Metadata、RSQL 与 RLS"
    },
    "description": {
      "en": "Safe metadata, bounded RSQL and trusted transactional RLS.",
      "zh": "安全元数据、有预算的 RSQL 与可信事务内 RLS。"
    }
  },
  {
    "source": "docs/guides/en/backend-selection.md",
    "translation": "docs/guides/zh/backend-selection.md",
    "slug": "backends/backend-selection",
    "group": "backends",
    "title": {
      "en": "Choose a backend",
      "zh": "选择数据库后端"
    },
    "description": {
      "en": "Compare portable and PostgreSQL-specific capabilities.",
      "zh": "对比便携查询能力与 PostgreSQL 专属能力。"
    }
  },
  {
    "source": "docs/guides/en/release.md",
    "translation": "docs/11-RELEASE-AND-BACKENDS.md",
    "slug": "backends/release",
    "group": "backends",
    "title": {
      "en": "Release and backend isolation",
      "zh": "发布与后端隔离"
    },
    "description": {
      "en": "Published packages, feature isolation and validation boundaries.",
      "zh": "已发布包、feature 隔离与验证边界。"
    }
  },
  {
    "source": "docs/guides/en/postgres.md",
    "translation": "docs/12-POSTGRES-CAPABILITIES.md",
    "slug": "backends/postgres",
    "group": "backends",
    "title": {
      "en": "PostgreSQL capabilities",
      "zh": "PostgreSQL 能力"
    },
    "description": {
      "en": "Optional native search, types and database extensions.",
      "zh": "可选原生搜索、类型与数据库扩展。"
    }
  },
  {
    "source": "docs/guides/en/transport-and-cache.md",
    "translation": "docs/09-TRANSPORT-AND-PERFORMANCE.md",
    "slug": "production/transport-and-cache",
    "group": "production",
    "title": {
      "en": "HTTP transport and cache",
      "zh": "HTTP 传输与缓存"
    },
    "description": {
      "en": "GET/POST contracts, TypeScript transport and SQL template caching.",
      "zh": "GET/POST 合同、TypeScript 传输与 SQL 模板缓存。"
    }
  },
  {
    "source": "docs/guides/en/performance.md",
    "translation": "docs/14-PERFORMANCE-BASELINE.md",
    "slug": "production/performance",
    "group": "production",
    "title": {
      "en": "Performance baselines",
      "zh": "性能基线"
    },
    "description": {
      "en": "Collect and interpret reproducible CPU and database benchmarks.",
      "zh": "采集和解读可复现的 CPU 与数据库基准。"
    }
  },
  {
    "source": "docs/guides/en/architecture.md",
    "translation": "docs/05-IMPLEMENTATION.md",
    "slug": "project/architecture",
    "group": "project",
    "title": {
      "en": "Architecture and validation",
      "zh": "架构与验证"
    },
    "description": {
      "en": "Crate responsibilities, implementation contracts and test strategy.",
      "zh": "crate 职责、实施合同与验证策略。"
    }
  },
  {
    "source": "docs/guides/en/status.md",
    "translation": "docs/06-STATUS.md",
    "slug": "project/status",
    "group": "project",
    "title": {
      "en": "Implementation status",
      "zh": "实现状态"
    },
    "description": {
      "en": "Implemented capabilities, release evidence and remaining validation.",
      "zh": "已实现能力、发布证据与剩余验证。"
    }
  },
  {
    "source": "docs/guides/en/conformance.md",
    "translation": "docs/07-BEAN-SEARCHER-CONFORMANCE.md",
    "slug": "project/conformance",
    "group": "project",
    "title": {
      "en": "Bean Searcher conformance",
      "zh": "Bean Searcher 对照"
    },
    "description": {
      "en": "Explicit compatibility scope and intentional Rust differences.",
      "zh": "明确的兼容范围与 Rust 有意差异。"
    }
  },
  {
    "source": "CHANGELOG.md",
    "translation": null,
    "slug": "project/changelog",
    "group": "project",
    "title": {
      "en": "Changelog",
      "zh": "变更记录"
    },
    "description": {
      "en": "Version history and released functionality.",
      "zh": "版本记录与已发布功能。"
    }
  },
  {
    "source": "SECURITY.md",
    "translation": null,
    "slug": "project/security",
    "group": "project",
    "title": {
      "en": "Security",
      "zh": "安全报告"
    },
    "description": {
      "en": "Private vulnerability reporting and supported versions.",
      "zh": "私密漏洞报告与受支持版本。"
    }
  }
];
