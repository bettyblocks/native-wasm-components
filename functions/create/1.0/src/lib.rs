struct Create;

wit_bindgen::generate!({ generate_all });
use crate::betty_blocks_utilities::crud::crud::{create as crud_create, HelperContext};
use crate::exports::betty_blocks::create::create::{Guest, JsonString};
use crate::betty_blocks_utilities::crud::crud::{BettyModel, BettyPropertyMapping};

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
