use anyhow::{Context, Result};
use lsp_types::{notification::Notification as LspNotification, request::Request as LspRequest};
use serde_json::Value;

use crate::jsonrpc;

pub trait StringIO {
    fn send(&mut self, msg: &str) -> Result<()>;
    fn recv(&mut self) -> Result<String>;
}

#[derive(Debug)]
pub struct Error {
    code: i64,
    message: String,
    data: serde_json::Value,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Error {} - {}: {}", self.code, self.message, self.data)
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

    pub fn request<R>(&mut self, params: Option<R::Params>) -> Result<R::Result>
    where
        R: LspRequest,
    {
        let request = jsonrpc::Request {
            jsonrpc: "2.0".to_string(),
            method: R::METHOD.to_string(),
            params,
            id: self.request_id_counter,
        };

        let msg = serde_json::to_string(&request).context("serializing request")?;

        self.io
            .send(&format!(
                "Content-Length: {}\r\n\r\n{}",
                msg.as_bytes().len(),
                msg
            ))
            .context("sending request")?;

        eprintln!("\t\tSent: {}", msg);

        let response = loop {
            let response = self.io.recv().context("receiving response")?;

            eprintln!("\t\tReceived: {}", response);

            let json_value: Value =
                serde_json::from_str(&response).context("deserializing response")?;

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

        let jsonrpc_response: jsonrpc::Response<R::Result, serde_json::Value> =
            serde_json::from_str(&response).context("deserializing response")?;

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

    pub fn notify<R>(&mut self, params: Option<R::Params>) -> Result<()>
    where
        R: LspNotification,
    {
        let notification = jsonrpc::Notification {
            jsonrpc: "2.0".to_string(),
            method: R::METHOD.to_string(),
            params,
        };

        let msg = serde_json::to_string(&notification).context("serializing notification")?;

        self.io
            .send(&format!(
                "Content-Length: {}\r\n\r\n{}",
                msg.as_bytes().len(),
                msg
            ))
            .context("sending notification")?;

        eprintln!("\t\tSent: {}", msg);

        Ok(())
    }
}
