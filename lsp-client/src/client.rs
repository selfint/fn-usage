use anyhow::{Context, Result};
use lsp_types::{notification::Notification as LspNotification, request::Request as LspRequest};
use serde::de::DeserializeOwned;
use std::sync::mpsc::{Receiver, Sender};

use crate::jsonrpc;

pub struct Client {
    tx: Sender<String>,
    rx: Receiver<String>,
    request_id_counter: i64,
}

impl Client {
    pub fn new(tx: Sender<String>, rx: Receiver<String>) -> Self {
        Self {
            tx,
            rx,
            request_id_counter: 0,
        }
    }

    fn lsp_encode(&self, msg: &str) -> String {
        let len = msg.as_bytes().len();
        format!("Content-Length: {}\r\n\r\n{}", len, msg)
    }

    pub fn request<R, E>(
        &mut self,
        params: R::Params,
    ) -> Result<jsonrpc::JsonRpcResult<R::Result, E>>
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

        self.tx
            .send(self.lsp_encode(&serde_json::to_string(&request).context("serializing request")?))
            .context("sending request")?;

        let response = self.rx.recv().context("receiving response")?;

        let jsonrpc_response: jsonrpc::Response<R::Result, E> =
            serde_json::from_str(&response).context("deserializing response")?;

        Ok(jsonrpc_response.result)
    }

    pub fn notify<R>(&self, params: R::Params) -> Result<()>
    where
        R: LspNotification,
    {
        let notification = jsonrpc::Notification {
            jsonrpc: "2.0".to_string(),
            method: R::METHOD.to_string(),
            params: Some(params),
        };

        self.tx
            .send(self.lsp_encode(
                &serde_json::to_string(&notification).context("serializing notification")?,
            ))
            .context("sending notification")
    }
}
