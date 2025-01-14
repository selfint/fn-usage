use std::collections::HashSet;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::str::FromStr;

use anyhow::{Context, Result};
use lsp_types::Url;
use serde_json::json;

use lsp_client::{Client, StdIO};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 3 {
        eprintln!("Usage: {} <root-uri> <lsp-cmd> [lsp-cmd-args...]", args[0]);
        std::process::exit(1);
    }

    let (root, cmd, args) = (&args[1], &args[2], &args[3..]);

    let root = Url::from_str(&root)?;
    eprintln!("Using root: {}", &root.as_str());

    // read all lines from stdin
    let uris: Vec<Url> = std::io::stdin()
        .lock()
        .lines()
        .filter_map(Result::ok)
        .filter_map(|line| root.join(&line).ok())
        .collect();

    let mut child = start_lsp_server(cmd, args);
    let mut client = lsp_client::Client::new(StdIO::new(&mut child));

    // start stderr logging thread
    let stderr = child.stderr.take().expect("Failed to take stderr");
    std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            match line {
                Ok(line) => eprintln!("stderr: {}", line),
                Err(err) => panic!("Error reading stderr: {}", err),
            }
        }
    });

    let server_capabilities = client.initialize(&root)?;

    if server_capabilities.document_symbol_provider.is_none() {
        anyhow::bail!("Server is not 'textDocument/documentSymbol' provider");
    }

    if server_capabilities.references_provider.is_none() {
        anyhow::bail!("Server is not 'textDocument/reference' provider");
    }

    let edges = get_edges(&mut client, root.as_str(), &uris)?;

    println!(
        "{}",
        json!({
            "root": root,
            "nodes": uris,
            "edges": edges
        })
    );

    Ok(())
}

fn start_lsp_server(cmd: &str, args: &[String]) -> Child {
    Command::new(cmd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to start lsp server")
}

fn read_uri(uri: &Url) -> Result<String> {
    match uri.scheme() {
        "file" => std::fs::read_to_string(uri.path()).context(format!("Reading {}", uri.as_str())),
        other => todo!("Got unexpected file scheme: {:?}", other),
    }
}

fn get_edges(
    client: &mut Client<StdIO>,
    root: &str,
    uris: &[Url],
) -> Result<HashSet<(String, String)>> {
    let mut edges: HashSet<(String, String)> = HashSet::new();

    for (i, uri) in uris.iter().enumerate() {
        eprintln!(
            "Loading uri ({:>4}/{:>4}): {}",
            i + 1,
            uris.len(),
            uri.as_str()
        );

        client.open(uri, &read_uri(uri)?)?;
    }

    eprintln!("Waiting 3 seconds for LSP to index code...");
    std::thread::sleep(std::time::Duration::from_secs(3));

    for (i, uri) in uris.iter().enumerate() {
        eprint!(
            "Scanning uri ({:>4}/{:>4}): {}",
            i + 1,
            uris.len(),
            uri.as_str()
        );

        // ignore uri not under root
        let Some(symbol_node) = uri.as_str().strip_prefix(root) else {
            continue;
        };

        let symbols = client.get_symbols(uri)?;

        eprintln!(" | Got symbols: {}", symbols.len());

        for (j, symbol) in symbols.iter().enumerate() {
            eprint!(
                "Requesting {} symbol ({:>4}/{:>4}): {:?} {:>25}",
                symbol_node,
                j + 1,
                symbols.len(),
                symbol.kind,
                symbol.name
            );

            let references = client.get_references(uri, symbol)?;

            eprintln!(" | Got references: {}", references.len());

            for reference in references {
                if reference != *uri && uris.contains(&reference) {
                    if let Some(reference_node) = reference.as_str().strip_prefix(root) {
                        edges.insert((reference_node.to_string(), symbol_node.to_string()));
                    }
                }
            }
        }
    }

    Ok(edges)
}
