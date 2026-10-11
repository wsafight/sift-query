//! Signed cursor framing and supported key representations.
use crate::ParamError;
mod codec;
mod value;
pub use codec::CursorCodec;
pub use value::{Cursor, CursorValue, IntoCursorValue};

fn invalid_cursor(reason: &str) -> ParamError {
    ParamError::InvalidValue {
        field: "cursor".to_owned(),
        reason: reason.to_owned(),
    }
}
