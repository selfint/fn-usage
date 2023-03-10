use jsonrpc::client::Client;
use tokio::{
    join,
    sync::mpsc::{UnboundedReceiver, UnboundedSender},
};

macro_rules! jsonrpc_server {
    {
        $msg:expr,
        {
            $(
                --> $a:literal
                <-- $b:literal
            )*
        }
    } => {
        match $msg {
            $($a => $b,)*
            msg => {println!("Got notification: {}", msg); ""}
        }
    }
}

macro_rules! parse_request {
    ({"jsonrpc": $jsonrpc:expr, "method": $method:expr, "id": $id:expr}) => {
        ($method.to_string(), None)
    };
    ({"jsonrpc": $jsonrpc:expr, "method": $method:expr, "params": $params:expr, "id": $id:expr}) => {
        ($method.to_string(), Some($params))
    };
}

async fn fake_jsonrpc_server(
    mut client_rx: UnboundedReceiver<String>,
    server_tx: UnboundedSender<String>,
) {
    while let Some(msg) = client_rx.recv().await {
        let response = jsonrpc_server! {
            msg.replace(':', ": ").replace(',', ", ").as_ref(),
            {
                --> r#"{"jsonrpc": "2.0", "method": "subtract", "params": [42, 23], "id": 0}"#
                <-- r#"{"jsonrpc": "2.0", "result": 19, "id": 0}"#
                --> r#"{"jsonrpc": "2.0", "method": "subtract", "params": [23, 42], "id": 1}"#
                <-- r#"{"jsonrpc": "2.0", "result": -19, "id": 1}"#
                --> r#"{"jsonrpc": "2.0", "method": "subtract", "params": {"subtrahend": 23, "minuend": 42}, "id": 2}"#
                <-- r#"{"jsonrpc": "2.0", "result": 19, "id": 2}"#
                --> r#"{"jsonrpc": "2.0", "method": "subtract", "params": {"subtrahend": 23, "minuend": 42}, "id": 3}"#
                <-- r#"{"jsonrpc": "2.0", "result": 19, "id": 3}"#
                --> r#"{"jsonrpc": "2.0", "method": "foobar", "id": 4}"#
                <-- r#"{"jsonrpc": "2.0", "error": {"code": -32601, "message": "Method not found"}, "id": 4}"#
                --> r#"{"jsonrpc": "2.0", "method": 1, "params": "bar"}"#
                <-- r#"{"jsonrpc": "2.0", "error": {"code": -32600, "message": "Invalid Request"}, "id": null}"#
            }
        };

        server_tx
            .send(response.to_string())
            .expect("failed to send response");
    }
}

#[tokio::test]
async fn test_client() {
    let (client_tx, client_rx) = tokio::sync::mpsc::unbounded_channel();
    let (server_tx, server_rx) = tokio::sync::mpsc::unbounded_channel();

    let server_handle = tokio::spawn(fake_jsonrpc_server(client_rx, server_tx));

    let client = Client::new(client_tx, server_rx);

    macro_rules! test_request {
        (P: $params:ty, R: $result:ty, E: $error:ty, $($request:tt)*) => {
            {
            let (method, params) = parse_request!($($request)*);
            client
                .request::<$params, $result, $error>(method, params)
            }
        };
    }

    #[derive(serde::Serialize, serde::Deserialize, Debug)]
    struct Params {
        subtrahend: i64,
        minuend: i64,
    }

    let f1 = test_request!(P: _, R: i64, E: (), {"jsonrpc": "2.0", "method": "subtract", "params": [42, 23], "id": 1});
    let f2 = test_request!(P: _, R: i64, E: (), {"jsonrpc": "2.0", "method": "subtract", "params": [23, 42], "id": 2});
    let f3 = test_request!(P: _, R: i64, E: (), {"jsonrpc": "2.0", "method": "subtract", "params": Params {subtrahend: 23, minuend: 42}, "id": 3});
    let f4 = test_request!(P: _, R: i64, E: (), {"jsonrpc": "2.0", "method": "subtract", "params": Params {minuend: 42, subtrahend: 23}, "id": 4});
    let f5 = test_request!(P: (), R: i64, E: (), {"jsonrpc": "2.0", "method": "foobar", "id": 5});

    let (f1, f2, f3, f4, f5) = join!(f1, f2, f3, f4, f5);
    let mut results = [
        serde_json::to_string(&f1.unwrap()).unwrap(),
        serde_json::to_string(&f2.unwrap()).unwrap(),
        serde_json::to_string(&f3.unwrap()).unwrap(),
        serde_json::to_string(&f4.unwrap()).unwrap(),
        serde_json::to_string(&f5.unwrap()).unwrap(),
    ];

    results.sort();

    insta::assert_debug_snapshot!(results,
        @r###"
    [
        "{\"jsonrpc\":\"2.0\",\"error\":{\"code\":-32601,\"message\":\"Method not found\",\"data\":null},\"id\":4}",
        "{\"jsonrpc\":\"2.0\",\"result\":-19,\"id\":1}",
        "{\"jsonrpc\":\"2.0\",\"result\":19,\"id\":0}",
        "{\"jsonrpc\":\"2.0\",\"result\":19,\"id\":2}",
        "{\"jsonrpc\":\"2.0\",\"result\":19,\"id\":3}",
    ]
    "###
    );

    server_handle.abort();
}
