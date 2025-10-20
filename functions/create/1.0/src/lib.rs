struct Create;

wit_bindgen::generate!({ generate_all });
use crate::betty_blocks::crud::crud::{create as crud_create, HelperContext};
use crate::exports::betty_blocks::create::create::{Guest, Input, JsonString};

impl Guest for Create {
    fn create(helper_context: HelperContext, input: Input) -> Result<JsonString, String> {
        let validates = match input.validates {
            true => vec!["default".to_string()],
            false => vec!["empty".to_string()],
        };
        let response = crud_create(
            &helper_context,
            &input.model,
            &input.mapping,
            Some(&validates),
        );
        Ok(response?)
    }
}

export! {Create}
