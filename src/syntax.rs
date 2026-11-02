#[cfg(feature = "terminal")]
use ratatui::style::{Color, Style};
#[cfg(not(feature = "terminal"))]
type Style = HighlightKind;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

const MAX_CACHE_ENTRIES: usize = 1000;

/// Renderer-independent roles. Adjacent text with the same role is coalesced.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HighlightKind {
    #[default]
    Plain,
    Keyword,
    Control,
    Literal,
    Type,
    String,
    Comment,
}

type TokenKind = HighlightKind;

/// Converts lexer roles without reparsing or copying cached text for a frontend.
pub trait HighlightStyle: Copy + Default + PartialEq {
    fn from_kind(kind: HighlightKind) -> Self;
}

impl HighlightStyle for HighlightKind {
    #[inline]
    fn from_kind(kind: HighlightKind) -> Self {
        kind
    }
}

#[cfg(feature = "terminal")]
impl HighlightStyle for Style {
    #[inline]
    fn from_kind(kind: HighlightKind) -> Self {
        let color = match kind {
            HighlightKind::Plain => return Self::default(),
            HighlightKind::Keyword => Color::Magenta,
            HighlightKind::Control => Color::Yellow,
            HighlightKind::Literal => Color::Cyan,
            HighlightKind::Type => Color::Blue,
            HighlightKind::String => Color::Green,
            HighlightKind::Comment => Color::DarkGray,
        };
        Self::default().fg(color)
    }
}

pub type SyntaxHighlighter = Highlighter<Style>;
pub type TokenHighlighter = Highlighter<HighlightKind>;

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

static HTML_KEYWORDS: phf::Map<&'static str, TokenKind> = phf::phf_map! {
    "html" => TokenKind::Keyword, "head" => TokenKind::Keyword, "body" => TokenKind::Keyword,
    "div" => TokenKind::Keyword, "span" => TokenKind::Keyword, "p" => TokenKind::Keyword,
    "a" => TokenKind::Keyword, "img" => TokenKind::Keyword, "ul" => TokenKind::Keyword,
    "ol" => TokenKind::Keyword, "li" => TokenKind::Keyword, "table" => TokenKind::Keyword,
    "tr" => TokenKind::Keyword, "td" => TokenKind::Keyword, "th" => TokenKind::Keyword,
    "form" => TokenKind::Keyword, "input" => TokenKind::Keyword, "button" => TokenKind::Keyword,
    "script" => TokenKind::Keyword, "style" => TokenKind::Keyword, "link" => TokenKind::Keyword,
    "meta" => TokenKind::Keyword, "title" => TokenKind::Keyword, "h1" => TokenKind::Keyword,
    "h2" => TokenKind::Keyword, "h3" => TokenKind::Keyword, "h4" => TokenKind::Keyword,
    "h5" => TokenKind::Keyword, "h6" => TokenKind::Keyword, "header" => TokenKind::Keyword,
    "footer" => TokenKind::Keyword, "nav" => TokenKind::Keyword, "section" => TokenKind::Keyword,
    "article" => TokenKind::Keyword, "aside" => TokenKind::Keyword, "main" => TokenKind::Keyword,
    "true" => TokenKind::Literal, "false" => TokenKind::Literal,
};

static CSS_KEYWORDS: phf::Map<&'static str, TokenKind> = phf::phf_map! {
    "important" => TokenKind::Keyword,
    "color" => TokenKind::Type, "background" => TokenKind::Type, "background-color" => TokenKind::Type,
    "font" => TokenKind::Type, "font-size" => TokenKind::Type, "font-weight" => TokenKind::Type,
    "margin" => TokenKind::Type, "padding" => TokenKind::Type, "border" => TokenKind::Type,
    "width" => TokenKind::Type, "height" => TokenKind::Type, "max-width" => TokenKind::Type,
    "min-width" => TokenKind::Type, "max-height" => TokenKind::Type, "min-height" => TokenKind::Type,
    "display" => TokenKind::Type, "position" => TokenKind::Type, "flex" => TokenKind::Type,
    "grid" => TokenKind::Type, "gap" => TokenKind::Type, "align-items" => TokenKind::Type,
    "justify-content" => TokenKind::Type, "flex-direction" => TokenKind::Type,
    "flex-wrap" => TokenKind::Type, "align-content" => TokenKind::Type,
    "transparent" => TokenKind::Literal, "none" => TokenKind::Literal,
    "auto" => TokenKind::Literal, "inherit" => TokenKind::Literal, "initial" => TokenKind::Literal,
    "unset" => TokenKind::Literal, "block" => TokenKind::Literal, "inline" => TokenKind::Literal,
    "inline-block" => TokenKind::Literal,
    "absolute" => TokenKind::Literal, "relative" => TokenKind::Literal, "fixed" => TokenKind::Literal,
    "sticky" => TokenKind::Literal,
};

