use jsonrpc::types::{JsonRpcResult, Response};
use jsonrpc::{client::Client, types::Request};
use std::sync::mpsc;
use tokio::join;
use tokio::sync::{oneshot, watch};

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

fn fake_jsonrpc_server(
    client_rx: mpsc::Receiver<String>,
    server_tx: watch::Sender<String>,
    mut kill_server_rx: oneshot::Receiver<()>,
) {
    while kill_server_rx.try_recv().is_err() {
        if let Ok(msg) = client_rx.try_recv() {
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
}

#[tokio::test]
async fn test_server() {
    let (client_tx, client_rx) = std::sync::mpsc::channel::<String>();
    let (server_tx, server_rx) = watch::channel::<String>("hello".to_string());

    let (kill_server_tx, kill_server_rx) = oneshot::channel::<()>();
    let server_handle =
        std::thread::spawn(move || fake_jsonrpc_server(client_rx, server_tx, kill_server_rx));

    let client = Client::new(client_tx, server_rx);

    macro_rules! test_request {
        ($params:ty, $result:ty, $error:ty, $($request:tt)*) => {
            insta::assert_debug_snapshot!(
                client
                    .request::<$params, $result, $error>(build_request!($($request)*))
                    .await
            );
        };
        ($result:ty, $error:ty, $($request:tt)*) => {
            insta::assert_debug_snapshot!(
                client
                    .request::<_, $result, $error>(build_request!($($request)*))
                    .await
            );
        };
    }

    #[derive(serde::Serialize, serde::Deserialize, Debug)]
    struct Params {
        subtrahend: i64,
        minuend: i64,
    }

    test_request!(i64, (), {"jsonrpc": "2.0", "method": "subtract", "params": [42, 23], "id": 1});
    test_request!(i64, (), {"jsonrpc": "2.0", "method": "subtract", "params": [42, 23], "id": 1});
    test_request!(i64, (), {"jsonrpc": "2.0", "method": "subtract", "params": [23, 42], "id": 2});
    test_request!(i64, (), {"jsonrpc": "2.0", "method": "subtract", "params": Params {subtrahend: 23, minuend: 42}, "id": 3});
    test_request!(i64, (), {"jsonrpc": "2.0", "method": "subtract", "params": Params {minuend: 42, subtrahend: 23}, "id": 4});
    test_request!((), i64, (), {"jsonrpc": "2.0", "method": "foobar", "id": 5});

    kill_server_tx
        .send(())
        .expect("failed to send kill signal to server");
    server_handle.join().expect("failed to join server");
}

#[tokio::test]
async fn test_concurrent_requests() {
    let (client_tx, client_rx) = std::sync::mpsc::channel::<String>();
    let (server_tx, server_rx) = watch::channel::<String>("hello".to_string());

    let (kill_server_tx, mut kill_server_rx) = oneshot::channel::<()>();
    let server_handle = std::thread::spawn(move || {
        while kill_server_rx.try_recv().is_err() {
            if let Ok(msg) = client_rx.try_recv() {
                let request = serde_json::from_str::<Request<[i32; 1]>>(&msg)
                    .expect("failed to parse request");

                let response = Response {
                    jsonrpc: "2.0".to_string(),
                    result: JsonRpcResult::<_, ()>::Result(request.params.unwrap()[0]),
                    id: Some(request.id),
                };

                server_tx
                    .send(serde_json::to_string(&response).expect("failed to serialize response"))
                    .expect("failed to send response");
            }
        }
    });

    let client = Client::new(client_tx, server_rx);

    let req1 = client.request::<_, i64, ()>(Request {
        jsonrpc: "2.0".to_string(),
        method: "echo".to_string(),
        params: Some([1]),
        id: 1,
    });
    let req2 = client.request::<_, i64, ()>(Request {
        jsonrpc: "2.0".to_string(),
        method: "echo".to_string(),
        params: Some([2]),
        id: 2,
    });

    let (res1, res2) = join!(req1, req2);
    let (res1, res2) = (res1.unwrap(), res2.unwrap());
    let (res1, res2) = if res1.id.unwrap() == 1 {
        (res1, res2)
    } else {
        (res2, res1)
    };

    insta::assert_debug_snapshot!(res1,
        @r###"
    Response {
        jsonrpc: "2.0",
        result: Result(
            1,
        ),
        id: Some(
            1,
        ),
    }
    "###
    );

    insta::assert_debug_snapshot!(res2,
        @r###"
    Response {
        jsonrpc: "2.0",
        result: Result(
            2,
        ),
        id: Some(
            2,
        ),
    }
    "###
    );

    kill_server_tx
        .send(())
        .expect("failed to send kill signal to server");
    server_handle.join().expect("failed to join server");
}
