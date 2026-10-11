extern crate sift_query_core as sift_query;

use sift_query_derive::SearchEntity;

#[derive(SearchEntity)]
#[search_entity(stable_order = "id:asc")]
struct BytesInList {
    #[search_field(sortable)]
    id: i64,
    #[search_field(operators = [InList])]
    blob: Vec<u8>,
}

fn main() {}
