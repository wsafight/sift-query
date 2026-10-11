extern crate sift_query_core as sift_query;

use sift_query_derive::SearchEntity;

#[derive(SearchEntity)]
#[search_entity(stable_order = "id:asc")]
struct DuplicateAlias {
    #[search_field(sortable, result_alias = "same")]
    id: i64,
    #[search_field(result_alias = "same")]
    other: i64,
}

fn main() {}
