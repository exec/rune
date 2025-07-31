use ratatui::style::{Color, Style};
use std::path::Path;

pub struct SyntaxHighlighter {
    // For now, we'll keep this simple without actual syntax highlighting
    // This can be extended later with proper syntect integration
}

impl SyntaxHighlighter {
    pub fn new() -> Self {
        Self {}
    }

    pub fn detect_syntax(&self, file_path: Option<&Path>, _first_line: Option<&str>) -> Option<String> {
        if let Some(path) = file_path {
            if let Some(extension) = path.extension().and_then(|e| e.to_str()) {
                match extension {
                    "rs" => Some("Rust".to_string()),
                    "py" => Some("Python".to_string()),
                    "js" | "ts" => Some("JavaScript".to_string()),
                    "c" | "h" => Some("C".to_string()),
                    "cpp" | "cc" | "cxx" => Some("C++".to_string()),
                    "go" => Some("Go".to_string()),
                    "java" => Some("Java".to_string()),
                    "html" => Some("HTML".to_string()),
                    "css" => Some("CSS".to_string()),
                    "json" => Some("JSON".to_string()),
                    "xml" => Some("XML".to_string()),
                    "md" => Some("Markdown".to_string()),
                    "sh" | "bash" => Some("Shell".to_string()),
                    "sql" => Some("SQL".to_string()),
                    _ => None,
                }
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn get_language_name(&self, file_path: Option<&Path>) -> String {
        self.detect_syntax(file_path, None).unwrap_or_else(|| "Plain Text".to_string())
    }

    pub fn highlight_text(&self, text: &str, syntax_name: Option<&str>) -> Vec<Vec<(Style, String)>> {
        // Simple syntax highlighting based on keywords and patterns
        text.lines()
            .map(|line| self.highlight_line(line, syntax_name))
            .collect()
    }

    fn highlight_line(&self, line: &str, syntax_name: Option<&str>) -> Vec<(Style, String)> {
        match syntax_name {
            Some("Rust") => self.highlight_rust_line(line),
            Some("Python") => self.highlight_python_line(line),
            Some("JavaScript") => self.highlight_js_line(line),
            _ => vec![(Style::default(), line.to_string())],
        }
    }

    fn highlight_rust_line(&self, line: &str) -> Vec<(Style, String)> {
        let mut result = Vec::new();
        let mut chars = line.chars().peekable();
        let mut current_word = String::new();
        
        while let Some(ch) = chars.next() {
            if ch.is_alphanumeric() || ch == '_' {
                current_word.push(ch);
            } else {
                if !current_word.is_empty() {
                    let style = match current_word.as_str() {
                        "fn" | "let" | "mut" | "pub" | "struct" | "enum" | "impl" | "trait" | "use" | "mod" | "const" | "static" => 
                            Style::default().fg(Color::Magenta),
                        "if" | "else" | "match" | "for" | "while" | "loop" | "break" | "continue" | "return" => 
                            Style::default().fg(Color::Yellow),
                        "true" | "false" => Style::default().fg(Color::Cyan),
                        "String" | "Vec" | "Option" | "Result" | "i32" | "u32" | "f64" | "bool" | "usize" => 
                            Style::default().fg(Color::Blue),
                        _ => Style::default(),
                    };
                    result.push((style, current_word.clone()));
                    current_word.clear();
                }
                
                // Handle comments
                if ch == '/' && chars.peek() == Some(&'/') {
                    chars.next(); // consume second '/'
                    let rest: String = chars.collect();
                    result.push((Style::default().fg(Color::Green), format!("//{}", rest)));
                    break;
                } else {
                    result.push((Style::default(), ch.to_string()));
                }
            }
        }
        
        if !current_word.is_empty() {
            let style = match current_word.as_str() {
                "fn" | "let" | "mut" | "pub" | "struct" | "enum" | "impl" | "trait" | "use" | "mod" | "const" | "static" => 
                    Style::default().fg(Color::Magenta),
                "if" | "else" | "match" | "for" | "while" | "loop" | "break" | "continue" | "return" => 
                    Style::default().fg(Color::Yellow),
                "true" | "false" => Style::default().fg(Color::Cyan),
                "String" | "Vec" | "Option" | "Result" | "i32" | "u32" | "f64" | "bool" | "usize" => 
                    Style::default().fg(Color::Blue),
                _ => Style::default(),
            };
            result.push((style, current_word));
        }
        
        result
    }

    fn highlight_python_line(&self, line: &str) -> Vec<(Style, String)> {
        let mut result = Vec::new();
        let mut chars = line.chars().peekable();
        let mut current_word = String::new();
        
        while let Some(ch) = chars.next() {
            if ch.is_alphanumeric() || ch == '_' {
                current_word.push(ch);
            } else {
                if !current_word.is_empty() {
                    let style = match current_word.as_str() {
                        "def" | "class" | "import" | "from" | "as" | "global" | "nonlocal" => 
                            Style::default().fg(Color::Magenta),
                        "if" | "elif" | "else" | "for" | "while" | "break" | "continue" | "return" | "try" | "except" | "finally" => 
                            Style::default().fg(Color::Yellow),
                        "True" | "False" | "None" => Style::default().fg(Color::Cyan),
                        "str" | "int" | "float" | "list" | "dict" | "tuple" | "set" => 
                            Style::default().fg(Color::Blue),
                        _ => Style::default(),
                    };
                    result.push((style, current_word.clone()));
                    current_word.clear();
                }
                
                // Handle comments
                if ch == '#' {
                    let rest: String = chars.collect();
                    result.push((Style::default().fg(Color::Green), format!("#{}", rest)));
                    break;
                } else {
                    result.push((Style::default(), ch.to_string()));
                }
            }
        }
        
        if !current_word.is_empty() {
            let style = match current_word.as_str() {
                "def" | "class" | "import" | "from" | "as" | "global" | "nonlocal" => 
                    Style::default().fg(Color::Magenta),
                "if" | "elif" | "else" | "for" | "while" | "break" | "continue" | "return" | "try" | "except" | "finally" => 
                    Style::default().fg(Color::Yellow),
                "True" | "False" | "None" => Style::default().fg(Color::Cyan),
                "str" | "int" | "float" | "list" | "dict" | "tuple" | "set" => 
                    Style::default().fg(Color::Blue),
                _ => Style::default(),
            };
            result.push((style, current_word));
        }
        
        result
    }

    fn highlight_js_line(&self, line: &str) -> Vec<(Style, String)> {
        let mut result = Vec::new();
        let mut chars = line.chars().peekable();
        let mut current_word = String::new();
        
        while let Some(ch) = chars.next() {
            if ch.is_alphanumeric() || ch == '_' || ch == '$' {
                current_word.push(ch);
            } else {
                if !current_word.is_empty() {
                    let style = match current_word.as_str() {
                        "function" | "var" | "let" | "const" | "class" | "import" | "export" | "default" => 
                            Style::default().fg(Color::Magenta),
                        "if" | "else" | "for" | "while" | "do" | "break" | "continue" | "return" | "try" | "catch" | "finally" => 
                            Style::default().fg(Color::Yellow),
                        "true" | "false" | "null" | "undefined" => Style::default().fg(Color::Cyan),
                        "String" | "Number" | "Boolean" | "Array" | "Object" => 
                            Style::default().fg(Color::Blue),
                        _ => Style::default(),
                    };
                    result.push((style, current_word.clone()));
                    current_word.clear();
                }
                
                // Handle comments
                if ch == '/' && chars.peek() == Some(&'/') {
                    chars.next(); // consume second '/'
                    let rest: String = chars.collect();
                    result.push((Style::default().fg(Color::Green), format!("//{}", rest)));
                    break;
                } else {
                    result.push((Style::default(), ch.to_string()));
                }
            }
        }
        
        if !current_word.is_empty() {
            let style = match current_word.as_str() {
                "function" | "var" | "let" | "const" | "class" | "import" | "export" | "default" => 
                    Style::default().fg(Color::Magenta),
                "if" | "else" | "for" | "while" | "do" | "break" | "continue" | "return" | "try" | "catch" | "finally" => 
                    Style::default().fg(Color::Yellow),
                "true" | "false" | "null" | "undefined" => Style::default().fg(Color::Cyan),
                "String" | "Number" | "Boolean" | "Array" | "Object" => 
                    Style::default().fg(Color::Blue),
                _ => Style::default(),
            };
            result.push((style, current_word));
        }
        
        result
    }
}

impl Default for SyntaxHighlighter {
    fn default() -> Self {
        Self::new()
    }
}