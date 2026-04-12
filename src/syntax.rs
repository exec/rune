use ratatui::style::{Color, Style};
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

const MAX_CACHE_ENTRIES: usize = 1000;

#[derive(Clone, Copy, PartialEq, Eq)]
enum TokenKind {
    Keyword, // Magenta
    Control, // Yellow
    Literal, // Cyan
    Type,    // Blue
}

impl TokenKind {
    #[inline]
    fn style(self) -> Style {
        match self {
            TokenKind::Keyword => Style::default().fg(Color::Magenta),
            TokenKind::Control => Style::default().fg(Color::Yellow),
            TokenKind::Literal => Style::default().fg(Color::Cyan),
            TokenKind::Type => Style::default().fg(Color::Blue),
        }
    }
}

static RUST_KEYWORDS: phf::Map<&'static str, TokenKind> = phf::phf_map! {
    "fn" => TokenKind::Keyword, "let" => TokenKind::Keyword, "mut" => TokenKind::Keyword,
    "pub" => TokenKind::Keyword, "struct" => TokenKind::Keyword, "enum" => TokenKind::Keyword,
    "impl" => TokenKind::Keyword, "trait" => TokenKind::Keyword, "use" => TokenKind::Keyword,
    "mod" => TokenKind::Keyword, "const" => TokenKind::Keyword, "static" => TokenKind::Keyword,
    "extern" => TokenKind::Keyword, "crate" => TokenKind::Keyword, "super" => TokenKind::Keyword,
    "self" => TokenKind::Keyword, "Self" => TokenKind::Keyword, "where" => TokenKind::Keyword,
    "async" => TokenKind::Keyword, "await" => TokenKind::Keyword, "unsafe" => TokenKind::Keyword,
    "dyn" => TokenKind::Keyword, "ref" => TokenKind::Keyword, "move" => TokenKind::Keyword,
    "if" => TokenKind::Control, "else" => TokenKind::Control, "match" => TokenKind::Control,
    "for" => TokenKind::Control, "while" => TokenKind::Control, "loop" => TokenKind::Control,
    "break" => TokenKind::Control, "continue" => TokenKind::Control, "return" => TokenKind::Control,
    "true" => TokenKind::Literal, "false" => TokenKind::Literal,
    "None" => TokenKind::Literal, "Some" => TokenKind::Literal,
    "String" => TokenKind::Type, "Vec" => TokenKind::Type, "Option" => TokenKind::Type,
    "Result" => TokenKind::Type, "HashMap" => TokenKind::Type, "HashSet" => TokenKind::Type,
    "i32" => TokenKind::Type, "u32" => TokenKind::Type, "i64" => TokenKind::Type,
    "u64" => TokenKind::Type, "f32" => TokenKind::Type, "f64" => TokenKind::Type,
    "bool" => TokenKind::Type, "char" => TokenKind::Type, "usize" => TokenKind::Type,
    "isize" => TokenKind::Type, "str" => TokenKind::Type, "Box" => TokenKind::Type,
    "Rc" => TokenKind::Type, "Arc" => TokenKind::Type, "RefCell" => TokenKind::Type,
    "Mutex" => TokenKind::Type,
};

