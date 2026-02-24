use crate::exports::betty_blocks::stringify::stringify::{Guest, Output};

wit_bindgen::generate!({ generate_all });

struct Component;

impl Guest for Component {
    fn stringify(value: String) -> Output {
        Output { result: value }
    }
}

export! {Component}
