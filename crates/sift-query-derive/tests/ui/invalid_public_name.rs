extern crate sift_query_core as sift_query;

use sift_query_derive::SearchEntity;

#[derive(SearchEntity)]
#[search_entity(stable_order = "id:asc")]
struct InvalidName {
    #[search_field(name = "1bad", sortable)]
    id: i64,
}

fn main() {}
