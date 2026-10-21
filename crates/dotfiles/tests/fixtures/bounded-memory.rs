const BLOCK_BYTES: usize = 1024 * 1024;
const BLOCKS: usize = 128;

fn main() {
    let mut blocks = Vec::new();
    for _ in 0..BLOCKS {
        blocks.push(vec![1_u8; BLOCK_BYTES]);
        std::hint::black_box(&blocks);
    }
}
