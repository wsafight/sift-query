use std::hint::black_box;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use sift_query_core::{
    EntityLimits, EntityMeta, FieldId, FieldKind, FieldMeta, Operator, OperatorSet, OrderMeta,
    ParamParser, QueryPlan, QueryPolicy, QueryTarget, RawParams, RowValue, ScalarTextCodec,
    SearchEntity, ServerScope, SortOrder, ValueType,
};
use sift_query_sqlx::{PgRenderer, QueryShapeCache};
use sift_query_sqlx_common::{
    CsvColumn, Dialect, Renderer as PortableRenderer, RowMap, write_row_batch, write_row_into,
};
use tokio::io::AsyncWrite;

const ID_OPS: &[Operator] = &[Operator::Eq];
const VALUE_OPS: &[Operator] = &[Operator::Eq, Operator::InList];
const NAME_OPS: &[Operator] = &[Operator::Eq, Operator::StartWith];
static FIELDS: &[FieldMeta] = &[
    FieldMeta {
        id: FieldId(0),
        public_name: "id",
        column: "b.id",
        result_alias: Some("id"),
        value_type: ValueType::I64,
        kind: FieldKind::Scalar,
        filterable: true,
        selectable: true,
        sortable: true,
        operators: OperatorSet(ID_OPS),
    },
    FieldMeta {
        id: FieldId(1),
        public_name: "value",
        column: "b.value",
        result_alias: Some("value"),
        value_type: ValueType::I32,
        kind: FieldKind::Scalar,
        filterable: true,
        selectable: true,
        sortable: false,
        operators: OperatorSet(VALUE_OPS),
    },
    FieldMeta {
        id: FieldId(2),
        public_name: "name",
        column: "b.name",
        result_alias: Some("name"),
        value_type: ValueType::Text,
        kind: FieldKind::Scalar,
        filterable: true,
        selectable: true,
        sortable: false,
        operators: OperatorSet(NAME_OPS),
    },
];
static ORDER: &[OrderMeta] = &[OrderMeta {
    field: FieldId(0),
    direction: SortOrder::Asc,
}];
static META: EntityMeta = EntityMeta {
    entity_name: "BenchEntity",
    from: "bench_entity b",
    base_where: None,
    fixed_having: None,
    distinct: false,
    fields: FIELDS,
    default_order: ORDER,
    stable_order: ORDER,
    limits: EntityLimits {
        max_page_size: 100,
        max_offset: 20_000,
        timeout_ms: 5_000,
    },
    schema_fingerprint: 42,
};

struct BenchEntity;

impl SearchEntity for BenchEntity {
    const META: &'static EntityMeta = &META;
}

const TWENTY_CONDITIONS: &str = concat!(
    "G0.value=0&G1.value=1&G2.value=2&G3.value=3&G4.value=4&",
    "G5.value=5&G6.value=6&G7.value=7&G8.value=8&G9.value=9&",
    "G10.value=10&G11.value=11&G12.value=12&G13.value=13&G14.value=14&",
    "G15.value=15&G16.value=16&G17.value=17&G18.value=18&G19.value=19"
);

fn parse_plan_render(criterion: &mut Criterion) {
    let policy = QueryPolicy::default();
    let scope = ServerScope::<BenchEntity>::unrestricted();
    criterion.bench_function("parse_plan_render_20_conditions", |bencher| {
        bencher.iter(|| {
            let raw = RawParams::parse(black_box(TWENTY_CONDITIONS)).unwrap();
            let query = ParamParser::new(&policy)
                .unwrap()
                .parse::<BenchEntity>(&raw)
                .unwrap();
            let plan = QueryPlan::new(&query, &scope, QueryTarget::TypedPage).unwrap();
            black_box(PgRenderer.render_list(&plan).unwrap());
        });
    });
    let cache = QueryShapeCache::new(256).unwrap();
    criterion.bench_function("parse_plan_template_20_conditions_capacity256", |bencher| {
        bencher.iter(|| {
            let raw = RawParams::parse(black_box(TWENTY_CONDITIONS)).unwrap();
            let query = ParamParser::new(&policy)
                .unwrap()
                .parse::<BenchEntity>(&raw)
                .unwrap();
            let plan = QueryPlan::new(&query, &scope, QueryTarget::TypedPage).unwrap();
            black_box(cache.render_list(&plan).unwrap());
        });
    });
}

fn make_plan(raw: &str) -> QueryPlan<BenchEntity> {
    let policy = QueryPolicy::default();
    let query = ParamParser::new(&policy)
        .unwrap()
        .parse::<BenchEntity>(&RawParams::parse(raw).unwrap())
        .unwrap();
    QueryPlan::new(&query, &ServerScope::unrestricted(), QueryTarget::TypedPage).unwrap()
}

