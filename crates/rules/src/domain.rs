use alloc::vec::Vec;

/// Every word over `alphabet` up to `longest` letters, shortest first.
#[must_use]
pub fn words(alphabet: &[u8], longest: usize) -> Vec<Vec<u8>> {
    let mut found = alloc::vec![Vec::new()];
    let mut start = 0;
    for _ in 0..longest {
        let end = found.len();
        for index in start..end {
            for &letter in alphabet {
                let mut word = found.get(index).cloned().unwrap_or_default();
                word.push(letter);
                found.push(word);
            }
        }
        start = end;
    }
    found
}

/// Every array of `N` flags.
pub fn flags<const N: usize>() -> impl Iterator<Item = [bool; N]> {
    (0..1u32 << N).map(|bits| core::array::from_fn(|index| bits & (1 << index) != 0))
}
