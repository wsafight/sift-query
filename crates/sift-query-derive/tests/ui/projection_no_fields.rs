extern crate sift_query_core as sift_query;

use sift_query_derive::SearchProjection;

#[derive(SearchProjection)]
#[search_projection(entity = User)]
struct NoFields {
    #[search_ignore]
    #[sqlx(skip)]
    hidden: i64,
}

fn main() {}
