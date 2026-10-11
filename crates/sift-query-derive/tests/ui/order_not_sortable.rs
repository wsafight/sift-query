extern crate sift_query_core as sift_query;

use sift_query_derive::SearchEntity;

#[derive(SearchEntity)]
#[search_entity(stable_order = "name:asc")]
struct OrderNotSortable {
    id: i64,
    name: String,
}

fn main() {}