static JAVA_KEYWORDS: phf::Map<&'static str, TokenKind> = phf::phf_map! {
    "public" => TokenKind::Keyword, "private" => TokenKind::Keyword, "protected" => TokenKind::Keyword,
    "static" => TokenKind::Keyword, "final" => TokenKind::Keyword, "abstract" => TokenKind::Keyword,
    "interface" => TokenKind::Keyword, "class" => TokenKind::Keyword, "enum" => TokenKind::Keyword,
    "extends" => TokenKind::Keyword, "implements" => TokenKind::Keyword, "package" => TokenKind::Keyword,
    "import" => TokenKind::Keyword, "return" => TokenKind::Keyword, "void" => TokenKind::Keyword,
    "native" => TokenKind::Keyword, "strictfp" => TokenKind::Keyword, "synchronized" => TokenKind::Keyword,
    "transient" => TokenKind::Keyword, "volatile" => TokenKind::Keyword, "throws" => TokenKind::Keyword,
    "throw" => TokenKind::Keyword, "try" => TokenKind::Control, "catch" => TokenKind::Control,
    "finally" => TokenKind::Control, "if" => TokenKind::Control, "else" => TokenKind::Control,
    "for" => TokenKind::Control, "while" => TokenKind::Control, "do" => TokenKind::Control,
    "switch" => TokenKind::Control, "case" => TokenKind::Control, "default" => TokenKind::Control,
    "break" => TokenKind::Control, "continue" => TokenKind::Control, "instanceof" => TokenKind::Control,
    "new" => TokenKind::Control, "super" => TokenKind::Keyword, "this" => TokenKind::Keyword,
    "true" => TokenKind::Literal, "false" => TokenKind::Literal, "null" => TokenKind::Literal,
    "boolean" => TokenKind::Type, "byte" => TokenKind::Type, "char" => TokenKind::Type,
    "short" => TokenKind::Type, "int" => TokenKind::Type, "long" => TokenKind::Type,
    "float" => TokenKind::Type, "double" => TokenKind::Type, "String" => TokenKind::Type,
    "Object" => TokenKind::Type, "Class" => TokenKind::Type, "System" => TokenKind::Type,
    "Integer" => TokenKind::Type, "Long" => TokenKind::Type, "Double" => TokenKind::Type,
    "Float" => TokenKind::Type, "Boolean" => TokenKind::Type, "Character" => TokenKind::Type,
    "StringBuilder" => TokenKind::Type, "ArrayList" => TokenKind::Type, "HashMap" => TokenKind::Type,
    "HashSet" => TokenKind::Type, "List" => TokenKind::Type, "Map" => TokenKind::Type,
    "Set" => TokenKind::Type, "Collection" => TokenKind::Type, "Iterator" => TokenKind::Type,
};

static RUBY_KEYWORDS: phf::Map<&'static str, TokenKind> = phf::phf_map! {
    "def" => TokenKind::Keyword, "end" => TokenKind::Keyword, "class" => TokenKind::Keyword,
    "module" => TokenKind::Keyword, "include" => TokenKind::Keyword, "extend" => TokenKind::Keyword,
    "require" => TokenKind::Keyword, "load" => TokenKind::Keyword, "attr_accessor" => TokenKind::Keyword,
    "attr_reader" => TokenKind::Keyword, "attr_writer" => TokenKind::Keyword,
    "if" => TokenKind::Control, "elsif" => TokenKind::Control, "else" => TokenKind::Control,
    "unless" => TokenKind::Control, "case" => TokenKind::Control, "when" => TokenKind::Control,
    "then" => TokenKind::Control, "begin" => TokenKind::Control, "rescue" => TokenKind::Control,
    "ensure" => TokenKind::Control, "retry" => TokenKind::Control, "raise" => TokenKind::Control,
    "for" => TokenKind::Control, "while" => TokenKind::Control, "until" => TokenKind::Control,
    "do" => TokenKind::Control, "break" => TokenKind::Control, "next" => TokenKind::Control,
    "redo" => TokenKind::Control, "return" => TokenKind::Control, "yield" => TokenKind::Control,
    "super" => TokenKind::Control, "self" => TokenKind::Keyword, "nil" => TokenKind::Literal,
    "true" => TokenKind::Literal, "false" => TokenKind::Literal,
    "puts" => TokenKind::Type, "print" => TokenKind::Type, "printf" => TokenKind::Type,
    "gets" => TokenKind::Type, "chomp" => TokenKind::Type, "to_s" => TokenKind::Type,
    "to_i" => TokenKind::Type, "to_f" => TokenKind::Type, "each" => TokenKind::Type,
    "map" => TokenKind::Type, "select" => TokenKind::Type, "reduce" => TokenKind::Type,
};

