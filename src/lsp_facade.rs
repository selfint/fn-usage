use anyhow::Result;
use lsp_types::{
    notification::{DidOpenTextDocument, Initialized},
    request::{DocumentSymbolRequest, GotoDefinition, Initialize, References},
    DocumentSymbol, DocumentSymbolResponse, GotoDefinitionResponse, ServerCapabilities, Url,
};
use serde_json::json;

use crate::Client;

impl Client {
    pub fn open(&mut self, uri: &Url, text: &str) -> Result<()> {
        self.notify::<DidOpenTextDocument>(
            serde_json::from_value(json!(
                {
                    "textDocument": {
                        "uri": uri,
                        "languageId": "",
                        "version": 1,
                        "text": text
                    }
                }
            ))
            .unwrap(),
        )
    }

    pub fn references(&mut self, uri: &Url, symbol: &DocumentSymbol) -> Result<Vec<Url>> {
        let references = self.request::<References>(
            serde_json::from_value(json!(
                {
                    "textDocument": {
                        "uri": uri,
                    },
                    "position": symbol.selection_range.start,
                    "context": {
                        "includeDeclaration": false
                    }
                }
            ))
            .unwrap(),
        )?;

        Ok(references
            .unwrap_or_default()
            .into_iter()
            .map(|r| r.uri)
            .collect())
    }

    pub fn goto_definition(&mut self, uri: &Url, symbol: &DocumentSymbol) -> Result<Vec<Url>> {
        let definition = self.request::<GotoDefinition>(
            serde_json::from_value(json!(
                {
                    "textDocument": {
                        "uri": uri,
                    },
                    "position": symbol.selection_range.start,
                }
            ))
            .unwrap(),
        )?;

        let definition = match definition {
            Some(GotoDefinitionResponse::Scalar(location)) => vec![location.uri],
            Some(GotoDefinitionResponse::Array(vec)) => vec.into_iter().map(|l| l.uri).collect(),
            Some(GotoDefinitionResponse::Link(vec)) => {
                vec.into_iter().map(|l| l.target_uri).collect()
            }
            None => vec![],
        };

        Ok(definition)
    }

    pub fn symbols(&mut self, uri: &Url) -> Result<Vec<DocumentSymbol>> {
        let symbols = self.request::<DocumentSymbolRequest>(
            serde_json::from_value(json!(
                {
                    "textDocument": {
                        "uri": uri
                    },
                }
            ))
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
            serde_json::from_value(json!(
                {
                    "capabilities": {
                        "textDocument": {
                            "documentSymbol": {
                                "hierarchicalDocumentSymbolSupport": true,
                            }
                        },
                    },
                    "workspaceFolders": [
                        {
                            "uri": uri,
                            "name": "name"
                        }
                    ]
                }
            ))
            .unwrap(),
        )?;

        self.notify::<Initialized>(None)?;

        Ok(response.capabilities)
    }
}
