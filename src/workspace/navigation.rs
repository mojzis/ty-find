use anyhow::Result;
use std::fs;

#[allow(dead_code)]
pub struct SymbolFinder {
    content: String,
    lines: Vec<String>,
}

#[allow(dead_code)]
impl SymbolFinder {
    pub fn new(file_path: &str) -> Result<Self> {
        let content = fs::read_to_string(file_path)?;
        let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

        Ok(Self { content, lines })
    }

    pub fn find_symbol_positions(&self, symbol: &str) -> Vec<(u32, u32)> {
        let mut positions = Vec::new();

        for (line_idx, line) in self.lines.iter().enumerate() {
            let mut char_pos = 0;
            while let Some(pos) = line[char_pos..].find(symbol) {
                let actual_pos = char_pos + pos;

                if self.is_whole_word_match(line, actual_pos, symbol) {
                    positions.push((line_idx as u32, actual_pos as u32));
                }

                char_pos = actual_pos + 1;
            }
        }

        positions
    }

    fn is_whole_word_match(&self, line: &str, pos: usize, symbol: &str) -> bool {
        let chars: Vec<char> = line.chars().collect();

        if pos > 0 {
            let prev_char = chars[pos.saturating_sub(1)];
            if prev_char.is_alphanumeric() || prev_char == '_' {
                return false;
            }
        }

        let end_pos = pos + symbol.len();
        if end_pos < chars.len() {
            let next_char = chars[end_pos];
            if next_char.is_alphanumeric() || next_char == '_' {
                return false;
            }
        }

        true
    }

    pub fn get_line(&self, line_number: u32) -> Option<&String> {
        self.lines.get(line_number as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_symbol_finder() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "def test_function():").unwrap();
        writeln!(temp_file, "    return test_function()").unwrap();
        writeln!(temp_file, "").unwrap();
        writeln!(temp_file, "result = test_function()").unwrap();

        let finder = SymbolFinder::new(temp_file.path().to_str().unwrap()).unwrap();
        let positions = finder.find_symbol_positions("test_function");

        assert_eq!(positions.len(), 3);
        assert_eq!(positions[0], (0, 4));
        assert_eq!(positions[1], (1, 11));
        assert_eq!(positions[2], (3, 9));
    }
}
