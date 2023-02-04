use serde::{Deserialize, Serialize};

use crate::JSONRPC_VERSION;

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Debug, Clone)]
pub struct Request<P> {
    jsonrpc: String,
    pub method: String,
    pub params: P,
    pub id: Option<u64>,
}

impl<P> Request<P> {
    pub fn new(method: String, params: P, id: Option<u64>) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            method,
            params,
            id,
        }
    }
}
