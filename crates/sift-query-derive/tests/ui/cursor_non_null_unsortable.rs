extern crate sift_query_core as sift_query;
use sift_query_derive::SearchEntity;

#[derive(SearchEntity)]
#[search_entity(stable_order = "id:asc")]
struct Invalid {
    #[search_field(sortable)]
    id: i64,
    #[search_field(cursor_non_null)]
    other: i64,
}
fn main() {}
