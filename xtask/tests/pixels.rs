use std::fs;
use std::path::Path;
use xtask::pixels::replay;

#[test]
fn replay_preserves_each_committed_terminal_cell() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/goldens");
    for entry in fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path
            .extension()
            .is_some_and(|extension| extension == "grid")
        {
            let original = fs::read_to_string(&path).unwrap();
            assert_eq!(
                xtask::theme::grid(replay(&original).unwrap().as_bytes()),
                original.lines().map(str::to_owned).collect::<Vec<_>>(),
                "{}",
                path.display()
            );
        }
    }
}

#[test]
fn malformed_grids_and_embedded_controls_never_reach_the_renderer() {
    for grid in [
        "",
        "0,80\tx\t-\t-\t",
        "24,0\tx\t-\t-\t",
        "0,0\t\x1b[2J\t-\t-\t",
        "0,0\tx\t-\t-\tq",
        "0,0\tx\t256\t-\t",
        "0,0\tx\t-\t-\t\n0,0\ty\t-\t-\t",
    ] {
        assert!(replay(grid).is_err(), "{grid:?}");
    }
}