fn template_cache(criterion: &mut Criterion) {
    let plans = [make_plan("id=1"), make_plan(TWENTY_CONDITIONS)];
    for (name, plan) in ["one_condition", "twenty_conditions"]
        .into_iter()
        .zip(&plans)
    {
        criterion.bench_function(&format!("render_direct/{name}"), |bencher| {
            bencher.iter(|| black_box(PgRenderer.render_list(black_box(plan)).unwrap()));
        });
        let cache = QueryShapeCache::new(256).unwrap();
        cache.render_list(plan).unwrap();
        criterion.bench_function(&format!("template_hit_capacity256/{name}"), |bencher| {
            bencher.iter(|| black_box(cache.render_list(black_box(plan)).unwrap()));
        });
        criterion.bench_function(&format!("template_cold_capacity256/{name}"), |bencher| {
            bencher.iter_batched(
                || QueryShapeCache::new(256).unwrap(),
                |cache| black_box(cache.render_list(plan).unwrap()),
                BatchSize::SmallInput,
            );
        });
    }
    // 256 distinct boolean structures contend for a 16-entry cache. Different
    // values alone deliberately do not create new template identities.
    let diverse = (0..256)
        .map(|bits| {
            let conditions = (0..8)
                .map(|index| format!("G{index}.value={index}"))
                .collect::<Vec<_>>()
                .join("&");
            let mut expr = "G0".to_owned();
            for index in 1..8 {
                expr = format!(
                    "({expr}{}G{index})",
                    if bits & (1 << index) == 0 { "%26" } else { "|" }
                );
            }
            let mut plan = make_plan(&format!("{conditions}&gexpr={expr}"));
            if bits & 1 != 0 {
                plan.order[0].direction = SortOrder::Desc;
            }
            plan
        })
        .collect::<Vec<_>>();
    let cache = QueryShapeCache::new(16).unwrap();
    let mut index = 0usize;
    criterion.bench_function("render_direct/high_cardinality_256", |bencher| {
        bencher.iter(|| {
            let plan = &diverse[index % diverse.len()];
            index += 1;
            black_box(PgRenderer.render_list(plan).unwrap());
        });
    });
    criterion.bench_function("template_high_cardinality_capacity16", |bencher| {
        bencher.iter(|| {
            let plan = &diverse[index % diverse.len()];
            index += 1;
            black_box(cache.render_list(plan).unwrap());
        });
    });
    let shared = QueryShapeCache::new(256).unwrap();
    shared.render_list(&plans[1]).unwrap();
    let mut workers = (0..4)
        .map(|_| make_plan(TWENTY_CONDITIONS))
        .collect::<Vec<_>>();
    criterion.bench_function("template_shared_4threads_400hits", |bencher| {
        bencher.iter(|| {
            std::thread::scope(|threads| {
                for plan in &mut workers {
                    let shared = &shared;
                    threads.spawn(move || {
                        for _ in 0..100 {
                            black_box(shared.render_list(plan).unwrap());
                        }
                    });
                }
            });
        });
    });
    criterion.bench_function("render_direct/shared_4threads_400renders", |bencher| {
        bencher.iter(|| {
            std::thread::scope(|threads| {
                for plan in &mut workers {
                    threads.spawn(move || {
                        for _ in 0..100 {
                            black_box(PgRenderer.render_list(plan).unwrap());
                        }
                    });
                }
            });
        });
    });
}

fn static_validation_and_codec_bindings(criterion: &mut Criterion) {
    let raw = RawParams::parse("id=42&value=4&name=alpha").unwrap();
    let warm_policy = QueryPolicy::default();
    criterion.bench_function("static_validation_warm_entity", |bencher| {
        bencher.iter(|| {
            let query = ParamParser::new(&warm_policy)
                .unwrap()
                .parse::<BenchEntity>(black_box(&raw))
                .unwrap();
            black_box(query);
        });
    });
    criterion.bench_function("static_validation_cold_entity", |bencher| {
        bencher.iter(|| {
            let policy = QueryPolicy::default();
            let query = ParamParser::new(&policy)
                .unwrap()
                .parse::<BenchEntity>(black_box(&raw))
                .unwrap();
            black_box(query);
        });
    });

    let codec = || ScalarTextCodec {
        name: "bench_text",
        field: "name",
        value_type: ValueType::Text,
        trim: true,
    };
    let warm_policy = QueryPolicy::default().with_codec(codec());
    criterion.bench_function("codec_bindings_warm_entity", |bencher| {
        bencher.iter(|| {
            let mut score = 0usize;
            warm_policy
                .for_each_codec_binding::<BenchEntity>(|field, codec| {
                    score += usize::from(field.id.0) + codec.name().len();
                })
                .unwrap();
            black_box(score);
        });
    });
    criterion.bench_function("codec_bindings_cold_entity", |bencher| {
        bencher.iter(|| {
            let policy = QueryPolicy::default().with_codec(codec());
            let mut score = 0usize;
            policy
                .for_each_codec_binding::<BenchEntity>(|field, codec| {
                    score += usize::from(field.id.0) + codec.name().len();
                })
                .unwrap();
            black_box(score);
        });
    });
}

