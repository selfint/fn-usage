use std::collections::HashSet;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::str::FromStr;

use anyhow::Result;
use lsp_types::{SymbolKind, Url};
use serde_json::json;

use lsp_client::{Client, StdIO};

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();

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

    let mut child = Command::new(cmd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let mut client = Client::new(StdIO::new(&mut child)?);

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

    let capabilities = client.initialize(root.clone())?;

    if capabilities.document_symbol_provider.is_none() {
        anyhow::bail!("Server is not 'textDocument/documentSymbol' provider");
    }

    if capabilities.references_provider.is_none() {
        anyhow::bail!("Server is not 'textDocument/reference' provider");
    }

    let mut edges: HashSet<(String, String)> = HashSet::new();

    for (i, uri) in uris.iter().enumerate() {
        eprintln!(
            "Loading uri ({:>4}/{:>4}): {}",
            i + 1,
            uris.len(),
            uri.as_str()
        );

        let text = std::fs::read_to_string(uri.path())?;
        client.open(uri.clone(), &text)?;
    }

    eprintln!("Waiting 3 seconds for LSP to index code...");
    std::thread::sleep(std::time::Duration::from_secs(3));

    for (i, uri) in uris.iter().enumerate() {
        eprintln!(
            "Scanning uri ({:>4}/{:>4}): {}",
            i + 1,
            uris.len(),
            uri.as_str()
        );

        // ignore uri not under root
        let Some(symbol_node) = uri.as_str().strip_prefix(root.as_str()) else {
            continue;
        };

        let symbols = client.symbols(
            uri.clone(),
            &[
                SymbolKind::FUNCTION,
                SymbolKind::STRUCT,
                SymbolKind::CLASS,
                SymbolKind::METHOD,
            ],
        )?;

        for (j, symbol) in symbols.iter().enumerate() {
            eprintln!(
                "Searching symbol ({:>4}/{:>4}): {:?} {}",
                j + 1,
                symbols.len(),
                symbol.kind,
                symbol.name,
            );

            let definitions = client.goto_definition(uri.clone(), symbol)?;
            if !definitions
                .iter()
                .any(|d| d.as_str().starts_with(root.as_str()))
            {
                eprintln!(
                    "Ignoring symbol outside of root, defined at: {:?}",
                    definitions.iter().map(|d| d.as_str()).collect::<Vec<_>>()
                );

                continue;
            }

            for reference in client.references(uri.clone(), symbol)? {
                // ignore symbols defined outside of project root
                if reference != *uri && uris.contains(&reference) {
                    let reference_node = reference.as_str().strip_prefix(root.as_str()).unwrap();
                    eprintln!("Found reference: {} -> {}", reference_node, symbol_node);

                    edges.insert((reference_node.to_string(), symbol_node.to_string()));
                }
            }
        }
    }

    let nodes: Vec<_> = uris
        .iter()
        .filter_map(|u| u.as_str().strip_prefix(root.as_str()))
        .collect();

    println!(
        "{}",
        json!({
            "root": root,
            "nodes": nodes,
            "edges": edges
        })
    );

    Ok(())
}
