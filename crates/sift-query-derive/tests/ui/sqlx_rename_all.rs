extern crate sift_query_core as sift_query;

use sift_query_derive::SearchEntity;

#[derive(SearchEntity)]
#[sqlx(rename_all = "snake_case")]
#[search_entity(stable_order = "id:asc")]
struct RenameAll {
    #[search_field(sortable)]
    id: i64,
}

fn main() {}
