use ratatui::style::{Color, Style};
use std::collections::HashMap;
use std::path::Path;
use syntect::{
    highlighting::ThemeSet,
    parsing::SyntaxSet,
};

#[derive(Clone)]
pub struct HighlightedLine {
    pub spans: Vec<(Style, String)>,
    pub version: u64, // For cache invalidation
}

pub struct SyntaxHighlighter {
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
    // Cache for highlighted lines
    line_cache: HashMap<usize, HighlightedLine>,
    // Current file version for cache invalidation
    file_version: u64,
}

impl SyntaxHighlighter {
    pub fn new() -> Self {
        let syntax_set = SyntaxSet::load_defaults_newlines();
        let theme_set = ThemeSet::load_defaults();

        Self {
            syntax_set,
            theme_set,
            line_cache: HashMap::new(),
            file_version: 0,
        }
    }

    pub fn detect_syntax(&self, file_path: Option<&Path>, first_line: Option<&str>) -> Option<String> {
        if let Some(path) = file_path {
            if let Some(extension) = path.extension().and_then(|e| e.to_str()) {
                if let Some(syntax) = self.syntax_set.find_syntax_by_extension(extension) {
                    return Some(syntax.name.clone());
                }
            }
        }
        
        if let Some(line) = first_line {
            if let Some(syntax) = self.syntax_set.find_syntax_by_first_line(line) {
                return Some(syntax.name.clone());
            }
        }
        
        None
    }

    pub fn set_syntax(&mut self, _syntax_name: Option<&str>) {
        // Clear cache when syntax changes
        self.line_cache.clear();
        self.file_version += 1;
    }

    pub fn invalidate_cache_from_line(&mut self, start_line: usize) {
        // Remove cached lines from start_line onwards
        self.line_cache.retain(|&line_num, _| line_num < start_line);
        self.file_version += 1;
    }

    pub fn highlight_line(&mut self, line_num: usize, line_text: &str) -> Vec<(Style, String)> {
        // Check cache first
        if let Some(cached) = self.line_cache.get(&line_num) {
            if cached.version == self.file_version {
                return cached.spans.clone();
            }
        }

        // Highlight the line using enhanced simple highlighting
        let spans = self.highlight_simple(line_text);

        // Cache the result
        self.line_cache.insert(line_num, HighlightedLine {
            spans: spans.clone(),
            version: self.file_version,
        });

        spans
    }

    fn highlight_simple(&self, line: &str) -> Vec<(Style, String)> {
        // Enhanced fallback highlighting with string literals, numbers, and comments
        let mut result = Vec::new();
        let mut chars = line.chars().peekable();
        let mut current_word = String::new();
        
        while let Some(ch) = chars.next() {
            if ch.is_alphanumeric() || ch == '_' {
                current_word.push(ch);
            } else {
                // Process accumulated word
                if !current_word.is_empty() {
                    let style = self.get_keyword_style(&current_word);
                    result.push((style, current_word.clone()));
                    current_word.clear();
                }
                
                // Handle special characters
                match ch {
                    // String literals
                    '"' => {
                        let mut string_literal = String::from('"');
                        let mut escaped = false;
                        while let Some(next_ch) = chars.next() {
                            string_literal.push(next_ch);
                            if next_ch == '"' && !escaped {
                                break;
                            }
                            escaped = next_ch == '\\' && !escaped;
                        }
                        result.push((Style::default().fg(Color::Green), string_literal));
                    }
                    '\'' => {
                        let mut char_literal = String::from('\'');
                        let mut escaped = false;
                        while let Some(next_ch) = chars.next() {
                            char_literal.push(next_ch);
                            if next_ch == '\'' && !escaped {
                                break;
                            }
                            escaped = next_ch == '\\' && !escaped;
                        }
                        result.push((Style::default().fg(Color::Green), char_literal));
                    }
                    // Comments
                    '/' if chars.peek() == Some(&'/') => {
                        chars.next(); // consume second '/'
                        let rest: String = chars.collect();
                        result.push((Style::default().fg(Color::DarkGray), format!("//{}", rest)));
                        break;
                    }
                    '#' => {
                        let rest: String = chars.collect();
                        result.push((Style::default().fg(Color::DarkGray), format!("#{}", rest)));
                        break;
                    }
                    // Numbers
                    c if c.is_ascii_digit() => {
                        let mut number = String::from(c);
                        while let Some(&next_ch) = chars.peek() {
                            if next_ch.is_ascii_digit() || next_ch == '.' || next_ch == '_' || next_ch.is_ascii_hexdigit() {
                                number.push(chars.next().unwrap());
                            } else {
                                break;
                            }
                        }
                        result.push((Style::default().fg(Color::Cyan), number));
                    }
                    // Regular characters and operators
                    _ => {
                        result.push((Style::default(), ch.to_string()));
                    }
                }
            }
        }
        
        // Handle final word
        if !current_word.is_empty() {
            let style = self.get_keyword_style(&current_word);
            result.push((style, current_word));
        }
        
        result
    }

    fn get_keyword_style(&self, word: &str) -> Style {
        match word {
            // Rust keywords
            "fn" | "let" | "mut" | "pub" | "struct" | "enum" | "impl" | "trait" | "use" | "mod" | 
            "const" | "static" | "extern" | "crate" | "super" | "self" | "Self" | "where" | "async" | "await" => 
                Style::default().fg(Color::Magenta),
            
            // Control flow (avoid duplicate "let" and "const")
            "if" | "else" | "match" | "for" | "while" | "loop" | "break" | "continue" | "return" |
            "try" | "catch" | "finally" | "throw" | "raise" | "def" | "class" | "import" | "from" |
            "function" | "var" => 
                Style::default().fg(Color::Yellow),
            
            // Literals
            "true" | "false" | "True" | "False" | "null" | "None" | "undefined" | "nil" => 
                Style::default().fg(Color::Cyan),
            
            // Types (common ones)
            "String" | "Vec" | "Option" | "Result" | "HashMap" | "HashSet" | "i32" | "u32" | "i64" | "u64" |
            "f32" | "f64" | "bool" | "char" | "usize" | "isize" | "str" | "int" | "float" | "list" | "dict" |
            "tuple" | "set" | "Array" | "Object" | "Number" | "Boolean" => 
                Style::default().fg(Color::Blue),
            
            _ => Style::default(),
        }
    }

    pub fn get_cache_size(&self) -> usize {
        self.line_cache.len()
    }

    pub fn clear_cache(&mut self) {
        self.line_cache.clear();
        self.file_version += 1;
    }
}

impl Default for SyntaxHighlighter {
    fn default() -> Self {
        Self::new()
    }
}