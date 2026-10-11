use sift_query_algorithm::StableHasher64;

use crate::attrs::{EntityConfig, FieldConfig};

pub(crate) fn schema_fingerprint(
    entity_name: &str,
    entity: &EntityConfig,
    fields: &[FieldConfig],
) -> u64 {
    let mut hash = StableHasher64::default();
    let mut add = |value: &str| {
        hash.write_delimited(value);
    };
    add(entity_name);
    add(&entity.from);
    if let Some(value) = &entity.base_where {
        add(value);
    }
    if let Some(value) = &entity.fixed_having {
        add(value);
    }
    add(if entity.distinct { "distinct" } else { "all" });
    if let Some(value) = &entity.default_order {
        add(value);
    }
    if let Some(value) = &entity.stable_order {
        add(value);
    }
    add(&entity.max_page_size.to_string());
    add(&entity.max_offset.to_string());
    add(&entity.timeout_ms.to_string());
    for field in fields {
        add(&field.public_name);
        if field.cursor_non_null {
            add("cursor_non_null");
        }
        for value in [&field.sort_from, &field.sort_operator, &field.sort_template]
            .into_iter()
            .flatten()
        {
            add("condition_order");
            add(value);
        }
        if let Some(codec) = &field.codec {
            add("codec");
            add(codec);
        }
        if let Some(label) = &field.label {
            add("label");
            add(label);
        }
        for operator in &field.custom_operators {
            add("http_custom_operator");
            add(operator);
        }
        add(&field.column);
        if let Some(alias) = &field.result_alias {
            add(alias);
        }
        add(&format!("{:?}", field.kind));
        add(&field.value_type.to_string());
        add(if field.filterable {
            "filterable"
        } else {
            "no-filter"
        });
        add(if field.selectable {
            "selectable"
        } else {
            "no-select"
        });
        add(if field.sortable {
            "sortable"
        } else {
            "no-sort"
        });
        for operator in &field.operators {
            add(&operator.to_string());
        }
    }
    hash.finish()
}
