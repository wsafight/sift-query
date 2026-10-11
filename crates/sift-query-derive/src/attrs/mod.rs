//! Attribute parsing and normalized declarations for code generation.
mod entity;
mod field;
pub(crate) mod order;
mod sqlx;
mod syntax;
pub(crate) use entity::{DEFAULT_CONTROL_NAMES, EntityConfig, parse_entity_config};
pub(crate) use field::{FieldConfig, Kind, parse_field};
pub(crate) use order::{order_tokens, parse_order_list};
pub(crate) use sqlx::{
    has_attribute, has_sqlx_skip_or_default, reject_sqlx_rename_all, sqlx_rename,
};
pub(crate) use syntax::option_string_tokens;
