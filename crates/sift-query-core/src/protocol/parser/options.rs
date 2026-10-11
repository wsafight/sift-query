use std::collections::BTreeSet;

use crate::{
    CursorValue, NullOrder, OrderBy, Paging, PagingMode, ParamError, Projection, QueryPolicy,
    SearchEntity, SortOrder,
};

pub(super) fn parse_projection<E: SearchEntity>(
    only_select: Option<&str>,
    select_exclude: Option<&str>,
) -> Result<Projection, ParamError> {
    if only_select.is_some() && select_exclude.is_some() {
        return Err(ParamError::ConflictingParameters(
            "onlySelect and selectExclude".to_owned(),
        ));
    }
    let Some((value, excluding)) = only_select
        .map(|value| (value, false))
        .or_else(|| select_exclude.map(|value| (value, true)))
    else {
        return Ok(Projection::All);
    };
    let names = parse_csv(value, "projection")?;
    let mut ids = BTreeSet::new();
    for name in names {
        let field = E::META
            .resolve_field(name)
            .ok_or_else(|| ParamError::UnknownField(name.to_owned()))?;
        if !field.selectable || field.result_alias.is_none() {
            return Err(ParamError::NotSelectable(name.to_owned()));
        }
        if !ids.insert(field.id) {
            return Err(ParamError::DuplicateParameter(name.to_owned()));
        }
    }
    let selected = E::META
        .fields
        .iter()
        .filter(|field| {
            field.selectable
                && field.result_alias.is_some()
                && (ids.contains(&field.id) != excluding)
        })
        .map(|field| field.id)
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return Err(ParamError::EmptyProjection);
    }
    Ok(Projection::Fields(selected))
}

pub(super) fn parse_order<E: SearchEntity>(
    policy: &QueryPolicy,
    sort: Option<&str>,
    order: Option<&str>,
    order_by: Option<&str>,
) -> Result<Vec<OrderBy>, ParamError> {
    if order_by.is_some() && (sort.is_some() || order.is_some()) {
        return Err(ParamError::ConflictingParameters(
            "sort/order and orderBy".to_owned(),
        ));
    }
    if order.is_some() && sort.is_none() {
        return Err(ParamError::ConflictingParameters(
            "order requires sort".to_owned(),
        ));
    }

    let mut parsed = Vec::new();
    if let Some(value) = order_by {
        for item in parse_csv(value, "orderBy")? {
            let (field, direction) =
                item.split_once(':')
                    .ok_or_else(|| ParamError::InvalidValue {
                        field: "orderBy".to_owned(),
                        reason: format!("missing direction for {item}"),
                    })?;
            parsed.push(resolve_order::<E>(field, direction)?);
        }
    } else if let Some(value) = sort {
        let fields = parse_csv(value, "sort")?;
        let directions = match order {
            Some(value) => parse_csv(value, "order")?,
            None => vec!["asc"; fields.len()],
        };
        if fields.len() != directions.len() {
            return Err(ParamError::ConflictingParameters(
                "sort and order lengths differ".to_owned(),
            ));
        }
        for (field, direction) in fields.into_iter().zip(directions) {
            parsed.push(resolve_order::<E>(field, direction)?);
        }
    }
    if parsed.len() > policy.limits().max_sort_fields {
        return Err(ParamError::TooComplex("sort field count"));
    }
    ensure_unique_order::<E>(&parsed)?;

    if parsed.is_empty() {
        parsed.extend(E::META.default_order.iter().copied().map(OrderBy::from));
    }
    if E::META.stable_order.is_empty() {
        return Err(ParamError::UnstablePagination);
    }
    for stable in E::META.stable_order {
        if !parsed.iter().any(|item| item.field == stable.field) {
            parsed.push((*stable).into());
        }
    }
    Ok(parsed)
}

fn resolve_order<E: SearchEntity>(field: &str, direction: &str) -> Result<OrderBy, ParamError> {
    let meta = E::META
        .resolve_field(field)
        .ok_or_else(|| ParamError::UnknownField(field.to_owned()))?;
    if !meta.sortable {
        return Err(ParamError::NotSortable(field.to_owned()));
    }
    let direction = SortOrder::parse(direction).ok_or_else(|| ParamError::InvalidValue {
        field: "order".to_owned(),
        reason: format!("invalid direction {direction}"),
    })?;
    Ok(OrderBy {
        field: meta.id,
        direction,
        nulls: NullOrder::from(direction),
    })
}

fn ensure_unique_order<E: SearchEntity>(order: &[OrderBy]) -> Result<(), ParamError> {
    let mut seen = BTreeSet::new();
    for item in order {
        if !seen.insert(item.field) {
            let name = E::META
                .field(item.field)
                .map_or("<unknown>", |field| field.public_name);
            return Err(ParamError::DuplicateParameter(format!("sort {name}")));
        }
    }
    Ok(())
}

