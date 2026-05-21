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
        globals.insert(key.into(), liquid::model::Value::scalar(value));
    }

    tmpl.render(&globals)
        .map_err(|e| format!("Failed to render Liquid template: {e}"))
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
}
