use anyhow::Result;
use lsp_types::Url;
use lsp_types::{notification::*, request::*, *};

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
            .notify::<DidOpenTextDocument>(Some(DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: uri.clone(),
                    language_id: "".to_string(),
                    version: 1,
                    text: text.to_string(),
                },
            }))
    }

    pub fn get_references(&mut self, uri: &Url, symbol: &DocumentSymbol) -> Result<Vec<Url>> {
        let references = self
            .lsp
            .request::<References>(Some(lsp_types::ReferenceParams {
                text_document_position: lsp_types::TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri: uri.clone() },
                    position: symbol.selection_range.start,
                },
                work_done_progress_params: WorkDoneProgressParams {
                    work_done_token: None,
                },
                partial_result_params: PartialResultParams {
                    partial_result_token: None,
                },
                context: lsp_types::ReferenceContext {
                    include_declaration: false,
                },
            }))?;

        Ok(references
            .unwrap_or_default()
            .into_iter()
            .map(|f| f.uri)
            .collect())
    }

    pub fn get_symbols(&mut self, uri: &Url) -> Result<Vec<DocumentSymbol>> {
        let symbols = self
            .lsp
            .request::<DocumentSymbolRequest>(Some(DocumentSymbolParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                work_done_progress_params: WorkDoneProgressParams {
                    work_done_token: None,
                },
                partial_result_params: PartialResultParams {
                    partial_result_token: None,
                },
            }))?;

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
        let response = self.lsp.request::<Initialize>(Some(InitializeParams {
            capabilities: ClientCapabilities {
                text_document: Some(TextDocumentClientCapabilities {
                    document_symbol: Some(DocumentSymbolClientCapabilities {
                        hierarchical_document_symbol_support: Some(true),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            },
            workspace_folders: Some(vec![WorkspaceFolder {
                uri: uri.clone(),
                name: "name".to_string(),
            }]),
            ..Default::default()
        }))?;

        self.lsp.notify::<Initialized>(None)?;

        Ok(response.capabilities)
    }
}
