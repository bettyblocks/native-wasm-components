struct Update;

wit_bindgen::generate!({ generate_all });
use crate::betty_blocks_types::crud::crud::update as crud_update;
use crate::betty_blocks_types::data_api::data_api::DataApi;
use crate::betty_blocks_types::types::types::BettyPropertyMapping;
use crate::exports::betty_blocks::update::update::{BettyRecordJson, BettySelectedRecord, Guest};

impl Guest for Update {
    fn update(
        data_api: &DataApi,
        mapping: BettyPropertyMapping,
        selected_record: BettySelectedRecord,
        validates: bool,
    ) -> Result<BettyRecordJson, String> {
        if let Some(data) = &selected_record.data {
            let validates = match validates {
                true => vec!["default".to_string()],
                false => vec!["empty".to_string()],
            };

            Ok(crud_update(
                data_api,
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
