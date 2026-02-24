use crate::exports::betty_blocks::stringify::stringify::{Guest, Output};

wit_bindgen::generate!({ generate_all });

struct Component;

impl Guest for Component {
    fn stringify(input: String) -> Output {
        Output {
            result: input
        }
    }
}

export! {Component}
    