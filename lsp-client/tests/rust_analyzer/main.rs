use std::process::Stdio;

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

    let mut stdout_reader = BufReader::new(child.stdout.take().unwrap());

    let server_output_handle = tokio::spawn(async move {
        let mut next_content_length = None;
        let mut next_content_type = None;

        loop {
            let mut line = String::new();
            stdout_reader.read_line(&mut line).await.unwrap();

            let words = line.split_ascii_whitespace().collect::<Vec<_>>();
            match (
                words.as_slice(),
                &mut next_content_length,
                &mut next_content_type,
            ) {
                (["Content-Length:", content_length], None, None) => {
                    next_content_length = Some(content_length.parse().unwrap())
                }
                (["Content-Type:", content_type], Some(_), None) => {
                    next_content_type = Some(content_type.to_string())
                }
                ([], Some(content_length), _) => {
                    let mut content = Vec::with_capacity(*content_length);
                    let mut bytes_left = *content_length;
                    while bytes_left > 0 {
                        let read_bytes =
                            stdout_reader.read_until(b'}', &mut content).await.unwrap();
                        bytes_left -= read_bytes;
                    }

                    let content = String::from_utf8(content).unwrap();
                    server_tx.send(content).unwrap();

                    next_content_length = None;
                    next_content_type = None;
                }
                _ => panic!("Got unexpected stdout"),
            };
        }
    });

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
