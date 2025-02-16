use std::io::BufReader;
use std::process::{Command, Stdio};
use std::str::FromStr;

use lsp_client::Client;
use lsp_types::Url;

#[test]
fn test_rust_analyzer() {
    let mut child = Command::new("rust-analyzer")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to start rust analyzer");

    let input = BufReader::new(child.stdout.take().expect("Failed to take stdout"));
    let output = child.stdin.take().expect("Failed to take stdin");

    let mut client = Client::new(Box::new(input), Box::new(output));

    let init_resp = client.initialize(Url::from_str("file:///").unwrap());

    assert!(init_resp.is_ok());
}
