extern crate sift_query_core as sift_query;

use sift_query_derive::SearchEntity;

#[derive(SearchEntity)]
#[search_entity(group_by = "course_id", stable_order = "course_id:asc")]
struct RemovedGroupBy {
    #[search_field(sortable, kind = GroupKey)]
    course_id: i64,
}

fn main() {}
