use crate::types::{Notification, Request, Response};
use anyhow::{Context, Result};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicI64, Ordering::Relaxed},
        Arc, Mutex,
    },
};
use tokio::{
    sync::{
        mpsc::{UnboundedReceiver, UnboundedSender},
        oneshot,
    },
    task::JoinHandle,
};

pub struct Client {
    client_tx: UnboundedSender<String>,
    pending_responses: Arc<Mutex<HashMap<i64, oneshot::Sender<Value>>>>,
    response_resolver_handle: JoinHandle<()>,
    request_id_counter: AtomicI64,
}

impl Drop for Client {
    fn drop(&mut self) {
        self.response_resolver_handle.abort();
    }
}

impl Client {
    pub fn new(
        client_tx: UnboundedSender<String>,
        mut server_rx: UnboundedReceiver<String>,
    ) -> Self {
        let pending_responses = Arc::new(Mutex::new(HashMap::<i64, oneshot::Sender<_>>::new()));
        let pending_responses_clone = Arc::clone(&pending_responses);

        let response_resolver_handle = tokio::spawn(async move {
            while let Some(response) = server_rx.recv().await {
                Client::handle_response(response, &pending_responses_clone)
                    .expect("failed to handle response");
            }
        });

        let request_id_counter = AtomicI64::new(0);

        Self {
            client_tx,
            pending_responses,
            response_resolver_handle,
            request_id_counter,
        }
    }

    fn handle_response(
        response: String,
        pending_responses: &Mutex<HashMap<i64, oneshot::Sender<Value>>>,
    ) -> Result<()> {
        let value = serde_json::from_str::<Value>(&response)
            .context(format!("failed to deserialize response: {:?}", response))?;

        let id = value
            .as_object()
            .context(format!("got non-object response: {:?}", value))?
            .get("id")
            .context(format!("got response without id: {:?}", value))?;

        let id = id.as_i64().context(format!("got non-i64 id: {:?}", id))?;

        pending_responses
            .lock()
            .expect("failed to acquire lock")
            .remove(&id)
            .context(format!("response id has no pending response: {:?}", id))?
            .send(value)
            .map_err(anyhow::Error::msg)
            .context("failed to send response")
    }

    pub async fn request<P: Serialize, R: DeserializeOwned, E: DeserializeOwned>(
        &self,
        method: String,
        params: Option<P>,
    ) -> Result<Response<R, E>> {
        let request = Request {
            jsonrpc: "2.0".to_string(),
            method,
            params,
            id: self.request_id_counter.fetch_add(1, Relaxed),
        };

        let (response_tx, response_rx) = oneshot::channel();

        drop(
            self.pending_responses
                .lock()
                .unwrap()
                .insert(request.id, response_tx),
        );

        let request_str = serde_json::to_string(&request).context("failed to serialize request")?;

        self.client_tx
            .send(request_str)
            .context("failed to send request")?;

        let response = response_rx.await.context("failed to await response")?;
        serde_json::from_value(response).context("failed to parse response")
    }

    pub fn notify<P: Serialize>(&self, method: String, params: Option<P>) -> Result<()> {
        let notification = Notification {
            jsonrpc: "2.0".to_string(),
            method,
            params,
        };

        let notification_str =
            serde_json::to_string(&notification).context("failed to serialize notification")?;

        self.client_tx
            .send(notification_str)
            .context("failed to send notification")
    }
}
