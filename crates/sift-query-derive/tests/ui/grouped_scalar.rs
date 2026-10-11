extern crate sift_query_core as sift_query;

use sift_query_derive::SearchEntity;

#[derive(SearchEntity)]
#[search_entity(stable_order = "courseId:asc")]
struct GroupedScalar {
    #[search_field(name = "courseId", kind = GroupKey, sortable)]
    course_id: i64,
    score: i32,
}

fn main() {}
