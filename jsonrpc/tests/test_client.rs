use jsonrpc::{client::Client, types::Request};
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

macro_rules! build_request {
    ({"jsonrpc": $jsonrpc:expr, "method": $method:expr, "id": $id:expr}) => {
        Request {
            jsonrpc: $jsonrpc.to_string(),
            method: $method.to_string(),
            params: None,
            id: $id,
        }
    };
    ({"jsonrpc": $jsonrpc:expr, "method": $method:expr, "params": $params:expr, "id": $id:expr}) => {
        Request {
            jsonrpc: $jsonrpc.to_string(),
            method: $method.to_string(),
            params: Some($params),
            id: $id,
        }
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
                --> r#"{"jsonrpc": "2.0", "method": "subtract", "params": [42, 23], "id": 1}"#
                <-- r#"{"jsonrpc": "2.0", "result": 19, "id": 1}"#
                --> r#"{"jsonrpc": "2.0", "method": "subtract", "params": [23, 42], "id": 2}"#
                <-- r#"{"jsonrpc": "2.0", "result": -19, "id": 2}"#
                --> r#"{"jsonrpc": "2.0", "method": "subtract", "params": {"subtrahend": 23, "minuend": 42}, "id": 3}"#
                <-- r#"{"jsonrpc": "2.0", "result": 19, "id": 3}"#
                --> r#"{"jsonrpc": "2.0", "method": "subtract", "params": {"subtrahend": 23, "minuend": 42}, "id": 4}"#
                <-- r#"{"jsonrpc": "2.0", "result": 19, "id": 4}"#
                --> r#"{"jsonrpc": "2.0", "method": "foobar", "id": 5}"#
                <-- r#"{"jsonrpc": "2.0", "error": {"code": -32601, "message": "Method not found"}, "id": 5}"#
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
    let (client_tx, client_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let (server_tx, server_rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    let server_handle = tokio::spawn(fake_jsonrpc_server(client_rx, server_tx));

    let client = Client::new(client_tx, server_rx);

    macro_rules! test_request {
        ($params:ty, $result:ty, $error:ty, $($request:tt)*) => {
            client
                .request::<$params, $result, $error>(build_request!($($request)*))
        };
        ($result:ty, $error:ty, $($request:tt)*) => {
            client
                .request::<_, $result, $error>(build_request!($($request)*))
        };
    }

    #[derive(serde::Serialize, serde::Deserialize, Debug)]
    struct Params {
        subtrahend: i64,
        minuend: i64,
    }

    let f1 = test_request!(i64, (), {"jsonrpc": "2.0", "method": "subtract", "params": [42, 23], "id": 1});
    let f2 = test_request!(i64, (), {"jsonrpc": "2.0", "method": "subtract", "params": [23, 42], "id": 2});
    let f3 = test_request!(i64, (), {"jsonrpc": "2.0", "method": "subtract", "params": Params {subtrahend: 23, minuend: 42}, "id": 3});
    let f4 = test_request!(i64, (), {"jsonrpc": "2.0", "method": "subtract", "params": Params {minuend: 42, subtrahend: 23}, "id": 4});
    let f5 = test_request!((), i64, (), {"jsonrpc": "2.0", "method": "foobar", "id": 5});

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
        "{\"jsonrpc\":\"2.0\",\"error\":{\"code\":-32601,\"message\":\"Method not found\",\"data\":null},\"id\":5}",
        "{\"jsonrpc\":\"2.0\",\"result\":-19,\"id\":2}",
        "{\"jsonrpc\":\"2.0\",\"result\":19,\"id\":1}",
        "{\"jsonrpc\":\"2.0\",\"result\":19,\"id\":3}",
        "{\"jsonrpc\":\"2.0\",\"result\":19,\"id\":4}",
    ]
    "###
    );

    server_handle.abort();
}
