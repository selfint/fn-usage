use anyhow::Result;
use lsp_types::{notification::Notification as LspNotification, request::Request as LspRequest};
use serde_json::Value;

use crate::jsonrpc;

pub trait StringIO {
    fn send(&mut self, msg: &str) -> Result<()>;
    fn recv(&mut self) -> Result<String>;
}

#[derive(Debug)]
pub struct Error {
    pub code: i64,
    pub message: String,
    pub data: Option<Value>,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Error {} {}: {}",
            self.code,
            self.message,
            self.data.as_ref().unwrap_or(&serde_json::json!(null))
        )
    }
}

impl std::error::Error for Error {}

pub struct Client<IO: StringIO> {
    io: IO,
    request_id_counter: i64,
}

impl<IO: StringIO> Client<IO> {
    pub fn new(io: IO) -> Self {
        Self {
            io,
            request_id_counter: 0,
        }
    }

    pub fn request<R: LspRequest>(&mut self, params: Option<R::Params>) -> Result<R::Result> {
        let msg = serde_json::to_string(&jsonrpc::Request {
            jsonrpc: "2.0".to_string(),
            method: R::METHOD.to_string(),
            params,
            id: self.request_id_counter,
        })?;

        self.io.send(&format!(
            "Content-Length: {}\r\n\r\n{}",
            msg.as_bytes().len(),
            msg
        ))?;

        let response = loop {
            let response = self.io.recv()?;

            let json_value: Value = serde_json::from_str(&response)?;

            // check if this is our response
            if let Some(id) = json_value.get("id").and_then(Value::as_i64) {
                if id == self.request_id_counter {
                    // this is a server sent method - not our response
                    if json_value.get("method").is_some() {
                        continue;
                    }

                    break response;
                }
            }
        };

        self.request_id_counter += 1;

        let jsonrpc_response: jsonrpc::Response<_, Value> = serde_json::from_str(&response)?;

        match jsonrpc_response.result {
            jsonrpc::JsonRpcResult::Result(result) => Ok(result),
            jsonrpc::JsonRpcResult::Error {
                code,
                message,
                data,
            } => Err((Error {
                code,
                message,
                data,
            })
            .into()),
        }
    }

    pub fn notify<N: LspNotification>(&mut self, params: Option<N::Params>) -> Result<()> {
        let msg = serde_json::to_string(&jsonrpc::Notification {
            jsonrpc: "2.0".to_string(),
            method: N::METHOD.to_string(),
            params,
        })?;

        self.io.send(&format!(
            "Content-Length: {}\r\n\r\n{}",
            msg.as_bytes().len(),
            msg
        ))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use anyhow::{anyhow, Result};
    use lsp_types::{
        notification::DidOpenTextDocument,
        request::{Initialize, References},
        InitializeError, InitializeResult, Location,
    };
    use serde::Serialize;
    use serde_json::json;

    use crate::{jsonrpc, lsp};

    use super::Client;

    #[derive(Debug, Serialize)]
    struct TestIO<'a> {
        sent: &'a mut Vec<String>,
        received: VecDeque<String>,
    }

    impl<'a> TestIO<'a> {
        fn new(sent: &'a mut Vec<String>, received: impl Into<VecDeque<String>>) -> Self {
            Self {
                sent,
                received: received.into(),
            }
        }
    }

    impl<'a> lsp::StringIO for TestIO<'a> {
        fn send(&mut self, msg: &str) -> Result<()> {
            self.sent.push(msg.to_string());

            Ok(())
        }

        fn recv(&mut self) -> Result<String> {
            self.received
                .pop_front()
                .ok_or(anyhow!("End of received queue"))
        }
    }

    #[test]
    fn test_initialize_request() {
        let mut sent = vec![];
        let mut client = Client::new(TestIO::new(
            &mut sent,
            [serde_json::to_string(&jsonrpc::Response {
                jsonrpc: "2.0".to_string(),
                result: jsonrpc::JsonRpcResult::<InitializeResult, InitializeError>::Result(
                    InitializeResult::default(),
                ),
                id: Some(0),
            })
            .unwrap()],
        ));

        let response = client.request::<Initialize>(
            serde_json::from_value(json!({
                "capabilities": {
                    "textDocument": {
                        "documentSymbol": {
                            "hierarchical_document_symbol_support": true
                        }
                    }
                }
            }))
            .unwrap(),
        );

        assert!(response.is_ok());
        assert!(sent.len() == 1);
        insta::assert_snapshot!(
            sent[0],
            @r#"
        Content-Length: 143

        {"jsonrpc":"2.0","method":"initialize","params":{"processId":null,"rootUri":null,"capabilities":{"textDocument":{"documentSymbol":{}}}},"id":0}
        "#
        );
    }

    #[test]
    fn test_open() {
        let mut sent = vec![];
        let mut client = Client::new(TestIO::new(&mut sent, []));

        let response = client.notify::<DidOpenTextDocument>(
            serde_json::from_value(json!({
                "textDocument": {
                    "uri": "file:///",
                    "languageId": "",
                    "version": 1,
                    "text": ""
                }
            }))
            .unwrap(),
        );

        assert!(response.is_ok());
        assert!(sent.len() == 1);
        insta::assert_snapshot!(
            sent[0],
            @r#"
        Content-Length: 132

        {"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///","languageId":"","version":1,"text":""}}}
        "#
        );
    }

    #[test]
    fn test_get_references() {
        let mut sent = vec![];
        let mut client = Client::new(TestIO::new(
            &mut sent,
            [serde_json::to_string(&jsonrpc::Response {
                jsonrpc: "2.0".to_string(),
                result: jsonrpc::JsonRpcResult::<Option<Vec<Location>>, ()>::Result(Some(vec![])),
                id: Some(0),
            })
            .unwrap()],
        ));

        let response = client.request::<References>(
            serde_json::from_value(json!({
                "textDocument": {
                    "uri": "file:///",
                },
                "position": {
                    "line": 0,
                    "character": 0
                },
                "context": {
                    "includeDeclaration": false
                }
            }))
            .unwrap(),
        );

        assert!(response.is_ok());
        assert!(sent.len() == 1);
        insta::assert_snapshot!(
            sent[0],
            @r#"
        Content-Length: 179

        {"jsonrpc":"2.0","method":"textDocument/references","params":{"textDocument":{"uri":"file:///"},"position":{"line":0,"character":0},"context":{"includeDeclaration":false}},"id":0}
        "#
        );
    }

    #[test]
    fn test_get_symbols() {
        let mut sent = vec![];
        let mut client = Client::new(TestIO::new(
            &mut sent,
            [serde_json::to_string(&jsonrpc::Response {
                jsonrpc: "2.0".to_string(),
                result: jsonrpc::JsonRpcResult::<Option<Vec<Location>>, ()>::Result(Some(vec![])),
                id: Some(0),
            })
            .unwrap()],
        ));

        let response = client.request::<References>(
            serde_json::from_value(json!({
                "textDocument": {
                    "uri": "file:///",
                },
                "position": {
                    "line": 0,
                    "character": 0
                },
                "context": {
                    "includeDeclaration": false
                }
            }))
            .unwrap(),
        );

        assert!(response.is_ok());
        assert!(sent.len() == 1);
        insta::assert_snapshot!(
            sent[0],
            @r#"
        Content-Length: 179

        {"jsonrpc":"2.0","method":"textDocument/references","params":{"textDocument":{"uri":"file:///"},"position":{"line":0,"character":0},"context":{"includeDeclaration":false}},"id":0}
        "#
        );
    }
}