pub(super) fn parse_paging<E: SearchEntity>(
    policy: &QueryPolicy,
    page: Option<&str>,
    size: Option<&str>,
    offset: Option<&str>,
    cursor: Option<&str>,
    order: &[OrderBy],
) -> Result<Paging, ParamError> {
    if page.is_some() && offset.is_some()
        || cursor.is_some() && (page.is_some() || offset.is_some())
    {
        return Err(ParamError::ConflictingParameters(
            "page, offset, and cursor are mutually exclusive".to_owned(),
        ));
    }
    let size_value = parse_u64(size.unwrap_or("15"), "size")?;
    let max_size = u64::from(
        policy
            .limits()
            .max_page_size
            .min(E::META.limits.max_page_size),
    );
    if size_value > max_size {
        return Err(ParamError::PagingOutOfRange("size".to_owned()));
    }
    let size_value =
        u32::try_from(size_value).map_err(|_| ParamError::PagingOutOfRange("size".to_owned()))?;
    let max_offset = policy.limits().max_offset.min(E::META.limits.max_offset);
    if let Some(cursor) = cursor {
        if order.iter().any(|order| {
            E::META
                .field(order.field)
                .is_none_or(|field| !field.value_type.supports_cursor())
        }) {
            return Err(ParamError::InvalidValue {
                field: policy.names().cursor.clone(),
                reason: "ordering type is not eligible for cursor pagination".to_owned(),
            });
        }
        if cursor.len() > policy.limits().max_cursor_bytes {
            return Err(ParamError::TooComplex("cursor length"));
        }
        if E::META.is_grouped() {
            return Err(ParamError::InvalidValue {
                field: policy.names().cursor.clone(),
                reason: "cursor pagination does not support grouped entities in M2".to_owned(),
            });
        }
        let codec = policy
            .cursor_codec()
            .ok_or_else(|| ParamError::InvalidValue {
                field: policy.names().cursor.clone(),
                reason: "cursor pagination is not configured".to_owned(),
            })?;
        let cursor = if cursor.is_empty() {
            None
        } else {
            let cursor = codec.decode::<E>(cursor, order)?;
            for (item, value) in order.iter().zip(cursor.values()) {
                let field = E::META
                    .field(item.field)
                    .ok_or_else(|| ParamError::UnknownField(format!("#{}", item.field.0)))?;
                if let CursorValue::Scalar(value) = value
                    && value.value_type() != field.value_type
                {
                    return Err(ParamError::InvalidValue {
                        field: policy.names().cursor.clone(),
                        reason: format!(
                            "cursor value type does not match ordering field {}",
                            field.public_name
                        ),
                    });
                }
            }
            Some(cursor)
        };
        Ok(Paging {
            mode: PagingMode::Cursor(cursor),
            size: size_value,
            offset: 0,
            explicit_page: false,
            explicit_size: size.is_some(),
            explicit_offset: false,
            explicit_cursor: true,
        })
    } else if let Some(offset) = offset {
        let offset_value = parse_u64(offset, "offset")?;
        if offset_value > max_offset {
            return Err(ParamError::PagingOutOfRange("offset".to_owned()));
        }
        Ok(Paging {
            mode: PagingMode::Offset,
            size: size_value,
            offset: offset_value,
            explicit_page: false,
            explicit_size: size.is_some(),
            explicit_offset: true,
            explicit_cursor: false,
        })
    } else {
        let page_value = parse_u64(page.unwrap_or("1"), "page")?;
        if page_value == 0 {
            return Err(ParamError::PagingOutOfRange("page".to_owned()));
        }
        let offset_value = (page_value - 1)
            .checked_mul(u64::from(size_value))
            .ok_or_else(|| ParamError::PagingOutOfRange("page offset overflow".to_owned()))?;
        if offset_value > max_offset {
            return Err(ParamError::PagingOutOfRange(
                "computed page offset".to_owned(),
            ));
        }
        Ok(Paging {
            mode: PagingMode::Page { page: page_value },
            size: size_value,
            offset: offset_value,
            explicit_page: page.is_some(),
            explicit_size: size.is_some(),
            explicit_offset: false,
            explicit_cursor: false,
        })
    }
}

fn parse_u64(value: &str, field: &str) -> Result<u64, ParamError> {
    value.parse().map_err(|_| ParamError::InvalidValue {
        field: field.to_owned(),
        reason: "expected a non-negative integer".to_owned(),
    })
}

fn parse_csv<'value>(value: &'value str, field: &str) -> Result<Vec<&'value str>, ParamError> {
    if value.is_empty() || value.bytes().any(|byte| byte.is_ascii_whitespace()) {
        return Err(ParamError::InvalidValue {
            field: field.to_owned(),
            reason: "list must be non-empty and contain no whitespace".to_owned(),
        });
    }
    let items = value.split(',').collect::<Vec<_>>();
    if items.iter().any(|item| item.is_empty()) {
        return Err(ParamError::InvalidValue {
            field: field.to_owned(),
            reason: "list contains an empty item".to_owned(),
        });
    }
    Ok(items)
}
