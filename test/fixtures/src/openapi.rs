use jsonschema::Registry;
use serde_json::Value;
use serde_json::json;
use std::sync::LazyLock;

static OPENAPI: LazyLock<Value> =
    LazyLock::new(|| serde_saphyr::from_str(include_str!("../../../openapi.yaml")).unwrap());
const OPENAPI_URI: &str = "urn:selfless:openapi";
pub(crate) const PROTOBUF: &str = "application/x-protobuf";

pub(crate) fn check_response(
    method: &str,
    path: &str,
    status: u16,
    content_type: Option<&str>,
    text: &str,
) -> Value {
    let pointer = format!(
        "/paths/{}/{}/responses/{status}",
        escape(path),
        method.to_lowercase()
    );
    let Some(response) = OPENAPI.pointer(&pointer) else {
        panic!("openapi.yaml has nothing at {pointer}");
    };
    let pointer = response["$ref"]
        .as_str()
        .map_or(pointer, |x| x.trim_start_matches('#').to_owned());
    let Some(content_type) = content_type.filter(|_| !text.is_empty()) else {
        assert!(
            OPENAPI.pointer(&format!("{pointer}/content")).is_none(),
            "{method} {path} {status} has no body, but openapi.yaml describes one"
        );
        return Value::Null;
    };
    let body = serde_json::from_str::<Value>(text).unwrap();
    let schema = format!("{pointer}/content/{}/schema", escape(content_type));
    assert!(
        OPENAPI.pointer(&schema).is_some(),
        "openapi.yaml has no schema at {schema}"
    );
    let registry = Registry::new()
        .add(OPENAPI_URI, OPENAPI.clone())
        .unwrap()
        .prepare()
        .unwrap();
    let validator = jsonschema::options()
        .with_registry(&registry)
        .build(&json!({ "$ref": format!("{OPENAPI_URI}#{schema}") }))
        .unwrap();
    let errors = validator
        .iter_errors(&body)
        .map(|x| x.to_string())
        .collect::<Vec<_>>();
    assert!(
        errors.is_empty(),
        "{method} {path} {status} does not match openapi.yaml: {errors:?}\n{body}"
    );
    body
}

pub(crate) fn check_protobuf(method: &str, path: &str, status: u16, content_type: Option<&str>) {
    let operation = format!("/paths/{}/{}", escape(path), method.to_lowercase());
    let media_type = escape(PROTOBUF);
    for pointer in [
        format!("{operation}/requestBody/content/{media_type}"),
        format!("{operation}/responses/{status}/content/{media_type}"),
    ] {
        assert!(
            OPENAPI.pointer(&pointer).is_some(),
            "openapi.yaml has nothing at {pointer}"
        );
    }
    assert_eq!(
        content_type,
        Some(PROTOBUF),
        "{method} {path} {status} is not {PROTOBUF}"
    );
}

fn escape(token: &str) -> String {
    token.replace('~', "~0").replace('/', "~1")
}
