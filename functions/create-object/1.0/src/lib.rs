use std::collections::HashMap;

use crate::exports::betty_blocks::create_object::create_object::{Guest, JsonString};

wit_bindgen::generate!({ generate_all });

struct Component;

impl Guest for Component {
    // The schema model only types the step's output in the IDE; the object passes through as is.
    fn create_object(key_value_map: JsonString, _schema_model: Option<String>) -> JsonString {
        key_value_map
    }
}

export! {Component}
