//! Signed cursor codec: framing, tamper resistance, and every failure mode.

use base64::Engine;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use sift_query_core::{
    ConfigError, CursorCodec, CursorValue, EntityLimits, EntityMeta, FieldId, FieldKind, NullOrder,
    Operator, OperatorSet, OrderBy, ParamError, RowValue, ScalarValue, SearchEntity, SortOrder,
    ValueType,
};

#[path = "../support/mod.rs"]
mod support;

use support::{CursorEntity, RichEntity, TestEntity};

type HmacSha256 = Hmac<Sha256>;

const KEY: [u8; 32] = [0x5a; 32];

fn codec() -> CursorCodec {
    CursorCodec::new(KEY).unwrap()
}

/// Builds an ordering the way the parser would, so cursors and codec agree on
/// field ids, directions, and null placement.
fn order_of(specs: &[(u16, SortOrder)]) -> Vec<OrderBy> {
    specs
        .iter()
        .map(|(field, direction)| OrderBy {
            field: FieldId(*field),
            direction: *direction,
            nulls: NullOrder::from(*direction),
        })
        .collect()
}

/// Re-signs an arbitrary payload so that only the payload semantics, not the
/// signature, decide the outcome. This is how a white-box test reaches the
/// framing checks that a valid token can never violate.
fn sign(payload: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(&KEY).unwrap();
    mac.update(payload);
    let mut bytes = payload.to_vec();
    bytes.extend_from_slice(&mac.finalize().into_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn payload(version: u8, fingerprint: u64, count: u8, body: &[u8]) -> Vec<u8> {
    let mut out = vec![version];
    out.extend_from_slice(&fingerprint.to_be_bytes());
    out.push(count);
    out.extend_from_slice(body);
    out
}

fn key_body(field: u16, direction: u8, nulls: u8, value: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&field.to_be_bytes());
    out.push(direction);
    out.push(nulls);
    out.extend_from_slice(value);
    out
}

mod framing_and_tamper_resistance;
mod multi_key_and_cross_schema_behaviour;
mod optional_value_types;
