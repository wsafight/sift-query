extern crate sift_query_core as sift_query;

use sift_query_derive::SearchProjection;

#[derive(SearchProjection)]
#[search_projection(entity = User)]
struct GenericProjection<T> {
    id: T,
}

fn main() {}
