use std::{path::PathBuf, process::Stdio, time::Duration};

use jsonrpc::types::{JsonRpcResult, Response};
use lsp_client::clients;
use lsp_types::{notification::*, request::*, *};
use tokio::process::{Child, Command};

fn start_server(cmd: &str) -> Child {
    let mut parts = cmd.split_ascii_whitespace();
    let name = parts.next().unwrap();
    let args = parts.collect::<Vec<_>>();

    Command::new(name)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to start rust analyzer")
}

#[tokio::main]
async fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let [project_root, project_glob, server_cmd] = args.as_slice() else {
        eprintln!("Got invalid args: {:?}", args);
        eprintln!("Usage: fn-usage <project-root> <project-glob> <server-cmd>");
        return;
    };

    let mut child = start_server(server_cmd);
    let root_uri =
        Url::from_file_path(&PathBuf::from(project_root).canonicalize().unwrap()).unwrap();

    let stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let (client, handles) = clients::stdio_client(stdin, stdout, stderr);

    let response = client
        .request::<Initialize, InitializeError>(InitializeParams {
            root_uri: Some(root_uri.clone()),
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
        .await
        .unwrap();

    match response.result {
        JsonRpcResult::Result(result) if result.capabilities.call_hierarchy_provider.is_none() => {
            eprintln!("Server has no call hierarchy provider, quitting...");
            return;
        }
        JsonRpcResult::Error {
            code,
            message,
            data: _,
        } => eprintln!("Failed to init server, error {code}:\n{message}"),
        _ => {}
    };

    client.notify::<Initialized>(InitializedParams {}).unwrap();

    let root_path = root_uri.to_file_path().unwrap();

    let project_files = glob::glob(root_path.join(project_glob).to_str().unwrap())
        .into_iter()
        .flat_map(|fs| fs.map(|f| f.unwrap()))
        .collect::<Vec<_>>();

    // wait for server to start
    let uri = Url::from_file_path(project_files.first().unwrap()).unwrap();
    // client
    //     .notify::<DidOpenTextDocument>(DidOpenTextDocumentParams {
    //         text_document: TextDocumentItem {
    //             uri: uri.clone(),
    //             language_id: "unknown".to_string(),
    //             version: 0,
    //             text: "".to_string(),
    //         },
    //     })
    //     .unwrap();

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

    let fn_definitions = fn_usage::get_project_functions(&project_files, &client).await;

    let (fn_definitions, fn_calls) =
        fn_usage::get_functions_graph(&fn_definitions, &client, root_path).await;

    let fn_usage = fn_usage::calc_fn_usage(&fn_definitions, &fn_calls);

    for (item, usage) in fn_usage {
        println!("{}#{}: {}", item.uri, item.name, usage);
    }

    for handle in handles {
        handle.abort();
    }
}
