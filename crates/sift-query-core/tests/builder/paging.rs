use super::*;

// ---------------------------------------------------------------------------
// Paging
// ---------------------------------------------------------------------------

#[test]
fn page_and_offset_paging_set_the_expected_flags() {
    let paged = QueryBuilder::<RichEntity>::new(&policy())
        .page(3, 10)
        .build()
        .unwrap()
        .paging();
    assert_eq!(paged.mode, PagingMode::Page { page: 3 });
    assert_eq!(paged.size, 10);
    assert_eq!(paged.offset, 20);
    assert!(paged.explicit_page);
    assert!(paged.explicit_size);
    assert!(!paged.explicit_offset);
    assert!(!paged.explicit_cursor);

    let offset = QueryBuilder::<RichEntity>::new(&policy())
        .offset(5, 10)
        .build()
        .unwrap()
        .paging();
    assert_eq!(offset.mode, PagingMode::Offset);
    assert_eq!(offset.offset, 5);
    assert!(offset.explicit_offset);
    assert!(!offset.explicit_page);
}

#[test]
fn paging_rejects_zero_pages_overflow_and_out_of_range_values() {
    assert_eq!(
        QueryBuilder::<RichEntity>::new(&policy())
            .page(0, 10)
            .build()
            .unwrap_err(),
        ParamError::PagingOutOfRange("page".to_owned())
    );

    assert_eq!(
        QueryBuilder::<RichEntity>::new(&policy())
            .page(u64::MAX, 10)
            .build()
            .unwrap_err(),
        ParamError::PagingOutOfRange("page".to_owned())
    );

    assert_eq!(
        QueryBuilder::<RichEntity>::new(&policy())
            .offset(20_001, 10)
            .build()
            .unwrap_err(),
        ParamError::PagingOutOfRange("page".to_owned())
    );

    assert_eq!(
        QueryBuilder::<RichEntity>::new(&policy())
            .offset(0, 101)
            .build()
            .unwrap_err(),
        ParamError::PagingOutOfRange("page".to_owned())
    );
}

#[test]
fn cursor_paging_requires_a_matching_order_length() {
    let codec = CursorCodec::new([0x11; 32]).unwrap();
    let id = FieldRef::<RichEntity, i64, ScalarKind>::new(FieldId(0));
    let name = FieldRef::<RichEntity, String, ScalarKind>::new(FieldId(1));

    let ordered = QueryBuilder::<RichEntity>::new(&policy())
        .order_by(id, SortOrder::Asc)
        .order_by(name, SortOrder::Asc)
        .build()
        .unwrap();
    let order = ordered.order().to_vec();
    assert_eq!(order.len(), 2);

    let two = codec
        .encode::<RichEntity>(
            &order,
            &[
                CursorValue::Scalar(ScalarValue::I64(1)),
                CursorValue::Scalar(ScalarValue::Text("a".to_owned())),
            ],
        )
        .unwrap();
    let cursor = codec.decode::<RichEntity>(&two, &order).unwrap();
    let paged = QueryBuilder::<RichEntity>::new(&policy())
        .order_by(id, SortOrder::Asc)
        .order_by(name, SortOrder::Asc)
        .cursor(cursor, 5)
        .build()
        .unwrap()
        .paging();
    assert!(matches!(paged.mode, PagingMode::Cursor(Some(_))));
    assert!(paged.explicit_cursor);
    assert_eq!(paged.offset, 0);

    // A cursor whose arity does not match the final ordering is rejected.
    let single = QueryBuilder::<RichEntity>::new(&policy())
        .order_by(id, SortOrder::Asc)
        .build()
        .unwrap();
    let single_order = single.order().to_vec();
    let one = codec
        .encode::<RichEntity>(&single_order, &[CursorValue::Scalar(ScalarValue::I64(1))])
        .unwrap();
    let short = codec.decode::<RichEntity>(&one, &single_order).unwrap();
    assert_eq!(
        QueryBuilder::<RichEntity>::new(&policy())
            .order_by(id, SortOrder::Asc)
            .order_by(name, SortOrder::Asc)
            .cursor(short, 5)
            .build()
            .unwrap_err(),
        ParamError::PagingOutOfRange("cursor".to_owned())
    );
}

#[test]
fn cursor_paging_rejects_an_oversized_page() {
    let codec = CursorCodec::new([0x22; 32]).unwrap();
    let base = QueryBuilder::<CursorEntity>::new(&policy())
        .build()
        .unwrap();
    let token = codec
        .encode::<CursorEntity>(base.order(), &[CursorValue::Scalar(ScalarValue::I64(1))])
        .unwrap();
    let cursor = codec.decode::<CursorEntity>(&token, base.order()).unwrap();
    assert_eq!(
        QueryBuilder::<CursorEntity>::new(&policy())
            .cursor(cursor, 101)
            .build()
            .unwrap_err(),
        ParamError::PagingOutOfRange("cursor".to_owned())
    );

    // The same cursor is accepted at exactly the entity page-size ceiling.
    let cursor = codec.decode::<CursorEntity>(&token, base.order()).unwrap();
    let accepted = QueryBuilder::<CursorEntity>::new(&policy())
        .cursor(cursor, 100)
        .build()
        .unwrap()
        .paging();
    assert_eq!(accepted.size, 100);
}
