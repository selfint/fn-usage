use anyhow::anyhow;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize, Deserialize, Debug)]
pub struct Request<Params> {
    pub jsonrpc: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Params>,
    pub id: i64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Notification<Params> {
    pub jsonrpc: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Params>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Response<T> {
    pub jsonrpc: String,
    #[serde(flatten)]
    pub result: JsonRpcResult<T>,
    pub id: Option<i64>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
pub enum JsonRpcResult<T> {
    Result(T),
    Error {
        code: i64,
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        data: Option<Value>,
    },
}

impl<T> From<JsonRpcResult<T>> for anyhow::Result<T> {
    fn from(value: JsonRpcResult<T>) -> Self {
        match value {
            JsonRpcResult::Result(result) => Ok(result),
            JsonRpcResult::Error {
                code,
                message,
                data,
            } => Err(anyhow!("Error ({}) {}: {:?}", code, message, data)),
        }
    }
}
