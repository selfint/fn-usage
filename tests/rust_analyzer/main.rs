use std::io::BufReader;
use std::process::{Command, Stdio};
use std::str::FromStr;

use lsp_client::Client;
use lsp_types::notification::DidOpenTextDocument;
use lsp_types::request::DocumentSymbolRequest;
use lsp_types::{
    DidOpenTextDocumentParams, DocumentSymbolParams, PartialResultParams, TextDocumentIdentifier,
    TextDocumentItem, Uri, WorkDoneProgressParams,
};

#[test]
fn test_rust_analyzer() {
    let mut child = Command::new("rust-analyzer")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to start rust analyzer");

    let input = BufReader::new(child.stdout.take().expect("Failed to take stdout"));
    let output = child.stdin.take().expect("Failed to take stdin");

    let mut client = Client::new(Box::new(input), Box::new(output));

    let init_resp = client.initialize(Uri::from_str("file:///").unwrap());

    assert!(init_resp.is_ok());

    client
        .notify::<DidOpenTextDocument>(Some(DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: Uri::from_str("file:///src/main.rs").unwrap(),
                language_id: "rust".to_string(),
                version: 1,
                text: "fn main() { if true { let a = 1; }}".to_string(),
            },
        }))
        .expect("failed to open file");

    let symbols = client
        .request::<DocumentSymbolRequest>(Some(DocumentSymbolParams {
            text_document: TextDocumentIdentifier {
                uri: Uri::from_str("file:///src/main.rs").unwrap(),
            },
            work_done_progress_params: WorkDoneProgressParams {
                work_done_token: None,
            },
            partial_result_params: PartialResultParams {
                partial_result_token: None,
            },
        }))
        .expect("failed to get symbols");

    insta::assert_json_snapshot!(symbols, @r#"
    [
      {
        "name": "main",
        "detail": "fn()",
        "kind": 12,
        "tags": [],
        "deprecated": false,
        "range": {
          "start": {
            "line": 0,
            "character": 0
          },
          "end": {
            "line": 0,
            "character": 35
          }
        },
        "selectionRange": {
          "start": {
            "line": 0,
            "character": 3
          },
          "end": {
            "line": 0,
            "character": 7
          }
        },
        "children": [
          {
            "name": "a",
            "kind": 13,
            "tags": [],
            "deprecated": false,
            "range": {
              "start": {
                "line": 0,
                "character": 22
              },
              "end": {
                "line": 0,
                "character": 32
              }
            },
            "selectionRange": {
              "start": {
                "line": 0,
                "character": 26
              },
              "end": {
                "line": 0,
                "character": 27
              }
            }
          }
        ]
      }
    ]
    "#);
}
