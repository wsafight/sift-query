extern crate sift_query_core as sift_query;

use sift_query_derive::SearchEntity;

#[derive(SearchEntity)]
#[search_entity(stable_order = "id:asc")]
struct BadColumn {
    #[search_field(column = "u.id -- comment", sortable)]
    id: i64,
}

fn main() {}
