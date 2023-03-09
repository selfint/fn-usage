use jsonrpc::types::*;

#[test]
fn test_request_serialization() {
    insta::assert_compact_json_snapshot!(
        Request {
            jsonrpc: "2.0".to_string(),
            method: "method".to_string(),
            params: Some(vec![42, 23]),
            id: 1,
        },
        @r###"{"jsonrpc": "2.0", "method": "method", "params": [42, 23], "id": 1}"###
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
            result: JsonRpcResult::<_, ()>::Result(19),
            id: Some(1),
        },
        @r###"{"jsonrpc": "2.0", "result": 19, "id": 1}"###
    );

    insta::assert_compact_json_snapshot!(
        Response {
            jsonrpc: "2.0".to_string(),
            result: JsonRpcResult::<(), _>::Error {
                code: -32601,
                message: "Method not found".to_string(),
                data: Some(vec!["Some", "data"])
            },
            id: None,
        },
        @r###"{"jsonrpc": "2.0", "error": {"code": -32601, "message": "Method not found", "data": ["Some", "data"]}, "id": null}"###
    );
}

#[test]
fn test_response_deserialization() {
    insta::assert_debug_snapshot!(
        serde_json::from_str::<Response<i32, ()>>(r#"{"jsonrpc": "2.0", "result": 19, "id": 1}"#),
        @r###"
        Ok(
            Response {
                jsonrpc: "2.0",
                result: Result(
                    19,
                ),
                id: Some(
                    1,
                ),
            },
        )
        "###
    );

    insta::assert_debug_snapshot!(
        serde_json::from_str::<Response<(), Vec<String>>>(r#"{"jsonrpc": "2.0", "error": {"code": -32601, "message": "Method not found", "data": ["Some", "data"]}, "id": null}"#),
        @r###"
        Ok(
            Response {
                jsonrpc: "2.0",
                result: Error {
                    code: -32601,
                    message: "Method not found",
                    data: Some(
                        [
                            "Some",
                            "data",
                        ],
                    ),
                },
                id: None,
            },
        )
        "###
    );
}
