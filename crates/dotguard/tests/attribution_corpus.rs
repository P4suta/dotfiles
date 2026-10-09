//! The anonymized golden corpus of commit messages, which the attribution strip must turn into exactly the expected messages.

/// The line between two messages of a corpus file.
const SEPARATOR: &str = "=====\n";

#[test]
fn the_golden_corpus_strips_to_exactly_the_expected_messages() {
    let corpus = include_str!("fixtures/attribution/corpus.txt");
    let expected = include_str!("fixtures/attribution/expected.txt");
    let stripped: Vec<(String, usize)> = corpus
        .split(SEPARATOR)
        .map(dotguard::attribution::strip)
        .collect();
    let messages: Vec<&str> = stripped.iter().map(|(text, _)| text.as_str()).collect();
    assert_eq!(messages.join(SEPARATOR), expected);
    assert_eq!(
        stripped
            .iter()
            .map(|(_, removed)| *removed)
            .collect::<Vec<_>>(),
        [2, 0, 0, 2, 1, 1, 0]
    );
}
