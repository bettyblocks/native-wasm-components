use std::collections::HashMap;

use crate::exports::betty_blocks::create_object::create_object::{Guest, Output, JsonString};

wit_bindgen::generate!({ generate_all });

struct Component;

impl Guest for Component {
    fn create_object(key_value_map: Vec::<(JsonString, JsonString)>) -> Output {
        let map = key_value_map.into_iter().collect::<HashMap<JsonString, JsonString>>();

        Output { result: format!("{map:?}") }
    }
}

export! {Component}
    