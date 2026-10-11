extern crate sift_query_core as sift_query;

use sift_query_derive::SearchProjection;

#[derive(SearchProjection)]
#[search_projection(entity = User)]
struct BadFieldOption {
    #[search_field(nope = "id")]
    id: i64,
}

fn main() {}
