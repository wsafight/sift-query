extern crate sift_query_core as sift_query;
use sift_query_derive::SearchEntity;
#[derive(SearchEntity)]
#[search_entity(stable_order = "id:asc")]
struct Invalid {
    #[search_field(sortable, custom_operators = ["custom", "custom"])]
    id: i64,
}
fn main() {}
