extern crate sift_query_core as sift_query;

use sift_query_derive::SearchEntity;

#[derive(SearchEntity)]
#[sqlx(flatten)]
#[search_entity(stable_order = "id:asc")]
struct Flattened {
    #[search_field(sortable)]
    id: i64,
}

fn main() {}