static PHP_KEYWORDS: phf::Map<&'static str, TokenKind> = phf::phf_map! {
    "function" => TokenKind::Keyword, "class" => TokenKind::Keyword, "interface" => TokenKind::Keyword,
    "trait" => TokenKind::Keyword, "namespace" => TokenKind::Keyword, "use" => TokenKind::Keyword,
    "extends" => TokenKind::Keyword, "implements" => TokenKind::Keyword, "abstract" => TokenKind::Keyword,
    "final" => TokenKind::Keyword, "static" => TokenKind::Keyword, "public" => TokenKind::Keyword,
    "private" => TokenKind::Keyword, "protected" => TokenKind::Keyword, "const" => TokenKind::Keyword,
    "return" => TokenKind::Keyword, "require" => TokenKind::Keyword, "require_once" => TokenKind::Keyword,
    "include" => TokenKind::Keyword, "include_once" => TokenKind::Keyword, "echo" => TokenKind::Keyword,
    "print" => TokenKind::Keyword, "var" => TokenKind::Keyword, "global" => TokenKind::Keyword,
    "if" => TokenKind::Control, "elseif" => TokenKind::Control, "else" => TokenKind::Control,
    "endif" => TokenKind::Control, "for" => TokenKind::Control, "foreach" => TokenKind::Control,
    "while" => TokenKind::Control, "endwhile" => TokenKind::Control, "do" => TokenKind::Control,
    "switch" => TokenKind::Control, "case" => TokenKind::Control, "default" => TokenKind::Control,
    "endswitch" => TokenKind::Control, "break" => TokenKind::Control, "continue" => TokenKind::Control,
    "try" => TokenKind::Control, "catch" => TokenKind::Control, "finally" => TokenKind::Control,
    "throw" => TokenKind::Control, "new" => TokenKind::Control, "clone" => TokenKind::Control,
    "true" => TokenKind::Literal, "false" => TokenKind::Literal, "null" => TokenKind::Literal,
    "array" => TokenKind::Type, "string" => TokenKind::Type, "int" => TokenKind::Type,
    "float" => TokenKind::Type, "bool" => TokenKind::Type, "object" => TokenKind::Type,
    "callable" => TokenKind::Type, "iterable" => TokenKind::Type, "void" => TokenKind::Type,
    "self" => TokenKind::Type, "parent" => TokenKind::Type,
};

static SWIFT_KEYWORDS: phf::Map<&'static str, TokenKind> = phf::phf_map! {
    "func" => TokenKind::Keyword, "class" => TokenKind::Keyword, "struct" => TokenKind::Keyword,
    "enum" => TokenKind::Keyword, "protocol" => TokenKind::Keyword, "extension" => TokenKind::Keyword,
    "let" => TokenKind::Keyword, "var" => TokenKind::Keyword, "typealias" => TokenKind::Keyword,
    "import" => TokenKind::Keyword, "module" => TokenKind::Keyword, "precedencegroup" => TokenKind::Keyword,
    "associatedtype" => TokenKind::Keyword, "where" => TokenKind::Keyword,
    "if" => TokenKind::Control, "else" => TokenKind::Control, "guard" => TokenKind::Control,
    "for" => TokenKind::Control, "in" => TokenKind::Control, "while" => TokenKind::Control,
    "repeat" => TokenKind::Control, "switch" => TokenKind::Control, "case" => TokenKind::Control,
    "default" => TokenKind::Control, "break" => TokenKind::Control, "continue" => TokenKind::Control,
    "fallthrough" => TokenKind::Control, "return" => TokenKind::Control, "throw" => TokenKind::Control,
    "do" => TokenKind::Control, "catch" => TokenKind::Control, "defer" => TokenKind::Control,
    "try" => TokenKind::Control, "await" => TokenKind::Control, "async" => TokenKind::Keyword,
    "true" => TokenKind::Literal, "false" => TokenKind::Literal, "nil" => TokenKind::Literal,
    "String" => TokenKind::Type, "Int" => TokenKind::Type, "Double" => TokenKind::Type,
    "Float" => TokenKind::Type, "Bool" => TokenKind::Type, "Array" => TokenKind::Type,
    "Dictionary" => TokenKind::Type, "Set" => TokenKind::Type, "Optional" => TokenKind::Type,
    "Result" => TokenKind::Type, "Void" => TokenKind::Type, "Any" => TokenKind::Type,
    "Self" => TokenKind::Type,
};

static KOTLIN_KEYWORDS: phf::Map<&'static str, TokenKind> = phf::phf_map! {
    "fun" => TokenKind::Keyword, "class" => TokenKind::Keyword, "object" => TokenKind::Keyword,
    "interface" => TokenKind::Keyword, "package" => TokenKind::Keyword, "import" => TokenKind::Keyword,
    "val" => TokenKind::Keyword, "var" => TokenKind::Keyword, "const" => TokenKind::Keyword,
    "abstract" => TokenKind::Keyword, "final" => TokenKind::Keyword, "override" => TokenKind::Keyword,
    "open" => TokenKind::Keyword, "inner" => TokenKind::Keyword, "sealed" => TokenKind::Keyword,
    "data" => TokenKind::Keyword, "inline" => TokenKind::Keyword, "tailrec" => TokenKind::Keyword,
    "operator" => TokenKind::Keyword, "infix" => TokenKind::Keyword, "external" => TokenKind::Keyword,
    "if" => TokenKind::Control, "else" => TokenKind::Control, "when" => TokenKind::Control,
    "for" => TokenKind::Control, "while" => TokenKind::Control, "do" => TokenKind::Control,
    "as" => TokenKind::Control, "is" => TokenKind::Control, "in" => TokenKind::Control,
    "break" => TokenKind::Control, "continue" => TokenKind::Control, "return" => TokenKind::Control,
    "throw" => TokenKind::Control, "try" => TokenKind::Control, "catch" => TokenKind::Control,
    "finally" => TokenKind::Control, "true" => TokenKind::Literal, "false" => TokenKind::Literal,
    "null" => TokenKind::Literal, "this" => TokenKind::Keyword, "super" => TokenKind::Keyword,
    "String" => TokenKind::Type, "Int" => TokenKind::Type, "Long" => TokenKind::Type,
    "Short" => TokenKind::Type, "Byte" => TokenKind::Type, "Double" => TokenKind::Type,
    "Float" => TokenKind::Type, "Boolean" => TokenKind::Type, "Char" => TokenKind::Type,
    "Any" => TokenKind::Type, "Unit" => TokenKind::Type, "Nothing" => TokenKind::Type,
    "List" => TokenKind::Type, "Map" => TokenKind::Type, "Set" => TokenKind::Type,
    "Array" => TokenKind::Type, "MutableList" => TokenKind::Type, "MutableMap" => TokenKind::Type,
};

