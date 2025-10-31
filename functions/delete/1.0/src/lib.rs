struct Delete;

wit_bindgen::generate!({ generate_all });
use crate::betty_blocks::crud::crud::{delete as crud_delete, HelperContext};
use crate::exports::betty_blocks::delete::delete::{Guest, Input, JsonString};

impl Guest for Delete {
    fn delete(helper_context: HelperContext, input: Input) -> Result<JsonString, String> {
        if let Some(data) = &input.record.data {
            Ok(crud_delete(
                &helper_context,
                &input.record.model,
                &data.id.to_string(),
            )?)
        } else {
            Err("Record does not exist".to_string())
        }
    }
}

export! {Delete}
