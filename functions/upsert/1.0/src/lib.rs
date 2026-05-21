use crate::bindings::betty_blocks::crud::crud::{upsert as crud_upsert, HelperContext};
use crate::bindings::exports::betty_blocks::upsert::upsert::{Guest, Input, JsonString};

mod bindings {
    use super::Upsert;

    wit_bindgen::generate!({ generate_all });

    export! {Upsert}
}

struct Upsert;

impl Guest for Upsert {
    fn upsert(helper_context: HelperContext, input: Input) -> Result<JsonString, String> {
        let validates = match input.validates {
            true => vec!["default".to_string()],
            false => vec!["empty".to_string()],
        };

        crud_upsert(
            &helper_context,
            &input.model,
            &input.mapping,
            &input.unique_by,
            Some(&validates),
        )
    }
}