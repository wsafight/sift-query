# 有界 fuzz

独立工作区；依赖不会进入发布库。五个目标覆盖 raw URL、逻辑分组、cursor（包括签名正确但 framing 任意的输入）、RSQL 和原生结构化值/RPC 往返。

Git 只保留手工 `basic` 种子和 `regression-*` 回归输入。自动生成的 hash 命名 corpus、artifacts 和 target 已忽略；定时/手动 CI 每目标运行 120 秒，并上传 corpus/crashes。遇到失败后，将最小复现输入命名为 `regression-*`，修复后纳入回归。

```sh
cargo install cargo-fuzz --locked
cargo +nightly fuzz run raw_params -- -max_total_time=60 -max_len=4096
cargo +nightly fuzz run group_expr -- -max_total_time=60 -max_len=4096
cargo +nightly fuzz run cursor -- -max_total_time=60 -max_len=2048
cargo +nightly fuzz run rsql -- -max_total_time=60 -max_len=4096
cargo +nightly fuzz run native_values -- -max_total_time=60 -max_len=4096
```

工作区 proptest 每项默认 256 个样本。性能基准使用 `cargo bench -p sift-query-sqlx --bench core_pipeline --all-features`；比较同一硬件/编译配置下 parser、计划、无缓存/冷缓存/命中/高基数/并发的相对耗时，不把本机数值宣称为生产 SLO。

2026-10-10 本地使用 `nightly-2026-09-16` 对五个目标各运行约 15 秒，均通过。native_values 曾发现 JSON 大整数经过浮点解析后无法无损 RPC 往返，已启用 arbitrary_precision 并保留 `regression-json-large-number`。有限 fuzz 预算不代表输入空间穷尽。
