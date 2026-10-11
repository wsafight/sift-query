extern crate sift_query_core as sift_query;

use sift_query_derive::SearchProjection;

#[derive(SearchProjection)]
#[search_projection(entity = User)]
struct DuplicateSource {
    #[search_field(source = "id")]
    id: i64,
    #[search_field(source = "id")]
    other: i64,
}

fn main() {}
