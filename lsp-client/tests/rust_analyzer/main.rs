use lsp_client::clients;
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

    let stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let (mut client, handles, stop_flag) = clients::stdio_client(stdin, stdout, stderr);

    let init_resp = client.request::<Initialize, InitializeError>(InitializeParams::default());

    insta::assert_debug_snapshot!(init_resp);

    client.notify::<Initialized>(InitializedParams {}).unwrap();

    // stop
    drop(child);
    std::thread::sleep(std::time::Duration::from_millis(100));

    stop_flag.store(true, std::sync::atomic::Ordering::Relaxed);

    for handle in handles {
        handle.join().expect("failed to join handle");
    }
}
