extern crate sift_query_core as sift_query;

use sift_query_derive::SearchProjection;

#[derive(SearchProjection)]
#[search_projection(entity = User)]
struct DuplicateAlias {
    id: i64,
    #[sqlx(rename = "id")]
    other: i64,
}

fn main() {}