static PYTHON_KEYWORDS: phf::Map<&'static str, TokenKind> = phf::phf_map! {
    "def" => TokenKind::Keyword, "class" => TokenKind::Keyword, "import" => TokenKind::Keyword,
    "from" => TokenKind::Keyword, "as" => TokenKind::Keyword, "with" => TokenKind::Keyword,
    "lambda" => TokenKind::Keyword, "async" => TokenKind::Keyword, "await" => TokenKind::Keyword,
    "global" => TokenKind::Keyword, "nonlocal" => TokenKind::Keyword, "yield" => TokenKind::Keyword,
    "pass" => TokenKind::Keyword, "del" => TokenKind::Keyword,
    "if" => TokenKind::Control, "elif" => TokenKind::Control, "else" => TokenKind::Control,
    "for" => TokenKind::Control, "while" => TokenKind::Control, "break" => TokenKind::Control,
    "continue" => TokenKind::Control, "return" => TokenKind::Control, "try" => TokenKind::Control,
    "except" => TokenKind::Control, "finally" => TokenKind::Control, "raise" => TokenKind::Control,
    "assert" => TokenKind::Control,
    "True" => TokenKind::Literal, "False" => TokenKind::Literal, "None" => TokenKind::Literal,
    "str" => TokenKind::Type, "int" => TokenKind::Type, "float" => TokenKind::Type,
    "bool" => TokenKind::Type, "list" => TokenKind::Type, "dict" => TokenKind::Type,
    "tuple" => TokenKind::Type, "set" => TokenKind::Type, "len" => TokenKind::Type,
    "range" => TokenKind::Type, "enumerate" => TokenKind::Type, "zip" => TokenKind::Type,
    "map" => TokenKind::Type, "filter" => TokenKind::Type, "print" => TokenKind::Type,
    "open" => TokenKind::Type, "file" => TokenKind::Type, "type" => TokenKind::Type,
    "isinstance" => TokenKind::Type, "hasattr" => TokenKind::Type,
};

static JS_KEYWORDS: phf::Map<&'static str, TokenKind> = phf::phf_map! {
    "function" => TokenKind::Keyword, "var" => TokenKind::Keyword, "let" => TokenKind::Keyword,
    "const" => TokenKind::Keyword, "class" => TokenKind::Keyword, "extends" => TokenKind::Keyword,
    "import" => TokenKind::Keyword, "export" => TokenKind::Keyword, "async" => TokenKind::Keyword,
    "await" => TokenKind::Keyword, "yield" => TokenKind::Keyword, "interface" => TokenKind::Keyword,
    "type" => TokenKind::Keyword, "enum" => TokenKind::Keyword, "namespace" => TokenKind::Keyword,
    "module" => TokenKind::Keyword, "declare" => TokenKind::Keyword,
    "if" => TokenKind::Control, "else" => TokenKind::Control, "for" => TokenKind::Control,
    "while" => TokenKind::Control, "do" => TokenKind::Control, "break" => TokenKind::Control,
    "continue" => TokenKind::Control, "return" => TokenKind::Control, "try" => TokenKind::Control,
    "catch" => TokenKind::Control, "finally" => TokenKind::Control, "throw" => TokenKind::Control,
    "switch" => TokenKind::Control, "case" => TokenKind::Control, "default" => TokenKind::Control,
    "true" => TokenKind::Literal, "false" => TokenKind::Literal,
    "null" => TokenKind::Literal, "undefined" => TokenKind::Literal,
    "string" => TokenKind::Type, "number" => TokenKind::Type, "boolean" => TokenKind::Type,
    "object" => TokenKind::Type, "Array" => TokenKind::Type, "Object" => TokenKind::Type,
    "Function" => TokenKind::Type, "Promise" => TokenKind::Type, "Map" => TokenKind::Type,
    "Set" => TokenKind::Type, "WeakMap" => TokenKind::Type, "WeakSet" => TokenKind::Type,
    "Symbol" => TokenKind::Type, "BigInt" => TokenKind::Type, "any" => TokenKind::Type,
    "unknown" => TokenKind::Type, "never" => TokenKind::Type, "void" => TokenKind::Type,
};

