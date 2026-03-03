use crate::exports::betty_blocks::add_variable_to_array::add_variable_to_array::{
    Guest, JsonString,
};

wit_bindgen::generate!({ generate_all });

struct Component;

impl Guest for Component {
    fn add_variable_to_array(
        collection_json: JsonString,
        item: JsonString,
    ) -> Result<JsonString, String> {
        let mut collection: Vec<serde_json::Value> = serde_json::from_str(&collection_json)
            .map_err(|_err| String::from("Arguments were not correctly formatted"))?;
        collection.push(
            serde_json::from_str(&item)
                .map_err(|_err| String::from("Arguments were not correctly formatted"))?,
        );
        serde_json::to_string(&collection)
            .map_err(|_err| String::from("Could not stringify collection"))
    }
}

export! {Component}
