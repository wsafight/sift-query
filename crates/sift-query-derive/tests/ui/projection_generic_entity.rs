extern crate sift_query_core as sift_query;

use sift_query_derive::SearchProjection;

#[derive(SearchProjection)]
#[search_projection(entity = Container<i64>)]
struct GenericEntity {
    id: i64,
}

fn main() {}