static GO_KEYWORDS: phf::Map<&'static str, TokenKind> = phf::phf_map! {
    "func" => TokenKind::Keyword, "var" => TokenKind::Keyword, "const" => TokenKind::Keyword,
    "type" => TokenKind::Keyword, "struct" => TokenKind::Keyword, "interface" => TokenKind::Keyword,
    "package" => TokenKind::Keyword, "import" => TokenKind::Keyword, "go" => TokenKind::Keyword,
    "defer" => TokenKind::Keyword, "chan" => TokenKind::Keyword, "select" => TokenKind::Keyword,
    "range" => TokenKind::Keyword, "map" => TokenKind::Keyword, "make" => TokenKind::Keyword,
    "new" => TokenKind::Keyword,
    "if" => TokenKind::Control, "else" => TokenKind::Control, "for" => TokenKind::Control,
    "switch" => TokenKind::Control, "case" => TokenKind::Control, "default" => TokenKind::Control,
    "break" => TokenKind::Control, "continue" => TokenKind::Control, "return" => TokenKind::Control,
    "goto" => TokenKind::Control, "fallthrough" => TokenKind::Control,
    "true" => TokenKind::Literal, "false" => TokenKind::Literal, "nil" => TokenKind::Literal,
    "string" => TokenKind::Type, "int" => TokenKind::Type, "int8" => TokenKind::Type,
    "int16" => TokenKind::Type, "int32" => TokenKind::Type, "int64" => TokenKind::Type,
    "uint" => TokenKind::Type, "uint8" => TokenKind::Type, "uint16" => TokenKind::Type,
    "uint32" => TokenKind::Type, "uint64" => TokenKind::Type, "bool" => TokenKind::Type,
    "byte" => TokenKind::Type, "rune" => TokenKind::Type, "float32" => TokenKind::Type,
    "float64" => TokenKind::Type, "complex64" => TokenKind::Type, "complex128" => TokenKind::Type,
    "error" => TokenKind::Type,
};

static SHELL_KEYWORDS: phf::Map<&'static str, TokenKind> = phf::phf_map! {
    "if" => TokenKind::Keyword, "then" => TokenKind::Keyword, "else" => TokenKind::Keyword,
    "elif" => TokenKind::Keyword, "fi" => TokenKind::Keyword, "case" => TokenKind::Keyword,
    "esac" => TokenKind::Keyword, "for" => TokenKind::Keyword, "while" => TokenKind::Keyword,
    "until" => TokenKind::Keyword, "do" => TokenKind::Keyword, "done" => TokenKind::Keyword,
    "function" => TokenKind::Keyword, "select" => TokenKind::Keyword, "time" => TokenKind::Keyword,
    "in" => TokenKind::Keyword,
    "echo" => TokenKind::Control, "printf" => TokenKind::Control, "cat" => TokenKind::Control,
    "grep" => TokenKind::Control, "awk" => TokenKind::Control, "sed" => TokenKind::Control,
    "sort" => TokenKind::Control, "uniq" => TokenKind::Control, "cut" => TokenKind::Control,
    "head" => TokenKind::Control, "tail" => TokenKind::Control, "find" => TokenKind::Control,
    "xargs" => TokenKind::Control, "ls" => TokenKind::Control, "cd" => TokenKind::Control,
    "pwd" => TokenKind::Control, "mkdir" => TokenKind::Control, "rm" => TokenKind::Control,
    "cp" => TokenKind::Control, "mv" => TokenKind::Control, "chmod" => TokenKind::Control,
    "chown" => TokenKind::Control, "export" => TokenKind::Control, "alias" => TokenKind::Control,
    "source" => TokenKind::Control, "exec" => TokenKind::Control, "exit" => TokenKind::Control,
    "return" => TokenKind::Control, "break" => TokenKind::Control, "continue" => TokenKind::Control,
    "true" => TokenKind::Literal, "false" => TokenKind::Literal,
};