static XML_KEYWORDS: phf::Map<&'static str, TokenKind> = phf::phf_map! {
    "xml" => TokenKind::Keyword,
    "true" => TokenKind::Literal, "false" => TokenKind::Literal,
};

static SQL_KEYWORDS: phf::Map<&'static str, TokenKind> = phf::phf_map! {
    "SELECT" => TokenKind::Keyword, "FROM" => TokenKind::Keyword, "WHERE" => TokenKind::Keyword,
    "INSERT" => TokenKind::Keyword, "INTO" => TokenKind::Keyword, "VALUES" => TokenKind::Keyword,
    "UPDATE" => TokenKind::Keyword, "SET" => TokenKind::Keyword, "DELETE" => TokenKind::Keyword,
    "CREATE" => TokenKind::Keyword, "ALTER" => TokenKind::Keyword, "DROP" => TokenKind::Keyword,
    "TABLE" => TokenKind::Keyword, "INDEX" => TokenKind::Keyword, "VIEW" => TokenKind::Keyword,
    "TRIGGER" => TokenKind::Keyword, "PROCEDURE" => TokenKind::Keyword, "FUNCTION" => TokenKind::Keyword,
    "DATABASE" => TokenKind::Keyword, "SCHEMA" => TokenKind::Keyword,
    "JOIN" => TokenKind::Keyword, "INNER" => TokenKind::Keyword, "LEFT" => TokenKind::Keyword,
    "RIGHT" => TokenKind::Keyword, "OUTER" => TokenKind::Keyword, "CROSS" => TokenKind::Keyword,
    "ON" => TokenKind::Keyword, "GROUP" => TokenKind::Keyword, "BY" => TokenKind::Keyword,
    "HAVING" => TokenKind::Keyword, "ORDER" => TokenKind::Keyword, "ASC" => TokenKind::Keyword,
    "DESC" => TokenKind::Keyword, "LIMIT" => TokenKind::Keyword, "OFFSET" => TokenKind::Keyword,
    "UNION" => TokenKind::Keyword, "ALL" => TokenKind::Keyword, "DISTINCT" => TokenKind::Keyword,
    "AS" => TokenKind::Keyword, "NULL" => TokenKind::Keyword, "NOT" => TokenKind::Keyword,
    "AND" => TokenKind::Control, "OR" => TokenKind::Control, "XOR" => TokenKind::Control,
    "EXISTS" => TokenKind::Control, "BETWEEN" => TokenKind::Control, "LIKE" => TokenKind::Control,
    "IN" => TokenKind::Control, "IS" => TokenKind::Control, "CASE" => TokenKind::Control,
    "WHEN" => TokenKind::Control, "THEN" => TokenKind::Control, "ELSE" => TokenKind::Control,
    "END" => TokenKind::Control, "PRIMARY" => TokenKind::Keyword, "KEY" => TokenKind::Keyword,
    "FOREIGN" => TokenKind::Keyword, "REFERENCES" => TokenKind::Keyword, "UNIQUE" => TokenKind::Keyword,
    "CHECK" => TokenKind::Keyword, "DEFAULT" => TokenKind::Keyword, "AUTO_INCREMENT" => TokenKind::Keyword,
    "INT" => TokenKind::Type, "INTEGER" => TokenKind::Type, "VARCHAR" => TokenKind::Type,
    "CHAR" => TokenKind::Type, "TEXT" => TokenKind::Type, "BOOLEAN" => TokenKind::Type,
    "BOOL" => TokenKind::Type, "DATE" => TokenKind::Type, "TIME" => TokenKind::Type,
    "DATETIME" => TokenKind::Type, "TIMESTAMP" => TokenKind::Type, "DECIMAL" => TokenKind::Type,
    "FLOAT" => TokenKind::Type, "DOUBLE" => TokenKind::Type, "REAL" => TokenKind::Type,
    "BLOB" => TokenKind::Type, "CLOB" => TokenKind::Type,
};

