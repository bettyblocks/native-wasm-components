use crate::bindings::betty_blocks_types::crud::crud::upsert as crud_upsert;
use crate::bindings::betty_blocks_types::data_api::data_api::HelperContext;
use crate::bindings::betty_blocks_types::types::types::{
    BettyModel, BettyProperty, BettyPropertyMapping, BettyPropertyPath,
};
use crate::bindings::exports::betty_blocks::upsert::upsert::{BettyRecordJson, Guest};

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
    ) -> Result<BettyRecordJson, String> {
        let validates = match validates {
            true => vec!["default".to_string()],
            false => vec!["empty".to_string()],
        };

        crud_upsert(
            &helper_context,
            &model,
            &mapping,
            // There can only ever be one unique by, but it's still passed as a list, so we just pop the only value out here.
            &BettyProperty {
                name: unique_by
                    .pop()
                    .ok_or_else(|| String::from("No unique by provided"))?
                    .name,
            },
            Some(&validates),
        )
    }
}
