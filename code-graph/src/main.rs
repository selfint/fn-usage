use std::collections::HashSet;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::str::FromStr;

use anyhow::{Context, Result};

use lsp_client::types::{notification::*, request::*, *};
use lsp_client::StdIO;
use std::io::{BufRead, BufReader};

fn start_lsp_server(cmd: &str, args: &[String]) -> Child {
    Command::new(cmd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to start lsp server")
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct Connection {
    pub src: String,
    pub dst: String,
}

fn path_to_uri(path: &PathBuf) -> Result<Uri> {
    let uri = "file://".to_string()
        + path
            .to_str()
            .context(format!("converting {:?} to str", path))?;

    Uri::from_str(&uri).context(format!("parsing uri {:?}", uri))
}

fn get_connections(
    client: &mut lsp_client::Client<StdIO>,
    base_path: &PathBuf,
    suffixes: &[&str],
    ignore: &[&str],
) -> Result<(Vec<String>, Vec<Connection>)> {
    let root_uri = path_to_uri(base_path)?;

    let initialize_params: InitializeParams = serde_json::from_value(serde_json::json!({
        "window": {
            "workDoneProgress": true,
        },
        "capabilities": {
            "textDocument": {
                "references": {
                    "dynamicRegistration": false
                },
                "documentSymbol": {
                    "hierarchicalDocumentSymbolSupport": true,
                },
                "selectionRange": {
                    "dynamicRegistration": false
                },
            }
        },
        "workspaceFolders": [{
            "uri": root_uri,
            "name": "name"
        }]
    }))?;

    let initialize = client.request::<Initialize>(Some(initialize_params))?;

    if initialize.capabilities.document_symbol_provider.is_none() {
        anyhow::bail!("Server is not 'documentSymbol' provider");
    }

    if initialize.capabilities.references_provider.is_none() {
        anyhow::bail!("Server does not support 'references' provider");
    }

    client
        .notify::<Initialized>(Some(InitializedParams {}))
        .context("Sending Initialized notification")?;

    // give server time to index project
    std::thread::sleep(std::time::Duration::from_secs(3));

    // get all files with suffixes in base path recursively
    let mut files = vec![];
    for entry in walkdir::WalkDir::new(base_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            if ignore.len() > 0
                && ignore
                    .iter()
                    .any(|i| e.path().to_str().unwrap().contains(i))
            {
                false
            } else if !suffixes
                .iter()
                .any(|s| e.path().extension().map_or(false, |ext| *ext == **s))
            {
                false
            } else {
                eprintln!("Found file: {:?} ", e.path());
                true
            }
        })
    {
        files.push(entry.path().to_path_buf());
    }

    let mut nodes: HashSet<String> = HashSet::new();
    let mut connections: HashSet<Connection> = HashSet::new();

    for (i, file) in files.iter().enumerate() {
        eprintln!("Processing file {}/{}: {:?}", i + 1, files.len(), file);
        let file_uri = path_to_uri(file)?;
        let symbol_uri = file_uri.to_string();

        let text_document = TextDocumentItem {
            uri: file_uri.clone(),
            language_id: "".to_string(),
            version: 1,
            text: std::fs::read_to_string(file).unwrap(),
        };

        client
            .notify::<DidOpenTextDocument>(Some(DidOpenTextDocumentParams { text_document }))
            .context("Sending DidOpenTextDocument notification")?;

        let Some(symbols) = client.request::<DocumentSymbolRequest>(Some(
            serde_json::from_value(serde_json::json!({
                "textDocument": {
                    "uri": file_uri.clone(),
                },
            }))?,
        ))?
        else {
            continue;
        };

        let symbols = match symbols {
            DocumentSymbolResponse::Flat(_) => todo!(),
            DocumentSymbolResponse::Nested(vec) => vec,
        };

        for symbol in symbols.iter() {
            nodes.insert(symbol_uri.clone());

            let Some(references) =
                client.request::<References>(Some(serde_json::from_value(serde_json::json!({
                    "textDocument": {
                        "uri": file_uri.clone(),
                    },
                    "position": {
                        "line": symbol.selection_range.start.line,
                        "character": symbol.selection_range.start.character,
                    },
                    "context": {
                        "includeDeclaration": false,
                    },
                }))?))?
            else {
                continue;
            };

            for reference in references {
                let reference_uri = reference.uri.to_string();
                nodes.insert(reference_uri.clone());

                if reference_uri != symbol_uri {
                    connections.insert(Connection {
                        src: reference_uri,
                        dst: symbol_uri.clone(),
                    });
                }
            }
        }
    }

    // strip root uri from all connections
    let root = root_uri.to_string();
    let connections = connections
        .into_iter()
        .filter_map(|c| {
            if let (Some(src), Some(dst)) = (c.src.strip_prefix(&root), c.dst.strip_prefix(&root)) {
                Some(Connection {
                    src: src.to_string(),
                    dst: dst.to_string(),
                })
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    let nodes = nodes
        .into_iter()
        .filter_map(|n| n.strip_prefix(&root).map(|s| s.to_string()))
        .collect::<Vec<_>>();

    Ok((nodes, connections))
}

fn main() -> Result<()> {
    // get sys args
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 3 {
        eprintln!("Usage: {} <base> <cmd> [args...]", args[0]);
        std::process::exit(1);
    }

    let base = &args[1];
    let suffix = &args[2]
        .split(";")
        .filter(|s| s.len() > 0)
        .collect::<Vec<_>>();
    let ignore = &args[3]
        .split(";")
        .filter(|i| i.len() > 0)
        .collect::<Vec<_>>();
    let cmd = &args[4];
    let args = &args[5..];

    // resolve root uri
    let base_path =
        std::fs::canonicalize(base).with_context(|| format!("Invalid base path: {:?}", base))?;

    eprintln!("Using base path: {}", &base_path.to_str().unwrap());
    eprintln!("Using suffix: {:?}", suffix);
    eprintln!("Using ignore: {:?}", ignore);

    let mut child = start_lsp_server(cmd, args);
    let io = lsp_client::StdIO::new(&mut child);
    let mut client = lsp_client::Client::new(io);

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

    let (nodes, connections) = get_connections(&mut client, &base_path, &suffix, &ignore)
        .context("failed to get connections")?;

    // print graphviz .dot file
    println!("digraph G {{");
    println!("    rankdir=TB;");
    println!("    node [shape=rect];");
    // println!("    nodesep=0.1;");
    // println!("    ranksep=0.1;");
    // println!("    splines=curved;");
    for node in &nodes {
        println!("    \"{}\";", node);
    }
    for connection in connections {
        println!("    \"{}\" -> \"{}\";", connection.src, connection.dst);
    }
    println!("}}");

    Ok(())
}
