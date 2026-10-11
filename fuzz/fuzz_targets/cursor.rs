#![no_main]
use base64::Engine;
use hmac::{Hmac, Mac};
use libfuzzer_sys::fuzz_target;
use sift_query_core::*;
mod common;
fuzz_target!(|data: &[u8]| {
    let codec = CursorCodec::new([42; 32]).unwrap();
    let order = [OrderBy::from(common::Entity::META.stable_order[0])];
    if let Ok(token) = std::str::from_utf8(data) {
        let _ = codec.decode::<common::Entity>(token, &order);
    }
    if data.len() <= 512 {
        let mut mac = Hmac::<sha2::Sha256>::new_from_slice(&[42; 32]).unwrap();
        mac.update(data);
        let mut bytes = data.to_vec();
        bytes.extend_from_slice(&mac.finalize().into_bytes());
        let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
        let _ = codec.decode::<common::Entity>(&token, &order);
    }
});
