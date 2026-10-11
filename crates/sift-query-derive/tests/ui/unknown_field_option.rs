extern crate sift_query_core as sift_query;

use sift_query_derive::SearchEntity;

#[derive(SearchEntity)]
#[search_entity(stable_order = "id:asc")]
struct UnknownFieldOption {
    #[search_field(sortable, nope = "1")]
    id: i64,
}

fn main() {}
