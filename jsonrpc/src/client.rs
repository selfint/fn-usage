use crate::types::{Notification, Request, Response};
use anyhow::{Context, Result};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use std::sync::mpsc;
use tokio::sync::watch;

pub struct Client {
    client_tx: mpsc::Sender<String>,
    server_rx: watch::Receiver<String>,
}

impl Client {
    pub fn new(client_tx: mpsc::Sender<String>, server_rx: watch::Receiver<String>) -> Self {
        Self {
            client_tx,
            server_rx,
        }
    }

    pub async fn request<P, R, E>(&self, request: Request<P>) -> Result<Response<R, E>>
    where
        P: Serialize,
        R: DeserializeOwned,
        E: DeserializeOwned,
    {
        self.client_tx
            .send(serde_json::to_string(&request).context("failed to serialize request")?)
            .context("failed to send request")?;

        let mut server_rx = self.server_rx.clone();
        let response = loop {
            server_rx.changed().await?;

            let msg = self.server_rx.borrow();
            match serde_json::from_str::<Value>(&msg)
                .context("failed to parse response as json")?
                .as_object()
                .and_then(|o| o.get("id"))
                .and_then(|id| id.as_i64())
            {
                Some(id) if id == request.id => {
                    break serde_json::from_value::<Response<R, E>>(
                        serde_json::from_str::<Value>(&msg)
                            .context("failed to parse response as json")?,
                    )
                    .context("failed to parse response as specific type")?;
                }
                _ => {}
            };
        };

        Ok(response)
    }

    pub fn notify<P: Serialize>(&self, notification: Notification<P>) -> Result<()> {
        self.client_tx
            .send(serde_json::to_string(&notification).context("failed to serialize notification")?)
            .context("failed to send notification")
    }
}
