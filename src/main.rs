use std::collections::HashSet;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::str::FromStr;

use anyhow::Result;
use lsp_types::{SymbolKind, Url};

use lsp_client::Client;
use serde_json::json;

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();

    if args.len() < 3 {
        eprintln!("Usage: {} <root-uri> <lsp-cmd> [lsp-cmd-args...]", args[0]);
        std::process::exit(1);
    }

    let (root, cmd, args) = (&args[1], &args[2], &args[3..]);

    let root = Url::from_str(&root)?;

    // read all lines from stdin
    let project_files: HashSet<Url> = std::io::stdin()
        .lock()
        .lines()
        .filter_map(Result::ok)
        .filter_map(|line| root.join(&line).ok())
        .collect();

    let mut child = Command::new(cmd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

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

    let input = BufReader::new(child.stdout.take().expect("Failed to take stdout"));
    let output = child.stdin.take().expect("Failed to take stdin");

    let mut client = Client::new(Box::new(input), Box::new(output));

    let capabilities = client.initialize(root.clone())?;

    if capabilities.document_symbol_provider.is_none() {
        panic!("Server is not 'textDocument/documentSymbol' provider");
    }

    if capabilities.references_provider.is_none() {
        panic!("Server is not 'textDocument/reference' provider");
    }

    if capabilities.definition_provider.is_none() {
        panic!("Server is not 'textDocument/definition' provider");
    }

    for uri in &project_files {
        eprintln!("Opening {}", uri.as_str());
        client.open(&uri, &std::fs::read_to_string(uri.path())?)?;
    }

    eprintln!("Waiting 3 seconds for LSP to index code...");
    std::thread::sleep(std::time::Duration::from_secs(3));

    let mut nodes = HashSet::new();
    let mut edges = HashSet::new();

    // only use these kinds of symbols
    let symbol_mask = [
        SymbolKind::FUNCTION,
        SymbolKind::STRUCT,
        SymbolKind::CLASS,
        SymbolKind::METHOD,
    ];

    for file in &project_files {
        let node = file.as_str().strip_prefix(root.as_str()).unwrap();
        nodes.insert(node);

        for symbol in &client.symbols(file)? {
            if !symbol_mask.contains(&symbol.kind) {
                continue;
            }

            // ignore symbols defined outside of current file
            if !client.definitions(file, symbol)?.iter().any(|d| d == file) {
                continue;
            }

            for reference in &client.references(file, symbol)? {
                // ignore references outside of project files
                let Some(reference) = project_files.get(reference) else {
                    continue;
                };

                let reference = reference.as_str().strip_prefix(root.as_str()).unwrap();

                edges.insert((reference, node));
            }
        }
    }

    let graph = json!({
        "nodes": nodes,
        "edges": edges,
    });

    println!("{}", serde_json::to_string_pretty(&graph)?);

    Ok(())
}
