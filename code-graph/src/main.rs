use std::collections::HashSet;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::str::FromStr;

use anyhow::{Context, Result};
use lsp_client::types::notification::{DidOpenTextDocument, Initialized};
use lsp_client::types::request::{DocumentSymbolRequest, Initialize, References};
use lsp_client::types::{DocumentSymbolResponse, Url};
use serde_json::json;

use lsp_client::StdIO;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 3 {
        eprintln!("Usage: {} <root-uri> <lsp-cmd> [lsp-cmd-args...]", args[0]);
        std::process::exit(1);
    }

    let root_uri = &args[1];
    let cmd = &args[2];
    let args = &args[3..];

    let root_uri = Url::from_str(&root_uri)?;
    eprintln!("Using root: {}", &root_uri.as_str());

    // read all lines from stdin
    let stdin = std::io::stdin();
    let files: Vec<_> = stdin
        .lock()
        .lines()
        .filter_map(|l| l.ok())
        .filter_map(|l| root_uri.join(&l).ok())
        .inspect(|f| eprintln!("Using file: {}", f.as_str()))
        .collect();

    let mut child = start_lsp_server(cmd, args);
    let io = lsp_client::StdIO::new(&mut child);
    let mut client = lsp_client::Client::new(io, false);

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

    initialize_lsp(&mut client, &root_uri)?;

    let nodes: Vec<_> = files
        .iter()
        .collect::<HashSet<_>>()
        .iter()
        .map(|n| n.as_str())
        .collect();

    let edges = get_edges(&mut client, &root_uri, &files)?;

    println!(
        "{}",
        json!({
            "nodes": nodes,
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
    client: &mut lsp_client::Client<StdIO>,
    root_uri: &Url,
    files: &[Url],
) -> Result<HashSet<(String, String)>> {
    let mut edges: HashSet<(String, String)> = HashSet::new();

    for uri in files {
        eprintln!("Loading uri: {}", uri.as_str());

        client.notify::<DidOpenTextDocument>(serde_json::from_value(json!({
            "textDocument": {
            "uri": uri.clone(),
            "languageId": "",
            "version": 1,
            "text": read_uri(uri)?,
            }
        }))?)?;
    }

    eprintln!("Waiting 3 seconds for LSP to index code...");
    std::thread::sleep(std::time::Duration::from_secs(3));

    for uri in files {
        eprintln!("Processing uri: {}", uri.as_str());

        let symbols = client.request::<DocumentSymbolRequest>(serde_json::from_value(json!({
            "textDocument": {
                "uri": uri.clone(),
            },
        }))?)?;

        let symbols = match symbols {
            Some(DocumentSymbolResponse::Nested(vec)) => {
                let mut symbols = vec![];
                let mut queue = vec;

                while let Some(symbol) = queue.pop() {
                    symbols.push(symbol.clone());
                    if let Some(children) = symbol.children {
                        queue.extend(children);
                    }
                }

                symbols
            }
            Some(DocumentSymbolResponse::Flat(flat)) => {
                if flat.len() > 0 {
                    panic!("Got non-empty flat documentSymbol response")
                }

                vec![]
            }
            None => vec![],
        };

        for symbol in symbols.iter() {
            eprintln!("Processing symbol: {:?} {}", symbol.kind, symbol.name);

            let Some(references) =
                client.request::<References>(serde_json::from_value(json!({
                    "textDocument": {
                        "uri": uri.clone(),
                    },
                    "position": {
                        "line": symbol.selection_range.start.line,
                        "character": symbol.selection_range.start.character,
                    },
                    "context": {
                        "includeDeclaration": false,
                    },
                }))?)?
            else {
                continue;
            };

            eprintln!("Got references: {}", references.len());

            for reference in references {
                if reference.uri == *uri {
                    continue;
                }

                if !reference.uri.as_str().starts_with(root_uri.as_str()) {
                    continue;
                }

                edges.insert((reference.uri.to_string(), uri.to_string()));
            }
        }
    }

    Ok(edges)
}

fn initialize_lsp(client: &mut lsp_client::Client<StdIO>, root_uri: &Url) -> Result<()> {
    let initialize = client.request::<Initialize>(serde_json::from_value(json!({
        "capabilities": {
            "textDocument": {
                "documentSymbol": {
                    "hierarchicalDocumentSymbolSupport": true,
                },
            }
        },
        "workspaceFolders": [{
            "uri": root_uri,
            "name": "name"
        }]
    }))?)?;

    if initialize.capabilities.document_symbol_provider.is_none() {
        anyhow::bail!("Server is not 'documentSymbol' provider");
    }

    if initialize.capabilities.references_provider.is_none() {
        anyhow::bail!("Server is not 'references' provider");
    }

    client.notify::<Initialized>(None)?;

    Ok(())
}
