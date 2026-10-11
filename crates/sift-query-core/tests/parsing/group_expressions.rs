use super::*;

// ---------------------------------------------------------------------------
// Group expressions
// ---------------------------------------------------------------------------

#[test]
fn group_expressions_combine_named_groups() {
    let query = parse_rich("A.id=1&B.name=A&gexpr=A%26B").unwrap();
    let root = query.predicate().root().unwrap();
    assert!(matches!(
        query.predicate().node(root),
        Some(PredicateNode::And(_))
    ));

    let query = parse_rich("A.id=1&B.name=A&gexpr=A%7CB").unwrap();
    let root = query.predicate().root().unwrap();
    assert!(matches!(
        query.predicate().node(root),
        Some(PredicateNode::Or(_))
    ));
}

#[test]
fn group_expressions_support_nesting_and_precedence() {
    // `&` binds tighter than `|`.
    let query = parse_rich("A.id=1&B.name=A&C.age=2&gexpr=A%7CB%26C").unwrap();
    let root = query.predicate().root().unwrap();
    let Some(PredicateNode::Or(children)) = query.predicate().node(root) else {
        panic!("expected an OR root");
    };
    assert_eq!(children.len(), 2);
    assert!(matches!(
        query.predicate().node(children[1]),
        Some(PredicateNode::And(_))
    ));

    let nested = parse_rich("A.id=1&B.name=A&C.age=2&gexpr=%28A%7CB%29%26C").unwrap();
    let root = nested.predicate().root().unwrap();
    assert!(matches!(
        nested.predicate().node(root),
        Some(PredicateNode::And(_))
    ));

    // Whitespace around operators is allowed.
    assert!(parse_rich("A.id=1&B.name=A&gexpr=A%20%7C%20B").is_ok());
}

#[test]
fn group_expressions_reject_bad_input() {
    let cases = [
        ("A.id=1&gexpr=", "empty expression"),
        ("A.id=1&gexpr=B", "undefined group B"),
        ("A.id=1&gexpr=%28A", "missing closing parenthesis"),
        ("A.id=1&gexpr=A%26", "expected group name"),
        ("A.id=1&gexpr=A%7C%7CA", "expected group name"),
        ("A.id=1&gexpr=%40A", "expected group name"),
        ("A.id=1&gexpr=A%20B", "unexpected token"),
        ("A.id=1&B.name=a&gexpr=A", "unused group B"),
    ];
    for (query, reason) in cases {
        let error = rich_error(query);
        match reason {
            "undefined group B" => assert_eq!(error, ParamError::UndefinedGroup("B".to_owned())),
            "unused group B" => assert_eq!(error, ParamError::UnusedGroup("B".to_owned())),
            _ => match error {
                ParamError::InvalidGroupExpr(message) => {
                    assert!(
                        message.contains(reason),
                        "{message} should contain {reason}"
                    );
                }
                other => panic!("unexpected error {other:?} for {query}"),
            },
        }
    }
}

#[test]
fn group_expressions_enforce_depth_and_node_budgets() {
    let deep = with_limits(|limits| limits.max_group_depth = 1);
    assert_eq!(
        rich_with(&deep, "A.id=1&gexpr=%28%28A%29%29").unwrap_err(),
        ParamError::TooComplex("gexpr depth")
    );

    let nodes = with_limits(|limits| limits.max_group_nodes = 1);
    assert_eq!(
        rich_with(&nodes, "A.id=1&B.name=a&gexpr=A%7CB").unwrap_err(),
        ParamError::TooComplex("gexpr node count")
    );
}

#[test]
fn groups_are_only_materialised_when_named() {
    // Without a group expression, named groups are ANDed together.
    let query = parse_rich("A.id=1&B.name=A").unwrap();
    let root = query.predicate().root().unwrap();
    assert!(matches!(
        query.predicate().node(root),
        Some(PredicateNode::And(_))
    ));
}