fn portable_template_cache(criterion: &mut Criterion) {
    let plan = make_plan("id=7&name-op=sw&name=alpha");
    for (name, dialect) in [("mysql", Dialect::MySql), ("sqlite", Dialect::Sqlite)] {
        let renderer = PortableRenderer::new(dialect);
        renderer.render_list(&plan).unwrap();
        criterion.bench_function(&format!("portable_template_hit/{name}"), |bencher| {
            bencher.iter(|| black_box(renderer.render_list(black_box(&plan)).unwrap()));
        });
        let diverse = (0..512)
            .map(|bits| {
                let conditions = (0..8)
                    .map(|index| format!("G{index}.value={index}"))
                    .collect::<Vec<_>>()
                    .join("&");
                let mut expression = "G0".to_owned();
                for index in 1..8 {
                    expression = format!(
                        "({expression}{}G{index})",
                        if bits & (1 << index) == 0 { "%26" } else { "|" }
                    );
                }
                let mut plan = make_plan(&format!("{conditions}&gexpr={expression}"));
                if bits & 0x100 != 0 {
                    plan.order[0].direction = SortOrder::Desc;
                }
                plan
            })
            .collect::<Vec<_>>();
        let mut index = 0usize;
        criterion.bench_function(&format!("portable_template_shapes/{name}"), |bencher| {
            bencher.iter(|| {
                let plan = &diverse[index % diverse.len()];
                index += 1;
                black_box(renderer.render_list(plan).unwrap());
            });
        });
    }
}

#[derive(Default)]
struct CountingWriter {
    writes: usize,
    bytes: usize,
}

impl AsyncWrite for CountingWriter {
    fn poll_write(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        this.writes += 1;
        this.bytes += buffer.len();
        Poll::Ready(Ok(buffer.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

fn csv_batching(criterion: &mut Criterion) {
    let rows = (0..512)
        .map(|id| {
            let mut row = RowMap::new();
            row.insert("id".to_owned(), RowValue::I64(id));
            row.insert(
                "name".to_owned(),
                RowValue::Text(format!("name-{id},line\nvalue")),
            );
            row
        })
        .collect::<Vec<_>>();
    let columns = [CsvColumn::new("id", "ID"), CsvColumn::new("name", "Name")];
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let per_row_writes = runtime.block_on(async {
        let mut writer = CountingWriter::default();
        let mut line = String::new();
        for row in &rows {
            write_row_into(&mut writer, &mut line, row, &columns)
                .await
                .unwrap();
        }
        writer.writes
    });
    let batch_writes = runtime.block_on(async {
        let mut writer = CountingWriter::default();
        write_row_batch(&mut writer, &rows, &columns).await.unwrap();
        writer.writes
    });
    assert_eq!(per_row_writes, rows.len());
    assert!(batch_writes < per_row_writes);
    criterion.bench_function("csv_rows_write_per_row", |bencher| {
        bencher.iter(|| {
            runtime.block_on(async {
                let mut writer = CountingWriter::default();
                let mut line = String::new();
                for row in black_box(&rows) {
                    write_row_into(&mut writer, &mut line, row, &columns)
                        .await
                        .unwrap();
                }
                black_box((writer.writes, writer.bytes));
            });
        });
    });
    criterion.bench_function("csv_rows_bounded_batch", |bencher| {
        bencher.iter(|| {
            runtime.block_on(async {
                let mut writer = CountingWriter::default();
                write_row_batch(&mut writer, black_box(&rows), &columns)
                    .await
                    .unwrap();
                black_box((writer.writes, writer.bytes));
            });
        });
    });
}

criterion_group!(
    benches,
    parse_plan_render,
    template_cache,
    static_validation_and_codec_bindings,
    portable_template_cache,
    csv_batching
);
criterion_main!(benches);
