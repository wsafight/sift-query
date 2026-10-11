#[test]
fn rejects_invalid_entity_declarations() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/*.rs");
}
