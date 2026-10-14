use crate::bindings::betty_blocks_types::crud::crud::upsert as crud_upsert;
use crate::bindings::betty_blocks_types::data_api::data_api::DataApi;
// `crud` uses `types@3.0.0`, and `betty-unique-by` starts in `types@3.2.0`. With two versions,
// wit-bindgen puts the version in the module name.
use crate::bindings::betty_blocks_types::types3_0_0::types::{
    BettyModel, BettyProperty, BettyPropertyMapping,
};
use crate::bindings::betty_blocks_types::types3_2_0::types::BettyUniqueBy;
use crate::bindings::exports::betty_blocks::upsert::upsert::{BettyRecordJson, Guest};

mod bindings {
    use super::Upsert;

    wit_bindgen::generate!({ generate_all });

    export! {Upsert}
}

struct Upsert;

impl Guest for Upsert {
    fn upsert(
        data_api: &DataApi,
        model: BettyModel,
        mapping: BettyPropertyMapping,
        mut unique_by: BettyUniqueBy,
        validates: bool,
    ) -> Result<BettyRecordJson, String> {
        let validates = match validates {
            true => vec!["default".to_string()],
            false => vec!["empty".to_string()],
        };

        crud_upsert(
            data_api,
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
