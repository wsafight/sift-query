use sift_query_algorithm::is_ascii_name;

#[test]
fn grammar_is_exact_and_independent_of_sql_rules() {
    for name in ["A", "name", "field_123", "HTTPQuery"] {
        assert!(is_ascii_name(name), "{name}");
    }
    for name in ["", "_field", "1field", "a-b", "a.b", "a$b", "中文", "a\0"] {
        assert!(!is_ascii_name(name), "{name}");
    }
    // SQL reserved words are intentionally outside this character grammar.
    assert!(is_ascii_name("select"));
}
