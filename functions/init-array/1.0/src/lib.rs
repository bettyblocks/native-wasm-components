use crate::exports::betty_blocks::init_array::init_array::{Guest, Output};

wit_bindgen::generate!({ generate_all });

struct Component;

impl Guest for Component {
    fn init_array() -> Output {
        Output {
            result: String::from("[]"),
        }
    }
}

export! {Component}