static LUA_KEYWORDS: phf::Map<&'static str, TokenKind> = phf::phf_map! {
    "function" => TokenKind::Keyword, "local" => TokenKind::Keyword, "end" => TokenKind::Keyword,
    "return" => TokenKind::Keyword, "break" => TokenKind::Control, "do" => TokenKind::Control,
    "if" => TokenKind::Control, "then" => TokenKind::Control, "else" => TokenKind::Control,
    "elseif" => TokenKind::Control, "for" => TokenKind::Control, "in" => TokenKind::Control,
    "while" => TokenKind::Control, "repeat" => TokenKind::Control, "until" => TokenKind::Control,
    "goto" => TokenKind::Control, "nil" => TokenKind::Literal, "true" => TokenKind::Literal,
    "false" => TokenKind::Literal, "and" => TokenKind::Control, "or" => TokenKind::Control,
    "not" => TokenKind::Control, "self" => TokenKind::Keyword,
    "print" => TokenKind::Type, "type" => TokenKind::Type, "pairs" => TokenKind::Type,
    "ipairs" => TokenKind::Type, "tonumber" => TokenKind::Type, "tostring" => TokenKind::Type,
    "string" => TokenKind::Type, "table" => TokenKind::Type, "math" => TokenKind::Type,
    "io" => TokenKind::Type, "os" => TokenKind::Type, "coroutine" => TokenKind::Type,
};

static R_KEYWORDS: phf::Map<&'static str, TokenKind> = phf::phf_map! {
    "function" => TokenKind::Keyword, "return" => TokenKind::Keyword, "if" => TokenKind::Control,
    "else" => TokenKind::Control, "for" => TokenKind::Control, "while" => TokenKind::Control,
    "repeat" => TokenKind::Control, "break" => TokenKind::Control, "next" => TokenKind::Control,
    "TRUE" => TokenKind::Literal, "FALSE" => TokenKind::Literal, "NULL" => TokenKind::Literal,
    "NA" => TokenKind::Literal, "NaN" => TokenKind::Literal, "Inf" => TokenKind::Literal,
    "library" => TokenKind::Keyword, "require" => TokenKind::Keyword, "source" => TokenKind::Keyword,
    "data" => TokenKind::Type, "list" => TokenKind::Type, "vector" => TokenKind::Type,
    "matrix" => TokenKind::Type, "data.frame" => TokenKind::Type, "factor" => TokenKind::Type,
    "array" => TokenKind::Type, "c" => TokenKind::Type, "cbind" => TokenKind::Type,
    "rbind" => TokenKind::Type, "dim" => TokenKind::Type, "length" => TokenKind::Type,
    "names" => TokenKind::Type, "class" => TokenKind::Type, "str" => TokenKind::Type,
    "summary" => TokenKind::Type, "plot" => TokenKind::Type, "ggplot" => TokenKind::Type,
};

static PERL_KEYWORDS: phf::Map<&'static str, TokenKind> = phf::phf_map! {
    "sub" => TokenKind::Keyword, "my" => TokenKind::Keyword, "our" => TokenKind::Keyword,
    "local" => TokenKind::Keyword, "use" => TokenKind::Keyword, "require" => TokenKind::Keyword,
    "package" => TokenKind::Keyword, "import" => TokenKind::Keyword, "export" => TokenKind::Keyword,
    "if" => TokenKind::Control, "unless" => TokenKind::Control, "else" => TokenKind::Control,
    "elsif" => TokenKind::Control, "for" => TokenKind::Control, "foreach" => TokenKind::Control,
    "while" => TokenKind::Control, "until" => TokenKind::Control, "do" => TokenKind::Control,
    "given" => TokenKind::Control, "when" => TokenKind::Control, "default" => TokenKind::Control,
    "try" => TokenKind::Control, "catch" => TokenKind::Control, "finally" => TokenKind::Control,
    "return" => TokenKind::Control, "last" => TokenKind::Control, "next" => TokenKind::Control,
    "redo" => TokenKind::Control, "goto" => TokenKind::Control,
    "true" => TokenKind::Literal, "false" => TokenKind::Literal, "undef" => TokenKind::Literal,
    "print" => TokenKind::Type, "say" => TokenKind::Type, "warn" => TokenKind::Type,
    "die" => TokenKind::Type, "open" => TokenKind::Type, "close" => TokenKind::Type,
    "read" => TokenKind::Type, "write" => TokenKind::Type,
};

static ZIG_KEYWORDS: phf::Map<&'static str, TokenKind> = phf::phf_map! {
    "fn" => TokenKind::Keyword, "const" => TokenKind::Keyword, "var" => TokenKind::Keyword,
    "comptime" => TokenKind::Keyword, "extern" => TokenKind::Keyword, "packed" => TokenKind::Keyword,
    "pub" => TokenKind::Keyword, "export" => TokenKind::Keyword,
    "inline" => TokenKind::Keyword, "noinline" => TokenKind::Keyword, "linksection" => TokenKind::Keyword,
    "callconv" => TokenKind::Keyword, "align" => TokenKind::Keyword, "section" => TokenKind::Keyword,
    "if" => TokenKind::Control, "else" => TokenKind::Control, "switch" => TokenKind::Control,
    "for" => TokenKind::Control, "while" => TokenKind::Control, "break" => TokenKind::Control,
    "continue" => TokenKind::Control, "return" => TokenKind::Control, "defer" => TokenKind::Control,
    "errdefer" => TokenKind::Control, "try" => TokenKind::Control, "catch" => TokenKind::Control,
    "test" => TokenKind::Keyword, "struct" => TokenKind::Keyword, "enum" => TokenKind::Keyword,
    "union" => TokenKind::Keyword, "opaque" => TokenKind::Keyword, "error" => TokenKind::Keyword,
    "true" => TokenKind::Literal, "false" => TokenKind::Literal, "null" => TokenKind::Literal,
    "undefined" => TokenKind::Literal,
    "i8" => TokenKind::Type, "i16" => TokenKind::Type, "i32" => TokenKind::Type,
    "i64" => TokenKind::Type, "i128" => TokenKind::Type, "u8" => TokenKind::Type,
    "u16" => TokenKind::Type, "u32" => TokenKind::Type, "u64" => TokenKind::Type,
    "u128" => TokenKind::Type, "f16" => TokenKind::Type, "f32" => TokenKind::Type,
    "f64" => TokenKind::Type, "f80" => TokenKind::Type, "f128" => TokenKind::Type,
    "bool" => TokenKind::Type, "void" => TokenKind::Type, "noreturn" => TokenKind::Type,
    "type" => TokenKind::Type, "anytype" => TokenKind::Type, "anyerror" => TokenKind::Type,
};

