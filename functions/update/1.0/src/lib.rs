struct Update;

wit_bindgen::generate!({ generate_all });
use crate::betty_blocks::crud::crud::{update as crud_update, HelperContext};
use crate::exports::betty_blocks::update::update::{Guest, Input, JsonString};

impl Guest for Update {
    fn update(helper_context: HelperContext, input: Input) -> Result<JsonString, String> {
        let validates = match input.validates {
            true => vec!["default".to_string()],
            false => vec!["empty".to_string()],
        };

        let response = crud_update(
            &helper_context,
            &input.selected_record.model,
            &input.selected_record.data.id.to_string(),
            &input.mapping,
            Some(&validates),
        );
        Ok(response?)
    }
}

export! {Update}
