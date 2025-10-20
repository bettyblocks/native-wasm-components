wit_bindgen::generate!({ generate_all });

use crate::betty_blocks::data_api::data_api::request;
use crate::exports::betty_blocks::crud::crud::{
    Guest, HelperContext, JsonString, Model, ObjectField, PropertyKey, PropertyMap, PropertyMapping,
};

#[derive(Debug, PartialEq)]
enum PropertyKind {
    Object,
    BelongsTo,
    HasMany,
    HasAndBelongsToMany,
    String,
    Other(String),
}

impl<T: AsRef<str>> From<T> for PropertyKind {
    fn from(input: T) -> PropertyKind {
        match input.as_ref() {
            "OBJECT" => PropertyKind::Object,
            "BELONGS_TO" => PropertyKind::BelongsTo,
            "HAS_MANY" => PropertyKind::HasMany,
            "HAS_AND_BELONGS_TO_MANY" => PropertyKind::HasAndBelongsToMany,
            "STRING" => PropertyKind::String,
            x => PropertyKind::Other(x.to_string()),
        }
    }
}

#[derive(Eq, PartialEq, Debug)]
struct GraphQL {
    name: String,
    gql: String,
}

fn capitalize_first_letter(mut s: String) -> String {
    if let Some(r) = s.get_mut(0..1) {
        r.make_ascii_uppercase();
    }
    s
}

fn is_record(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(map) => !map.is_empty() && map.contains_key("id"),
        _ => false,
    }
}

fn is_collection(value: &serde_json::Value) -> bool {
    as_collection(value).is_some()
}

fn as_collection(value: &serde_json::Value) -> Option<&Vec<serde_json::Value>> {
    match value {
        serde_json::Value::Array(arr) if arr.first().map(is_record).unwrap_or(false) => Some(arr),
        _ => None,
    }
}

fn convert_object_to_graphql(field: &str, data: &serde_json::Value) -> String {
    match data {
        serde_json::Value::Object(object) if !object.is_empty() && object.contains_key("id") => {
            object_to_graphql_query(field, object)
        }
        serde_json::Value::Array(arr) if arr.first().map(is_record).unwrap_or(false) => {
            // NOTE:
            // The assumption is that the first object in the array has the same keys as the other
            // objects in the array. This assumption is checked in the data-api.
            let first_object = arr
                .first()
                .expect("this always exists, it is checked in guard")
                .as_object()
                .expect("this always is an object, it is checked in guard");

            object_to_graphql_query(field, first_object)
        }
        _ => field.to_string(),
    }
}

fn object_to_graphql_query(
    field: &str,
    object: &serde_json::Map<String, serde_json::Value>,
) -> String {
    format!(
        "{field} {{ {} }}",
        object
            .iter()
            .map(|(key, val)| convert_object_to_graphql(key, val,))
            .collect::<Vec<_>>()
            .join("\n")
    )
}

fn parse_json_or_string(value: &str) -> serde_json::Value {
    serde_json::from_str(value).unwrap_or_else(|_| serde_json::Value::String(value.to_string()))
}

fn parse_property_value(value: Option<&str>) -> Option<serde_json::Value> {
    value.map(parse_json_or_string)
}

