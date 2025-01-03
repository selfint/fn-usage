# LSP outside the editor

[LSP](https://microsoft.github.io/language-server-protocol/) (Language Server Protocol) was created in order to simplify both editor and language tooling (converts the MxN problem to an M + N problem).
Although not it's original purpose, we can also use LSP to write static analysis tools, independent of an editor. Since LSP is language-agnostic, these tools will also work on any language (only those that have language servers, of course).

This article can be:

1. A reference for writing code that communicates with LSP servers.

2. An overview of how LSP client/servers work.

3. Hopefully, inspiration for creating tools that use LSP is new ways.

# The project

Write a linter that displays the "usage percentage" of a function. 
Where "usage percentage" is the amount of functions that eventually call X, divided by the total amount of functions.
The meaning of "eventually" is if foo calls bar, which calls baz, then foo eventually calls baz (i.e. foo _transitively_ calls baz).
While this article will be using Rust, following along with any language should be possible. Anything Rust-specific will be marked as such, and you will need to adjust the step for your language.

## Architecture

We will build 3 separate projects (_crates_ in Rust terms):

1. JSON-RPC types: A library containing types definitions as specified in the [JSON-RPC 2.0 spec](https://www.jsonrpc.org/specification).

2. LSP Client: A library for communicating with LSP servers, designed in the [sans-io](https://youtu.be/7cC3_jGwl_U) pattern.

3. fn-usage: Our linter executable. It will receive the full path to the target codebase, and a command to run that starts the appropriate LSP server. The output will be a json containing all the functions inside the codebase, and their "usage".

   Example usage:

   ```sh
   $ fn-usage path/to/project rust-analyzer
   {
       "path/to/file#fn_name": 99,
       "path/to/file#fn_name2": 95
   }
   ```

Let's setup the workspace:

```sh
$ cd /path/to/fn-usage
$ touch Cargo.toml
```

And inside the `Cargo.toml` file:

```toml
[workspace]
members = []
```

## 1. JSON-RPC

While there are other implementations of JSON-RPC types already, they
are a bit too complicated for this project. Our implementation will
be very simple.

Setup the crate inside our workspace:

```sh
$ cargo new --lib jsonrpc-types
```

And inside the root `Cargo.toml` file:

```toml
[workspace]
members = [
    "jsonrpc-types"
]
```

Run `cargo test` to make sure everything works.

### Types

A very minimal (yet sufficient) implementation of the JSON-RPC types.

```rs
// jsonrpc/src/types.rs
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct Request<Params> {
    pub jsonrpc: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Params>,
    pub id: i64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Notification<Params> {
    pub jsonrpc: String,
    pub method: String,
    pub params: Option<Params>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Response<T, E> {
    pub jsonrpc: String,
    #[serde(flatten)]
    pub result: JsonRpcResult<T, E>,
    pub id: Option<i64>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
pub enum JsonRpcResult<T, E> {
    Result(T),
    Error {
        code: i64,
        message: String,
        data: Option<E>,
    },
}

```

Notice that there is no validation between ensuring the correct
method/param/data types together. We will take care of that in
the LSP-specific code later.

### Client

```rs
// jsonrpc/src/client.rs
```

That's all. All the code is inside the `lib.rs` file.

> While writing the definitions, I used the [insta](https://github.com/mitsuhiko/insta) library for writing the tests. You can view the tests [here](https://github.com/selfint/fn-usage/blob/7a117e281b4861b97bf2e5913b5cb9b9ee25a2da/jsonrpc-types/src/lib.rs#L39).

### Client

## 2. LSP Client

Our LSP client is really just a JSON-RPC client, that only sends LSP messages.
For the LSP types, we will use the [lsp-types](https://github.com/gluon-lang/lsp-types) crate.

Setup the crate inside our workspace:

```sh
$ cargo new --lib lsp-client
```

And inside the root `Cargo.toml` file:

```toml
[workspace]
members = [
    "jsonrpc-types",
    "lsp-client"
]
```

Run `cargo test` again to make sure everything works.
