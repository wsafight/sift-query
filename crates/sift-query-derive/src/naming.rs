pub(crate) use sift_query_algorithm::is_ascii_name as valid_name;

pub(crate) fn to_snake_case(value: &str) -> String {
    let mut output = String::new();
    for (index, character) in value.chars().enumerate() {
        if character.is_ascii_uppercase() {
            if index > 0 {
                output.push('_');
            }
            output.push(character.to_ascii_lowercase());
        } else {
            output.push(character);
        }
    }
    output
}

pub(crate) fn to_screaming_snake(value: &str) -> String {
    to_snake_case(value.trim_start_matches("r#")).to_ascii_uppercase()
}
