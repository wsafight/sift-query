extern crate sift_query_core as sift_query;

use sift_query_derive::SearchProjection;

#[derive(SearchProjection)]
struct NoEntity {
    id: i64,
}

fn main() {}
