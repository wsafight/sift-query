extern crate sift_query_core as sift_query;
use sift_query_derive::{SearchEntity, search_entities};
search_entities! {
    #[derive(SearchEntity)]
    #[search_mapping(Missing)]
    struct Unknown { id: i64 }
}
fn main() {}
