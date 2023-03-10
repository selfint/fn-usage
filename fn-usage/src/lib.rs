use jsonrpc::types::JsonRpcResult;
use lsp_types::request::*;

use lsp_types::*;

use petgraph::graph;
use petgraph::stable_graph::NodeIndex;
use petgraph::Graph;





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

pub async fn get_function_calls(
    symbols: &Vec<(Url, DocumentSymbol)>,
    client: lsp_client::client::Client,
    root_path: std::path::PathBuf,
) -> Vec<(CallHierarchyItem, CallHierarchyItem)> {
    let mut fn_calls_futures = vec![];
    for (file, symbol) in symbols {
        let item = CallHierarchyItem {
            name: symbol.name.clone(),
            kind: symbol.kind,
            tags: symbol.tags.clone(),
            detail: symbol.detail.clone(),
            uri: file.clone(),
            range: symbol.range,
            selection_range: symbol.selection_range,
            data: None,
        };

        let request =
            client.request::<CallHierarchyIncomingCalls, ()>(CallHierarchyIncomingCallsParams {
                item: item.clone(),
                partial_result_params: lsp_types::PartialResultParams {
                    partial_result_token: None,
                },
                work_done_progress_params: WorkDoneProgressParams {
                    work_done_token: None,
                },
            });

        fn_calls_futures.push((item, request));
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
            } => panic!("Got error {code}\n{message}"),
        }
    }
    fn_calls
}

pub fn calc_fn_usage(
    symbols: Vec<(Url, DocumentSymbol)>,
    fn_calls: Vec<(CallHierarchyItem, CallHierarchyItem)>,
) -> Vec<(CallHierarchyItem, f32)> {
    let mut graph = graph::DiGraph::<CallHierarchyItem, (), _>::new();
    let mut nodes = vec![];
    for (uri, symbol) in &symbols {
        let item = CallHierarchyItem {
            name: symbol.name.clone(),
            kind: symbol.kind,
            tags: symbol.tags.clone(),
            detail: symbol.detail.clone(),
            uri: uri.clone(),
            range: symbol.range,
            selection_range: symbol.selection_range,
            data: None,
        };
        let node = graph.add_node(item.clone());
        nodes.push((item, node));
    }

    for (src, dst) in &fn_calls {
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

    fn has_path(
        graph: &Graph<CallHierarchyItem, ()>,
        src: &NodeIndex,
        dst: &NodeIndex,
        visited: &[&NodeIndex],
    ) -> bool {
        if src == dst {
            true
        } else if visited.contains(&src) {
            false
        } else {
            let other = [src];
            let neighbor_visited: Vec<&NodeIndex> = visited
                .iter()
                .copied()
                .chain(other.into_iter())
                .collect::<Vec<_>>();

            for neighbor in graph.neighbors(*src) {
                if has_path(graph, &neighbor, dst, &neighbor_visited) {
                    return true;
                }
            }

            false
        }
    }

    let node_usage = nodes
        .iter()
        .map(|(item, node)| {
            let usage = (nodes
                .iter()
                .filter(|(_, other)| has_path(&graph, other, node, &[]))
                .count()
                - 1) as f32
                / nodes.len() as f32
                * 100.;

            (item.clone(), usage)
        })
        .collect::<Vec<_>>();

    node_usage
}
