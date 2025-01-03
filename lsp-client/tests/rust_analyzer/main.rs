use lsp_types::{
    notification::Initialized, request::Initialize, InitializeError, InitializeParams,
    InitializedParams,
};
use std::process::{Child, Command, Stdio};

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

    let (tx, rx, handle) = lsp_client::ChildStdioChannel::wrap(&mut child);

    let mut client = lsp_client::Client::new(tx, rx);

    let init_resp = client.request::<Initialize, InitializeError>(InitializeParams::default());

    insta::assert_debug_snapshot!(init_resp);

    client.notify::<Initialized>(InitializedParams {}).unwrap();

    // stop
    drop(child);
    std::thread::sleep(std::time::Duration::from_millis(100));

    handle.stop();
}
