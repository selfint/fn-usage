use anyhow::Result;
use jsonrpc::{
    client::Client as JsonRpcClient,
    types::{Notification as JsonRpcNotification, Request as JsonRpcRequest, Response},
};
use lsp_types::{notification::Notification as LspNotification, request::Request as LspRequest};
use serde::de::DeserializeOwned;
use std::sync::atomic::{AtomicI64, Ordering::SeqCst};

pub struct Client {
    jsonrpc_client: JsonRpcClient,
    request_id: AtomicI64,
}

impl Client {
    pub fn new(
        client_tx: tokio::sync::mpsc::UnboundedSender<String>,
        server_rx: tokio::sync::mpsc::UnboundedReceiver<String>,
    ) -> Self {
        let jsonrpc_client = JsonRpcClient::new(client_tx, server_rx);
        let request_id = AtomicI64::new(0);
        Self {
            jsonrpc_client,
            request_id,
        }
    }

    pub async fn request<R, E>(&self, params: R::Params) -> Result<Response<R::Result, E>>
    where
        R: LspRequest,
        E: DeserializeOwned,
    {
        self.jsonrpc_client
            .request::<R::Params, R::Result, E>(
                JsonRpcRequest {
                    jsonrpc: "2.0".to_string(),
                    method: R::METHOD.to_string(),
                    params: Some(params),
                    id: self.request_id.fetch_add(1, SeqCst),
                },
                true,
            )
            .await
    }

    pub fn notify<R>(&self, params: R::Params) -> Result<()>
    where
        R: LspNotification,
    {
        self.jsonrpc_client.notify::<_>(
            JsonRpcNotification {
                jsonrpc: "2.0".to_string(),
                method: R::METHOD.to_string(),
                params: Some(params),
            },
            true,
        )
    }
}
