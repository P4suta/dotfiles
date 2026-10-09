//! The file the theme gate pages through bat, so its cell grid shows the palette on code.

/// A greeting with every token class the pager colors.
pub fn greet(name: &str, times: u32) -> Option<String> {
    let mut text = String::from("Hello, ");
    text.push_str(name);
    if times == 0 {
        return None;
    }
    Some(text.repeat(times as usize))
}
