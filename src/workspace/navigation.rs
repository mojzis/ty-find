use anyhow::{Context, Result};

#[allow(dead_code)]
pub struct SymbolFinder {
    lines: Vec<String>,
}

#[allow(dead_code)]
impl SymbolFinder {
    pub async fn new(file_path: &str) -> Result<Self> {
        let content = tokio::fs::read_to_string(file_path)
            .await
            .with_context(|| format!("Failed to read file: {file_path}"))?;
        let lines: Vec<String> = content.lines().map(String::from).collect();

        Ok(Self { lines })
    }

    pub fn find_symbol_positions(&self, symbol: &str) -> Vec<(u32, u32)> {
        let mut positions = Vec::new();

        for (line_idx, line) in self.lines.iter().enumerate() {
            let mut char_pos = 0;
            while let Some(pos) = line[char_pos..].find(symbol) {
                let actual_pos = char_pos + pos;

                if Self::is_whole_word_match(line, actual_pos, symbol) {
                    #[allow(clippy::cast_possible_truncation)]
                    positions.push((line_idx as u32, actual_pos as u32));
                }

                char_pos = actual_pos + 1;
            }
        }

        positions
    }

    fn is_whole_word_match(line: &str, pos: usize, symbol: &str) -> bool {
        let bytes = line.as_bytes();

        if pos > 0 {
            let prev = bytes[pos - 1];
            if prev.is_ascii_alphanumeric() || prev == b'_' {
                return false;
            }
        }

        let end_pos = pos + symbol.len();
        if end_pos < bytes.len() {
            let next = bytes[end_pos];
            if next.is_ascii_alphanumeric() || next == b'_' {
                return false;
            }
        }

        true
    }

    pub fn get_line(&self, line_number: u32) -> Option<&str> {
        self.lines.get(line_number as usize).map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_symbol_finder() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "def test_function():").unwrap();
        writeln!(temp_file, "    return test_function()").unwrap();
        writeln!(temp_file).unwrap();
        writeln!(temp_file, "result = test_function()").unwrap();

        let finder = SymbolFinder::new(temp_file.path().to_str().unwrap()).await.unwrap();
        let positions = finder.find_symbol_positions("test_function");

        assert_eq!(positions.len(), 3);
        assert_eq!(positions[0], (0, 4));
        assert_eq!(positions[1], (1, 11));
        assert_eq!(positions[2], (3, 9));
    }

    #[tokio::test]
    async fn test_symbol_not_found() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "def foo():").unwrap();

        let finder = SymbolFinder::new(temp_file.path().to_str().unwrap()).await.unwrap();
        let positions = finder.find_symbol_positions("bar");
        assert!(positions.is_empty());
    }

    #[tokio::test]
    async fn test_empty_file() {
        let temp_file = NamedTempFile::new().unwrap();

        let finder = SymbolFinder::new(temp_file.path().to_str().unwrap()).await.unwrap();
        let positions = finder.find_symbol_positions("anything");
        assert!(positions.is_empty());
    }

    #[tokio::test]
    async fn test_symbol_at_line_start() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "foo = 1").unwrap();

        let finder = SymbolFinder::new(temp_file.path().to_str().unwrap()).await.unwrap();
        let positions = finder.find_symbol_positions("foo");
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0], (0, 0));
    }

    #[tokio::test]
    async fn test_symbol_at_line_end() {
        let mut temp_file = NamedTempFile::new().unwrap();
        write!(temp_file, "x = foo").unwrap();

        let finder = SymbolFinder::new(temp_file.path().to_str().unwrap()).await.unwrap();
        let positions = finder.find_symbol_positions("foo");
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0], (0, 4));
    }

    #[tokio::test]
    async fn test_partial_match_rejected() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "foobar = 1").unwrap();
        writeln!(temp_file, "bar_foo = 2").unwrap();

        let finder = SymbolFinder::new(temp_file.path().to_str().unwrap()).await.unwrap();
        let positions = finder.find_symbol_positions("foo");
        assert!(positions.is_empty());
    }

    #[tokio::test]
    async fn test_get_line() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "line zero").unwrap();
        writeln!(temp_file, "line one").unwrap();

        let finder = SymbolFinder::new(temp_file.path().to_str().unwrap()).await.unwrap();
        assert_eq!(finder.get_line(0), Some("line zero"));
        assert_eq!(finder.get_line(1), Some("line one"));
        assert_eq!(finder.get_line(2), None);
    }
}
