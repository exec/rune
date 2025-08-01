use ratatui::style::{Color, Style};
use std::collections::HashMap;
use std::path::Path;
use syntect::{highlighting::ThemeSet, parsing::SyntaxSet};

#[derive(Clone)]
pub struct HighlightedLine {
    pub spans: Vec<(Style, String)>,
    pub version: u64, // For cache invalidation
}

pub struct SyntaxHighlighter {
    syntax_set: SyntaxSet,
    #[allow(dead_code)]
    theme_set: ThemeSet,
    // Cache for highlighted lines
    line_cache: HashMap<usize, HighlightedLine>,
    // Current file version for cache invalidation
    file_version: u64,
    // Current syntax for language-specific highlighting
    current_syntax: Option<String>,
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
            current_syntax: None,
        }
    }

    pub fn detect_syntax(
        &self,
        file_path: Option<&Path>,
        first_line: Option<&str>,
    ) -> Option<String> {
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

    pub fn set_syntax(&mut self, syntax_name: Option<&str>) {
        self.current_syntax = syntax_name.map(|s| s.to_string());
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

        // Highlight the line using language-aware highlighting
        let spans = self.highlight_simple(line_text);

        // Cache the result
        self.line_cache.insert(
            line_num,
            HighlightedLine {
                spans: spans.clone(),
                version: self.file_version,
            },
        );

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
                        #[allow(clippy::while_let_on_iterator)]
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
                        #[allow(clippy::while_let_on_iterator)]
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
                        result.push((Style::default().fg(Color::DarkGray), format!("//{rest}")));
                        break;
                    }
                    '/' if chars.peek() == Some(&'*') => {
                        chars.next(); // consume '*'
                        let mut comment = String::from("/*");
                        let mut found_end = false;
                        
                        while let Some(next_ch) = chars.next() {
                            comment.push(next_ch);
                            if next_ch == '*' && chars.peek() == Some(&'/') {
                                chars.next(); // consume '/'
                                comment.push('/');
                                found_end = true;
                                break;
                            }
                        }
                        
                        result.push((Style::default().fg(Color::DarkGray), comment));
                        if found_end {
                            // Continue processing if block comment ended on this line
                            // (chars iterator is consumed, so this will exit the loop)
                        }
                        break; // End processing for this line
                    }
                    '#' => {
                        let rest: String = chars.collect();
                        result.push((Style::default().fg(Color::DarkGray), format!("#{rest}")));
                        break;
                    }
                    '-' if chars.peek() == Some(&'-') => {
                        // SQL-style comments
                        chars.next(); // consume second '-'
                        let rest: String = chars.collect();
                        result.push((Style::default().fg(Color::DarkGray), format!("--{rest}")));
                        break;
                    }
                    // Numbers
                    c if c.is_ascii_digit() => {
                        let mut number = String::from(c);
                        while let Some(&next_ch) = chars.peek() {
                            if next_ch.is_ascii_digit()
                                || next_ch == '.'
                                || next_ch == '_'
                                || next_ch.is_ascii_hexdigit()
                            {
                                if let Some(ch) = chars.next() {
                                    number.push(ch);
                                } else {
                                    break;
                                }
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
        // Language-specific highlighting based on current syntax
        match self.current_syntax.as_deref() {
            Some("Rust") => self.get_rust_keyword_style(word),
            Some("Python") => self.get_python_keyword_style(word),
            Some("JavaScript") | Some("TypeScript") => self.get_js_keyword_style(word),
            Some("Go") => self.get_go_keyword_style(word),
            Some("Shell Script (Bash)") | Some("Bourne Again Shell (bash)") => self.get_shell_keyword_style(word),
            Some("C") | Some("C++") => self.get_c_keyword_style(word),
            Some("JSON") => self.get_json_keyword_style(word),
            Some("YAML") => self.get_yaml_keyword_style(word),
            Some("Dockerfile") => self.get_dockerfile_keyword_style(word),
            _ => self.get_generic_keyword_style(word),
        }
    }

    fn get_rust_keyword_style(&self, word: &str) -> Style {
        match word {
            // Rust keywords
            "fn" | "let" | "mut" | "pub" | "struct" | "enum" | "impl" | "trait" | "use" | "mod"
            | "const" | "static" | "extern" | "crate" | "super" | "self" | "Self" | "where"
            | "async" | "await" | "unsafe" | "dyn" | "ref" | "move" => Style::default().fg(Color::Magenta),

            // Control flow
            "if" | "else" | "match" | "for" | "while" | "loop" | "break" | "continue"
            | "return" => Style::default().fg(Color::Yellow),

            // Literals
            "true" | "false" | "None" | "Some" => Style::default().fg(Color::Cyan),

            // Rust types
            "String" | "Vec" | "Option" | "Result" | "HashMap" | "HashSet" | "i32" | "u32"
            | "i64" | "u64" | "f32" | "f64" | "bool" | "char" | "usize" | "isize" | "str"
            | "Box" | "Rc" | "Arc" | "RefCell" | "Mutex" => Style::default().fg(Color::Blue),

            _ => Style::default(),
        }
    }

    fn get_python_keyword_style(&self, word: &str) -> Style {
        match word {
            // Python keywords
            "def" | "class" | "import" | "from" | "as" | "with" | "lambda" | "async" | "await"
            | "global" | "nonlocal" | "yield" | "pass" | "del" => Style::default().fg(Color::Magenta),

            // Control flow
            "if" | "elif" | "else" | "for" | "while" | "break" | "continue" | "return"
            | "try" | "except" | "finally" | "raise" | "assert" => Style::default().fg(Color::Yellow),

            // Literals
            "True" | "False" | "None" => Style::default().fg(Color::Cyan),

            // Python built-ins
            "str" | "int" | "float" | "bool" | "list" | "dict" | "tuple" | "set"
            | "len" | "range" | "enumerate" | "zip" | "map" | "filter" | "print"
            | "open" | "file" | "type" | "isinstance" | "hasattr" => Style::default().fg(Color::Blue),

            _ => Style::default(),
        }
    }

    fn get_js_keyword_style(&self, word: &str) -> Style {
        match word {
            // JavaScript/TypeScript keywords
            "function" | "var" | "let" | "const" | "class" | "extends" | "import" | "export"
            | "async" | "await" | "yield" | "interface" | "type" | "enum"
            | "namespace" | "module" | "declare" => Style::default().fg(Color::Magenta),

            // Control flow
            "if" | "else" | "for" | "while" | "do" | "break" | "continue" | "return"
            | "try" | "catch" | "finally" | "throw" | "switch" | "case" | "default" => Style::default().fg(Color::Yellow),

            // Literals
            "true" | "false" | "null" | "undefined" => Style::default().fg(Color::Cyan),

            // JS/TS types
            "string" | "number" | "boolean" | "object" | "Array" | "Object" | "Function"
            | "Promise" | "Map" | "Set" | "WeakMap" | "WeakSet" | "Symbol" | "BigInt"
            | "any" | "unknown" | "never" | "void" => Style::default().fg(Color::Blue),

            _ => Style::default(),
        }
    }

    fn get_go_keyword_style(&self, word: &str) -> Style {
        match word {
            // Go keywords
            "func" | "var" | "const" | "type" | "struct" | "interface" | "package" | "import"
            | "go" | "defer" | "chan" | "select" | "range" | "map" | "make" | "new" => Style::default().fg(Color::Magenta),

            // Control flow
            "if" | "else" | "for" | "switch" | "case" | "default" | "break" | "continue"
            | "return" | "goto" | "fallthrough" => Style::default().fg(Color::Yellow),

            // Literals
            "true" | "false" | "nil" => Style::default().fg(Color::Cyan),

            // Go types
            "string" | "int" | "int8" | "int16" | "int32" | "int64" | "uint" | "uint8"
            | "uint16" | "uint32" | "uint64" | "bool" | "byte" | "rune" | "float32"
            | "float64" | "complex64" | "complex128" | "error" => Style::default().fg(Color::Blue),

            _ => Style::default(),
        }
    }

    fn get_shell_keyword_style(&self, word: &str) -> Style {
        match word {
            // Shell keywords
            "if" | "then" | "else" | "elif" | "fi" | "case" | "esac" | "for" | "while"
            | "until" | "do" | "done" | "function" | "select" | "time" | "in" => Style::default().fg(Color::Magenta),

            // Shell commands
            "echo" | "printf" | "cat" | "grep" | "awk" | "sed" | "sort" | "uniq" | "cut"
            | "head" | "tail" | "find" | "xargs" | "ls" | "cd" | "pwd" | "mkdir" | "rm"
            | "cp" | "mv" | "chmod" | "chown" | "export" | "alias" | "source" | "exec"
            | "exit" | "return" | "break" | "continue" => Style::default().fg(Color::Yellow),

            // Shell variables/operators
            "true" | "false" => Style::default().fg(Color::Cyan),

            _ => Style::default(),
        }
    }

    fn get_c_keyword_style(&self, word: &str) -> Style {
        match word {
            // C/C++ keywords
            "int" | "char" | "float" | "double" | "void" | "long" | "short" | "unsigned"
            | "signed" | "const" | "static" | "extern" | "volatile" | "register"
            | "struct" | "union" | "enum" | "typedef" | "sizeof" => Style::default().fg(Color::Magenta),

            // C++ specific
            "class" | "public" | "private" | "protected" | "virtual" | "override"
            | "namespace" | "using" | "template" | "typename" | "new" | "delete"
            | "this" | "friend" | "inline" | "explicit" | "operator" => Style::default().fg(Color::Magenta),

            // Control flow
            "if" | "else" | "for" | "while" | "do" | "break" | "continue" | "return"
            | "switch" | "case" | "default" | "goto" => Style::default().fg(Color::Yellow),

            // Literals
            "true" | "false" | "NULL" | "nullptr" => Style::default().fg(Color::Cyan),

            // Standard types
            "bool" | "size_t" | "uint8_t" | "uint16_t" | "uint32_t" | "uint64_t"
            | "int8_t" | "int16_t" | "int32_t" | "int64_t" | "string" | "vector"
            | "map" | "set" | "list" | "array" => Style::default().fg(Color::Blue),

            _ => Style::default(),
        }
    }

    fn get_json_keyword_style(&self, word: &str) -> Style {
        match word {
            // JSON literals
            "true" | "false" | "null" => Style::default().fg(Color::Cyan),
            _ => Style::default(),
        }
    }

    fn get_yaml_keyword_style(&self, word: &str) -> Style {
        match word {
            // YAML literals
            "true" | "false" | "null" | "yes" | "no" | "on" | "off" => Style::default().fg(Color::Cyan),
            _ => Style::default(),
        }
    }

    fn get_dockerfile_keyword_style(&self, word: &str) -> Style {
        match word {
            // Dockerfile keywords
            "FROM" | "RUN" | "CMD" | "LABEL" | "MAINTAINER" | "EXPOSE" | "ENV" | "ADD"
            | "COPY" | "ENTRYPOINT" | "VOLUME" | "USER" | "WORKDIR" | "ARG" | "ONBUILD"
            | "STOPSIGNAL" | "HEALTHCHECK" | "SHELL" => Style::default().fg(Color::Magenta),
            _ => Style::default(),
        }
    }

    fn get_generic_keyword_style(&self, word: &str) -> Style {
        match word {
            // Generic control flow
            "if" | "else" | "for" | "while" | "break" | "continue" | "return" => Style::default().fg(Color::Yellow),

            // Generic literals
            "true" | "false" | "null" | "None" | "undefined" | "nil" => Style::default().fg(Color::Cyan),

            _ => Style::default(),
        }
    }

    #[allow(dead_code)]
    pub fn get_cache_size(&self) -> usize {
        self.line_cache.len()
    }

    #[allow(dead_code)]
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
