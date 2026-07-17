struct Create;

wit_bindgen::generate!({ generate_all });
use crate::betty_blocks_types::crud::crud::create as crud_create;
use crate::betty_blocks_types::data_api::data_api::HelperContext;
use crate::betty_blocks_types::types::types::{BettyModel, BettyPropertyMapping};
use crate::exports::betty_blocks::create::create::{Guest, JsonString};

impl Guest for Create {
    fn create(
        helper_context: HelperContext,
        model: BettyModel,
        mapping: BettyPropertyMapping,
        validates: bool,
    ) -> Result<JsonString, String> {
        let validates = match validates {
            true => vec!["default".to_string()],
            false => vec!["empty".to_string()],
        };
        let response = crud_create(&helper_context, &model, &mapping, Some(&validates));
        Ok(response?)
    }
}

export! {Create}