static TOML_KEYWORDS: phf::Map<&'static str, TokenKind> = phf::phf_map! {
    "true" => TokenKind::Literal, "false" => TokenKind::Literal,
    "inf" => TokenKind::Literal, "nan" => TokenKind::Literal,
};

static MARKDOWN_KEYWORDS: phf::Map<&'static str, TokenKind> = phf::phf_map! {
    // Markdown doesn't have keywords in the traditional sense
    // This is used for basic recognition
};

#[derive(Clone)]
pub struct HighlightedLine<S = Style> {
    pub spans: Rc<Vec<(S, String)>>,
    pub version: u64,      // For cache invalidation
    pub access_order: u64, // For LRU eviction
}

pub struct Highlighter<S: HighlightStyle> {
    // Cache for highlighted lines
    line_cache: HashMap<usize, HighlightedLine<S>>,
    // Syntax generation for cache invalidation
    pub(crate) syntax_version: u64,
    // Counter for LRU tracking
    access_counter: u64,
    // Current syntax for language-specific highlighting
    current_syntax: Option<String>,
}

impl<S: HighlightStyle> Highlighter<S> {
    pub fn new() -> Self {
        Self {
            line_cache: HashMap::new(),
            syntax_version: 0,
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
        if self.current_syntax.as_deref() == syntax_name {
            return;
        }
        self.current_syntax = syntax_name.map(|s| s.to_string());
        // Clear cache when syntax changes
        self.line_cache.clear();
        self.syntax_version += 1;
    }

    pub fn invalidate_cache_from_line(&mut self, start_line: usize) {
        // Remove cached lines from start_line onwards. Lines before the
        // edit stay textually valid (highlighting is per-line, with no
        // cross-line state), and any line shifted by an insert/delete is at
        // or after start_line and thus evicted — so the retained entries
        // remain correct. Do NOT bump `syntax_version` here: that would mark
        // the retained entries stale and clear the whole cache on every
        // edit.
        self.invalidate_cache_range(start_line, usize::MAX);
    }

    /// Only stable line numbers outside this range may retain their cached highlighting.
    pub fn invalidate_cache_range(&mut self, start: usize, end: usize) {
        if end.saturating_sub(start) <= self.line_cache.len() {
            for line in start..end {
                self.line_cache.remove(&line);
            }
        } else {
            self.line_cache
                .retain(|&line, _| line < start || line >= end);
        }
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
        let to_remove = self.line_cache.len() - target;
        entries.select_nth_unstable_by_key(to_remove, |&(_, order)| order);
        for &(line, _) in entries.iter().take(to_remove) {
            self.line_cache.remove(&line);
        }
    }

    pub fn highlight_line(&mut self, line_num: usize, line_text: &str) -> Rc<Vec<(S, String)>> {
        // Check cache first — return Rc clone (cheap) instead of deep clone
        if let Some(cached) = self.line_cache.get_mut(&line_num) {
            if cached.version == self.syntax_version {
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
                version: self.syntax_version,
                access_order: self.access_counter,
            },
        );

        // LRU eviction if cache is too large
        self.evict_lru();

        spans
    }

    fn highlight_simple(&self, line: &str) -> Vec<(S, String)> {
        let Some(keyword_map) = self.current_keyword_map() else {
            return if line.is_empty() {
                Vec::new()
            } else {
                vec![(S::default(), line.to_owned())]
            };
        };
        let mut result: Vec<(S, String)> = Vec::new();
        let mut buf = String::new();
        let mut buf_style = S::default();

        // Flush accumulated buffer into result, starting fresh with new style.
        // Adjacent same-style writes merge into one span automatically.
        let flush =
            |result: &mut Vec<(S, String)>, buf: &mut String, buf_style: &mut S, new_style: S| {
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

        let default_style = S::default();
        let string_style = S::from_kind(HighlightKind::String);
        let comment_style = S::from_kind(HighlightKind::Comment);
        let number_style = S::from_kind(HighlightKind::Literal);

        let mut chars = line.chars();

        while let Some(ch) = chars.next() {
            if ch.is_alphanumeric() || ch == '_' {
                let start = line.len() - chars.as_str().len() - ch.len_utf8();
                while let Some(next_ch) = chars.clone().next() {
                    if next_ch.is_alphanumeric() || next_ch == '_' {
                        chars.next();
                    } else {
                        break;
                    }
                }
                let word = &line[start..line.len() - chars.as_str().len()];
                let style = Self::lookup_keyword(keyword_map, word);
                if style != buf_style {
                    flush(&mut result, &mut buf, &mut buf_style, style);
                }
                buf.push_str(word);
                continue;
            }

            match ch {
                quote @ ('"' | '\'') => {
                    if buf_style != string_style {
                        flush(&mut result, &mut buf, &mut buf_style, string_style);
                    }
                    buf.push(quote);
                    let tail = chars.as_str();
                    let end = match tail.find(quote) {
                        None => tail.len(),
                        Some(byte)
                            if tail[..byte]
                                .bytes()
                                .rev()
                                .take_while(|&b| b == b'\\')
                                .count()
                                % 2
                                == 0 =>
                        {
                            byte + 1
                        }
                        Some(byte) => {
                            // UTF-8 continuation bytes cannot contain ASCII quotes or backslashes.
                            let mut escaped = false;
                            let mut end = tail.len();
                            for (offset, &ch) in tail.as_bytes()[byte + 1..].iter().enumerate() {
                                if ch == quote as u8 && !escaped {
                                    end = byte + offset + 2;
                                    break;
                                }
                                escaped = ch == b'\\' && !escaped;
                            }
                            end
                        }
                    };
                    buf.push_str(&tail[..end]);
                    chars = tail[end..].chars();
                }
                '/' if chars.clone().next() == Some('/') => {
                    chars.next();
                    if buf_style != comment_style {
                        flush(&mut result, &mut buf, &mut buf_style, comment_style);
                    }
                    buf.push_str("//");
                    buf.push_str(chars.as_str());
                    break;
                }
                '/' if chars.clone().next() == Some('*') => {
                    chars.next();
                    if buf_style != comment_style {
                        flush(&mut result, &mut buf, &mut buf_style, comment_style);
                    }
                    buf.push_str("/*");
                    let tail = chars.as_str();
                    let end = tail.find("*/").map_or(tail.len(), |byte| byte + 2);
                    buf.push_str(&tail[..end]);
                    chars = tail[end..].chars();
                    flush(&mut result, &mut buf, &mut buf_style, default_style);
                }
                '#' => {
                    if buf_style != comment_style {
                        flush(&mut result, &mut buf, &mut buf_style, comment_style);
                    }
                    buf.push('#');
                    buf.push_str(chars.as_str());
                    break;
                }
                '-' if chars.clone().next() == Some('-') => {
                    chars.next();
                    if buf_style != comment_style {
                        flush(&mut result, &mut buf, &mut buf_style, comment_style);
                    }
                    buf.push_str("--");
                    buf.push_str(chars.as_str());
                    break;
                }
                c if c.is_ascii_digit() => {
                    if buf_style != number_style {
                        flush(&mut result, &mut buf, &mut buf_style, number_style);
                    }
                    buf.push(c);
                    while let Some(next_ch) = chars.clone().next() {
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
    fn current_keyword_map(&self) -> Option<&'static phf::Map<&'static str, TokenKind>> {
        Some(match self.current_syntax.as_deref() {
            Some("Rust") => &RUST_KEYWORDS,
            Some("Python") => &PYTHON_KEYWORDS,
            Some("JavaScript") | Some("TypeScript") => &JS_KEYWORDS,
            Some("Go") => &GO_KEYWORDS,
            Some("Shell Script (Bash)") | Some("Bourne Again Shell (bash)") => &SHELL_KEYWORDS,
            Some("C") | Some("C++") => &C_KEYWORDS,
            Some("JSON") => &JSON_KEYWORDS,
            Some("YAML") => &YAML_KEYWORDS,
            Some("Dockerfile") => &DOCKERFILE_KEYWORDS,
            Some("HTML") => &HTML_KEYWORDS,
            Some("CSS") => &CSS_KEYWORDS,
            Some("Java") => &JAVA_KEYWORDS,
            Some("Ruby") => &RUBY_KEYWORDS,
            Some("PHP") => &PHP_KEYWORDS,
            Some("Swift") => &SWIFT_KEYWORDS,
            Some("Kotlin") => &KOTLIN_KEYWORDS,
            Some("XML") => &XML_KEYWORDS,
            Some("SQL") => &SQL_KEYWORDS,
            Some("Lua") => &LUA_KEYWORDS,
            Some("R") => &R_KEYWORDS,
            Some("Perl") => &PERL_KEYWORDS,
            Some("Zig") => &ZIG_KEYWORDS,
            Some("TOML") => &TOML_KEYWORDS,
            Some("Markdown") => &MARKDOWN_KEYWORDS,
            _ => return None,
        })
    }

    #[inline]
    fn lookup_keyword(map: &phf::Map<&'static str, TokenKind>, word: &str) -> S {
        match map.get(word) {
            Some(kind) => S::from_kind(*kind),
            None => S::default(),
        }
    }
}

impl<S: HighlightStyle> Default for Highlighter<S> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(all(test, feature = "terminal"))]
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
        let map = h.current_keyword_map().unwrap();
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
    #[test]
    fn bulk_line_comments_preserve_unicode_and_line_endings() {
        for marker in ["//", "#", "--"] {
            for tail in [
                "",
                " comment 界e\u{301}",
                " \"quoted\" /* nested */\r\n",
                "\u{1b}[31m",
            ] {
                let mut highlighter = SyntaxHighlighter::new();
                highlighter.set_syntax(Some("Rust"));
                let text = format!("{marker}{tail}");
                let spans = highlighter.highlight_line(0, &text);
                assert_eq!(*spans, vec![(Style::default().fg(Color::DarkGray), text)]);
            }
        }
    }
    #[test]
    fn quoted_slices_match_the_original_escape_state_machine() {
        for quote in ['"', '\''] {
            for tail in [
                format!("a\\{quote}").repeat(1000),
                "simple 界e\u{301}".to_string(),
                "\\".into(),
                "\\\\".into(),
                format!("odd \\{quote}b"),
                format!("even \\\\{quote}b"),
                format!("odd \\\\\\{quote}b"),
                "\0\t\n".into(),
            ] {
                for closed in [false, true] {
                    let prefix = "let value = ";
                    let line = format!(
                        "{prefix}{quote}{tail}{}",
                        if closed {
                            format!("{quote}; suffix")
                        } else {
                            String::new()
                        }
                    );
                    let rest = &line[prefix.len() + 1..];
                    let mut escaped = false;
                    let mut end = rest.len();
                    for (byte, ch) in rest.char_indices() {
                        if ch == quote && !escaped {
                            end = byte + ch.len_utf8();
                            break;
                        }
                        escaped = ch == '\\' && !escaped;
                    }
                    let expected = format!("{quote}{}", &rest[..end]);
                    let mut h = SyntaxHighlighter::new();
                    h.set_syntax(Some("Rust"));
                    let spans = h.highlight_line(0, &line);
                    let first = spans
                        .iter()
                        .find(|(style, _)| style.fg == Some(Color::Green))
                        .unwrap();
                    assert_eq!(first.1, expected, "{line:?}");
                }
            }
        }
    }

    #[test]
    fn block_comment_slices_preserve_termination_and_following_text() {
        for (text, comment, rest) in [
            ("/* */ suffix", "/* */", " suffix"),
            ("/* 界e\u{301} **/ suffix", "/* 界e\u{301} **/", " suffix"),
            ("/*unterminated", "/*unterminated", ""),
            ("/*/*nested*/ suffix", "/*/*nested*/", " suffix"),
        ] {
            let mut h = SyntaxHighlighter::new();
            h.set_syntax(Some("Rust"));
            let spans = h.highlight_line(0, text);
            let mut expected = vec![(Style::default().fg(Color::DarkGray), comment.to_owned())];
            if !rest.is_empty() {
                expected.push((Style::default(), rest.to_owned()));
            }
            assert_eq!(*spans, expected);
        }
    }

    #[test]
    fn partial_lru_selection_evicts_exactly_the_oldest_entries() {
        let mut h = SyntaxHighlighter::new();
        for line in 0..MAX_CACHE_ENTRIES {
            h.highlight_line(line, "");
        }
        let warm = h.highlight_line(0, "");
        h.highlight_line(MAX_CACHE_ENTRIES, "");
        assert_eq!(h.line_cache.len(), MAX_CACHE_ENTRIES * 3 / 4);
        assert!(Rc::ptr_eq(&warm, &h.line_cache[&0].spans));
        let removed = MAX_CACHE_ENTRIES + 1 - MAX_CACHE_ENTRIES * 3 / 4;
        for line in 1..=removed {
            assert!(!h.line_cache.contains_key(&line));
        }
        for line in removed + 1..=MAX_CACHE_ENTRIES {
            assert!(h.line_cache.contains_key(&line));
        }
    }
    #[test]
    fn bounded_invalidation_keeps_unaffected_highlighting() {
        let mut h = SyntaxHighlighter::new();
        let old: Vec<_> = (0..3).map(|line| h.highlight_line(line, "text")).collect();
        h.invalidate_cache_range(1, 2);
        assert!(Rc::ptr_eq(&old[0], &h.highlight_line(0, "text")));
        assert!(!Rc::ptr_eq(&old[1], &h.highlight_line(1, "changed")));
        assert!(Rc::ptr_eq(&old[2], &h.highlight_line(2, "text")));
        h.invalidate_cache_from_line(1);
        assert!(Rc::ptr_eq(&old[0], &h.highlight_line(0, "text")));
        assert!(!Rc::ptr_eq(&old[2], &h.highlight_line(2, "new")));
    }
    #[test]
    fn unchanged_syntax_keeps_cached_highlighting_and_generation() {
        let mut h = SyntaxHighlighter::new();
        h.set_syntax(Some("Rust"));
        let old = h.highlight_line(0, "fn value");
        let version = h.syntax_version;
        h.set_syntax(Some("Rust"));
        assert_eq!(h.syntax_version, version);
        assert!(Rc::ptr_eq(&old, &h.highlight_line(0, "fn value")));
        h.set_syntax(Some("Python"));
        assert_ne!(h.syntax_version, version);
        assert!(!Rc::ptr_eq(&old, &h.highlight_line(0, "fn value")));
    }
}
