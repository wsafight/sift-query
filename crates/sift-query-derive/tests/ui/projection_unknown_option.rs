extern crate sift_query_core as sift_query;

use sift_query_derive::SearchProjection;

#[derive(SearchProjection)]
#[search_projection(entity = User, nope = "1")]
struct UnknownOption {
    id: i64,
}

fn main() {}
