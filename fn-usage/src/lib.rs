use jsonrpc::types::JsonRpcResult;
use lsp_types::request::*;
use lsp_types::*;

pub async fn get_project_functions(
    project_files: Vec<std::path::PathBuf>,
    client: &lsp_client::client::Client,
) -> Vec<(Url, DocumentSymbol)> {
    let mut symbol_futures = vec![];
    for f in project_files {
        let uri = Url::from_file_path(f).unwrap();
        symbol_futures.push((
            uri.clone(),
            client.request::<DocumentSymbolRequest, ()>(DocumentSymbolParams {
                text_document: lsp_types::TextDocumentIdentifier { uri },
                partial_result_params: lsp_types::PartialResultParams {
                    partial_result_token: None,
                },
                work_done_progress_params: WorkDoneProgressParams {
                    work_done_token: None,
                },
            }),
        ));
    }

    let mut symbols = vec![];
    for (uri, s) in symbol_futures {
        let response = match s.await.unwrap().result {
            JsonRpcResult::Result(Some(response)) => response,
            JsonRpcResult::Result(None) => panic!("Got no symbols in doc: {}", uri),
            JsonRpcResult::Error {
                code,
                message,
                data,
            } => panic!(
                "Got error for symbols in uri: {}, code: {}, message: {}, data: {:?}",
                uri, code, message, data
            ),
        };

        match response {
            DocumentSymbolResponse::Flat(flat) => flat.iter().for_each(|_s| {
                // if matches!(s.kind, SymbolKind::FUNCTION | SymbolKind::METHOD) {
                //     symbols.push((uri.clone(), s));
                // }
                panic!("got flat");
            }),
            DocumentSymbolResponse::Nested(nested) => {
                fn walk_nested_symbols(
                    uri: &Url,
                    nested: Vec<DocumentSymbol>,
                    symbols: &mut Vec<(Url, DocumentSymbol)>,
                ) {
                    for s in nested {
                        if matches!(s.kind, SymbolKind::FUNCTION | SymbolKind::METHOD) {
                            symbols.push((uri.clone(), s.clone()));
                        }

                        if let Some(children) = s.children {
                            walk_nested_symbols(uri, children, symbols);
                        }
                    }
                }

                walk_nested_symbols(&uri, nested, &mut symbols);
            }
        };
    }
    symbols
}
