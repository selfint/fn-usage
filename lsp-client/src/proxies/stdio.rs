use tokio::io::AsyncBufReadExt;
use tokio::process::ChildStdout;
use tokio::{io::BufReader, sync::mpsc::UnboundedSender};

pub async fn stdio_proxy(mut rx: BufReader<ChildStdout>, tx: UnboundedSender<String>) {
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
}
