extern crate sift_query_core as sift_query;
use sift_query_derive::{SearchEntity, search_entities};
search_entities! {
    fields Shared {
        #[search_field(sortable, result_alias = "same")]
        id: i64,
    }
    #[derive(SearchEntity)]
    #[search_entity(stable_order = "id:asc")]
    struct Collision {
        include Shared;
        #[search_field(result_alias = "same")]
        other: i64
    }
}
fn main() {}
