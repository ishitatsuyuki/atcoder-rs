use std::io::{self, BufRead, Write};

fn main() {
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let stdout = io::stdout();
    let mut output = stdout.lock();
    let mut buf = String::new();
    input.read_line(&mut buf).unwrap();
    let a: usize = buf.trim().parse().unwrap();
    buf.clear();
    input.read_line(&mut buf).unwrap();
    let (b, c): (usize, usize) = {
        let mut split = buf.split_whitespace().map(|s| s.parse().unwrap());
        (split.next().unwrap(), split.next().unwrap())
    };
    buf.clear();
    input.read_line(&mut buf).unwrap();
    let s = buf.trim().to_owned();
    writeln!(output, "{} {}", a + b + c, s).unwrap();
}
