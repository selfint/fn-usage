use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout};

use anyhow::Context;

pub struct StdIO {
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl StdIO {
    pub fn new(child: &mut Child) -> Self {
        let stdin = child.stdin.take().expect("child has no stdin");
        let stdout = BufReader::new(child.stdout.take().expect("child has no stdout"));
        Self { stdin, stdout }
    }
}

impl crate::client::StringIO for StdIO {
    fn send(&mut self, msg: &str) -> anyhow::Result<()> {
        self.stdin
            .write_all(msg.as_bytes())
            .context("writing msg to stdin")
    }

    fn recv(&mut self) -> anyhow::Result<String> {
        let mut content_length = None;
        let mut content_type = None;

        loop {
            let mut line = String::new();
            self.stdout
                .read_line(&mut line)
                .context("reading line from stdout")?;
            let words = line.split_ascii_whitespace().collect::<Vec<_>>();

            match (words.as_slice(), &mut content_length, &mut content_type) {
                (["Content-Length:", c_length], None, None) => {
                    content_length = Some(c_length.parse().context("parsing Content-Length")?)
                }
                (["Content-Type:", c_type], Some(_), None) => {
                    content_type = Some(c_type.to_string())
                }
                ([], Some(content_length), _) => {
                    let mut content = Vec::with_capacity(*content_length);
                    let mut bytes_left = *content_length;
                    while bytes_left > 0 {
                        let read_bytes = self.stdout.read_until(b'}', &mut content).unwrap();
                        bytes_left -= read_bytes;
                    }

                    let content = String::from_utf8(content).unwrap();
                    return Ok(content);
                }
                ([], None, None) => panic!("Unexpected server shut down"),
                unexpected => panic!("Got unexpected stdout: {:?}", unexpected),
            };
        }
    }
}
