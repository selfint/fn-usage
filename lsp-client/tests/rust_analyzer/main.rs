use std::process::Stdio;

use lsp_client::proxies::stdio::stdio_proxy;
use lsp_types::{
    notification::Initialized, request::Initialize, InitializeError, InitializeParams,
    InitializedParams,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process,
};

fn start_rust_analyzer() -> process::Child {
    process::Command::new("rust-analyzer")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to start rust analyzer")
}

#[tokio::test]
async fn test_rust_analyzer() {
    let mut child = start_rust_analyzer();

    let (client_tx, mut client_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let (server_tx, server_rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    let mut stdin = child.stdin.take().unwrap();
    let server_input_handle = tokio::spawn(async move {
        while let Some(msg) = client_rx.recv().await {
            stdin.write_all(msg.as_bytes()).await.unwrap();
        }
    });

    let server_output_handle = tokio::spawn(stdio_proxy(
        BufReader::new(child.stdout.take().unwrap()),
        server_tx,
    ));

    let mut stderr_lines = BufReader::new(child.stderr.take().unwrap()).lines();
    let server_error_handle = tokio::spawn(async move {
        while let Ok(Some(line)) = stderr_lines.next_line().await {
            eprintln!("Got err from server: {}", line);
        }
    });

    let client = lsp_client::Client::new(client_tx, server_rx);

    let init_resp = client
        .request::<Initialize, InitializeError>(InitializeParams::default())
        .await;

    insta::assert_debug_snapshot!(init_resp);

    client.notify::<Initialized>(InitializedParams {}).unwrap();

    server_input_handle.abort();
    server_output_handle.abort();
    server_error_handle.abort();
}
