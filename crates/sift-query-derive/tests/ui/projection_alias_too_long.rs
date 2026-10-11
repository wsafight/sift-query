extern crate sift_query_core as sift_query;

use sift_query_derive::SearchProjection;

#[derive(SearchProjection)]
#[search_projection(entity = User)]
struct LongAlias {
    #[sqlx(rename = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")]
    id: i64,
}

fn main() {}
