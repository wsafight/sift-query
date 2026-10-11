extern crate sift_query_core as sift_query;

use sift_query_derive::SearchEntity;

#[derive(SearchEntity)]
#[search_entity(stable_order = "id:asc")]
struct NoFields {
    #[search_ignore]
    #[sqlx(skip)]
    hidden: String,
}

fn main() {}