fn object_fields_str(object_fields: &Option<Vec<ObjectField>>) -> String {
    object_fields.as_ref().map_or("".to_string(), |fields| {
        fields
            .iter()
            .map(|f| f.name.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    })
}

fn id_fragment(name: &str) -> String {
    format!("{name} {{ id\n }}",)
}

fn get_query_fields(property_map: PropertyMapping) -> String {
    property_map
        .iter()
        .map(|property: &PropertyMap| {
        assert!(property.key.len() == 1, "Currently the builder doesn't support nested assignments, so we also take the first one");
            let PropertyKey {
                name,
                kind,
                object_fields,
            } = property.key.first().unwrap();

            let kind = PropertyKind::from(kind);

            let property_json = parse_property_value(property.value.as_deref());

            match kind {
                PropertyKind::Object => {
                    format!("{name} {{ {} }}", object_fields_str(object_fields))
                }
                PropertyKind::BelongsTo if property_json.is_some() => {
                    let property_json = property_json.as_ref().expect("is always some");
                    if is_record(property_json) {
                        return convert_object_to_graphql(name, property_json);
                    }

                    id_fragment(name)
                }
                PropertyKind::HasMany | PropertyKind::HasAndBelongsToMany
                    if property_json.is_some() =>
                {
                    let property_json = property_json.as_ref().expect("is always some");
                    if is_collection(property_json) {
                        return convert_object_to_graphql(name, property_json);
                    }
                    id_fragment(name)
                }
                PropertyKind::BelongsTo
                | PropertyKind::HasMany
                | PropertyKind::HasAndBelongsToMany
                    if property_json.is_none() =>
                {
                    id_fragment(name)
                }
                _ => name.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn parse_to_gql_fragment(model_name: &str, property_map: PropertyMapping) -> GraphQL {
    if model_name.is_empty() {
        return GraphQL {
            name: "".to_string(),
            gql: "".to_string(),
        };
    }

    GraphQL {
        name: format!("{}Fields", model_name.to_lowercase()),
        gql: format!(
            "fragment {}Fields on {} {{id {} }}",
            model_name.to_lowercase(),
            capitalize_first_letter(model_name.to_string()),
            get_query_fields(property_map)
        ),
    }
}

fn get_assigned_value(kind: &PropertyKind, value: serde_json::Value) -> serde_json::Value {
    match kind {
        PropertyKind::BelongsTo => {
            if is_record(&value) {
                return value
                    .get("id")
                    .expect("always contains id, checked in is_record")
                    .to_owned();
            }
            value
        }
        PropertyKind::HasMany | PropertyKind::HasAndBelongsToMany => {
            if let Some(items) = as_collection(&value) {
                let record_ids: Vec<serde_json::Value> = items
                    .iter()
                    .map(|val| {
                        if is_record(val) {
                            return val
                                .get("id")
                                .expect("always contains id, checked in is_record")
                                .to_owned();
                        }
                        val.to_owned()
                    })
                    .collect();

                return serde_json::json!({
                  "id": record_ids
                });
            }
            value
        }
        _ => value,
    }
}

fn parse_assigned_properties(property_map: PropertyMapping) -> serde_json::Value {
    let mut result = serde_json::Map::new();
    property_map.iter().for_each(|property: &PropertyMap| {
        assert!(property.key.len() == 1, "Currently the builder doesn't support nested assignments, so we also take the first one");
        let PropertyKey { name, kind, .. } = property.key.first().unwrap();

        let kind = PropertyKind::from(kind);
        let property_json = parse_property_value(property.value.as_deref());

        if let Some(json) = property_json {
            result.insert(
                name.to_string(),
                get_assigned_value(
                    &kind,
                    json,
                ),
            );
        } else {
            result.insert(name.to_string(), serde_json::Value::Null);
        }
    });

    serde_json::Value::Object(result)
}

fn fetch_record(
    helper_context: HelperContext,
    model_name: &str,
    id: &str,
    fragment: &GraphQL,
) -> Result<JsonString, String> {
    let query_name = format!("one{model_name}",);
    let GraphQL { name, gql } = fragment;

    let selection_set: String = if gql.is_empty() {
        format!("...{name}",)
    } else {
        "id".to_string()
    };

    let query = format!(
        r#"{gql}
        query($where: {model_name}FilterInput) {{
            {query_name}(where: $where) {{
                {selection_set}
            }}
        }}"#,
    );

    let result = request(
        &helper_context,
        &query,
        &serde_json::json!(
        {
            "where": {
                "id" : {
                    "eq": id
                },
            },
        })
        .to_string(),
    );

    match result {
        Ok(data) => match serde_json::from_str(&data).unwrap() {
            serde_json::Value::Object(record) => Ok(serde_json::to_string(&record).unwrap()),
            _ => Err("Return type of provider should always be an object".to_string()),
        },
        Err(e) => Err(e),
    }
}

fn format_input_mutation(mutation_name: &str, model_name: &str) -> String {
    format!(
        r#"mutation($input: {model_name}Input, $validationSets: [String]) {{
            {mutation_name}(input: $input, validationSets: $validationSets) {{
                id
            }}
        }}"#,
    )
}

fn format_update_mutation(mutation_name: &str, model_name: &str) -> String {
    format!(
        r#"mutation($id: Int!, $input: {model_name}Input, $validationSets: [String]) {{
            {mutation_name}(id: $id, input: $input, validationSets: $validationSets) {{
                id
            }}
        }}"#,
    )
}

fn format_delete_mutation(mutation_name: &str) -> String {
    format!(
        r#"mutation($id: Int!) {{
            {mutation_name}(id: $id) {{
                id
            }}
        }}"#,
    )
}

