#![cfg(feature = "serde")]

use indexmap::IndexMap;
use sift_query_core::RowValue;
use sift_query_sqlx::{Page, RowMap};

#[test]
fn row_map_page_has_a_stable_json_shape() {
    let mut row = IndexMap::new();
    row.insert("id".to_owned(), RowValue::I64(9_007_199_254_740_993));
    row.insert("name".to_owned(), RowValue::Text("Alice".to_owned()));
    let page = Page::<RowMap> {
        data: vec![row],
        total_count: 1,
        summaries: Vec::new(),
        next_cursor: None,
    };

    let json = serde_json::to_value(page).unwrap();
    assert_eq!(json["data"][0]["id"], "9007199254740993");
    assert_eq!(json["data"][0]["name"], "Alice");
    assert_eq!(json["total_count"], 1);
}
