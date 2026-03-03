use crate::exports::betty_blocks::stringify::stringify::Guest;

wit_bindgen::generate!({ generate_all });

struct Component;

impl Guest for Component {
    fn stringify(value: String) -> String {
        value
    }
}

export! {Component}
