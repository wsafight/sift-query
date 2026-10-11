extern crate sift_query_core as sift_query;
use sift_query_derive::{SearchEntity, search_entities};
search_entities! {
    fields One { include Two; }
    fields Two { include One; }
    #[derive(SearchEntity)]
    struct Cyclic { include One; }
}
fn main() {}
