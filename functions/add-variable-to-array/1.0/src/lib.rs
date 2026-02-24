use crate::exports::betty_blocks::add_variable_to_array::add_variable_to_array::{
    Guest, JsonString, Output,
};

wit_bindgen::generate!({ generate_all });

struct Component;

impl Guest for Component {
    fn add_variable_to_array(mut collection: Vec<JsonString>, item: JsonString) -> Output {
        collection.push(item);
        Output { result: collection }
    }
}

export! {Component}
