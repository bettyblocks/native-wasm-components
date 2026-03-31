use crate::exports::betty_blocks::parse_json::parse_json::Guest;

wit_bindgen::generate!({ generate_all });

struct Component;

impl Guest for Component {
    fn parse_json(input: String) -> String {
        input
    }
}

export! {Component}
