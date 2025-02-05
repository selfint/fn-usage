use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout};

use anyhow::{Context, Result};

use crate::lsp;

pub struct StdIO {
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl StdIO {
    pub fn new(child: &mut Child) -> Result<Self> {
        let stdin = child.stdin.take().context("child has no stdin")?;
        let stdout = BufReader::new(child.stdout.take().context("child has no stdout")?);

        Ok(Self { stdin, stdout })
    }
}

impl lsp::StringIO for StdIO {
    fn send(&mut self, msg: &str) -> Result<()> {
        let length = msg.as_bytes().len();
        let msg = &format!("Content-Length: {}\r\n\r\n{}", length, msg);

        self.stdin
            .write_all(msg.as_bytes())
            .context("writing msg to stdin")
    }

    fn recv(&mut self) -> Result<String> {
        let mut content_length = None;

        loop {
            let mut line = String::new();
            self.stdout
                .read_line(&mut line)
                .context("reading line from stdout")?;

            let words = line.split_ascii_whitespace().collect::<Vec<_>>();

            match (words.as_slice(), &content_length) {
                (["Content-Length:", c_length], None) => content_length = Some(c_length.parse()?),
                (["Content-Type:", _], Some(_)) => {}
                ([], Some(content_length)) => {
                    let mut content = Vec::with_capacity(*content_length);
                    let mut bytes_left = *content_length;
                    while bytes_left > 0 {
                        let read_bytes = self.stdout.read_until(b'}', &mut content).unwrap();
                        bytes_left -= read_bytes;
                    }

                    let content = String::from_utf8(content).unwrap();
                    return Ok(content);
                }
                unexpected => panic!("Got unexpected stdout: {:?}", unexpected),
            };
        }
    }
}
