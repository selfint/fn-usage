use anyhow::Result;
use jsonrpc::{
    client::Client as JsonRpcClient,
    types::{Notification as JsonRpcNotification, Request as JsonRpcRequest, Response},
};
use lsp_types::{notification::Notification as LspNotification, request::Request as LspRequest};
use serde::de::DeserializeOwned;
use std::sync::atomic::{AtomicI64, Ordering::SeqCst};
use tokio::{
    sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
    task::JoinHandle,
};

pub struct Client {
    jsonrpc_client: JsonRpcClient,
    request_id: AtomicI64,
    handle: JoinHandle<()>,
}

impl Drop for Client {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

impl Client {
    pub fn new(client_tx: UnboundedSender<String>, server_rx: UnboundedReceiver<String>) -> Self {
        let (jsonrpc_client_tx, jsonrpc_client_rx) = unbounded_channel();
        let jsonrpc_client = JsonRpcClient::new(jsonrpc_client_tx, server_rx);

        let handle = tokio::spawn(Client::lsp_encode(jsonrpc_client_rx, client_tx));

        let request_id = AtomicI64::new(0);
        Self {
            jsonrpc_client,
            request_id,
            handle,
        }
    }

    async fn lsp_encode(mut rx: UnboundedReceiver<String>, tx: UnboundedSender<String>) {
        while let Some(msg) = rx.recv().await {
            let len = msg.as_bytes().len();
            let msg = format!("Content-Length: {}\r\n\r\n{}", len, msg);
            tx.send(msg).expect("failed to send message");
        }
    }

    pub async fn request<R, E>(&self, params: R::Params) -> Result<Response<R::Result, E>>
    where
        R: LspRequest,
        E: DeserializeOwned,
    {
        self.jsonrpc_client
            .request::<R::Params, R::Result, E>(JsonRpcRequest {
                jsonrpc: "2.0".to_string(),
                method: R::METHOD.to_string(),
                params: Some(params),
                id: self.request_id.fetch_add(1, SeqCst),
            })
            .await
    }

    pub fn notify<R>(&self, params: R::Params) -> Result<()>
    where
        R: LspNotification,
    {
        self.jsonrpc_client.notify::<_>(JsonRpcNotification {
            jsonrpc: "2.0".to_string(),
            method: R::METHOD.to_string(),
            params: Some(params),
        })
    }
}
