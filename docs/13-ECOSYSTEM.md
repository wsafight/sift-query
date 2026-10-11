# 查询描述、RSQL 与 RLS

三项能力通过 `metadata`、`rsql`、`rls` 分别开启，默认关闭。metadata/RSQL 可在无数据库驱动的 core 中使用；RLS 属于 PG 后端。

## 元数据与 OpenAPI

`metadata::describe::<E>(&policy)` 返回 version=1 的机器描述，包括公开字段名、valueType、filterable/sortable/selectable、运算符及输入类型、分页上限、参数名称、编码约定和 schema/policy fingerprint。稳定顺序由静态字段与注册顺序决定；只暴露公开名，不输出表名、column、alias、base_where 或 sort_template。描述使用与执行相同的 policy，并先验证 operator/Codec 配置。

`metadata::openapi::<E>(&policy)` 返回 OpenAPI 3.1 parameters/requestBody 片段，应用合入自己的路径。GET 使用重复字符串参数，POST 对应 Web adapter 实际接受的 application/x-www-form-urlencoded，schema 值为 string/string[]；结构化值仍以单个编码字符串传输。动态投影、group.field、gexpr 和控制参数合同附在 x-sift-query 描述中。它不是数据库 schema 自动发现，也不是完整 OpenAPI 服务。

axum 示例 `/users/query-description` 实际返回生成描述和 schema，测试检查稳定性、fingerprint 变化和无 SQL 泄漏。

## RSQL 子集

`rsql::parse::<E>(input, &policy)` 返回同一个 UserQuery<E>。支持 `==`、`!=`、`=gt=`、`=ge=`、`=lt=`、`=le=`、`=in=`、`=out=`；`;` 表示 AND，`,` 表示 OR，括号改变组合，引号和反斜杠用于值转义。

例：`(status==active,status==pending);id=gt=10`。字段和运算符必须由实体允许。adapter 转成 RawParams 后经过现有 filter、Codec、白名单、阶段检查和预算；不支持操作、通配符、空列表、非法字段和超深表达式严格报错。分页与投影仍使用库现有 API，adapter 不接收 SQL、Scope 或路由配置。

首版选择 RSQL 有界子集；没有完整 OData `$expand/$compute/$apply` 或 metadata 服务。协议回归位于 core/tests/ecosystem.rs，独立 rsql fuzz target 进入定时 CI。

## 可信 RLS 请求上下文

`RlsConfig::new(keys, roles)` 由服务端声明自定义 GUC key 和允许角色。最多 16 个 key，使用 namespace.name，禁止 pg_ 命名空间；role 必须是合法、可信白名单标识符。`config.context(values, role)` 从已认证上下文创建请求对象，要求 claims 齐全、无重复，每值最多 4096 字节、无 NUL。HTTP 查询参数不能生成这个对象；Debug 隐藏 claim 值和具体角色。

`search_rls` 在获取连接前解析并验证查询，随后创建只读事务，绑定调用 set_config(..., true) 设置 local role、claims 和 statement_timeout。`begin_rls` 可显式运行多个查询，整个句柄共享 deadline；完成时 `finish()` 回滚 local 状态再归还连接。任何查询错误、超时、await 被取消或未 finish 的 drop 都关闭连接，让数据库撤销事务。只有 SequentialBestEffort 可用于该入口，事务预算最多一小时，并受 execution.timeout 限制。

普通查询仍直接执行。RLS 有意承担事务和 SET LOCAL 成本，应只在需要数据库权限上下文的入口使用。RLS 不代替 ServerScope，测试同时应用两者；数据库角色应受 RLS 约束，避免业务执行使用 BYPASSRLS/superuser。

`scan_map_stream_rls` 使用同一事务和单查询流，保持 scan 行数/deadline/并发预算；提前 drop 关闭连接，完整消费回滚后归还连接。`CsvExporter::export_rls` 从该流批量格式化/增强并写入，使用同一列白名单、公式保护和 writer deadline。两个租户、空权限、复用、失败、取消、stream 与 CSV 都有真实 PG 回归。

外部事务使用 `context.apply_to(&mut transaction)` 后调用现有 `search_on/search_map_on`。库不提交/回滚调用方事务，调用方必须在成功/失败/取消后完成清理；local 设置持续到该事务结束。若一个外部事务用于多个权限上下文，应分别使用独立事务。多 pool 使用可信路由选出的 `searcher` 调用这些 API，每个 pool 独立持有连接与预算。

数据库表、RLS POLICY、角色及 grants 由应用迁移管理。库不创建权限策略、提升角色或自动安装扩展。
