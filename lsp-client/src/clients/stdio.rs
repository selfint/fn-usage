use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::process::{ChildStderr, ChildStdin, ChildStdout};
use tokio::task::JoinHandle;
use tokio::{io::BufReader, sync::mpsc::UnboundedSender};

use crate::Client;

pub fn stdio_client(
    mut stdin: ChildStdin,
    stdout: ChildStdout,
    stderr: ChildStderr,
) -> (Client, Vec<JoinHandle<()>>) {
    let (client_tx, mut client_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let (server_tx, server_rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    let server_input_handle = tokio::spawn(async move {
        while let Some(msg) = client_rx.recv().await {
            stdin.write_all(msg.as_bytes()).await.unwrap();
        }
    });

    let server_output_handle = stdout_proxy(BufReader::new(stdout), server_tx);

    let mut stderr_lines = BufReader::new(stderr).lines();
    let server_error_handle = tokio::spawn(async move {
        while let Ok(Some(line)) = stderr_lines.next_line().await {
            eprintln!("Got err from server: {}", line);
        }
    });

    let client = Client::new(client_tx, server_rx);

    (
        client,
        vec![
            server_input_handle,
            server_output_handle,
            server_error_handle,
        ],
    )
}

fn stdout_proxy(mut rx: BufReader<ChildStdout>, tx: UnboundedSender<String>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut next_content_length = None;
        let mut next_content_type = None;

        loop {
            let mut line = String::new();
            rx.read_line(&mut line).await.unwrap();

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
                        let read_bytes = rx.read_until(b'}', &mut content).await.unwrap();
                        bytes_left -= read_bytes;
                    }

                    let content = String::from_utf8(content).unwrap();
                    tx.send(content).unwrap();

                    next_content_length = None;
                    next_content_type = None;
                }
                _ => panic!("Got unexpected stdout"),
            };
        }
    })
}
