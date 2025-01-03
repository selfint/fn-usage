use std::process::{Child, Command, Stdio};

use lsp_client::StdIO;

fn start_lsp_server(cmd: &str, args: &[String]) -> Child {
    Command::new(cmd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to start lsp server")
}

struct Connection {
    src: String,
    dst: String,
}

fn get_connections(client: &mut lsp_client::Client<StdIO>, base: &str) -> Vec<Connection> {
    vec![Connection {
        src: "src".into(),
        dst: "dst".into(),
    }]
}

fn main() {
    // get sys args
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 3 {
        eprintln!("Usage: {} <base> <cmd> [args...]", args[0]);
        std::process::exit(1);
    }

    let base = &args[1];
    let cmd = &args[2];
    let args = &args[3..];

    let mut child = start_lsp_server(cmd, args);
    let io = lsp_client::StdIO::new(&mut child);
    let mut client = lsp_client::Client::new(io);

    let connections = get_connections(&mut client, base);

    // print graphviz .dot file
    println!("digraph G {{");
    for connection in connections {
        println!("    \"{}\" -> \"{}\";", connection.src, connection.dst);
    }
    println!("}}");
}
