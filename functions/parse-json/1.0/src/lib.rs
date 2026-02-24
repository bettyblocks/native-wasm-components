use crate::exports::betty_blocks::parse_json::parse_json::{Guest, Output};

wit_bindgen::generate!({ generate_all });

struct Component;

impl Guest for Component {
    fn parse_json(input: String) -> Output {
        Output { result: input }
    }
}

export! {Component}
