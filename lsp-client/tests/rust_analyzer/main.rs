use std::process::Stdio;

use lsp_types::{
    notification::Initialized, request::Initialize, InitializeError, InitializeParams,
    InitializedParams,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process,
    sync::oneshot,
};

fn start_rust_analyzer() -> process::Child {
    process::Command::new("rust-analyzer")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        // .stderr(Stdio::piped())
        .spawn()
        .expect("failed to start rust analyzer")
}

// #[tokio::test]
async fn test_foo() {
    let mut cmd = process::Command::new("rust-analyzer");

    // Specify that we want the command's standard output piped back to us.
    // By default, standard input/output/error will be inherited from the
    // current process (for example, this means that standard input will
    // come from the keyboard and standard output/error will go directly to
    // the terminal if this process is invoked from the command line).
    cmd.stdout(Stdio::piped());
    cmd.stdin(Stdio::piped());

    let mut child = cmd.spawn().expect("failed to spawn command");

    let stdout = child
        .stdout
        .take()
        .expect("child did not have a handle to stdout");

    let mut stdin = child.stdin.take().unwrap();

    let mut reader = BufReader::new(stdout);

    dbg!("writing");
    let handle1 = tokio::spawn(async move { stdin.write_all(b"hello world\n").await.unwrap() });
    dbg!("wrote");

    let handle = tokio::spawn(async move {
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        println!("Line: {}", line);
    });

    handle1.await.unwrap();
    handle.await.unwrap();

    assert!(false);
}

#[tokio::test]
async fn test_rust_analyzer() {
    let mut child = start_rust_analyzer();

    let (client_tx, mut client_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let (server_tx, server_rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    let (kill_server_input_tx, mut kill_server_input_rx) = oneshot::channel::<()>();
    let mut stdin = child.stdin.take().unwrap();

    let server_input_handle = tokio::spawn(async move {
        while kill_server_input_rx.try_recv().is_err() {
            let Some(msg) = client_rx.recv().await else { continue };
            stdin.write_all(msg.as_bytes()).await.unwrap();
        }
    });

    let (kill_server_output_tx, mut kill_server_output_rx) = oneshot::channel::<()>();
    let mut stdout_reader = BufReader::new(child.stdout.take().unwrap());

    let server_output_handle = tokio::spawn(async move {
        let mut next_content_length = None;
        let mut next_content_type = None;

        while kill_server_output_rx.try_recv().is_err() {
            dbg!("waiting");
            let mut header = String::new();
            stdout_reader.read_line(&mut header).await.unwrap();

            println!("stdout = {}", header);
            let parts = header.split_ascii_whitespace().collect::<Vec<_>>();
            dbg!(&parts);
            match parts.as_slice() {
                ["Content-Length:", content_length] if next_content_length.is_none() => {
                    next_content_length = Some(content_length.parse().unwrap())
                }
                ["Content-Type:", content_type] if next_content_type.is_none() => {
                    next_content_type = Some(content_type.to_string())
                }
                [] if next_content_length.is_some() => {
                    let mut content = Vec::with_capacity(next_content_length.unwrap());
                    let read_bytes = stdout_reader.read_exact(&mut content).await.unwrap();
                    dbg!((read_bytes, next_content_length));

                    let content = String::from_utf8(content).unwrap();
                    server_tx.send(content).unwrap();
                    next_content_length = None;
                    next_content_type = None;
                }
                _ => panic!("Got unexpected stdout"),
            };
        }
    });

    // let (kill_server_error_tx, mut kill_server_error_rx) = oneshot::channel::<()>();
    // let mut stderr_lines = BufReader::new(child.stderr.take().unwrap()).lines();

    // let server_error_handle = tokio::spawn(async move {
    //     while kill_server_error_rx.try_recv().is_err() {
    //         let Ok(Some(line)) = stderr_lines.next_line().await else {continue };
    //         eprintln!("Got err from server: {}", line);
    //     }
    // });

    let client = lsp_client::Client::new(client_tx, server_rx);

    let init_resp = client
        .request::<Initialize, InitializeError>(InitializeParams::default())
        .await;

    insta::assert_debug_snapshot!(init_resp,
        @""
    );

    client.notify::<Initialized>(InitializedParams {}).unwrap();

    kill_server_input_tx.send(()).unwrap();
    kill_server_output_tx.send(()).unwrap();
    // kill_server_error_tx.send(()).unwrap();
    server_input_handle.await.unwrap();
    server_output_handle.await.unwrap();
    // server_error_handle.await.unwrap();
}
