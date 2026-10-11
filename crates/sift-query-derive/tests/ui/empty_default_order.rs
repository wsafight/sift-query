extern crate sift_query_core as sift_query;

use sift_query_derive::SearchEntity;

#[derive(SearchEntity)]
#[search_entity(default_order = "", stable_order = "id:asc")]
struct EmptyDefaultOrder {
    #[search_field(sortable)]
    id: i64,
}

fn main() {}
