pub const BYTES_PER_ROW: usize = 16;

pub struct HexViewState {
    pub raw_bytes: Vec<u8>,
    pub cursor: usize,
    pub scroll_offset: usize,
}

impl HexViewState {
    pub fn new(raw_bytes: Vec<u8>) -> Self {
        Self {
            raw_bytes,
            cursor: 0,
            scroll_offset: 0,
        }
    }
}