static C_KEYWORDS: phf::Map<&'static str, TokenKind> = phf::phf_map! {
    "int" => TokenKind::Keyword, "char" => TokenKind::Keyword, "float" => TokenKind::Keyword,
    "double" => TokenKind::Keyword, "void" => TokenKind::Keyword, "long" => TokenKind::Keyword,
    "short" => TokenKind::Keyword, "unsigned" => TokenKind::Keyword, "signed" => TokenKind::Keyword,
    "const" => TokenKind::Keyword, "static" => TokenKind::Keyword, "extern" => TokenKind::Keyword,
    "volatile" => TokenKind::Keyword, "register" => TokenKind::Keyword, "struct" => TokenKind::Keyword,
    "union" => TokenKind::Keyword, "enum" => TokenKind::Keyword, "typedef" => TokenKind::Keyword,
    "sizeof" => TokenKind::Keyword,
    "class" => TokenKind::Keyword, "public" => TokenKind::Keyword, "private" => TokenKind::Keyword,
    "protected" => TokenKind::Keyword, "virtual" => TokenKind::Keyword, "override" => TokenKind::Keyword,
    "namespace" => TokenKind::Keyword, "using" => TokenKind::Keyword, "template" => TokenKind::Keyword,
    "typename" => TokenKind::Keyword, "new" => TokenKind::Keyword, "delete" => TokenKind::Keyword,
    "this" => TokenKind::Keyword, "friend" => TokenKind::Keyword, "inline" => TokenKind::Keyword,
    "explicit" => TokenKind::Keyword, "operator" => TokenKind::Keyword,
    "if" => TokenKind::Control, "else" => TokenKind::Control, "for" => TokenKind::Control,
    "while" => TokenKind::Control, "do" => TokenKind::Control, "break" => TokenKind::Control,
    "continue" => TokenKind::Control, "return" => TokenKind::Control, "switch" => TokenKind::Control,
    "case" => TokenKind::Control, "default" => TokenKind::Control, "goto" => TokenKind::Control,
    "true" => TokenKind::Literal, "false" => TokenKind::Literal,
    "NULL" => TokenKind::Literal, "nullptr" => TokenKind::Literal,
    "bool" => TokenKind::Type, "size_t" => TokenKind::Type, "uint8_t" => TokenKind::Type,
    "uint16_t" => TokenKind::Type, "uint32_t" => TokenKind::Type, "uint64_t" => TokenKind::Type,
    "int8_t" => TokenKind::Type, "int16_t" => TokenKind::Type, "int32_t" => TokenKind::Type,
    "int64_t" => TokenKind::Type, "string" => TokenKind::Type, "vector" => TokenKind::Type,
    "map" => TokenKind::Type, "set" => TokenKind::Type, "list" => TokenKind::Type,
    "array" => TokenKind::Type,
};

static JSON_KEYWORDS: phf::Map<&'static str, TokenKind> = phf::phf_map! {
    "true" => TokenKind::Literal, "false" => TokenKind::Literal, "null" => TokenKind::Literal,
};

static YAML_KEYWORDS: phf::Map<&'static str, TokenKind> = phf::phf_map! {
    "true" => TokenKind::Literal, "false" => TokenKind::Literal, "null" => TokenKind::Literal,
    "yes" => TokenKind::Literal, "no" => TokenKind::Literal,
    "on" => TokenKind::Literal, "off" => TokenKind::Literal,
};

static DOCKERFILE_KEYWORDS: phf::Map<&'static str, TokenKind> = phf::phf_map! {
    "FROM" => TokenKind::Keyword, "RUN" => TokenKind::Keyword, "CMD" => TokenKind::Keyword,
    "LABEL" => TokenKind::Keyword, "MAINTAINER" => TokenKind::Keyword, "EXPOSE" => TokenKind::Keyword,
    "ENV" => TokenKind::Keyword, "ADD" => TokenKind::Keyword, "COPY" => TokenKind::Keyword,
    "ENTRYPOINT" => TokenKind::Keyword, "VOLUME" => TokenKind::Keyword, "USER" => TokenKind::Keyword,
    "WORKDIR" => TokenKind::Keyword, "ARG" => TokenKind::Keyword, "ONBUILD" => TokenKind::Keyword,
    "STOPSIGNAL" => TokenKind::Keyword, "HEALTHCHECK" => TokenKind::Keyword, "SHELL" => TokenKind::Keyword,
};

static GENERIC_KEYWORDS: phf::Map<&'static str, TokenKind> = phf::phf_map! {
    "if" => TokenKind::Control, "else" => TokenKind::Control, "for" => TokenKind::Control,
    "while" => TokenKind::Control, "break" => TokenKind::Control, "continue" => TokenKind::Control,
    "return" => TokenKind::Control,
    "true" => TokenKind::Literal, "false" => TokenKind::Literal, "null" => TokenKind::Literal,
    "None" => TokenKind::Literal, "undefined" => TokenKind::Literal, "nil" => TokenKind::Literal,
};

