pub fn allowed(value: bool) -> bool {
    value
}
#[cfg(test)]
mod tests {
    #[test]
    fn contract() {
        assert!(!super::allowed(false));
    }
}
