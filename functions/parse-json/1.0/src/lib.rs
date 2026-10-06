use crate::exports::betty_blocks::parse_json::parse_json::Guest;

wit_bindgen::generate!({ generate_all });

struct Component;

impl Guest for Component {
    // The schema model only types the step's output in the IDE; the JSON passes through as is.
    fn parse_json(input: String, _schema_model: Option<String>) -> String {
        input
    }
}

export! {Component}
