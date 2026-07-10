struct Update;

wit_bindgen::generate!({ generate_all });
use crate::betty_blocks_utilities::crud::crud::{update as crud_update, HelperContext};
use crate::exports::betty_blocks::update::update::{BettySelectedRecord, Guest, JsonString};
use crate::betty_blocks_utilities::crud::crud::BettyPropertyMapping;

impl Guest for Update {
    fn update(
        helper_context: HelperContext,
        mapping: BettyPropertyMapping,
        selected_record: BettySelectedRecord,
        validates: bool,
    ) -> Result<JsonString, String> {
        if let Some(data) = &selected_record.data {
            let validates = match validates {
                true => vec!["default".to_string()],
                false => vec!["empty".to_string()],
            };

            Ok(crud_update(
                &helper_context,
                &selected_record.model,
                &data.id.to_string(),
                &mapping,
                Some(&validates),
            )?)
        } else {
            Err("Record does not exist".to_string())
        }
    }
}

export! {Update}
