use anyhow::Result;
use lsp_types::{
    notification::{DidOpenTextDocument, Initialized},
    request::{DocumentSymbolRequest, Initialize, References},
    DocumentSymbol, DocumentSymbolResponse, ServerCapabilities, Url,
};
use serde_json::json;

use crate::{StringIO, LSP};

pub struct Client<IO: StringIO> {
    lsp: LSP<IO>,
}

impl<IO: StringIO> Client<IO> {
    pub fn new(io: IO) -> Self {
        Self {
            lsp: LSP::new(io, false),
        }
    }

    pub fn open(&mut self, uri: &Url, text: &str) -> Result<()> {
        self.lsp
            .notify::<DidOpenTextDocument>(serde_json::from_value(json!({
                "textDocument": {
                "uri": uri,
                "languageId": "",
                "version": 1,
                "text": text,
                }
            }))?)?;

        Ok(())
    }

    pub fn get_references(&mut self, uri: &Url, symbol: &DocumentSymbol) -> Result<Vec<Url>> {
        let references = self
            .lsp
            .request::<References>(serde_json::from_value(json!({
                "textDocument": {
                    "uri": uri.clone(),
                },
                "position": {
                    "line": symbol.selection_range.start.line,
                    "character": symbol.selection_range.start.character,
                },
                "context": {
                    "includeDeclaration": false,
                },
            }))?)?;

        Ok(references
            .unwrap_or_default()
            .into_iter()
            .map(|f| f.uri)
            .collect())
    }

    pub fn get_symbols(&mut self, uri: &Url) -> Result<Vec<DocumentSymbol>> {
        let symbols = self
            .lsp
            .request::<DocumentSymbolRequest>(serde_json::from_value(json!({
                "textDocument": {
                    "uri": uri,
                },
            }))?)?;

        let symbols = match symbols {
            Some(DocumentSymbolResponse::Nested(vec)) => {
                let mut symbols = vec![];
                let mut queue = vec;

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

    pub fn initialize(&mut self, uri: &Url) -> Result<ServerCapabilities> {
        let response = self
            .lsp
            .request::<Initialize>(serde_json::from_value(json!({
                "capabilities": {
                    "textDocument": {
                        "documentSymbol": {
                            "hierarchicalDocumentSymbolSupport": true,
                        },
                    }
                },
                "workspaceFolders": [{
                    "uri": uri,
                    "name": "name"
                }]
            }))?)?;

        self.lsp.notify::<Initialized>(None)?;

        Ok(response.capabilities)
    }
}
