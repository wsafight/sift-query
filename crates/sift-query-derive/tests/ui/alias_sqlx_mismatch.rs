extern crate sift_query_core as sift_query;

use sift_query_derive::SearchEntity;

#[derive(SearchEntity)]
#[search_entity(stable_order = "id:asc")]
struct AliasMismatch {
    #[search_field(sortable, result_alias = "left")]
    #[sqlx(rename = "right")]
    id: i64,
}

fn main() {}
