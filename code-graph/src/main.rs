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

    let initialize_params = InitializeParams {
        capabilities: ClientCapabilities {
            workspace: Some(WorkspaceClientCapabilities {
                workspace_folders: Some(false),
                semantic_tokens: Some(SemanticTokensWorkspaceClientCapabilities {
                    refresh_support: Some(true),
                }),
                symbol: Some(WorkspaceSymbolClientCapabilities {
                    dynamic_registration: Some(false),
                    resolve_support: Some(WorkspaceSymbolResolveSupportCapability {
                        properties: vec!["location.range".to_string()],
                    }),
                    tag_support: None,
                    symbol_kind: Some(SymbolKindCapability {
                        value_set: Some(vec![
                            SymbolKind::FILE,
                            SymbolKind::MODULE,
                            SymbolKind::NAMESPACE,
                            SymbolKind::PACKAGE,
                            SymbolKind::CLASS,
                            SymbolKind::METHOD,
                            SymbolKind::PROPERTY,
                            SymbolKind::FIELD,
                            SymbolKind::CONSTRUCTOR,
                            SymbolKind::ENUM,
                            SymbolKind::INTERFACE,
                            SymbolKind::FUNCTION,
                            SymbolKind::VARIABLE,
                            SymbolKind::CONSTANT,
                            SymbolKind::STRING,
                            SymbolKind::NUMBER,
                            SymbolKind::BOOLEAN,
                            SymbolKind::ARRAY,
                            SymbolKind::OBJECT,
                            SymbolKind::KEY,
                            SymbolKind::NULL,
                            SymbolKind::ENUM_MEMBER,
                            SymbolKind::STRUCT,
                            SymbolKind::EVENT,
                            SymbolKind::OPERATOR,
                            SymbolKind::TYPE_PARAMETER,
                        ]),
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            text_document: Some(TextDocumentClientCapabilities {
                references: Some(DynamicRegistrationClientCapabilities {
                    dynamic_registration: Some(false),
                }),
                definition: Some(GotoCapability {
                    dynamic_registration: Some(false),
                    link_support: Some(true),
                }),
                document_symbol: Some(DocumentSymbolClientCapabilities {
                    hierarchical_document_symbol_support: Some(true),
                    symbol_kind: Some(SymbolKindCapability {
                        value_set: Some(vec![
                            SymbolKind::FILE,
                            SymbolKind::MODULE,
                            SymbolKind::NAMESPACE,
                            SymbolKind::PACKAGE,
                            SymbolKind::CLASS,
                            SymbolKind::METHOD,
                            SymbolKind::PROPERTY,
                            SymbolKind::FIELD,
                            SymbolKind::CONSTRUCTOR,
                            SymbolKind::ENUM,
                            SymbolKind::INTERFACE,
                            SymbolKind::FUNCTION,
                            SymbolKind::VARIABLE,
                            SymbolKind::CONSTANT,
                            SymbolKind::STRING,
                            SymbolKind::NUMBER,
                            SymbolKind::BOOLEAN,
                            SymbolKind::ARRAY,
                            SymbolKind::OBJECT,
                            SymbolKind::KEY,
                            SymbolKind::NULL,
                            SymbolKind::ENUM_MEMBER,
                            SymbolKind::STRUCT,
                            SymbolKind::EVENT,
                            SymbolKind::OPERATOR,
                            SymbolKind::TYPE_PARAMETER,
                        ]),
                    }),
                    ..Default::default()
                }),
                document_link: Some(DocumentLinkClientCapabilities {
                    dynamic_registration: Some(false),
                    tooltip_support: Some(true),
                }),
                type_definition: Some(GotoCapability {
                    dynamic_registration: Some(false),
                    link_support: Some(true),
                }),
                selection_range: Some(SelectionRangeClientCapabilities {
                    dynamic_registration: Some(false),
                }),
                call_hierarchy: Some(DynamicRegistrationClientCapabilities {
                    dynamic_registration: Some(false),
                }),
                ..Default::default()
            }),
            ..Default::default()
        },
        // notify lsp we opened workspace folder
        workspace_folders: Some(vec![WorkspaceFolder {
            uri: root_uri.clone(),
            name: base_path
                .clone()
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .to_string(),
        }]),
        ..Default::default()
    };

    let initialize = client.request::<Initialize, InitializeError>(Some(initialize_params))??;

    initialize
        .capabilities
        .document_symbol_provider
        .ok_or_else(|| anyhow::anyhow!("Server is not 'document_symbol_provider'"))?;

    let supports_references = initialize.capabilities.references_provider.is_some();
    let supports_definition = initialize.capabilities.definition_provider.is_some();
    let supports_call_hierarchy = initialize.capabilities.call_hierarchy_provider.is_some();

    if !(supports_references || supports_definition || supports_call_hierarchy) {
        anyhow::bail!("Server does not support references, definition or call hierarchy");
    }

    client
        .notify::<Initialized>(Some(InitializedParams {}))
        .context("Sending Initialized notification")?;

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

    let mut connections: Vec<Connection> = vec![];

    for (i, file) in files.iter().enumerate() {
        eprintln!("Processing file {}/{}: {:?}", i + 1, files.len(), file);
        let file_uri = path_to_uri(file)?;

        let text_document = TextDocumentItem {
            uri: file_uri.clone(),
            language_id: "".to_string(),
            version: 1,
            text: std::fs::read_to_string(file).unwrap(),
        };

        client
            .notify::<DidOpenTextDocument>(Some(DidOpenTextDocumentParams { text_document }))
            .context("Sending DidOpenTextDocument notification")?;

        // sleep 3 seconds
        std::thread::sleep(std::time::Duration::from_secs(3));

        let Some(symbols) =
            client.request::<DocumentSymbolRequest, ()>(Some(DocumentSymbolParams {
                text_document: TextDocumentIdentifier {
                    uri: file_uri.clone(),
                },
                work_done_progress_params: WorkDoneProgressParams {
                    work_done_token: None,
                },
                partial_result_params: PartialResultParams {
                    partial_result_token: None,
                },
            }))??
        else {
            continue;
        };

        let symbols = match symbols {
            DocumentSymbolResponse::Flat(_) => todo!(),
            DocumentSymbolResponse::Nested(vec) => vec,
        };

        for (j, symbol) in symbols.iter().enumerate() {
            eprintln!(
                "\tProcessing symbol {}/{}: {:?} {:?} at {}:{}:{}",
                j + 1,
                symbols.len(),
                symbol.kind,
                symbol.name,
                file_uri.path(),
                symbol.selection_range.start.line + 1,
                symbol.selection_range.start.character + 1
            );

            let symbol_uri = file_uri.to_string();
            nodes.insert(symbol_uri.clone());

            let position = Position {
                line: symbol.selection_range.start.line,
                character: symbol.selection_range.start.character,
            };

            if supports_references {
                if let Some(references) =
                    client.request::<References, ()>(Some(ReferenceParams {
                        text_document_position: TextDocumentPositionParams {
                            text_document: TextDocumentIdentifier {
                                uri: file_uri.clone(),
                            },
                            position,
                        },
                        work_done_progress_params: WorkDoneProgressParams {
                            work_done_token: None,
                        },
                        partial_result_params: PartialResultParams {
                            partial_result_token: None,
                        },
                        context: ReferenceContext {
                            include_declaration: true,
                        },
                    }))??
                {
                    // get symbol references
                    for reference in references {
                        let reference_uri = reference.uri.to_string();
                        nodes.insert(reference_uri.clone());

                        if reference_uri != symbol_uri {
                            connections.push(Connection {
                                src: reference_uri,
                                dst: symbol_uri.clone(),
                            });
                        }
                    }
                }
            }

            if supports_definition {
                if let Some(definitions) =
                    client.request::<GotoDefinition, ()>(Some(GotoDefinitionParams {
                        text_document_position_params: TextDocumentPositionParams {
                            text_document: TextDocumentIdentifier {
                                uri: file_uri.clone(),
                            },
                            position,
                        },
                        work_done_progress_params: WorkDoneProgressParams {
                            work_done_token: None,
                        },
                        partial_result_params: PartialResultParams {
                            partial_result_token: None,
                        },
                    }))??
                {
                    let definitions = match definitions {
                        GotoDefinitionResponse::Scalar(location) => vec![location.uri],
                        GotoDefinitionResponse::Array(vec) => {
                            vec.into_iter().map(|l| l.uri).collect()
                        }
                        GotoDefinitionResponse::Link(vec) => {
                            vec.into_iter().map(|l| l.target_uri).collect()
                        }
                    };

                    for definition in definitions {
                        let definition_uri = definition.to_string();
                        nodes.insert(definition_uri.clone());

                        if definition_uri != symbol_uri {
                            connections.push(Connection {
                                src: definition_uri,
                                dst: symbol_uri.clone(),
                            });
                        }
                    }
                }
            }

            if supports_call_hierarchy {
                if let Some(call_hierarchy_items) = client.request::<CallHierarchyPrepare, ()>(
                    Some(CallHierarchyPrepareParams {
                        text_document_position_params: TextDocumentPositionParams {
                            text_document: TextDocumentIdentifier {
                                uri: file_uri.clone(),
                            },
                            position,
                        },
                        work_done_progress_params: WorkDoneProgressParams {
                            work_done_token: None,
                        },
                    }),
                )?? {
                    for item in call_hierarchy_items {
                        if let Some(incoming_calls) = client
                            .request::<CallHierarchyIncomingCalls, ()>(Some(
                                CallHierarchyIncomingCallsParams {
                                    item: item.clone(),
                                    partial_result_params: PartialResultParams {
                                        partial_result_token: None,
                                    },
                                    work_done_progress_params: WorkDoneProgressParams {
                                        work_done_token: None,
                                    },
                                },
                            ))??
                        {
                            for call in incoming_calls {
                                let caller_uri = call.from.uri.to_string();
                                nodes.insert(caller_uri.clone());

                                if caller_uri != symbol_uri {
                                    connections.push(Connection {
                                        src: caller_uri.clone(),
                                        dst: symbol_uri.clone(),
                                    });
                                }
                            }
                        }

                        // if let Some(outgoing_calls) = client
                        //     .request::<CallHierarchyOutgoingCalls, ()>(Some(
                        //         CallHierarchyOutgoingCallsParams {
                        //             item,
                        //             partial_result_params: PartialResultParams {
                        //                 partial_result_token: None,
                        //             },
                        //             work_done_progress_params: WorkDoneProgressParams {
                        //                 work_done_token: None,
                        //             },
                        //         },
                        //     ))??
                        // {
                        //     for call in outgoing_calls {
                        //         let target_uri = call.to.uri.to_string();
                        //         nodes.insert(target_uri.clone());

                        //         if target_uri != symbol_uri {
                        //             connections.push(Connection {
                        //                 src: symbol_uri.clone(),
                        //                 dst: target_uri,
                        //             });
                        //         }
                        //     }
                        // }
                    }
                }
            }
        }

        client
            .notify::<DidCloseTextDocument>(Some(DidCloseTextDocumentParams {
                text_document: TextDocumentIdentifier { uri: file_uri },
            }))
            .context("Sending DidCloseTextDocument notification")?;
    }

    let dedup_connections = connections
        .iter()
        .map(|c| (c.src.clone(), c.dst.clone()))
        .collect::<std::collections::HashSet<_>>()
        .iter()
        .map(|(src, dst)| Connection {
            src: src.clone(),
            dst: dst.clone(),
        })
        .collect::<Vec<_>>();

    // strip root uri from all connections
    let stripped_connections = dedup_connections
        .into_iter()
        // keep only src/dst that start with root_uri
        .filter(|c| {
            c.src.starts_with(&root_uri.to_string()) && c.dst.starts_with(&root_uri.to_string())
        })
        .map(|c| Connection {
            src: c
                .src
                .strip_prefix(&root_uri.to_string())
                .unwrap_or(&c.src)
                .to_string(),
            dst: c
                .dst
                .strip_prefix(&root_uri.to_string())
                .unwrap_or(&c.dst)
                .to_string(),
        })
        .collect::<Vec<_>>();

    let stripped_nodes = nodes
        .iter()
        .filter(|n| n.starts_with(&root_uri.to_string()))
        .map(|n| {
            n.strip_prefix(&root_uri.to_string())
                .unwrap_or(n)
                .to_string()
        })
        .collect::<Vec<_>>();

    Ok((stripped_nodes, stripped_connections))
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
    println!("    rankdir=LR;");
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
