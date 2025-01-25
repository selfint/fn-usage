use anyhow::Result;
use lsp_types::Url;
use lsp_types::{
    notification::{DidOpenTextDocument, Initialized},
    request::{DocumentSymbolRequest, Initialize, References},
    DocumentSymbol, DocumentSymbolResponse, ServerCapabilities,
};
use serde_json::json;

use crate::{Client, StringIO};

impl<IO: StringIO> Client<IO> {
    pub fn open(&mut self, uri: Url, text: &str) -> Result<()> {
        self.notify::<DidOpenTextDocument>(
            serde_json::from_value(json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": "",
                    "version": 1,
                    "text": text
                }
            }))
            .unwrap(),
        )
    }

    pub fn get_references(&mut self, uri: Url, symbol: &DocumentSymbol) -> Result<Vec<Url>> {
        let references = self.request::<References>(
            serde_json::from_value(json!({
                "textDocument": {
                    "uri": uri,
                },
                "position": symbol.selection_range.start,
                "context": {
                    "includeDeclaration": false
                }
            }))
            .unwrap(),
        )?;

        let references = references
            .unwrap_or_default()
            .into_iter()
            .map(|f| f.uri)
            .collect();

        Ok(references)
    }

    pub fn get_symbols(&mut self, uri: Url) -> Result<Vec<DocumentSymbol>> {
        let symbols = self.request::<DocumentSymbolRequest>(
            serde_json::from_value(json!({
                "textDocument": {
                    "uri": uri
                },
            }))
            .unwrap(),
        )?;

        let symbols = match symbols {
            Some(DocumentSymbolResponse::Nested(vec)) => {
                let mut symbols = vec![];
                let mut queue = vec;

                // flatten nested document symbols
                while let Some(symbol) = queue.pop() {
                    symbols.push(symbol.clone());
                    if let Some(children) = symbol.children {
                        queue.extend(children);
                    }
                }

                symbols
            }
            Some(DocumentSymbolResponse::Flat(flat)) => {
                if flat.len() > 0 {
                    panic!("Got non-empty flat documentSymbol response")
                }

                vec![]
            }
            None => vec![],
        };

        Ok(symbols)
    }

    pub fn initialize(&mut self, uri: Url) -> Result<ServerCapabilities> {
        let response = self.request::<Initialize>(
            serde_json::from_value(json!({
                "capabilities": {
                    "textDocument": {
                        "documentSymbol": {
                            "hierarchicalDocumentSymbolSupport": true,
                        }
                    },
                },
                "workspaceFolders": [{
                    "uri": uri,
                    "name": "name"
                }]
            }))
            .unwrap(),
        )?;

        self.notify::<Initialized>(None)?;

        Ok(response.capabilities)
    }
}
