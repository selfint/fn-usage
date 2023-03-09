use crate::types::{Notification, Request, Response};
use anyhow::{Context, Result};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
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

        Self {
            client_tx,
            pending_responses,
            response_resolver_handle,
        }
    }

    fn handle_response(
        response: String,
        pending_responses: &Mutex<HashMap<i64, oneshot::Sender<Value>>>,
    ) -> Result<()> {
        let value =
            serde_json::from_str::<Value>(&response).expect("failed to deserialize response");

        let id = value
            .as_object()
            .context("got non-object response")?
            .get("id")
            .context(format!("got response without id: {:?}", value))?;

        let id = id.as_i64().context(format!("got non i64 id: {:?}", id))?;

        pending_responses
            .lock()
            .expect("failed to acquire lock")
            .remove(&id)
            .context("no pending response matching server response")?
            .send(value)
            .map_err(anyhow::Error::msg)
            .context("failed to send response")
    }

    pub async fn request<P, R, E>(&self, request: Request<P>) -> Result<Response<R, E>>
    where
        P: Serialize,
        R: DeserializeOwned,
        E: DeserializeOwned,
    {
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
        serde_json::from_value::<Response<R, E>>(response).context("failed to parse response")
    }

    pub fn notify<P: Serialize>(&self, notification: Notification<P>) -> Result<()> {
        let notification_str =
            serde_json::to_string(&notification).context("failed to serialize notification")?;

        self.client_tx
            .send(notification_str)
            .context("failed to send notification")
    }
}
