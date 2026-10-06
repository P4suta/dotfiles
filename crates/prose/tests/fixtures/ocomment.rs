//! A stand-in for `ocomment scan --format jsonl` that reads the `//` comment lines of each path.

fn escaped(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let mut paths = Vec::new();
    let mut index = 0;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "scan" | "--quiet" => index += 1,
            "--format" | "--config" => index += 2,
            path => {
                paths.push(path.to_owned());
                index += 1;
            }
        }
    }
    for path in paths {
        let text = std::fs::read_to_string(&path).expect("a readable source file");
        let comments: Vec<String> = text
            .lines()
            .enumerate()
            .filter(|(_, line)| line.trim_start().starts_with("//"))
            .map(|(number, line)| {
                let column = line.len() - line.trim_start().len() + 1;
                format!(
                    "{{\"line\":{0},\"column\":{column},\"end_line\":{0},\"kind\":\"line\",\"text\":\"{1}\"}}",
                    number + 1,
                    escaped(line.trim())
                )
            })
            .collect();
        println!(
            "{{\"path\":\"{}\",\"report\":{{\"comments\":[{}]}}}}",
            escaped(&path),
            comments.join(",")
        );
    }
}
