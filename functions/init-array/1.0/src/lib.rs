use crate::exports::betty_blocks::init_array::init_array::Guest;

wit_bindgen::generate!({ generate_all });

struct Component;

impl Guest for Component {
    fn init_array() -> String {
        String::from("[]")
    }
}

export! {Component}