#[derive(Clone)]
pub struct HighlightedLine {
    pub spans: Rc<Vec<(Style, String)>>,
    pub version: u64,      // For cache invalidation
    pub access_order: u64, // For LRU eviction
}

pub struct SyntaxHighlighter {
    // Cache for highlighted lines
    line_cache: HashMap<usize, HighlightedLine>,
    // Current file version for cache invalidation
    file_version: u64,
    // Counter for LRU tracking
    access_counter: u64,
    // Current syntax for language-specific highlighting
    current_syntax: Option<String>,
}

impl SyntaxHighlighter {
    pub fn new() -> Self {
        Self {
            line_cache: HashMap::new(),
            file_version: 0,
            access_counter: 0,
            current_syntax: None,
        }
    }

    pub fn detect_syntax(
        &self,
        file_path: Option<&Path>,
        _first_line: Option<&str>,
    ) -> Option<String> {
        if let Some(path) = file_path {
            if let Some(extension) = path.extension().and_then(|e| e.to_str()) {
                return Self::syntax_name_for_extension(extension);
            }
        }
        None
    }

    /// Map file extensions to syntax names without loading SyntaxSet/ThemeSet
    fn syntax_name_for_extension(ext: &str) -> Option<String> {
        let name = match ext.to_lowercase().as_str() {
            "rs" => "Rust",
            "py" | "pyw" => "Python",
            "js" | "mjs" | "cjs" => "JavaScript",
            "ts" | "mts" | "cts" => "TypeScript",
            "go" => "Go",
            "sh" | "bash" | "zsh" => "Shell Script (Bash)",
            "c" => "C",
            "cpp" | "cc" | "cxx" | "hpp" | "h" => "C++",
            "json" => "JSON",
            "yml" | "yaml" => "YAML",
            "toml" => "TOML",
            "md" | "markdown" => "Markdown",
            "html" | "htm" => "HTML",
            "css" => "CSS",
            "java" => "Java",
            "rb" => "Ruby",
            "php" => "PHP",
            "swift" => "Swift",
            "kt" | "kts" => "Kotlin",
            "dockerfile" => "Dockerfile",
            "xml" => "XML",
            "sql" => "SQL",
            "lua" => "Lua",
            "r" => "R",
            "pl" | "pm" => "Perl",
            "zig" => "Zig",
            _ => return None,
        };
        Some(name.to_string())
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

    /// Evict least recently used entries when cache exceeds MAX_CACHE_ENTRIES
    fn evict_lru(&mut self) {
        if self.line_cache.len() <= MAX_CACHE_ENTRIES {
            return;
        }
        let target = MAX_CACHE_ENTRIES * 3 / 4; // Evict down to 75%
        let mut entries: Vec<(usize, u64)> = self
            .line_cache
            .iter()
            .map(|(&line, cached)| (line, cached.access_order))
            .collect();
        entries.sort_by_key(|&(_, order)| order);
        let to_remove = self.line_cache.len() - target;
        for &(line, _) in entries.iter().take(to_remove) {
            self.line_cache.remove(&line);
        }
    }

    pub fn highlight_line(&mut self, line_num: usize, line_text: &str) -> Rc<Vec<(Style, String)>> {
        // Check cache first — return Rc clone (cheap) instead of deep clone
        if let Some(cached) = self.line_cache.get_mut(&line_num) {
            if cached.version == self.file_version {
                self.access_counter += 1;
                cached.access_order = self.access_counter;
                return Rc::clone(&cached.spans);
            }
        }

        // Highlight the line using language-aware highlighting
        let spans = Rc::new(self.highlight_simple(line_text));

        self.access_counter += 1;

        // Cache the result
        self.line_cache.insert(
            line_num,
            HighlightedLine {
                spans: Rc::clone(&spans),
                version: self.file_version,
                access_order: self.access_counter,
            },
        );

        // LRU eviction if cache is too large
        self.evict_lru();

        spans
    }

    fn highlight_simple(&self, line: &str) -> Vec<(Style, String)> {
        let keyword_map = self.current_keyword_map();
        let mut result: Vec<(Style, String)> = Vec::new();
        let mut buf = String::new();
        let mut buf_style = Style::default();

        // Flush accumulated buffer into result, starting fresh with new style.
        // Adjacent same-style writes merge into one span automatically.
        let flush = |result: &mut Vec<(Style, String)>, buf: &mut String, buf_style: &mut Style, new_style: Style| {
            if !buf.is_empty() {
                if let Some(last) = result.last_mut() {
                    if last.0 == *buf_style {
                        last.1.push_str(buf);
                        buf.clear();
                        *buf_style = new_style;
                        return;
                    }
                }
                result.push((*buf_style, std::mem::take(buf)));
            }
            *buf_style = new_style;
        };

        let default_style = Style::default();
        let string_style = Style::default().fg(Color::Green);
        let comment_style = Style::default().fg(Color::DarkGray);
        let number_style = Style::default().fg(Color::Cyan);

        let mut chars = line.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch.is_alphanumeric() || ch == '_' {
                // Accumulate identifier chars — we need the full word before we know its style.
                // Collect into a temporary (short-lived) and then flush with correct style.
                let mut word = String::new();
                word.push(ch);
                while let Some(&next_ch) = chars.peek() {
                    if next_ch.is_alphanumeric() || next_ch == '_' {
                        word.push(next_ch);
                        chars.next();
                    } else {
                        break;
                    }
                }
                let style = Self::lookup_keyword(keyword_map, &word);
                if style != buf_style {
                    flush(&mut result, &mut buf, &mut buf_style, style);
                }
                buf.push_str(&word);
                continue;
            }

            match ch {
                '"' => {
                    if buf_style != string_style {
                        flush(&mut result, &mut buf, &mut buf_style, string_style);
                    }
                    buf.push('"');
                    let mut escaped = false;
                    for next_ch in chars.by_ref() {
                        buf.push(next_ch);
                        if next_ch == '"' && !escaped {
                            break;
                        }
                        escaped = next_ch == '\\' && !escaped;
                    }
                }
                '\'' => {
                    if buf_style != string_style {
                        flush(&mut result, &mut buf, &mut buf_style, string_style);
                    }
                    buf.push('\'');
                    let mut escaped = false;
                    for next_ch in chars.by_ref() {
                        buf.push(next_ch);
                        if next_ch == '\'' && !escaped {
                            break;
                        }
                        escaped = next_ch == '\\' && !escaped;
                    }
                }
                '/' if chars.peek() == Some(&'/') => {
                    chars.next();
                    if buf_style != comment_style {
                        flush(&mut result, &mut buf, &mut buf_style, comment_style);
                    }
                    buf.push_str("//");
                    for c in chars.by_ref() {
                        buf.push(c);
                    }
                    break;
                }
                '/' if chars.peek() == Some(&'*') => {
                    chars.next();
                    if buf_style != comment_style {
                        flush(&mut result, &mut buf, &mut buf_style, comment_style);
                    }
                    buf.push_str("/*");
                    while let Some(next_ch) = chars.next() {
                        buf.push(next_ch);
                        if next_ch == '*' && chars.peek() == Some(&'/') {
                            chars.next();
                            buf.push('/');
                            break;
                        }
                    }
                    break;
                }
                '#' => {
                    if buf_style != comment_style {
                        flush(&mut result, &mut buf, &mut buf_style, comment_style);
                    }
                    buf.push('#');
                    for c in chars.by_ref() {
                        buf.push(c);
                    }
                    break;
                }
                '-' if chars.peek() == Some(&'-') => {
                    chars.next();
                    if buf_style != comment_style {
                        flush(&mut result, &mut buf, &mut buf_style, comment_style);
                    }
                    buf.push_str("--");
                    for c in chars.by_ref() {
                        buf.push(c);
                    }
                    break;
                }
                c if c.is_ascii_digit() => {
                    if buf_style != number_style {
                        flush(&mut result, &mut buf, &mut buf_style, number_style);
                    }
                    buf.push(c);
                    while let Some(&next_ch) = chars.peek() {
                        if next_ch.is_ascii_digit()
                            || next_ch == '.'
                            || next_ch == '_'
                            || next_ch.is_ascii_hexdigit()
                        {
                            buf.push(next_ch);
                            chars.next();
                        } else {
                            break;
                        }
                    }
                }
                _ => {
                    if buf_style != default_style {
                        flush(&mut result, &mut buf, &mut buf_style, default_style);
                    }
                    buf.push(ch);
                }
            }
        }

        if !buf.is_empty() {
            if let Some(last) = result.last_mut() {
                if last.0 == buf_style {
                    last.1.push_str(&buf);
                    return result;
                }
            }
            result.push((buf_style, buf));
        }

        result
    }

