use crate::exports::betty_blocks::init_array::init_array::Guest;

wit_bindgen::generate!({ generate_all });

struct Component;

impl Guest for Component {
    // The schema model only types the array's elements in the IDE; the array starts empty either way.
    fn init_array(_schema_model: Option<String>) -> String {
        String::from("[]")
    }
}

export! {Component}
