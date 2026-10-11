extern crate sift_query_core as sift_query;

use sift_query_derive::SearchEntity;

#[derive(SearchEntity)]
#[search_entity(stable_order = "id:asc")]
struct DuplicateRename {
    #[search_field(sortable)]
    #[sqlx(rename = "a")]
    #[sqlx(rename = "b")]
    id: i64,
}

fn main() {}