fn get_record_id(gql_result: &str, mutation_name: &str) -> Option<String> {
    match parse_json_or_string(gql_result) {
        serde_json::Value::Object(record) => {
            // NOTE:
            // A successfull create/update mutation will always contain an id as string
            Some(
                record
                    .get(mutation_name)
                    .expect("always has this key")
                    .get("id")
                    .expect("always contains an id")
                    .as_str()
                    .expect("always is a string")
                    .to_string(),
            )
        }
        _ => None,
    }
}

fn get_affected_record(
    request_result: &str,
    mutation_name: &str,
    helper_context: HelperContext,
    model_name: &str,
    fragment: &GraphQL,
) -> Result<String, String> {
    let id = get_record_id(request_result, mutation_name)
        .expect("Succesfull create should always return an id");

    fetch_record(helper_context, model_name, &id, fragment)
}

struct CrudComponent {}

impl Guest for CrudComponent {
    fn create(
        helper_context: HelperContext,
        model: Model,
        mapping: PropertyMapping,
        validation_sets: Option<Vec<String>>,
    ) -> Result<JsonString, String> {
        let fragment = parse_to_gql_fragment(&model.name, mapping.clone());

        let assign_properties = parse_assigned_properties(mapping.clone());

        let mutation_name = format!("create{}", model.name);
        let mutation = format_input_mutation(&mutation_name, &model.name);

        let input = serde_json::json!(
            {
                "input": assign_properties,
                "validationSets": validation_sets.unwrap_or_else(|| vec!["default".to_string()]),
            }
        );

        let result = request(
            &helper_context.clone(),
            &mutation,
            &serde_json::to_string(&input).unwrap(),
        );

        match result {
            Ok(data) => get_affected_record(
                &data,
                &mutation_name,
                helper_context,
                &model.name,
                &fragment,
            ),
            Err(e) => Err(e),
        }
    }

    fn update(
        helper_context: HelperContext,
        model: Model,
        record_id: String,
        mapping: PropertyMapping,
        validation_sets: Option<Vec<String>>,
    ) -> Result<JsonString, String> {
        let fragment = parse_to_gql_fragment(&model.name, mapping.clone());

        let assign_properties = parse_assigned_properties(mapping.clone());

        let mutation_name = format!("update{}", model.name);
        let mutation = format_update_mutation(&mutation_name, &model.name);

        let input = serde_json::json!(
            {
                "id": record_id,
                "input": assign_properties,
                "validationSets": validation_sets.unwrap_or_else(|| vec!["default".to_string()]),
            }
        );

        let result = request(
            &helper_context.clone(),
            &mutation,
            &serde_json::to_string(&input).unwrap(),
        );

        match result {
            Ok(data) => get_affected_record(
                &data,
                &mutation_name,
                helper_context,
                &model.name,
                &fragment,
            ),
            Err(e) => Err(e),
        }
    }

    fn delete(
        helper_context: HelperContext,
        model: Model,
        record_id: String,
    ) -> Result<JsonString, String> {
        let mutation_name = format!("delete{}", model.name);
        let mutation = format_delete_mutation(&mutation_name);
        let input = serde_json::json!(
            {
                "id": record_id,
            }
        );

        let result = request(
            &helper_context,
            &mutation,
            &serde_json::to_string(&input).unwrap(),
        );

        match result {
            Ok(_) => Ok(serde_json::json!({"result": "Record deleted"}).to_string()),
            Err(e) => Err(e),
        }
    }
}

export!(CrudComponent);

#[cfg(test)]
mod tests {

    impl GraphQL {
        fn minify(&self) -> Self {
            GraphQL {
                name: self.name.clone(),
                gql: graphql_minify::minify(&self.gql).unwrap(),
            }
        }
    }

    use crate::exports::betty_blocks::crud::crud::ObjectField;
    use serde_json::json;

    use super::*;

    #[test]
    fn returns_task_fragment() {
        let property_map = vec![PropertyMap {
            key: vec![PropertyKey {
                kind: "STRING".to_string(),
                name: "name".to_string(),
                object_fields: None,
            }],
            value: Some("New Task".to_string()),
        }];

        let model_name = "Task";

        let fragment = parse_to_gql_fragment(model_name, property_map);

        assert_eq!(
            fragment.minify(),
            GraphQL {
                gql: r#"
fragment taskFields on Task {
  id
  name
}"#
                .to_string(),
                name: "taskFields".to_string()
            }
            .minify()
        );
    }

