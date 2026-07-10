use crate::bindings::betty_blocks_utilities::crud::crud::{upsert as crud_upsert, HelperContext, BettyProperty, BettyModel, BettyPropertyMapping};
use crate::bindings::betty_blocks_utilities::types::types::BettyPropertyPath;
use crate::bindings::exports::betty_blocks::upsert::upsert::{Guest, JsonString};

mod bindings {
    use super::Upsert;

    wit_bindgen::generate!({ generate_all });

    export! {Upsert}
}

struct Upsert;

impl Guest for Upsert {
    fn upsert(
        helper_context: HelperContext,
        model: BettyModel,
        mapping: BettyPropertyMapping,
        mut unique_by: Vec<BettyPropertyPath>,
        validates: bool,
    ) -> Result<JsonString, String> {
        let validates = match validates {
            true => vec!["default".to_string()],
            false => vec!["empty".to_string()],
        };

        crud_upsert(
            &helper_context,
            &model,
            &mapping,
            // There can only ever be one unique by, but it's still passed as a list, so we just pop the only value out here.
            &BettyProperty{name: unique_by.pop().ok_or_else(|| String::from("No unique by provided"))?.name},
            Some(&validates),
        )
    }
}