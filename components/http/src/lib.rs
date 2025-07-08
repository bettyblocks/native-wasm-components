use std::collections::HashMap;

use url::Url;
use waki::{header::HeaderMap, Client};

use crate::exports::betty_blocks::http::http::{self, Input, Method, Output, Scheme};

wit_bindgen::generate!({ with: {
    "wasi:io/streams@0.2.6": ::wasi::io::streams,
    "wasi:io/error@0.2.6": ::wasi::io::error,
    "wasi:clocks/monotonic-clock@0.2.6": ::wasi::clocks::monotonic_clock,
    "wasi:io/poll@0.2.6": ::wasi::io::poll,
    "wasi:http/types@0.2.6": ::wasi::http::types,
    "wasi:http/outgoing-handler@0.2.6": ::wasi::http::outgoing_handler,
    }
});

type SerdeJsonObject = serde_json::Map<String, serde_json::Value>;

struct HttpSender;

fn render_liquid(template: &str, vars: &serde_json::Value) -> Result<String, liquid::Error> {
    let template = liquid::ParserBuilder::with_stdlib()
        .build()?
        .parse(template)?;

    let globals = liquid::to_object(vars)?;
    let output = template.render(&globals)?;

    Ok(output)
}

fn schema_as_str(scheme: &Scheme) -> &'static str {
    match scheme {
        Scheme::Http => "http",
        Scheme::Https => "https",
        Scheme::Other(_) => unimplemented!(),
    }
}

fn to_url_parameter_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.to_string(),
        serde_json::Value::Null => String::new(),
        serde_json::Value::Bool(bool) => bool.to_string(),
        serde_json::Value::Number(number) => number.to_string(),
        // js encodeURIComponent will convert array to values separated with comma
        // we will do the same
        serde_json::Value::Array(values) => values.iter().fold(String::new(), |mut acc, x| {
            if !acc.is_empty() {
                acc.push(',');
            }
            acc.push_str(&to_url_parameter_string(x));
            acc
        }),
        // js encodeURIComponent will print encoded [object Object], we will print it as json
        serde_json::Value::Object(map) => {
            serde_json::to_string(map).expect("we parsed this before")
        }
    }
}

fn generate_url(
    url: &str,
    scheme: &Scheme,
    query_params: &SerdeJsonObject,
) -> Result<String, String> {
    let mut url_parts = if url.starts_with("http://") || url.starts_with("https://") {
        Url::parse(url)
    } else {
        Url::parse(&format!("http://{url}"))
    }
    .map_err(|e| e.to_string())?;

    url_parts
        .set_scheme(schema_as_str(scheme))
        .map_err(|()| String::from("invalid scheme"))?;

    {
        let mut query_pairs = url_parts.query_pairs_mut();
        query_pairs.clear();

        for (key, value) in query_params {
            query_pairs.append_pair(key, &to_url_parameter_string(value));
        }
    }
    Ok(url_parts.to_string())
}

fn to_waki_method(method: &Method) -> waki::Method {
    match method {
        Method::Get => waki::Method::Get,
        Method::Head => waki::Method::Head,
        Method::Post => waki::Method::Post,
        Method::Put => waki::Method::Put,
        Method::Delete => waki::Method::Delete,
        Method::Connect => waki::Method::Connect,
        Method::Options => waki::Method::Options,
        Method::Trace => waki::Method::Trace,
        Method::Patch => waki::Method::Patch,
        Method::Other(other) => waki::Method::Other(other.clone()),
    }
}

impl http::Guest for HttpSender {
    fn http(input: Input) -> Result<Output, String> {
        let url_vars: serde_json::Value =
            serde_json::from_str(&input.url_parameters).map_err(|e| e.to_string())?;
        let body_vars: serde_json::Value =
            serde_json::from_str(&input.body_parameters).map_err(|e| e.to_string())?;
        let query_vars: SerdeJsonObject =
            serde_json::from_str(&input.query_parameters).map_err(|e| e.to_string())?;
        let headers: HashMap<String, String> =
            serde_json::from_str(&input.headers).map_err(|e| e.to_string())?;
        let headers: HeaderMap<String> = match (&headers).try_into() {
            Ok(headers) => headers,
            Err(e) => return Err(e.to_string()),
        };

        let url = render_liquid(&input.url, &url_vars).map_err(|e| e.to_string())?;
        let body = render_liquid(&input.body, &body_vars).map_err(|e| e.to_string())?;
        let url = generate_url(&url, &input.protocol, &query_vars)?;

        let client = Client::new();
        let method = to_waki_method(&input.method);
        let mut builder = client.request(method, &url);
        builder = builder.body(body);
        builder = builder.headers(headers.iter());

        let response = builder.send().map_err(|e| e.to_string())?;
        let response_code = response.status_code();
        let response_body = response.body().map_err(|e| e.to_string())?;
        let response_body = String::from_utf8(response_body).map_err(|e| e.to_string())?;

        // ensure that the 'as' field is always valid json
        let output_body: serde_json::Value = match serde_json::from_str(&response_body) {
            Ok(x) => x,
            Err(_) => serde_json::Value::String(response_body)
        };

        Ok(Output {
            response_code,
            as_: output_body.to_string(),
        })
    }
}

export! {HttpSender}

#[test]
fn render_liquid_works() {
    let vars = serde_json::json!({"num": 123});

    assert_eq!(
        render_liquid("Liquid! {{num | minus: 2}}", &vars).unwrap(),
        "Liquid! 121"
    );
}

#[test]
fn render_liquid_error_invalid_template() {
    let vars = serde_json::json!({"num": 123});
    let err = render_liquid("Liquid! {{num | minus: 2", &vars).unwrap_err();
    assert!(err.to_string().contains("expected Identifier"));
}

#[test]
fn render_liquid_error_missing_variable() {
    let vars = serde_json::json!({"num": 123});
    let err = render_liquid("Liquid! {{ testing }}", &vars).unwrap_err();
    assert!(err.to_string().contains("Unknown variable"));
}

#[test]
fn render_liquid_variables_not_a_map() {
    let vars = serde_json::json!(123);
    let err = render_liquid("Liquid! {{ testing }}", &vars).unwrap_err();
    assert!(err.to_string().contains("Object cannot be a scalar"));
}

#[test]
fn generate_url_applies_query_params() {
    let json = serde_json::json!({"test": 1, "query": "get"});
    let vars = json.as_object().unwrap();

    let url = generate_url("http://example.com", &Scheme::Https, &vars).unwrap();

    assert_eq!("https://example.com/?query=get&test=1", url);
}

#[test]
fn generate_url_applies_query_params_with_odd_values() {
    let json = serde_json::json!({"arr": [1,2,3,4], "obj": {"get": 1}});
    let vars = json.as_object().unwrap();

    let url = generate_url("http://example.com", &Scheme::Https, &vars).unwrap();

    assert_eq!(
        "https://example.com/?arr=1%2C2%2C3%2C4&obj=%7B%22get%22%3A1%7D",
        url
    );
}
