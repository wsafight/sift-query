/// Checks the ASCII grammar `[A-Za-z][A-Za-z0-9_]*`.
/// This does not validate SQL identifiers, reserved names, or length limits.
#[must_use]
pub fn is_ascii_name(value: &str) -> bool {
    let mut bytes = value.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    first.is_ascii_alphabetic() && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}
