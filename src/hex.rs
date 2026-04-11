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

    pub fn total_rows(&self) -> usize {
        (self.raw_bytes.len() + BYTES_PER_ROW - 1) / BYTES_PER_ROW
    }

    pub fn cursor_row(&self) -> usize {
        self.cursor / BYTES_PER_ROW
    }

    pub fn cursor_col(&self) -> usize {
        self.cursor % BYTES_PER_ROW
    }
}
