struct Create;

wit_bindgen::generate!({ generate_all });
use crate::data_api::crud::crud::{create as crud_create, HelperContext};
use crate::exports::betty_blocks::create::create::{Guest, Input, JsonString};

impl Guest for Create {
    fn create(input: Input) -> Result<JsonString, String> {
        let helper_context = HelperContext {
            application_id: "application_id".to_string(),
            action_id: "action_id".to_string(),
            log_id: "log_id".to_string(),
            encrypted_configurations: None,
            jwt: None,
        };
        let validates = match input.validates {
            true => vec!["default".to_string()],
            false => vec!["empty".to_string()],
        };
        let response = crud_create(&helper_context, &input.model, &input.mapping, Some(&validates));
        response
    }
}

export! {Create}
