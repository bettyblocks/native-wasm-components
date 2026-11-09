pub fn render_body(
    template: String,
    variables: Option<Vec<(String, String)>>,
) -> Result<String, String> {
    let Some(vars) = variables else {
        return Ok(template);
    };

    if vars.is_empty() {
        return Ok(template);
    }

    let parser = liquid::ParserBuilder::with_stdlib()
        .build()
        .map_err(|e| format!("Failed to build Liquid parser: {e}"))?;

    let tmpl = parser
        .parse(&template)
        .map_err(|e| format!("Failed to parse Liquid template: {e}"))?;

    let mut globals = liquid::Object::new();
    for (key, value) in vars {
        let value = decode_json_string(&value);
        globals.insert(key.into(), liquid::model::Value::scalar(value));
    }

    tmpl.render(&globals)
        .map_err(|e| format!("Failed to render Liquid template: {e}"))
}

fn decode_json_string(value: &str) -> String {
    match serde_json::from_str::<String>(value) {
        Ok(decoded) => decoded,
        Err(_) => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::render_body;

    #[test]
    fn render_no_variables_returns_template_as_is() {
        let body = "<h1>Hello</h1>".to_string();
        assert_eq!(render_body(body.clone(), None).unwrap(), body);
        assert_eq!(render_body(body.clone(), Some(vec![])).unwrap(), body);
    }

    #[test]
    fn render_substitutes_variables() {
        let body = "Hello {{ name }}!".to_string();
        let vars = vec![("name".to_string(), "World".to_string())];
        assert_eq!(render_body(body, Some(vars)).unwrap(), "Hello World!");
    }

    #[test]
    fn render_decodes_json_encoded_string_variables() {
        let body = "Hello {{ name }}, {{ quote }}!".to_string();
        let vars = vec![
            ("name".to_string(), r#""John""#.to_string()),
            ("quote".to_string(), r#""say \"hi\"""#.to_string()),
        ];
        assert_eq!(
            render_body(body, Some(vars)).unwrap(),
            r#"Hello John, say "hi"!"#
        );
    }

    #[test]
    fn render_keeps_values_that_are_not_json_strings() {
        let body = "{{ count }} {{ flag }} {{ obj }}".to_string();
        let vars = vec![
            ("count".to_string(), "42".to_string()),
            ("flag".to_string(), "true".to_string()),
            ("obj".to_string(), r#"{"a":1}"#.to_string()),
        ];
        assert_eq!(render_body(body, Some(vars)).unwrap(), r#"42 true {"a":1}"#);
    }
}
