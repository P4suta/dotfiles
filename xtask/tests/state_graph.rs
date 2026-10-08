use xtask::state_graph::{Graph, canonical, render, transitions};

/// A dump in the `dot,actionlabels` form of `TLC`: the model that the report below counts, with its nodes declared after their first edge and one self-loop.
const DOT: &str = r#"strict digraph DiskGraph {
node [shape=box,style=rounded]
nodesep=0.35;
subgraph cluster_graph {
color="white";
-17 [label="/\\ m_on = FALSE\n/\\ m_seen = ( 0 :> [Idle |-> [tag |-> \"UNIT\"]] @@\n  1 :> [Idle |-> [tag |-> \"UNIT\"]] )",style = filled]
-17 -> 42 [label="m_flip",color="black",fontcolor="black"];
42 [label="/\\ m_on = TRUE\n/\\ m_seen = ( 0 :> [Idle |-> [tag |-> \"UNIT\"]] @@ 1 :> [Idle |-> [tag |-> \"UNIT\"]] )",tooltip="/\\ m_on = TRUE"];
42 -> 42 [label="m_hold([Idle |-> [tag |-> \"UNIT\"]])",color="black",fontcolor="black"];
42 -> -17 [label="m_flip",color="black",fontcolor="black"];
{rank = same; -17;42;}
}
}
"#;

const TLC: &str = "Finished computing initial states: 1 distinct state generated at 2026-10-07 13:25:04.\nModel checking completed. No error has been found.\n4 states generated, 2 distinct states found, 0 states left on queue.\n";

fn state(on: &str) -> Vec<(String, String)> {
    vec![
        ("m_on".to_owned(), on.to_owned()),
        (
            "m_seen".to_owned(),
            r#"( 0 :> [Idle |-> [tag |-> "UNIT"]] @@ 1 :> [Idle |-> [tag |-> "UNIT"]] )"#
                .to_owned(),
        ),
    ]
}

#[test]
fn a_dump_becomes_sorted_states_and_edges_with_their_values_unwrapped() {
    let graph = canonical(DOT, TLC).unwrap();
    assert_eq!(
        graph,
        Graph {
            generated: 4,
            initial: 1,
            states: vec![
                state("FALSE").into_iter().collect(),
                state("TRUE").into_iter().collect()
            ],
            edges: vec![
                (0, "m_flip".to_owned(), 1),
                (1, "m_flip".to_owned(), 0),
                (1, r#"m_hold([Idle |-> [tag |-> "UNIT"]])"#.to_owned(), 1),
            ],
        }
    );
    assert_eq!(transitions(TLC), Some((4, 1)));
}

#[test]
fn the_rendering_is_one_record_per_line_and_independent_of_fingerprints() {
    let graph = canonical(DOT, TLC).unwrap();
    let renamed = DOT.replace("-17", "903").replace("42", "-5");
    assert_eq!(render(&canonical(&renamed, TLC).unwrap()), render(&graph));
    let text = render(&graph);
    assert!(text.ends_with("]]}\n"));
    // The counts, the states key, each state, the edges key, and each edge.
    assert_eq!(text.lines().count(), 1 + 1 + 2 + 1 + 3);
    let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        parsed["edges"][2][1],
        r#"m_hold([Idle |-> [tag |-> "UNIT"]])"#
    );
}

#[test]
fn a_dump_that_misses_a_transition_tlc_generated_or_names_an_unknown_node_is_refused() {
    let missing = DOT.replace(
        "42 -> -17 [label=\"m_flip\",color=\"black\",fontcolor=\"black\"];\n",
        "",
    );
    let error = canonical(&missing, TLC).unwrap_err().to_string();
    assert!(error.contains("2 edges"), "{error}");
    let dangling = DOT.replace("42 -> -17", "42 -> 7");
    assert!(canonical(&dangling, TLC).is_err());
    assert!(canonical(DOT, "no report").is_err());
}
