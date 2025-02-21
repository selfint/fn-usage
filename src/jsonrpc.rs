use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

#[derive(Serialize, Deserialize, Debug)]
pub struct Request<Params> {
    pub jsonrpc: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Params>,
    pub id: i64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Notification<Params> {
    pub jsonrpc: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Params>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Response<T: Serialize + DeserializeOwned> {
    pub jsonrpc: String,
    #[serde(flatten)]
    #[serde(with = "JsonRpcResult")]
    pub result: Result<T, Error>,
    pub id: Option<i64>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Error {
    code: i64,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

type Remote<T> = Result<T, Error>;

#[derive(Serialize, Deserialize, Debug)]
#[serde(remote = "Remote")]
enum JsonRpcResult<T> {
    #[serde(rename = "result")]
    Ok(T),
    #[serde(rename = "error")]
    Err(Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Error {}: {}", self.code, self.message)
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn test_request_serialization() {
        insta::assert_compact_json_snapshot!(
            Request {
                jsonrpc: "2.0".to_string(),
                method: "subtract".to_string(),
                params: Some(vec![42, 23]),
                id: 1,
            },
            @r#"{"jsonrpc": "2.0", "method": "subtract", "params": [42, 23], "id": 1}"#
        );

        // a but awkward, as Some(()) should probably just no be serialized
        // implementing this requires a lot of ugly code, so won't be implemented
        // unless it is a problem
        insta::assert_compact_json_snapshot!(
            Request {
                jsonrpc: "2.0".to_string(),
                method: "method".to_string(),
                params: Some(()),
                id: 1,
            },
            @r###"{"jsonrpc": "2.0", "method": "method", "params": null, "id": 1}"###
        );

        insta::assert_compact_json_snapshot!(
            Request::<Option<()>> {
                jsonrpc: "2.0".to_string(),
                method: "method".to_string(),
                params: None,
                id: 1,
            },
            @r#"{"jsonrpc": "2.0", "method": "method", "id": 1}"#
        );
    }

    #[test]
    fn test_request_deserialization() {
        insta::assert_debug_snapshot!(
            serde_json::from_str::<Request<Vec<i32>>>(r#"{"jsonrpc": "2.0", "method": "method", "params": [42, 23], "id": 1}"#),
            @r###"
        Ok(
            Request {
                jsonrpc: "2.0",
                method: "method",
                params: Some(
                    [
                        42,
                        23,
                    ],
                ),
                id: 1,
            },
        )
        "###
        );
    }

    #[test]
    fn test_notification_serialization() {
        insta::assert_compact_json_snapshot!(
            Notification {
                jsonrpc: "2.0".to_string(),
                method: "method".to_string(),
                params: Some(vec![42, 23]),
            },
            @r###"{"jsonrpc": "2.0", "method": "method", "params": [42, 23]}"###
        );
    }

    #[test]
    fn test_notification_deserialization() {
        insta::assert_debug_snapshot!(
            serde_json::from_str::<Notification<Vec<i32>>>(r#"{"jsonrpc": "2.0", "method": "method", "params": [42, 23]}"#),
            @r###"
        Ok(
            Notification {
                jsonrpc: "2.0",
                method: "method",
                params: Some(
                    [
                        42,
                        23,
                    ],
                ),
            },
        )
        "###
        );
    }

    #[test]
    fn test_response_serialization() {
        insta::assert_compact_json_snapshot!(
            Response {
                jsonrpc: "2.0".to_string(),
                result: Ok(19),
                id: Some(1),
            },
            @r###"{"jsonrpc": "2.0", "result": 19, "id": 1}"###
        );

        insta::assert_compact_json_snapshot!(
            Response::<()> {
                jsonrpc: "2.0".to_string(),
                result: Err(Error {
                    code: -32601,
                    message: "Method not found".to_string(),
                    data: Some(json!(["Some", "data"]))
                }),
                id: None,
            },
            @r###"{"jsonrpc": "2.0", "error": {"code": -32601, "message": "Method not found", "data": ["Some", "data"]}, "id": null}"###
        );
    }

    #[test]
    fn test_response_deserialization() {
        insta::assert_debug_snapshot!(
            serde_json::from_str::<Response<i32>>(r#"{"jsonrpc": "2.0", "result": 19, "id": 1}"#),
            @r#"
        Ok(
            Response {
                jsonrpc: "2.0",
                result: Ok(
                    19,
                ),
                id: Some(
                    1,
                ),
            },
        )
        "#
        );

        insta::assert_debug_snapshot!(
            serde_json::from_str::<Response<()>>(r#"{"jsonrpc": "2.0", "error": {"code": -32601, "message": "Method not found", "data": ["Some", "data"]}, "id": null}"#),
            @r#"
        Ok(
            Response {
                jsonrpc: "2.0",
                result: Err(
                    Error {
                        code: -32601,
                        message: "Method not found",
                        data: Some(
                            Array [
                                String("Some"),
                                String("data"),
                            ],
                        ),
                    },
                ),
                id: None,
            },
        )
        "#
        );
    }
}
