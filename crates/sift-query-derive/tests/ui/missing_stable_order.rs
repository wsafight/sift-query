extern crate sift_query_core as sift_query;

use sift_query_derive::SearchEntity;

#[derive(SearchEntity)]
struct MissingStableOrder {
    id: i64,
}

fn main() {}
