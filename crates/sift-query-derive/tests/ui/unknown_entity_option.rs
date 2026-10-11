extern crate sift_query_core as sift_query;

use sift_query_derive::SearchEntity;

#[derive(SearchEntity)]
#[search_entity(stable_order = "id:asc", nope = "1")]
struct UnknownOption {
    #[search_field(sortable)]
    id: i64,
}

fn main() {}
