use liquid::ParserBuilder;
use liquid::model::{Object, Value};
use liquid_core::{
    Display_filter, Expression, Filter, FilterParameters, FilterReflection, FromFilterParameters,
    ParseFilter, Result, Runtime, ValueView,
};

pub fn render(
    html: &str,
    variables: &[(String, Option<String>)],
) -> std::result::Result<String, String> {
    let globals = globals(variables)?;
    ParserBuilder::with_stdlib()
        .filter(Group)
        .build()
        .map_err(|e| e.to_string())?
        .parse(html)
        .map_err(|e| format!("Invalid template: {e}"))?
        .render(&globals)
        .map_err(|e| format!("Failed to render template: {e}"))
}

fn globals(variables: &[(String, Option<String>)]) -> std::result::Result<Object, String> {
    let mut globals = Object::new();
    for (name, json) in variables {
        let value = match json {
            Some(json) => {
                let json: serde_json::Value = serde_json::from_str(json)
                    .map_err(|e| format!("Variable {name} is not valid JSON: {e}"))?;
                liquid::model::to_value(&json).map_err(|e| e.to_string())?
            }
            None => Value::Nil,
        };
        globals.insert(name.clone().into(), value);
    }
    Ok(globals)
}

#[derive(Debug, FilterParameters)]
struct GroupArgs {
    #[parameter(description = "The (dotted) property to group by.", arg_type = "str")]
    key: Expression,
}

#[derive(Clone, ParseFilter, FilterReflection)]
#[filter(
    name = "group",
    description = "Groups a collection by a property.",
    parameters(GroupArgs),
    parsed(GroupFilter)
)]
struct Group;

#[derive(Debug, FromFilterParameters, Display_filter)]
#[name = "group"]
struct GroupFilter {
    #[parameters]
    args: GroupArgs,
}

impl Filter for GroupFilter {
    fn evaluate(&self, input: &dyn ValueView, runtime: &dyn Runtime) -> Result<Value> {
        let args = self.args.evaluate(runtime)?;
        let path: Vec<&str> = args.key.split('.').collect();
        let items: Vec<Value> = if let Some(array) = input.as_array() {
            array.values().map(|v| v.to_value()).collect()
        } else if let Some(object) = input.as_object() {
            object.values().map(|v| v.to_value()).collect()
        } else {
            Vec::new()
        };
        let mut groups: Vec<(String, Vec<Value>)> = Vec::new();
        for item in items {
            let group = lookup(&item, &path)
                .filter(|v| !v.is_nil())
                .map_or_else(|| "undefined".to_string(), |v| v.to_kstr().to_string());
            match groups.iter_mut().find(|(name, _)| *name == group) {
                Some((_, members)) => members.push(item),
                None => groups.push((group, vec![item])),
            }
        }
        Ok(Value::Array(
            groups
                .into_iter()
                .map(|(name, members)| {
                    Value::Array(vec![Value::scalar(name), Value::Array(members)])
                })
                .collect(),
        ))
    }
}

fn lookup<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    path.iter().try_fold(value, |value, key| match value {
        Value::Object(object) => object.get(*key),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::render;

    fn var(name: &str, json: &str) -> (String, Option<String>) {
        (name.to_string(), Some(json.to_string()))
    }

    #[test]
    fn renders_variables_from_their_json_values() {
        let html = "<h1>{{ invoice.number }}</h1>{% for line in lines %}<p>{{ line.total | times: 2 }}</p>{% endfor %}";
        let out = render(
            html,
            &[
                var("invoice", r#"{"number":"INV-7"}"#),
                var("lines", r#"[{"total":3},{"total":4.5}]"#),
            ],
        )
        .unwrap();
        assert_eq!(out, "<h1>INV-7</h1><p>6</p><p>9</p>");
    }

    #[test]
    fn a_variable_without_a_value_renders_as_nothing() {
        let out = render("[{{ unset }}]", &[("unset".to_string(), None)]).unwrap();
        assert_eq!(out, "[]");
    }

    #[test]
    fn a_missing_variable_or_property_is_an_error() {
        let vars = [var("invoice", r#"{"number":"INV-7"}"#)];
        let err = render("{{ missing }}", &vars).unwrap_err();
        assert!(err.starts_with("Failed to render template"), "{err}");
        assert!(err.contains("Unknown variable"), "{err}");
        let err = render("{{ invoice.missing }}", &vars).unwrap_err();
        assert!(err.contains("Unknown index"), "{err}");
    }

    #[test]
    fn groups_like_lodash_group_by_keeping_input_order() {
        let rows = r#"[{"n":1,"c":{"x":"b"}},{"n":2,"c":{"x":"a"}},{"n":3,"c":{"x":"b"}},{"n":4}]"#;
        let html = "{% assign g = rows | group: 'c.x' %}{% for pair in g %}{{ pair[0] }}:{% for r in pair[1] %}{{ r.n }}{% endfor %};{% endfor %}";
        let out = render(html, &[var("rows", rows)]).unwrap();
        assert_eq!(out, "b:13;a:2;undefined:4;");
    }

    #[test]
    fn reports_template_and_variable_errors() {
        assert!(
            render("{% if %}", &[])
                .unwrap_err()
                .starts_with("Invalid template")
        );
        assert!(
            render("x", &[var("v", "{oops")])
                .unwrap_err()
                .contains("Variable v")
        );
    }
}
