use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::str::FromStr;

use anyhow::{Context, Result};

use lsp_client::types::{notification::*, request::*, *};
use lsp_client::StdIO;
use serde_json::json;
use std::io::{BufRead, BufReader};

#[derive(Debug, PartialEq, Eq, Hash)]
struct Connection {
    pub src: Url,
    pub dst: Url,
}

fn main() -> Result<()> {
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

    let base_path =
        std::fs::canonicalize(base).context(format!("Invalid base path: {:?}", base))?;

    eprintln!("Using base path: {}", &base_path.to_str().unwrap());
    eprintln!("Using suffix: {:?}", suffix);
    eprintln!("Using ignore: {:?}", ignore);

    let mut child = start_lsp_server(cmd, args);
    let io = lsp_client::StdIO::new(&mut child);
    let mut client = lsp_client::Client::new(io, false);

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

    let root_uri = path_to_uri(&base_path)?;
    let files = get_files(&base_path, &suffix, &ignore)?;
    let (nodes, connections) =
        get_connections(&mut client, &root_uri, &files).context("failed to get connections")?;

    print_graph(&root_uri, nodes, connections);

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

fn path_to_uri(path: &PathBuf) -> Result<Url> {
    let uri = "file://".to_string()
        + path
            .to_str()
            .context(format!("converting {:?} to str", path))?;

    Url::from_str(&uri).context(format!("parsing uri {:?}", uri))
}

fn get_files(base_path: &PathBuf, suffixes: &[&str], ignore: &[&str]) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut dirs = vec![base_path.clone()];

    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                dirs.push(path);
            } else if path.is_file() {
                let path_str = path.to_str().unwrap_or_default();
                if !ignore.iter().any(|i| path_str.contains(i))
                    && suffixes
                        .iter()
                        .any(|s| path.extension().map_or(false, |ext| ext == *s))
                {
                    eprintln!("Found file {}: {:?}", files.len() + 1, path);

                    files.push(path);
                }
            }
        }
    }

    Ok(files)
}

fn get_connections(
    client: &mut lsp_client::Client<StdIO>,
    root_uri: &Url,
    files: &[PathBuf],
) -> Result<(HashSet<Url>, HashSet<Connection>)> {
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

    client.notify::<Initialized>(Some(InitializedParams {}))?;

    // give server time to index project
    eprintln!("Waiting 3 seconds for LSP to index code...");
    std::thread::sleep(std::time::Duration::from_secs(3));

    // get all files with suffixes in base path recursively
    let mut nodes: HashSet<Url> = HashSet::new();
    let mut connections: HashSet<Connection> = HashSet::new();

    for (i, file) in files.iter().enumerate() {
        eprintln!("Processing file {}/{}: {:?}", i + 1, files.len(), file);

        let file_uri = path_to_uri(file)?;
        nodes.insert(file_uri.clone());

        client.notify::<DidOpenTextDocument>(serde_json::from_value(json!({
            "textDocument": {
            "uri": file_uri.clone(),
            "languageId": "",
            "version": 1,
            "text": std::fs::read_to_string(file)?,
            }
        }))?)?;

        let symbols = client.request::<DocumentSymbolRequest>(serde_json::from_value(json!({
            "textDocument": {
                "uri": file_uri.clone(),
            },
        }))?)?;

        let symbols = match symbols {
            Some(DocumentSymbolResponse::Flat(flat)) => {
                if flat.len() > 0 {
                    panic!("Got non-empty flat documentSymbol response")
                } else {
                    vec![]
                }
            }
            Some(DocumentSymbolResponse::Nested(vec)) => vec,
            None => continue,
        };

        for symbol in symbols.iter() {
            let Some(references) =
                client.request::<References>(serde_json::from_value(json!({
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
                }))?)?
            else {
                continue;
            };

            for reference in references {
                nodes.insert(reference.uri.clone());

                if reference.uri != file_uri {
                    connections.insert(Connection {
                        src: reference.uri,
                        dst: file_uri.clone(),
                    });
                }
            }
        }
    }

    Ok((nodes, connections))
}

fn build_node_entry(node: &Url, root_uri: &Url) -> String {
    let node_str = match node.as_str().strip_prefix(root_uri.as_str()) {
        Some(stripped) => stripped.trim_start_matches('/'),
        None => return String::new(),
    };

    let mut parts: Vec<_> = node_str.split('/').collect();
    parts.insert(0, root_uri.as_str());

    let mut result = String::new();

    for i in 0..parts.len() - 1 {
        let label = parts[i];
        let cluster = (&parts[..i + 1]).join("/");
        let indent = "\t".repeat(i + 1);

        result.push_str(&format!(
            "{}subgraph \"cluster_{}\" {{\n\t{}label = \"{}\";\n",
            indent, cluster, indent, label
        ));
    }

    let label = parts.last().unwrap();
    result.push_str(&format!(
        "{}\"{}\" [ label = \"{}\" ];\n",
        "\t".repeat(parts.len()),
        node.as_str(),
        label
    ));

    for i in (0..parts.len() - 1).rev() {
        result.push_str(&format!("{}}}\n", "\t".repeat(i + 1)));
    }

    result
}

fn print_graph(root_uri: &Url, nodes: HashSet<Url>, connections: HashSet<Connection>) {
    println!("digraph G {{");
    println!("\trankdir=TB;");
    println!("\tnode [shape=rect];");
    println!("\tcompound=true;");

    for node in &nodes {
        let node_entry = build_node_entry(node, &root_uri);
        print!("{}", node_entry);
    }

    let mut unique_connections = HashMap::<(String, String), String>::new();

    for Connection { src, dst } in connections {
        let src_parts: Vec<&str> = src.as_str().split('/').collect();
        let dst_parts: Vec<&str> = dst.as_str().split('/').collect();
        let mut common_parts = vec![];

        for (s, d) in src_parts.iter().zip(dst_parts.iter()) {
            if s == d {
                common_parts.push(*s);
            } else {
                break;
            }
        }

        let common_end = common_parts.len();
        let tail = src_parts[..common_end + 1].join("/");
        let head = dst_parts[..common_end + 1].join("/");

        let src = src.to_string();
        let dst = dst.to_string();

        let (value, key) = match [
            src_parts.len() == common_end + 1,
            dst_parts.len() == common_end + 1,
        ] {
            [true, true] => (format!("\t\"{}\" -> \"{}\";", src, dst), (src, dst)),
            [true, false] => (
                format!(
                    "\t\"{}\" -> \"{}\" [minlen=2 lhead=\"cluster_{}\"];",
                    src, dst, head
                ),
                (src, head),
            ),
            [false, true] => (
                format!(
                    "\t\"{}\" -> \"{}\" [minlen=2 ltail=\"cluster_{}\"];",
                    src, dst, tail
                ),
                (tail, dst),
            ),
            [false, false] => (
                format!(
                    "\t\"{}\" -> \"{}\" [minlen=2 ltail=\"cluster_{}\" lhead=\"cluster_{}\"];",
                    src, dst, tail, head
                ),
                (tail, head),
            ),
        };

        unique_connections.insert(key, value);
    }

    // dedup connections
    for connection in unique_connections.values() {
        println!("{}", connection);
    }

    println!("}}");
}
