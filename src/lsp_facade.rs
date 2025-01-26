use anyhow::Result;
use lsp_types::{
    notification::{DidOpenTextDocument, Initialized},
    request::{DocumentSymbolRequest, GotoDefinition, Initialize, References},
    DocumentSymbol, DocumentSymbolResponse, GotoDefinitionResponse, ServerCapabilities, SymbolKind,
    Url,
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

    pub fn references(&mut self, uri: Url, symbol: &DocumentSymbol) -> Result<Vec<Url>> {
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

        Ok(references
            .unwrap_or_default()
            .into_iter()
            .map(|r| r.uri)
            .collect())
    }

    pub fn goto_definition(&mut self, uri: Url, symbol: &DocumentSymbol) -> Result<Vec<Url>> {
        let definition = self.request::<GotoDefinition>(
            serde_json::from_value(json!({
            "textDocument": {
                "uri": uri,
            },
            "position": symbol.selection_range.start,
            }))
            .unwrap(),
        )?;

        let definition = if let Some(definition) = definition {
            match definition {
                GotoDefinitionResponse::Scalar(location) => vec![location.uri],
                GotoDefinitionResponse::Array(vec) => vec.into_iter().map(|l| l.uri).collect(),
                GotoDefinitionResponse::Link(vec) => {
                    vec.into_iter().map(|l| l.target_uri).collect()
                }
            }
        } else {
            vec![]
        };

        Ok(definition)
    }

    pub fn symbols(&mut self, uri: Url, mask: &[SymbolKind]) -> Result<Vec<DocumentSymbol>> {
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

                if mask.len() == 0 {
                    symbols
                } else {
                    symbols
                        .into_iter()
                        .filter(|s| mask.contains(&s.kind))
                        .collect()
                }
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
