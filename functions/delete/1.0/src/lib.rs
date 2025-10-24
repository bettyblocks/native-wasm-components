struct Delete;

wit_bindgen::generate!({ generate_all });
use crate::betty_blocks::crud::crud::{delete as crud_delete, HelperContext};
use crate::exports::betty_blocks::delete::delete::{Guest, Input, JsonString};

impl Guest for Delete {
    fn create(helper_context: HelperContext, input: Input) -> Result<JsonString, String> {
        let response = crud_delete(
            &helper_context,
            &input.record.model,
            &input.record.data.id.to_string(),
        );
        Ok(response?)
    }
}

export! {Delete}