    #[test]
    fn returns_task_fragment_with_object_property() {
        let property_map = vec![PropertyMap {
            key: vec![PropertyKey {
                kind: "OBJECT".to_string(),
                name: "object".to_string(),
                object_fields: Some(vec![
                    ObjectField {
                        name: "uuid".to_string(),
                    },
                    ObjectField {
                        name: "answer".to_string(),
                    },
                    ObjectField {
                        name: "score".to_string(),
                    },
                ]),
            }],
            value: Some("New Task".to_string()),
        }];

        let model_name = "Task";

        let fragment = parse_to_gql_fragment(model_name, property_map);

        assert_eq!(
            fragment.minify(),
            GraphQL {
                gql: r#"
fragment taskFields on Task {
    id
    object {
        uuid
        answer
        score
    }
}"#
                .to_string(),
                name: "taskFields".to_string(),
            }
            .minify()
        );
    }

    #[test]
    fn returns_task_fragment_with_belongs_to_relation() {
        let property_map = vec![PropertyMap {
            key: vec![PropertyKey {
                kind: "BELONGS_TO".to_string(),
                name: "model".to_string(),
                object_fields: None,
            }],
            value: Some(
                json!({
                    "createdAt": "2023-04-25T10:11:55+02:00",
                    "id": 1,
                    "name": "model",
                    "updatedAt": "2023-04-25T10:24:05+02:00"
                })
                .to_string(),
            ),
        }];

        let model_name = "Task";

        let fragment = parse_to_gql_fragment(model_name, property_map);

        assert_eq!(
            fragment.minify(),
            GraphQL {
                gql: r#"
fragment taskFields on Task {
    id
    model {
        createdAt
        id
        name
        updatedAt
    }
}"#
                .to_string(),
                name: "taskFields".to_string(),
            }
            .minify()
        );
    }

    #[test]
    fn returns_task_fragment_with_has_many_relation() {
        let property_map = vec![PropertyMap {
            key: vec![PropertyKey {
                kind: "HAS_MANY".to_string(),
                name: "users".to_string(),
                object_fields: None,
            }],
            value: Some(
                json!([
                  {
                    "active": true,
                    "casToken": "c530c3f4079a09f7804259002307b5e50c656d52",
                    "createdAt": "2023-02-08T16:48:14+01:00",
                    "developer": true,
                    "email": "test@example.com",
                    "id": 1,
                    "locale": null,
                    "name": "Admin Blocks",
                    "password": "test",
                    "readMetadata": false,
                    "receivesNotifications": true,
                    "updatedAt": "2023-02-08T16:48:14+01:00"
                  }
                ])
                .to_string(),
            ),
        }];

        let model_name = "Task";

        let fragment = parse_to_gql_fragment(model_name, property_map);

        assert_eq!(
            fragment.minify(),
            GraphQL {
                gql: r#"
fragment taskFields on Task {
    id
    users {
        active
        casToken
        createdAt
        developer
        email
        id
        locale
        name
        password
        readMetadata
        receivesNotifications
        updatedAt
    }
}"#
                .to_string(),
                name: "taskFields".to_string(),
            }
            .minify()
        );
    }

    #[test]
    fn returns_task_fragment_with_multiple_belongs_to_relations() {
        let property_map = vec![PropertyMap {
            key: vec![PropertyKey {
                kind: "BELONGS_TO".to_string(),
                name: "information".to_string(),
                object_fields: None,
            }],
            value: Some(
                json!({
                  "createdAt": "2023-04-26T13:12:08+02:00",
                  "id": 1,
                  "name": "fdsfsfd",
                  "updatedAt": "2023-04-26T13:20:57+02:00",
                  "user": {
                    "active": true,
                    "casToken": "c530c3f4079a09f7804259002307b5e50c656d52",
                    "createdAt": "2023-02-08T16:48:14+01:00",
                    "developer": true,
                    "email": "test@example.com",
                    "id": 1,
                    "locale": null,
                    "name": "Admin Blocks",
                    "password": "test",
                    "readMetadata": false,
                    "receivesNotifications": true,
                    "updatedAt": "2023-04-26T13:06:49+02:00"
                  }
                })
                .to_string(),
            ),
        }];

        let model_name = "Task";

        let fragment = parse_to_gql_fragment(model_name, property_map);

        assert_eq!(
            fragment.minify(),
            GraphQL {
                gql: r#"
fragment taskFields on Task {
    id
    information {
        createdAt
        id
        name
        updatedAt
        user {
            active
            casToken
            createdAt
            developer
            email
            id
            locale
            name
            password
            readMetadata
            receivesNotifications
            updatedAt
        }
    }
}"#
                .to_string(),
                name: "taskFields".to_string(),
            }
            .minify()
        );
    }

