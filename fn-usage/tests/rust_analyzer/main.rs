use jsonrpc::types::{JsonRpcResult, Response};
use lsp_client::clients;
use lsp_types::{notification::*, request::*, *};
use std::{path::Path, process::Stdio, time::Duration};
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

    let symbols = fn_usage::get_project_functions(&project_files, &client).await;

    let mut symbols_short = symbols
        .iter()
        .map(|(uri, s)| {
            let content =
                String::from_utf8(std::fs::read(uri.to_file_path().unwrap()).unwrap()).unwrap();
            let line_content = content.lines().nth(s.line as usize).unwrap().to_string();
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
                s,
                line_content,
                " ".repeat(s.character as usize) + "^",
            )
        })
        .collect::<Vec<_>>();

    symbols_short.sort();
    insta::assert_debug_snapshot!(symbols_short);

    let (fn_items, fn_calls) = fn_usage::get_function_calls(&symbols, client, root_path).await;

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

    let fn_usage = fn_usage::calc_fn_usage(&fn_items, &fn_calls);

    let mut short_usage = fn_usage
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
