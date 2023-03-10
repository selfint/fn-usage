use jsonrpc::types::JsonRpcResult;
use jsonrpc::types::Response;
use lsp_client::clients;
use lsp_types::notification::*;
use lsp_types::request::*;
use lsp_types::*;
use petgraph::graph;
use petgraph::stable_graph::NodeIndex;
use petgraph::Graph;

use std::time::Duration;
use std::{path::Path, process::Stdio};
use tokio::process::{Child, Command};

const SAMPLE_PROJECT_PATH: &str = "tests/rust_analyzer/sample_rust_project";

fn start_rust_analyzer() -> Child {
    Command::new("rust-analyzer")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to start rust analyzer")
}

fn get_sample_root() -> Url {
    let sample_project_path = Path::new(SAMPLE_PROJECT_PATH).canonicalize().unwrap();

    Url::from_file_path(sample_project_path).expect("failed to convert project path to URL")
}

async fn _test_rust_analyzer() {
    let mut child = start_rust_analyzer();

    let stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let (client, handles) = clients::stdio_client(stdin, stdout, stderr);

    let init_resp = client
        .request::<Initialize, InitializeError>(InitializeParams {
            root_uri: Some(get_sample_root()),
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
            ..Default::default()
        })
        .await;

    assert!(init_resp.is_ok());

    client.notify::<Initialized>(InitializedParams {}).unwrap();

    let root_path = get_sample_root().to_file_path().unwrap();

    let project_files = glob::glob(root_path.join("**/*.rs").to_str().unwrap())
        .into_iter()
        .flat_map(|fs| fs.map(|f| f.unwrap()))
        .collect::<Vec<_>>();

    let mut short_project_files = project_files
        .iter()
        .map(|f| {
            f.as_path()
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .to_string()
        })
        .collect::<Vec<_>>();

    short_project_files.sort();
    insta::assert_debug_snapshot!(short_project_files);

    // wait for server to start
    let uri = Url::from_file_path(project_files.first().unwrap()).unwrap();
    while let Ok(Response {
        jsonrpc: _,
        result,
        id: _,
    }) = client
        .request::<FoldingRangeRequest, ()>(FoldingRangeParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            partial_result_params: lsp_types::PartialResultParams {
                partial_result_token: None,
            },
            work_done_progress_params: WorkDoneProgressParams {
                work_done_token: None,
            },
        })
        .await
    {
        match result {
            JsonRpcResult::Result(_) => break,
            JsonRpcResult::Error {
                code,
                message,
                data: _,
            } => {
                println!("error {}:\n{}", code, message);
                assert!(
                    code == -32801,
                    "got unexpected error {}, message: {}",
                    code,
                    message
                );
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
    }

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

    let mut symbols_short = symbols
        .iter()
        .map(|(uri, s)| {
            let content =
                String::from_utf8(std::fs::read(uri.to_file_path().unwrap()).unwrap()).unwrap();
            let line_content = content
                .lines()
                .nth(s.selection_range.start.line as usize)
                .unwrap()
                .to_string();
            let file_name = uri
                .to_file_path()
                .unwrap()
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .to_string();

            (
                file_name,
                s.selection_range.start,
                line_content,
                " ".repeat(s.selection_range.start.character as usize) + "^",
            )
        })
        .collect::<Vec<_>>();

    symbols_short.sort();
    insta::assert_debug_snapshot!(symbols_short);

    let mut fn_calls_futures = vec![];
    for (file, symbol) in &symbols {
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

    let mut short_fn_calls = fn_calls
        .iter()
        .map(|(src, dst)| {
            let src_path = src.uri.to_file_path().unwrap();
            let src_name = src_path.file_name().unwrap().to_str().unwrap().to_string();
            let src_content = String::from_utf8(std::fs::read(src_path).unwrap()).unwrap();
            let src_line_content = src_content
                .lines()
                .nth(src.selection_range.start.line as usize)
                .unwrap()
                .to_string();

            let dst_path = dst.uri.to_file_path().unwrap();
            let dst_name = dst_path.file_name().unwrap().to_str().unwrap().to_string();
            let dst_content = String::from_utf8(std::fs::read(dst_path).unwrap()).unwrap();
            let dst_line_content = dst_content
                .lines()
                .nth(dst.selection_range.start.line as usize)
                .unwrap()
                .to_string();

            (src_name, src_line_content, dst_name, dst_line_content)
        })
        .collect::<Vec<_>>();

    short_fn_calls.sort();
    insta::assert_debug_snapshot!(short_fn_calls);

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

            (item, usage)
        })
        .collect::<Vec<_>>();

    let mut short_usage = node_usage
        .iter()
        .map(|(src, usage)| {
            let src_path = src.uri.to_file_path().unwrap();
            let src_name = src_path.file_name().unwrap().to_str().unwrap().to_string();
            let src_content = String::from_utf8(std::fs::read(src_path).unwrap()).unwrap();
            let src_line_content = src_content
                .lines()
                .nth(src.selection_range.start.line as usize)
                .unwrap()
                .to_string();

            (src_name, src_line_content, usage.to_string())
        })
        .collect::<Vec<_>>();

    short_usage.sort();
    insta::assert_debug_snapshot!(short_usage);

    for handle in handles {
        handle.abort();
    }
}

#[tokio::test]
async fn test_rust_analyzer() {
    _test_rust_analyzer().await
}
