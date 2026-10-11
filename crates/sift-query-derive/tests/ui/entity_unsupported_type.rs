extern crate sift_query_core as sift_query;

use sift_query_derive::SearchEntity;

#[derive(SearchEntity)]
#[search_entity(stable_order = "id:asc")]
struct UnsupportedType {
    #[search_field(sortable)]
    id: i64,
    #[search_field(sortable)]
    label: &'static str,
}

fn main() {}
