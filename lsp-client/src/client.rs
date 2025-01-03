use anyhow::{Context, Result};
use lsp_types::{notification::Notification as LspNotification, request::Request as LspRequest};
use serde::de::DeserializeOwned;

use crate::jsonrpc;

pub trait StringIO {
    fn send(&mut self, msg: &str) -> Result<()>;
    fn recv(&mut self) -> Result<String>;
}

#[derive(Debug)]
pub struct Error<D> {
    pub code: i64,
    pub message: String,
    pub data: D,
}

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

    pub fn request<R, E>(&mut self, params: R::Params) -> Result<Result<R::Result, Error<E>>>
    where
        R: LspRequest,
        E: DeserializeOwned,
    {
        let request = jsonrpc::Request {
            jsonrpc: "2.0".to_string(),
            method: R::METHOD.to_string(),
            params: Some(params),
            id: self.request_id_counter,
        };

        self.request_id_counter += 1;

        let msg = serde_json::to_string(&request).context("serializing request")?;

        self.io
            .send(&format!(
                "Content-Length: {}\r\n\r\n{}",
                msg.as_bytes().len(),
                msg
            ))
            .context("sending request")?;

        let response = self.io.recv().context("receiving response")?;

        let jsonrpc_response: jsonrpc::Response<R::Result, E> =
            serde_json::from_str(&response).context("deserializing response")?;

        Ok(match jsonrpc_response.result {
            jsonrpc::JsonRpcResult::Result(result) => Ok(result),
            jsonrpc::JsonRpcResult::Error {
                code,
                message,
                data,
            } => Err(Error {
                code,
                message,
                data,
            }),
        })
    }

    pub fn notify<R>(&mut self, params: R::Params) -> Result<()>
    where
        R: LspNotification,
    {
        let notification = jsonrpc::Notification {
            jsonrpc: "2.0".to_string(),
            method: R::METHOD.to_string(),
            params: Some(params),
        };

        let msg = serde_json::to_string(&notification).context("serializing notification")?;

        self.io
            .send(&format!(
                "Content-Length: {}\r\n\r\n{}",
                msg.as_bytes().len(),
                msg
            ))
            .context("sending notification")
    }
}
