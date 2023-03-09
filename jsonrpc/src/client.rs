use anyhow::{Context, Result};
use serde::{de::DeserializeOwned, Serialize};
use tokio::sync::oneshot;

use crate::types::{Notification, Request, Response};
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

pub struct Client {
    client_tx: tokio::sync::mpsc::UnboundedSender<String>,
    pending_responses: Arc<Mutex<HashMap<i64, oneshot::Sender<Value>>>>,
    handle: tokio::task::JoinHandle<()>,
}

impl Drop for Client {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

impl Client {
    pub fn new(
        client_tx: tokio::sync::mpsc::UnboundedSender<String>,
        mut server_rx: tokio::sync::mpsc::UnboundedReceiver<String>,
    ) -> Self {
        let pending_responses = Arc::new(Mutex::new(HashMap::<i64, oneshot::Sender<Value>>::new()));

        let pending_responses_clone = pending_responses.clone();

        let handle = tokio::spawn(async move {
            while let Some(response) = server_rx.recv().await {
                let value = serde_json::from_str::<Value>(&response)
                    .expect("failed to deserialize response");

                let id = value
                    .as_object()
                    .expect("got non-object response")
                    .get("id")
                    .unwrap_or_else(|| panic!("got response without id: {:?}", value));

                let id = id
                    .as_i64()
                    .unwrap_or_else(|| panic!("got non i64 id: {:?}", id));

                pending_responses_clone
                    .lock()
                    .expect("failed to acquire lock")
                    .remove(&id)
                    .expect("no pending response matching server response")
                    .send(value)
                    .expect("failed to send response to pending response");
            }
        });

        Self {
            client_tx,
            pending_responses,
            handle,
        }
    }

    pub async fn request<P, R, E>(
        &self,
        request: Request<P>,
        add_header: bool,
    ) -> Result<Response<R, E>>
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

        let mut request_str =
            serde_json::to_string(&request).context("failed to serialize request")?;

        if add_header {
            let length = request_str.as_bytes().len();
            request_str = format!("Content-Length: {}\r\n\r\n{}", length, request_str);
        }

        self.client_tx
            .send(request_str)
            .context("failed to send request")?;

        let response = response_rx.await.context("failed to await response")?;
        serde_json::from_value::<Response<R, E>>(response).context("failed to parse response")
    }

    pub fn notify<P: Serialize>(
        &self,
        notification: Notification<P>,
        add_header: bool,
    ) -> Result<()> {
        let mut notification_str =
            serde_json::to_string(&notification).context("failed to serialize notification")?;

        if add_header {
            let length = notification_str.as_bytes().len();
            notification_str = format!("Content-Length: {}\r\n\r\n{}", length, notification_str);
        }

        self.client_tx
            .send(notification_str)
            .context("failed to send notification")
    }
}