    #[test]
    fn returns_task_fragment_with_nested_belongs_to_relations() {
        let property_map = vec![PropertyMap {
            key: vec![PropertyKey {
                kind: "BELONGS_TO".to_string(),
                name: "information".to_string(),
                object_fields: None,
            }],
            value: Some(
                json!({
                  "createdAt": "2023-04-26T13:12:08+02:00",
                  "id": 1,
                  "name": "fdsfsfd",
                  "updatedAt": "2023-04-26T13:20:57+02:00",
                  "createdBy": {
                    "active": true,
                    "casToken": "c530c3f4079a09f7804259002307b5e50c656d52",
                    "createdAt": "2023-02-08T16:48:14+01:00",
                    "developer": true,
                    "email": "test@example.com",
                    "id": 1,
                    "locale": null,
                    "name": "Admin Blocks",
                    "password": "test",
                    "readMetadata": false,
                    "receivesNotifications": true,
                    "updatedAt": "2023-04-26T13:06:49+02:00",
                    "department": { "id": 1, "name": "Betty" }
                  }
                })
                .to_string(),
            ),
        }];

        let model_name = "Task";

        let fragment = parse_to_gql_fragment(model_name, property_map);

        let expected = r#"fragment taskFields on Task {
  id
  information {
    createdAt
    createdBy {
      active
      casToken
      createdAt
      department {
        id
        name
      }
      developer
      email
      id
      locale
      name
      password
      readMetadata
      receivesNotifications
      updatedAt
    }
    id
    name
    updatedAt
  }
}"#;