    #[inline]
    fn current_keyword_map(&self) -> &'static phf::Map<&'static str, TokenKind> {
        match self.current_syntax.as_deref() {
            Some("Rust") => &RUST_KEYWORDS,
            Some("Python") => &PYTHON_KEYWORDS,
            Some("JavaScript") | Some("TypeScript") => &JS_KEYWORDS,
            Some("Go") => &GO_KEYWORDS,
            Some("Shell Script (Bash)") | Some("Bourne Again Shell (bash)") => &SHELL_KEYWORDS,
            Some("C") | Some("C++") => &C_KEYWORDS,
            Some("JSON") => &JSON_KEYWORDS,
            Some("YAML") => &YAML_KEYWORDS,
            Some("Dockerfile") => &DOCKERFILE_KEYWORDS,
            _ => &GENERIC_KEYWORDS,
        }
    }

    #[inline]
    fn lookup_keyword(map: &phf::Map<&'static str, TokenKind>, word: &str) -> Style {
        match map.get(word) {
            Some(kind) => kind.style(),
            None => Style::default(),
        }
    }
}

impl Default for SyntaxHighlighter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_detect_syntax_rust() {
        let h = SyntaxHighlighter::new();
        let result = h.detect_syntax(Some(Path::new("main.rs")), None);
        assert_eq!(result, Some("Rust".to_string()));
    }

    #[test]
    fn test_detect_syntax_python() {
        let h = SyntaxHighlighter::new();
        let result = h.detect_syntax(Some(Path::new("script.py")), None);
        assert_eq!(result, Some("Python".to_string()));
    }

    #[test]
    fn test_detect_syntax_javascript() {
        let h = SyntaxHighlighter::new();
        let result = h.detect_syntax(Some(Path::new("app.js")), None);
        assert_eq!(result, Some("JavaScript".to_string()));
    }

    #[test]
    fn test_detect_syntax_unknown() {
        let h = SyntaxHighlighter::new();
        let result = h.detect_syntax(Some(Path::new("file.xyz")), None);
        assert_eq!(result, None);
    }

    #[test]
    fn test_get_rust_keyword_style() {
        let mut h = SyntaxHighlighter::new();
        h.set_syntax(Some("Rust"));
        let map = h.current_keyword_map();
        assert_eq!(
            SyntaxHighlighter::lookup_keyword(map, "fn").fg,
            Some(Color::Magenta)
        );
        assert_eq!(
            SyntaxHighlighter::lookup_keyword(map, "if").fg,
            Some(Color::Yellow)
        );
        assert_eq!(
            SyntaxHighlighter::lookup_keyword(map, "String").fg,
            Some(Color::Blue)
        );
        assert_eq!(
            SyntaxHighlighter::lookup_keyword(map, "foobar"),
            Style::default()
        );
    }

    #[test]
    fn test_highlight_line_returns_non_empty_spans() {
        let mut h = SyntaxHighlighter::new();
        h.set_syntax(Some("Rust"));
        let spans = h.highlight_line(0, "fn main() { let x = 42; }");
        assert!(!spans.is_empty());
    }

    #[test]
    fn test_highlight_line_empty_input() {
        let mut h = SyntaxHighlighter::new();
        let spans = h.highlight_line(0, "");
        assert!(spans.is_empty());
    }
}
