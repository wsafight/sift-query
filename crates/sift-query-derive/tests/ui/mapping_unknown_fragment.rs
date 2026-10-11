extern crate sift_query_core as sift_query;
use sift_query_derive::{SearchEntity, search_entities};
search_entities! {
    #[derive(SearchEntity)]
    struct Unknown { include Missing; }
}
fn main() {}
