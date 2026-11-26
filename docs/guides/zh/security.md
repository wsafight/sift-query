# 安全报告

SiftQuery 目前处于 pre-1.0 阶段。安全修复面向最新的 `0.2.x` 版本发布，不承诺长期支持（LTS）窗口。

如发现疑似安全漏洞，请通过 [GitHub 私密漏洞报告](https://github.com/wsafight/sift-query/security/advisories/new) 私下反馈（本仓库已开启私密报告功能）。一般缺陷反馈请使用 [GitHub Issues](https://github.com/wsafight/sift-query/issues)。

请勿在公开 Issue 中披露利用细节、凭证、真实租户数据或私有 SQL。安全报告建议包含：受影响的版本与 feature 组合、使用临时测试数据的最小复现、预期行为与实际行为以及潜在影响。SQL 注入、授权 Scope 绕过、游标伪造、无界资源消耗和连接上下文泄漏均属于关注范围内的安全问题。
