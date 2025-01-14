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
    pub data: Option<serde_json::Value>,
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

pub struct LSP<IO: StringIO> {
    io: IO,
    request_id_counter: i64,
    verbose: bool,
}

impl<IO: StringIO> LSP<IO> {
    pub fn new(io: IO, verbose: bool) -> Self {
        Self {
            io,
            request_id_counter: 0,
            verbose,
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

        if self.verbose {
            eprintln!("\t\tSent: {}", msg);
        }

        let response = loop {
            let response = self.io.recv()?;

            if self.verbose {
                eprintln!("\t\tReceived: {}", response);
            }

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

        let jsonrpc_response: jsonrpc::Response<_, serde_json::Value> =
            serde_json::from_str(&response)?;

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

        if self.verbose {
            eprintln!("\t\tSent: {}", msg);
        }

        Ok(())
    }
}
