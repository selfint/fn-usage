use std::process::{Child, Command, Stdio};
use std::str::FromStr;

use lsp_types::Url;

fn start_rust_analyzer() -> Child {
    Command::new("rust-analyzer")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to start rust analyzer")
}

#[test]
fn test_rust_analyzer() {
    let mut child = start_rust_analyzer();

    let io = lsp_client::StdIO::new(&mut child).expect("failed to get io");
    let mut client = lsp_client::Client::new(io);

    // let init_resp = client.initialize(&Url::from_str("file:///").unwrap());

    // assert!(init_resp.is_ok());
}
