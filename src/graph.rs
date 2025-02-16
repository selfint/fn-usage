use std::collections::HashSet;

use anyhow::Result;
use lsp_types::{SymbolKind, Url};
use serde_json::{json, Value};

use crate::Client;

pub fn build_graph(client: &mut Client, root: &Url, uris: &[Url]) -> Result<Value> {
    let mut nodes: HashSet<&str> = HashSet::new();
    let mut edges: HashSet<(String, &str)> = HashSet::new();

    for uri in uris {
        client.open(&uri, &std::fs::read_to_string(uri.path())?)?;
    }

    eprintln!("Waiting 3 seconds for LSP to index code...");
    std::thread::sleep(std::time::Duration::from_secs(3));

    for uri in uris {
        // ignore uri not under root
        let Some(node) = uri.as_str().strip_prefix(root.as_str()) else {
            continue;
        };

        nodes.insert(node);

        let ignore = [SymbolKind::VARIABLE];

        for symbol in client.symbols(&uri)? {
            if ignore.contains(&symbol.kind) {
                continue;
            }

            // ignore symbols defined outside of project root
            if !client
                .goto_definition(&uri, &symbol)?
                .iter()
                .any(|d| d.as_str().starts_with(root.as_str()))
            {
                continue;
            }

            for reference in client.references(&uri, &symbol)? {
                // ignore references outside of project files
                if reference == *uri || !uris.contains(&reference) {
                    continue;
                }

                let reference_node = reference.as_str().strip_prefix(root.as_str()).unwrap();
                eprintln!("Found reference: {} -> {}", reference_node, node);

                edges.insert((reference_node.to_string(), node));
            }
        }
    }

    Ok(json!({
        "root": root,
        "nodes": nodes,
        "edges": edges
    }))
}
