use crate::exports::betty_blocks::reassign_variable::reassign_variable::Guest;

wit_bindgen::generate!({ generate_all });

struct Component;

impl Guest for Component {
    fn reassign_variable(value: String) -> String {
        value
    }
}

export! {Component}
