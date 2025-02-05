use anyhow::Result;
use lsp_types::{notification::Notification, request::Request};
use serde_json::Value;

use crate::jsonrpc;

pub trait StringIO {
    fn send(&mut self, msg: &str) -> Result<()>;
    fn recv(&mut self) -> Result<String>;
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

    pub fn notify<N: Notification>(&mut self, params: Option<N::Params>) -> Result<()> {
        let notification = jsonrpc::Notification {
            jsonrpc: "2.0".to_string(),
            method: N::METHOD.to_string(),
            params,
        };

        self.io.send(&serde_json::to_string(&notification)?)
    }

    pub fn request<R: Request>(&mut self, params: Option<R::Params>) -> Result<R::Result> {
        let request = jsonrpc::Request {
            jsonrpc: "2.0".to_string(),
            method: R::METHOD.to_string(),
            params,
            id: self.request_id_counter,
        };

        self.io.send(&serde_json::to_string(&request)?)?;

        let response: jsonrpc::Response<_> = loop {
            let response: Value = serde_json::from_str(&self.io.recv()?)?;

            // check if this is our response
            if response.get("method").is_none()
                && response
                    .get("id")
                    .is_some_and(|id| id.as_i64() == Some(self.request_id_counter))
            {
                break serde_json::from_value(response)?;
            }
        };

        self.request_id_counter += 1;

        response.result.into()
    }
}