        assert_eq!(
            fragment.minify(),
            GraphQL {
                gql: expected.to_string(),
                name: "taskFields".to_string()
            }
            .minify()
        );
    }

    #[test]
    fn returns_task_fragment_with_nested_relations_array() {
        let property_map = vec![PropertyMap {
            key: vec![PropertyKey {
                kind: "BELONGS_TO".to_string(),
                name: "information".to_string(),
                object_fields: None,
            }],
            value: Some(
                json!({
                  "createdAt": "2023-04-26T13:12:08+02:00",
                  "id": 1,
                  "name": "fdsfsfd",
                  "updatedAt": "2023-04-26T13:20:57+02:00",
                  "createdBy": {
                    "active": true,
                    "casToken": "c530c3f4079a09f7804259002307b5e50c656d52",
                    "createdAt": "2023-02-08T16:48:14+01:00",
                    "developer": true,
                    "email": "test@example.com",
                    "id": 1,
                    "locale": null,
                    "name": "Admin Blocks",
                    "password": "test",
                    "readMetadata": false,
                    "receivesNotifications": true,
                    "updatedAt": "2023-04-26T13:06:49+02:00",
                    "department": { "id": 1, "name": "Betty" },
                    "roles": [
                      { "id": 1, "name": "Admin" },
                      { "id": 2, "name": "Developer" }
                    ]
                  }
                })
                .to_string(),
            ),
        }];
        let model_name = "Task";

        let fragment = parse_to_gql_fragment(model_name, property_map);

        let expected = r#"
fragment taskFields on Task {
  id
  information {
    createdAt
    createdBy {
      active
      casToken
      createdAt
      department {
        id
        name
      }
      developer
      email
      id
      locale
      name
      password
      readMetadata
      receivesNotifications
      roles {
        id
        name
      }
      updatedAt
    }
    id
    name
    updatedAt
  }
}"#;

        assert_eq!(
            fragment.minify(),
            GraphQL {
                gql: expected.to_string(),
                name: "taskFields".to_string()
            }
            .minify()
        );
    }

    #[test]
    fn returns_task_fragment_belongs_to_by_id() {
        let property_map = vec![PropertyMap {
            key: vec![PropertyKey {
                kind: "BELONGS_TO".to_string(),
                name: "model".to_string(),
                object_fields: None,
            }],
            value: Some("1".to_string()),
        }];

        let model_name = "Task";

        let fragment = parse_to_gql_fragment(model_name, property_map);

        assert_eq!(
            fragment.minify(),
            GraphQL {
                gql: r#"
fragment taskFields on Task {
    id
    model {
        id
    }
}"#
                .to_string(),
                name: "taskFields".to_string(),
            }
            .minify()
        );
    }

    #[test]
    fn returns_task_fragment_has_many_by_id() {
        let property_map = vec![PropertyMap {
            key: vec![PropertyKey {
                kind: "HAS_MANY".to_string(),
                name: "users".to_string(),
                object_fields: None,
            }],
            value: None,
        }];
        let model_name = "Task";

        let fragment = parse_to_gql_fragment(model_name, property_map);

        assert_eq!(
            fragment.minify(),
            GraphQL {
                gql: r#"
fragment taskFields on Task {
    id
    users {
        id
    }
}"#
                .to_string(),
                name: "taskFields".to_string(),
            }
            .minify()
        );
    }

    #[test]
    fn returns_empty_when_model_missing() {
        let property_map = vec![PropertyMap {
            key: vec![PropertyKey {
                kind: "STRING".to_string(),
                name: "name".to_string(),
                object_fields: None,
            }],
            value: None,
        }];

        let model_name = "";

        let fragment = parse_to_gql_fragment(model_name, property_map);

        assert_eq!(
            fragment.minify(),
            GraphQL {
                gql: "".to_string(),
                name: "".to_string()
            }
            .minify()
        );
    }

    #[test]
    fn parse_assigned_propertie_should_create_simple_input_variables() {
        let property_map = vec![PropertyMap {
            key: vec![PropertyKey {
                kind: "STRING".to_string(),
                name: "name".to_string(),
                object_fields: None,
            }],
            value: Some("New Task".to_string()),
        }];

        let assigned_properties = parse_assigned_properties(property_map);
        let expected_result: serde_json::Value = json!({ "name": "New Task" });
        assert_eq!(assigned_properties, expected_result,);
    }

    #[test]
    fn parse_assigned_propertie_should_create_nested_input_variables() {
        let property_map = vec![
            PropertyMap {
                key: vec![PropertyKey {
                    kind: "STRING".to_string(),
                    name: "name".to_string(),
                    object_fields: None,
                }],
                value: Some("test".to_string()),
            },
            PropertyMap {
                key: vec![PropertyKey {
                    kind: "HAS_MANY".to_string(),
                    name: "abilities".to_string(),
                    object_fields: None,
                }],
                value: Some(
                    serde_json::json!([
                        {"name": "Tackle"},
                    ])
                    .to_string(),
                ),
            },
        ];

        let assigned_properties = parse_assigned_properties(property_map);
        let expected_result: serde_json::Value =
            json!({ "name": "test", "abilities": [{"name": "Tackle"}] });
        assert_eq!(assigned_properties, expected_result,);
    }

    #[test]
    fn format_input_mutation_should_return_correct_mutation() {
        let mutation_name = "createuser";
        let model_name = "user";

        assert_eq!(
            graphql_minify::minify(format_input_mutation(&mutation_name, &model_name)),
            graphql_minify::minify(
                r#"
mutation ($input: userInput, $validationSets: [String]) {
  createuser(input: $input, validationSets: $validationSets) {
    id
  }
}"#
            ),
        );
    }

    #[test]
    fn format_update_mutation_should_return_correct_mutation() {
        let mutation_name = "updateuser";
        let model_name = "user";

        assert_eq!(
            graphql_minify::minify(format_update_mutation(&mutation_name, &model_name)),
            graphql_minify::minify(
                r#"
mutation ($id: Int!, $input: userInput, $validationSets: [String]) {
  updateuser(id: $id, input: $input, validationSets: $validationSets) {
    id
  }
}"#
            ),
        );
    }

    #[test]
    fn format_delete_mutation_should_return_correct_mutation() {
        let mutation_name = "deleteuser";

        assert_eq!(
            graphql_minify::minify(format_delete_mutation(&mutation_name)),
            graphql_minify::minify(
                r#"
mutation ($id: Int!) {
  deleteuser(id: $id) {
    id
  }
}"#
            ),
        );
    }

    #[test]
    fn get_record_id_should_get_id_from_request_result() {
        let mutation_name = "createuser";
        let request_result = serde_json::json!({mutation_name: {"id": "uuid"}}).to_string();

        assert_eq!(
            get_record_id(&request_result, &mutation_name),
            Some("uuid".to_string())
        );
    }
}
