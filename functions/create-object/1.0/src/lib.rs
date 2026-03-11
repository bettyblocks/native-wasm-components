use std::collections::HashMap;

use crate::exports::betty_blocks::create_object::create_object::{Guest, JsonString};

wit_bindgen::generate!({ generate_all });

struct Component;

impl Guest for Component {
    fn create_object(key_value_map: JsonString) -> Result<JsonString, String> {
        let parsed_map: Vec<serde_json::Value> = serde_json::from_str(&key_value_map)
            .map_err(|_err| String::from("Arguments were not correctly formatted"))?;

        let mut hashmap = HashMap::new();

        for mut key_value in parsed_map {
            hashmap.insert(
                key_value
                    .get_mut("key")
                    .ok_or_else(|| String::from("Key value map was missing a key"))?.take(),
                key_value
                    .get_mut("value")
                    .ok_or_else(|| String::from("Key value map was missing a value"))?.take(),
            );
        }

        serde_json::to_string(&hashmap)
            .map_err(|_err| String::from("Could not stringify collection"))
    }
}

export! {Component}
