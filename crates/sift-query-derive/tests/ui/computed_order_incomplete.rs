extern crate sift_query_core as sift_query;
use sift_query_derive::SearchEntity;
use sift_query_core::FilterOnly;

#[derive(SearchEntity)]
#[search_entity(stable_order = "id:asc")]
struct Invalid {
    #[search_field(sortable)]
    id: i64,
    #[sqlx(skip)]
    #[search_field(kind=FilterOnly, selectable=false, sortable, sort_from="id")]
    distance: FilterOnly<f32>,
}
fn main() {}
