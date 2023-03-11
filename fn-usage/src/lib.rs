use jsonrpc::types::JsonRpcResult;
use lsp_client::client::Client;
use lsp_types::{request::*, *};
use petgraph::{algo::has_path_connecting, graph::DiGraph, visit::NodeRef};
use std::path::PathBuf;

pub async fn get_project_functions(
    project_files: &[PathBuf],
    client: &Client,
) -> Vec<(Url, DocumentSymbol)> {
    let project_file_uris = project_files
        .iter()
        .map(|file| Url::from_file_path(file).unwrap())
        .collect::<Vec<_>>();

    let mut symbol_futures = vec![];
    for uri in &project_file_uris {
        symbol_futures.push(
            client.request::<DocumentSymbolRequest, ()>(DocumentSymbolParams {
                text_document: lsp_types::TextDocumentIdentifier { uri: uri.clone() },
                partial_result_params: lsp_types::PartialResultParams {
                    partial_result_token: None,
                },
                work_done_progress_params: WorkDoneProgressParams {
                    work_done_token: None,
                },
            }),
        );
    }

    let mut fn_definitions = vec![];
    for (uri, s) in project_file_uris.iter().zip(symbol_futures.into_iter()) {
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
            DocumentSymbolResponse::Flat(_) => {
                panic!("Got flat document symbol");
            }
            DocumentSymbolResponse::Nested(nested) => {
                fn walk_nested_symbols(
                    uri: &Url,
                    children: Vec<DocumentSymbol>,
                    fn_definitions: &mut Vec<(Url, DocumentSymbol)>,
                ) {
                    for child in children {
                        if matches!(child.kind, SymbolKind::FUNCTION | SymbolKind::METHOD) {
                            fn_definitions.push((uri.clone(), child.clone()));
                        }

                        if let Some(children) = child.children {
                            walk_nested_symbols(uri, children, fn_definitions);
                        }
                    }
                }

                walk_nested_symbols(uri, nested, &mut fn_definitions);
            }
        };
    }

    fn_definitions
}

pub async fn get_functions_graph(
    fn_definitions: &[(Url, DocumentSymbol)],
    client: &Client,
    root_path: PathBuf,
) -> (
    Vec<CallHierarchyItem>,
    Vec<(CallHierarchyItem, CallHierarchyItem)>,
) {
    let mut fn_call_items = vec![];
    let mut fn_calls_futures = vec![];
    for (file, symbol) in fn_definitions {
        let fn_definition_items = match client
            .request::<CallHierarchyPrepare, ()>(CallHierarchyPrepareParams {
                text_document_position_params: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri: file.clone() },
                    position: symbol.selection_range.start,
                },
                work_done_progress_params: WorkDoneProgressParams {
                    work_done_token: None,
                },
            })
            .await
            .unwrap()
            .result
        {
            JsonRpcResult::Result(Some(items)) => items
                .into_iter()
                .filter(|i| matches!(i.kind, SymbolKind::FUNCTION | SymbolKind::METHOD)),
            JsonRpcResult::Result(None) => todo!(),
            JsonRpcResult::Error {
                code: _,
                message: _,
                data: _,
            } => todo!(),
        };

        for fn_definition_item in fn_definition_items {
            fn_call_items.push(fn_definition_item.clone());

            let request = client.request::<CallHierarchyIncomingCalls, ()>(
                CallHierarchyIncomingCallsParams {
                    item: fn_definition_item.clone(),
                    partial_result_params: lsp_types::PartialResultParams {
                        partial_result_token: None,
                    },
                    work_done_progress_params: WorkDoneProgressParams {
                        work_done_token: None,
                    },
                },
            );
            fn_calls_futures.push((fn_definition_item, request));
        }
    }

    let mut fn_calls = vec![];
    for (symbol, fn_call_future) in fn_calls_futures {
        let response = fn_call_future.await.unwrap();

        match response.result {
            JsonRpcResult::Result(Some(result)) => {
                for call in result {
                    if call
                        .from
                        .uri
                        .to_file_path()
                        .unwrap()
                        .as_os_str()
                        .to_str()
                        .unwrap()
                        .starts_with(root_path.as_os_str().to_str().unwrap())
                    {
                        fn_calls.push((call.from, symbol.clone()));
                    }
                }
            }
            JsonRpcResult::Result(None) => {
                todo!()
            }
            JsonRpcResult::Error {
                code,
                message,
                data: _,
            } => eprintln!("Got error {code}\n{message}"),
        }
    }

    (fn_call_items, fn_calls)
}

pub fn calc_fn_usage<'a>(
    fn_definitions: &'a [CallHierarchyItem],
    fn_calls: &[(CallHierarchyItem, CallHierarchyItem)],
) -> Vec<(&'a CallHierarchyItem, f32)> {
    let mut graph = DiGraph::<(), (), _>::new();
    let mut nodes = vec![];
    for fn_definition in fn_definitions {
        let node = graph.add_node(());
        nodes.push((fn_definition, node));
    }

    for (src, dst) in fn_calls {
        let src_node = nodes
            .iter()
            .find(|(n, _)| n.selection_range == src.selection_range)
            .unwrap()
            .1;
        let dst_node = nodes
            .iter()
            .find(|(n, _)| n.selection_range == dst.selection_range)
            .unwrap()
            .1;

        graph.add_edge(src_node, dst_node, ());
    }

    nodes
        .iter()
        .map(|(item, node)| {
            let usage = (nodes
                .iter()
                .filter(|(_, other)| has_path_connecting(&graph, other.id(), node.id(), None))
                .count()
                - 1) as f32
                / nodes.len() as f32
                * 100.;

            (*item, usage)
        })
        .collect::<Vec<_>>()
}
