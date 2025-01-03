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

    let mut client = lsp_client::Client::new(lsp_client::StdIO::new(&mut child));

    let init_resp =
        client.request::<Initialize, InitializeError>(Some(InitializeParams::default()));

    insta::assert_debug_snapshot!(init_resp);

    client
        .notify::<Initialized>(Some(InitializedParams {}))
        .unwrap();
}
