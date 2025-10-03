use rquickjs::{
    embed, loader::Bundle, CatchResultExt, CaughtError, Context, Ctx, Module, Runtime, Value,
};

use crate::exports::betty_blocks::expression::expression::{Guest, Input, Output};

wit_bindgen::generate!({ generate_all });

struct Expression;

static TEMPLATED_JS: Bundle = embed! {
    "templayed": "includes/templated.js/templayed.js",
};

fn into_json<'a>(value: Value<'a>, ctx: &Ctx<'a>) -> Result<String, String> {
    match ctx.json_stringify(value).map_err(|e| e.to_string())? {
        Some(x) => Ok(x.to_string().map_err(|e| e.to_string())?),
        None => Ok("null".to_string()),
    }
}

fn handle_catch_error<'a>(error: CaughtError<'a>, ctx: &Ctx<'a>) -> String {
    match error {
        CaughtError::Error(e) => e.to_string(),
        CaughtError::Exception(e) => e.to_string(),
        CaughtError::Value(e) => match into_json(e, ctx) {
            Ok(x) => x,
            Err(e) => e,
        },
    }
}

impl Guest for Expression {
    fn expression(input: Input) -> Result<Output, String> {
        let rt = Runtime::new().expect("if not enough memory, should we just crash");
        let ctx = Context::full(&rt).expect("if not enough memory, should we just crash");

        let escaped_expression = format!("{:?}", input.expression);
        let escaped_variables = format!("{:?}", input.variables);

        rt.set_loader(TEMPLATED_JS, TEMPLATED_JS);
        let out: Result<String, String> = ctx.with(|ctx| {
            let source = format!(
                r#"
import templayed from 'templayed';
let template = templayed({escaped_expression})(JSON.parse({escaped_variables}));
export const result = JSON.stringify(new Function(`return ${{template}}`)() ?? null)
        "#
            );

            let (module, promise) = Module::declare(ctx.clone(), "main", source)
                .catch(&ctx)
                .map_err(|e| handle_catch_error(e, &ctx))?
                .eval()
                .catch(&ctx)
                .map_err(|e| handle_catch_error(e, &ctx))?;

            promise
                .finish::<()>()
                .catch(&ctx)
                .map_err(|e| handle_catch_error(e, &ctx))?;

            let result: String = module
                .get("result")
                .catch(&ctx)
                .map_err(|e| handle_catch_error(e, &ctx))?;

            Ok(result)
        });

        Ok(Output { result: out? })
    }
}

export! {Expression}

#[cfg(test)]
fn run_expression(expression: String, variables: String) -> Result<Output, String> {
    Expression::expression(Input {
        expression,
        variables,
        schema_model: None,
        debug_logging: None,
    })
}

#[test]
fn simple_number_expression_test() {
    let out = run_expression("1 + 2".to_string(), "{}".to_string()).unwrap();
    assert_eq!("3".to_string(), out.result);
}

#[test]
fn simple_number_expression_with_substitution_test() {
    let out = run_expression("1 + {{number}}".to_string(), r#"{"number": 6}"#.to_string()).unwrap();
    assert_eq!("7".to_string(), out.result);
}

#[test]
fn simple_text_expression_with_substitution_test() {
    let out = run_expression(
        r#""{{ first_name}}" + " " + "{{ last_name }}""#.to_string(),
        r#"{"first_name": "John", "last_name": "Doe"}"#.to_string(),
    )
    .unwrap();
    assert_eq!(r#""John Doe""#.to_string(), out.result);
}

#[test]
fn templated_magic_test_1() {
    let out = run_expression(
        r#"{{ array.length }}"#.to_string(),
        r#"{"array": [1,2,3,4,5]}"#.to_string(),
    )
    .unwrap();
    assert_eq!("5".to_string(), out.result);
}

#[test]
fn templated_magic_test_2() {
    let out = run_expression(
        r#"{{ array.reduce((x, y) => x+y, 0) }}"#.to_string(),
        r#"{"array": [1,2,3,4,5]}"#.to_string(),
    )
    .unwrap();
    assert_eq!("15".to_string(), out.result);
}

#[test]
fn templated_magic_test_3() {
    let out = run_expression(
        r#""{{ map.nested.text }}""#.to_string(),
        r#"{"map": {"nested": {"text": "testing"}}}"#.to_string(),
    )
    .unwrap();
    assert_eq!(r#""testing""#.to_string(), out.result);
}

#[test]
fn variable_not_found_test() {
    let out = run_expression(r#"{{ testing }}"#.to_string(), r#"{}"#.to_string()).unwrap();

    assert_eq!(r#"null"#.to_string(), out.result);
}

#[test]
fn invalid_template_test() {
    let error = run_expression(r#"{{ oke"#.to_string(), r#"{}"#.to_string()).unwrap_err();

    assert!(error.contains("invalid property name"))
}

#[test]
fn unquoted_string_test() {
    let error = run_expression(
        r#"{{ item }}"#.to_string(),
        r#"{"item": "testing"}"#.to_string(),
    )
    .unwrap_err();

    assert!(error.contains("testing is not defined"))
}
