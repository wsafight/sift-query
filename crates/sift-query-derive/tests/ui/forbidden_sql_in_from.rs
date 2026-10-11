extern crate sift_query_core as sift_query;

use sift_query_derive::SearchEntity;

#[derive(SearchEntity)]
#[search_entity(from = "app_user u; drop table app_user", stable_order = "id:asc")]
struct BadFrom {
    #[search_field(sortable)]
    id: i64,
}

fn main() {}
