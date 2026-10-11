extern crate sift_query_core as sift_query;

use sift_query_derive::SearchEntity;

#[derive(SearchEntity)]
#[search_entity(stable_order = "id:asc")]
struct JsonFilterable {
    #[search_field(sortable)]
    id: i64,
    payload: serde_json::Value,
}

fn main() {}
