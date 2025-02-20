use std::collections::HashSet;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::str::FromStr;

use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use lsp_types::{SymbolKind, Uri};
use serde_json::json;

use lsp_client::Client;

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();

    if args.len() < 3 {
        eprintln!("Usage: {} <root-uri> <lsp-cmd> [lsp-cmd-args...]", args[0]);
        std::process::exit(1);
    }

    let (root, cmd, args) = (&args[1], &args[2], &args[3..]);

    // read all lines from stdin
    let project_files: HashSet<_> = std::io::stdin()
        .lock()
        .lines()
        .filter_map(Result::ok)
        .filter_map(|line| Uri::from_str(&format!("{}/{}", root, line)).ok())
        .collect();

    let root = Uri::from_str(&root)?;

    eprintln!("     \x1b[1;32mRunning\x1b[0m `{} {}`", cmd, args.join(" "));
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

    eprintln!("    \x1b[1;32mIndexing\x1b[0m {}", root.as_str());
    for uri in &project_files {
        client.open(&uri, &std::fs::read_to_string(uri.path().as_str())?)?;
    }

    eprintln!("     \x1b[1;32mWaiting\x1b[0m 3 seconds for LSP to index code...");
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

    let bar = ProgressBar::new(project_files.len() as u64).with_style(
        ProgressStyle::with_template(
            "    \x1b[1;36mScanning\x1b[0m [{bar:27}] ({eta}) {pos}/{len}: {wide_msg:!}",
        )
        .unwrap()
        .progress_chars("=> "),
    );

    for file in &project_files {
        let node = file.as_str().strip_prefix(root.as_str()).unwrap();
        nodes.insert(node);

        bar.println(format!("     \x1b[1;32mScanned\x1b[0m {}", node));
        bar.inc(1);

        for symbol in &client.symbols(file)? {
            if !symbol_mask.contains(&symbol.kind) {
                continue;
            }

            bar.set_message(format!("{:?} {}", symbol.kind, symbol.name));

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

    bar.finish();

    let graph = json!({
        "nodes": nodes,
        "edges": edges,
    });

    println!("{}", serde_json::to_string_pretty(&graph)?);

    Ok(())
}
