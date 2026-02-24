use crate::exports::betty_blocks::reassign_variable::reassign_variable::{Guest, Output};

wit_bindgen::generate!({ generate_all });

struct Component;

impl Guest for Component {
    fn reassign_variable(value: String) -> Output {
        Output { result: value }
    }
}

export! {Component}
