wit_bindgen::generate!({ generate_all });
use wasmcloud_component::info;

use crate::data_api::crud::crud::{
    create, delete, update, HelperContext, Model, PropertyKey, PropertyKind, PropertyMap,
};
use crate::exports::test::runner::test::{Guest, JsonString};

struct TestComponent;

impl Guest for TestComponent {
    fn run_create() -> Result<JsonString, String> {
        info!("Calling run");

        let test_context = HelperContext {
            application_id: "test".to_string(),
            action_id: "test".to_string(),
            log_id: "test".to_string(),
            jwt: None,
            encrypted_configurations: None,
        };

        let model = Model {
            name: "test".to_string(),
        };
        let mapping = vec![PropertyMap {
            key: vec![PropertyKey {
                kind: PropertyKind::String,
                name: "name".to_string(),
                object_fields: None,
            }],
            value: Some("New Task".to_string()),
        }];

        info!("Calling create");
        let result = create(&test_context, &model, &mapping, None);
        info!("Called create");

        dbg!(&result);

        match result {
            Ok(reply) => {
                info!("Success: {}", reply);
                Ok("success".to_string())
            }
            Err(e) => {
                info!("Error: {}", e);
                Err("success".to_string())
            }
        }
    }

    fn run_update() -> Result<JsonString, String> {
        info!("Calling run");

        let test_context = HelperContext {
            application_id: "test".to_string(),
            action_id: "test".to_string(),
            log_id: "test".to_string(),
            jwt: None,
            encrypted_configurations: None,
        };

        let model = Model {
            name: "test".to_string(),
        };
        let mapping = vec![PropertyMap {
            key: vec![PropertyKey {
                kind: PropertyKind::String,
                name: "name".to_string(),
                object_fields: None,
            }],
            value: Some("New Task".to_string()),
        }];

        info!("Calling update");
        let result = update(&test_context, &model, "1", &mapping, None);
        info!("Update called");

        dbg!(&result);

        match result {
            Ok(reply) => {
                info!("Success: {}", reply);
                Ok("success".to_string())
            }
            Err(e) => {
                info!("Error: {}", e);
                Err("success".to_string())
            }
        }
    }

    fn run_delete() -> Result<JsonString, String> {
        info!("Calling run");

        let test_context = HelperContext {
            application_id: "test".to_string(),
            action_id: "test".to_string(),
            log_id: "test".to_string(),
            jwt: None,
            encrypted_configurations: None,
        };

        let model = Model {
            name: "test".to_string(),
        };

        info!("Calling delete");
        let result = delete(&test_context, &model, "1");
        info!("Update delete");

        dbg!(&result);

        match result {
            Ok(reply) => {
                info!("Success: {}", reply);
                Ok("success".to_string())
            }
            Err(e) => {
                info!("Error: {}", e);
                Err("success".to_string())
            }
        }
    }
}

export!(TestComponent);
