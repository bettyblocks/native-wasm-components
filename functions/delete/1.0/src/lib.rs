struct Delete;

wit_bindgen::generate!({ generate_all });
use crate::betty_blocks_utilities::crud::crud::{delete as crud_delete, HelperContext};
use crate::exports::betty_blocks::delete::delete::{BettySelectedRecord, Guest, JsonString};

impl Guest for Delete {
    fn delete(
        helper_context: HelperContext,
        record: BettySelectedRecord,
    ) -> Result<JsonString, String> {
        if let Some(data) = &record.data {
            Ok(crud_delete(&helper_context, &record.model, &data.id.to_string())?)
        } else {
            Err("Record does not exist".to_string())
        }
    }
}

export! {Delete}
