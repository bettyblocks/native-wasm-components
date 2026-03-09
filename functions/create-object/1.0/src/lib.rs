use std::collections::HashMap;

use crate::exports::betty_blocks::create_object::create_object::{Guest, JsonString};

wit_bindgen::generate!({ generate_all });

struct Component;

impl Guest for Component {
    fn create_object(key_value_map: JsonString) -> JsonString {
        key_value_map
    }
}

export! {Component}
