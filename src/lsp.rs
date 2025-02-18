use std::io::{BufRead, Write};

use anyhow::{Context, Result};
use lsp_types::{notification::Notification, request::Request};
use serde::Serialize;
use serde_json::Value;

use crate::jsonrpc::{self};

pub struct Client {
    input: Box<dyn BufRead>,
    output: Box<dyn Write>,
    request_id_counter: i64,
}

impl Client {
    pub fn new(input: Box<dyn BufRead>, output: Box<dyn Write>) -> Self {
        Self {
            input,
            output,
            request_id_counter: 0,
        }
    }

    pub fn notify<N: Notification>(&mut self, params: Option<N::Params>) -> Result<()> {
        let notification = jsonrpc::Notification {
            jsonrpc: "2.0".to_string(),
            method: N::METHOD.to_string(),
            params,
        };

        self.send(&notification)
    }

    pub fn request<R: Request>(&mut self, params: Option<R::Params>) -> Result<R::Result> {
        let request = jsonrpc::Request {
            jsonrpc: "2.0".to_string(),
            method: R::METHOD.to_string(),
            params,
            id: self.request_id_counter,
        };

        self.send(&request)?;

        let response: jsonrpc::Response<_> = loop {
            let response = self.recv()?;

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

        response.result.context("getting response result")
    }

    fn send(&mut self, msg: &impl Serialize) -> Result<()> {
        let msg = serde_json::to_string(msg)?;

        let length = msg.as_bytes().len();
        let msg = &format!("Content-Length: {}\r\n\r\n{}", length, msg);

        self.output
            .write_all(msg.as_bytes())
            .context("writing msg to output")
    }

    fn recv(&mut self) -> Result<Value> {
        let mut content_length = None;

        loop {
            let mut line = String::new();
            self.input.read_line(&mut line)?;

            let words: Vec<_> = line.split_ascii_whitespace().collect();

            match (words.as_slice(), &content_length) {
                (["Content-Length:", c_length], None) => content_length = Some(c_length.parse()?),
                (["Content-Type:", _], Some(_)) => {}
                ([], Some(content_length)) => {
                    let mut content = Vec::with_capacity(*content_length);

                    // make sure we don't seek past the current message
                    let mut bytes_left = *content_length;
                    while bytes_left > 0 {
                        let read_bytes = self.input.read_until(b'}', &mut content)?;
                        bytes_left -= read_bytes;
                    }

                    return serde_json::from_slice(&content).context("deserializing response");
                }
                unexpected => panic!("Got unexpected stdout: {:?}", unexpected),
            };
        }
    }
}
