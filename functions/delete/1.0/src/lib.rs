struct Delete;

wit_bindgen::generate!({ generate_all });
use crate::betty_blocks_types::crud::crud::delete as crud_delete;
use crate::betty_blocks_types::data_api::data_api::DataApi;
use crate::exports::betty_blocks::delete::delete::{BettySelectedRecord, Guest};

impl Guest for Delete {
    fn delete(data_api: &DataApi, record: BettySelectedRecord) -> Result<String, String> {
        if let Some(data) = &record.data {
            Ok(crud_delete(data_api, &record.model, &data.id.to_string())?)
        } else {
            Err("Record does not exist".to_string())
        }
    }
}

export! {Delete}
