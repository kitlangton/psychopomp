//! Syntax highlighting for code on screen: a small lexer for TypeScript, TSX,
//! Rust, SQL, and JSON that splits source into lines of role-tagged tokens,
//! and [`SyntaxTheme`]s that give every role an explicit sRGB colour.
//!
//! It is a lexer, not a parser: roles come from the token and its neighbours
//! (an identifier before `(` is a call, before `:` a property, capitalised a
//! type). Block comments and multi-line strings carry across lines, so a
//! snippet cut from a real file colours the way the editor shows it.

/// What a token is, for colour.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Role {
    Plain,
    Keyword,
    Type,
    Function,
    String,
    Number,
    Comment,
    Property,
    Punctuation,
    Operator,
    /// A TSX element name.
    Tag,
    /// A TSX attribute name.
    Attribute,
    /// `true`, `null`, `ALL_CAPS`, and other literals by name.
    Constant,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Token {
    pub text: String,
    pub role: Role,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Lang {
    TypeScript,
    Tsx,
    Rust,
    Sql,
    Json,
    /// No colour beyond comments, strings, and numbers.
    Plain,
}

impl Lang {
    /// The language for a file extension (`ts`, `tsx`, `rs`, `sql`, `json`).
    pub fn from_extension(extension: &str) -> Self {
        match extension {
            "ts" | "mts" | "cts" | "js" | "mjs" => Self::TypeScript,
            "tsx" | "jsx" => Self::Tsx,
            "rs" => Self::Rust,
            "sql" => Self::Sql,
            "json" | "jsonc" => Self::Json,
            _ => Self::Plain,
        }
    }

    fn keywords(self) -> &'static [&'static str] {
        match self {
            Self::TypeScript | Self::Tsx => TS_KEYWORDS,
            Self::Rust => RUST_KEYWORDS,
            Self::Sql => SQL_KEYWORDS,
            Self::Json | Self::Plain => &[],
        }
    }

    fn builtin_types(self) -> &'static [&'static str] {
        match self {
            Self::TypeScript | Self::Tsx => TS_TYPES,
            Self::Rust => RUST_TYPES,
            Self::Sql => SQL_TYPES,
            Self::Json | Self::Plain => &[],
        }
    }

    fn constants(self) -> &'static [&'static str] {
        match self {
            Self::TypeScript | Self::Tsx => &["true", "false", "null", "undefined"],
            Self::Rust => &["true", "false", "None", "Some", "Ok", "Err", "self", "Self"],
            Self::Sql => &["null", "true", "false"],
            Self::Json => &["true", "false", "null"],
            Self::Plain => &[],
        }
    }

    fn line_comment(self) -> Option<&'static str> {
        match self {
            Self::TypeScript | Self::Tsx | Self::Rust | Self::Json => Some("//"),
            Self::Sql => Some("--"),
            Self::Plain => None,
        }
    }

    fn case_insensitive(self) -> bool {
        self == Self::Sql
    }
}

const TS_KEYWORDS: &[&str] = &[
    "as",
    "async",
    "await",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "declare",
    "default",
    "delete",
    "do",
    "else",
    "enum",
    "export",
    "extends",
    "finally",
    "for",
    "from",
    "function",
    "if",
    "implements",
    "import",
    "in",
    "instanceof",
    "interface",
    "keyof",
    "let",
    "new",
    "of",
    "private",
    "public",
    "readonly",
    "return",
    "satisfies",
    "static",
    "switch",
    "this",
    "throw",
    "try",
    "type",
    "typeof",
    "var",
    "void",
    "while",
    "yield",
];
const TS_TYPES: &[&str] = &[
    "string", "number", "boolean", "bigint", "symbol", "unknown", "never", "any", "object",
];
const RUST_KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref",
    "return", "static", "struct", "super", "trait", "type", "unsafe", "use", "where", "while",
];
const RUST_TYPES: &[&str] = &[
    "bool", "char", "str", "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64",
    "i128", "isize", "f32", "f64",
];
const SQL_KEYWORDS: &[&str] = &[
    "add",
    "alter",
    "and",
    "as",
    "asc",
    "by",
    "case",
    "column",
    "constraint",
    "create",
    "default",
    "delete",
    "desc",
    "distinct",
    "drop",
    "else",
    "end",
    "exists",
    "foreign",
    "from",
    "group",
    "having",
    "if",
    "in",
    "index",
    "inner",
    "insert",
    "into",
    "is",
    "join",
    "key",
    "left",
    "limit",
    "not",
    "on",
    "or",
    "order",
    "primary",
    "references",
    "rename",
    "returning",
    "select",
    "set",
    "table",
    "then",
    "to",
    "union",
    "unique",
    "update",
    "values",
    "when",
    "where",
    "with",
];
const SQL_TYPES: &[&str] = &[
    "bigint",
    "binary",
    "blob",
    "boolean",
    "char",
    "date",
    "datetime",
    "decimal",
    "enum",
    "float",
    "int",
    "integer",
    "json",
    "smallint",
    "text",
    "timestamp",
    "tinyint",
    "varchar",
];

/// What carries over a line break.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum Carry {
    #[default]
    Code,
    BlockComment,
    /// Inside a string opened by this quote (template literals and raw text).
    String(char),
}

/// Split `source` into lines of tokens. Joining every line's token text gives
/// the line back exactly; tabs are left to the caller.
pub fn lex(lang: Lang, source: &str) -> Vec<Vec<Token>> {
    let mut carry = Carry::Code;
    let mut in_tag = false;
    source
        .split('\n')
        .map(|line| {
            let (tokens, next, tag) = lex_line(lang, line, carry, in_tag);
            carry = next;
            in_tag = tag;
            tokens
        })
        .collect()
}

/// Tokens of one line, given what the previous line left open.
fn lex_line(
    lang: Lang,
    line: &str,
    mut carry: Carry,
    mut in_tag: bool,
) -> (Vec<Token>, Carry, bool) {
    let chars = line.chars().collect::<Vec<_>>();
    let mut out = Vec::new();
    let mut i = 0;
    let text = |from: usize, to: usize| chars[from..to].iter().collect::<String>();
    while i < chars.len() {
        match carry {
            Carry::BlockComment => {
                let end = find(&chars, i, "*/").map_or(chars.len(), |at| at + 2);
                push(&mut out, text(i, end), Role::Comment);
                if end <= chars.len() && find(&chars, i, "*/").is_some() {
                    carry = Carry::Code;
                }
                i = end;
                continue;
            }
            Carry::String(quote) => {
                let (end, closed) = string_end(&chars, i, quote);
                push(&mut out, text(i, end), Role::String);
                if closed {
                    carry = Carry::Code;
                }
                i = end;
                continue;
            }
            Carry::Code => {}
        }
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if lang
            .line_comment()
            .is_some_and(|marker| starts_with(&chars, i, marker))
        {
            push(&mut out, text(i, chars.len()), Role::Comment);
            break;
        }
        if lang != Lang::Plain && c == '/' && next == Some('*') {
            carry = Carry::BlockComment;
            push(&mut out, text(i, i + 2), Role::Comment);
            i += 2;
            continue;
        }
        if matches!(c, '"' | '\'' | '`')
            && !(lang == Lang::Rust && c == '\'' && is_lifetime(&chars, i))
        {
            let (end, closed) = string_end(&chars, i + 1, c);
            let role = if lang == Lang::Json && c == '"' && followed_by(&chars, end, ':') {
                Role::Property
            } else {
                Role::String
            };
            push(&mut out, text(i, end), role);
            // Only template literals (and SQL/Rust strings) may span lines.
            if !closed && (c == '`' || matches!(lang, Lang::Rust | Lang::Sql)) {
                carry = Carry::String(c);
            }
            i = end;
            continue;
        }
        if c.is_ascii_digit() || (c == '.' && next.is_some_and(|n| n.is_ascii_digit())) {
            let end = scan(&chars, i, |c| {
                c.is_ascii_alphanumeric() || matches!(c, '_' | '.')
            });
            push(&mut out, text(i, end), Role::Number);
            i = end;
            continue;
        }
        if c.is_alphabetic() || c == '_' || c == '$' || (c == '#' && lang == Lang::Rust) {
            let end = scan(&chars, i + 1, |c| {
                c.is_alphanumeric() || matches!(c, '_' | '$')
            });
            let word = text(i, end);
            let role = word_role(lang, &word, &chars, i, end, in_tag);
            push(&mut out, word, role);
            i = end;
            continue;
        }
        if lang == Lang::Tsx && c == '<' && next.is_some_and(|n| n.is_alphabetic() || n == '/') {
            let name_start = if next == Some('/') { i + 2 } else { i + 1 };
            let end = scan(&chars, name_start, |c| {
                c.is_alphanumeric() || matches!(c, '.' | '_' | '-')
            });
            if end > name_start {
                push(&mut out, text(i, name_start), Role::Punctuation);
                push(&mut out, text(name_start, end), Role::Tag);
                in_tag = true;
                i = end;
                continue;
            }
        }
        if lang == Lang::Tsx && in_tag && c == '>' {
            in_tag = false;
        }
        let role = if "{}()[];,.:".contains(c) {
            Role::Punctuation
        } else if "=+-*/%<>!&|^~?@".contains(c) {
            Role::Operator
        } else {
            Role::Plain
        };
        push(&mut out, c.to_string(), role);
        i += 1;
    }
    (out, carry, in_tag)
}

fn word_role(
    lang: Lang,
    word: &str,
    chars: &[char],
    start: usize,
    end: usize,
    in_tag: bool,
) -> Role {
    let folded = word.to_ascii_lowercase();
    let key = if lang.case_insensitive() {
        folded.as_str()
    } else {
        word
    };
    let member = start > 0 && chars[start - 1] == '.';
    let next = chars[end..].iter().copied().find(|c| !c.is_whitespace());
    let immediately = chars.get(end).copied();
    let after_next = chars[end..]
        .iter()
        .position(|c| !c.is_whitespace())
        .and_then(|at| chars.get(end + at + 1).copied());
    if lang == Lang::Tsx && in_tag && !member && next == Some('=') && after_next != Some('=') {
        return Role::Attribute;
    }
    if lang == Lang::Rust && word.starts_with('#') {
        return Role::Keyword;
    }
    if lang.constants().contains(&key) && !member {
        return Role::Constant;
    }
    if lang.keywords().contains(&key) && !member {
        return Role::Keyword;
    }
    if lang.builtin_types().contains(&key) {
        return Role::Type;
    }
    if next == Some('(') || (lang == Lang::Rust && immediately == Some('!')) {
        return Role::Function;
    }
    if lang == Lang::Json {
        return Role::Plain;
    }
    // `name:` and `name?:` in objects and types; Rust paths (`a::b`) are not.
    let colon = immediately == Some(':') && chars.get(end + 1) != Some(&':');
    let optional = immediately == Some('?') && chars.get(end + 1) == Some(&':');
    if (colon || optional) && lang != Lang::Sql {
        return Role::Property;
    }
    if member {
        return Role::Property;
    }
    if word.len() > 1
        && word
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        && word.chars().any(|c| c.is_ascii_uppercase())
        && lang != Lang::Sql
    {
        return Role::Constant;
    }
    if word.chars().next().is_some_and(char::is_uppercase) && lang != Lang::Sql {
        return Role::Type;
    }
    Role::Plain
}

fn push(out: &mut Vec<Token>, text: String, role: Role) {
    if text.is_empty() {
        return;
    }
    // Whitespace joins whatever it follows, so a line has few tokens.
    let blank = text.chars().all(char::is_whitespace);
    match out.last_mut() {
        Some(last) if last.role == role || blank => last.text.push_str(&text),
        // Leading indentation takes the role of the first real token.
        Some(last) if last.text.chars().all(char::is_whitespace) => {
            last.text.push_str(&text);
            last.role = role;
        }
        _ => out.push(Token { text, role }),
    }
}

fn scan(chars: &[char], from: usize, keep: impl Fn(char) -> bool) -> usize {
    chars[from.min(chars.len())..]
        .iter()
        .position(|&c| !keep(c))
        .map_or(chars.len(), |at| from + at)
}

fn starts_with(chars: &[char], at: usize, marker: &str) -> bool {
    marker
        .chars()
        .enumerate()
        .all(|(k, m)| chars.get(at + k) == Some(&m))
}

fn find(chars: &[char], from: usize, marker: &str) -> Option<usize> {
    (from..chars.len()).find(|&at| starts_with(chars, at, marker))
}

/// Where a string that opened before `from` ends, and whether it closed.
fn string_end(chars: &[char], from: usize, quote: char) -> (usize, bool) {
    let mut i = from;
    while i < chars.len() {
        if chars[i] == '\\' {
            i += 2;
            continue;
        }
        if chars[i] == quote {
            return (i + 1, true);
        }
        i += 1;
    }
    (chars.len(), false)
}

fn followed_by(chars: &[char], from: usize, target: char) -> bool {
    chars[from.min(chars.len())..]
        .iter()
        .find(|c| !c.is_whitespace())
        == Some(&target)
}

/// A Rust lifetime or label (`'a`), not a char literal (`'a'`).
fn is_lifetime(chars: &[char], at: usize) -> bool {
    chars.get(at + 1).is_some_and(|c| c.is_alphabetic()) && chars.get(at + 2) != Some(&'\'')
}

/// A colour for every role, plus the surfaces code sits on. Colours are
/// explicit sRGB, so they bypass the render theme and come out as written.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SyntaxTheme {
    pub name: &'static str,
    pub background: [u8; 3],
    /// Line numbers.
    pub gutter: [u8; 3],
    /// The bar behind focused lines.
    pub focus: [u8; 3],
    /// Bars behind inserted and deleted lines.
    pub inserted: [u8; 3],
    pub deleted: [u8; 3],
    pub plain: [u8; 3],
    pub keyword: [u8; 3],
    pub kind: [u8; 3],
    pub function: [u8; 3],
    pub string: [u8; 3],
    pub number: [u8; 3],
    pub comment: [u8; 3],
    pub property: [u8; 3],
    pub punctuation: [u8; 3],
    pub operator: [u8; 3],
    pub tag: [u8; 3],
    pub attribute: [u8; 3],
    pub constant: [u8; 3],
    /// Inserted and deleted ink, for diff signs.
    pub add: [u8; 3],
    pub remove: [u8; 3],
    /// How bright an unfocused line stays. Ink on paper fades into the
    /// ground sooner than light on dark, so the day theme dims less.
    pub dim: f32,
}

impl SyntaxTheme {
    /// Strong dark: saturated, distinct hues on near-black, so a line never
    /// reads as one grey run.
    pub const fn night() -> Self {
        Self {
            name: "night",
            background: [11, 13, 18],
            gutter: [78, 86, 104],
            focus: [30, 36, 50],
            inserted: [20, 52, 36],
            deleted: [62, 24, 32],
            plain: [232, 234, 240],
            keyword: [255, 92, 138],
            kind: [92, 212, 255],
            function: [255, 209, 102],
            string: [155, 229, 100],
            number: [255, 159, 67],
            comment: [112, 121, 140],
            property: [138, 180, 255],
            punctuation: [140, 147, 163],
            operator: [199, 146, 234],
            tag: [255, 92, 138],
            attribute: [255, 209, 102],
            constant: [255, 159, 67],
            add: [110, 231, 150],
            remove: [255, 107, 107],
            dim: 0.28,
        }
    }

    /// Ink on warm paper: carbon, vermilion, and Prussian blue.
    pub const fn day() -> Self {
        Self {
            name: "day",
            background: [244, 239, 230],
            gutter: [160, 152, 140],
            focus: [232, 223, 206],
            inserted: [214, 230, 208],
            deleted: [240, 210, 200],
            plain: [27, 26, 24],
            keyword: [200, 48, 24],
            kind: [29, 78, 137],
            function: [122, 62, 157],
            string: [46, 111, 64],
            number: [176, 108, 12],
            comment: [138, 132, 122],
            property: [60, 72, 110],
            punctuation: [110, 105, 97],
            operator: [200, 48, 24],
            tag: [200, 48, 24],
            attribute: [122, 62, 157],
            constant: [176, 108, 12],
            add: [46, 111, 64],
            remove: [196, 43, 35],
            dim: 0.42,
        }
    }

    pub fn color(&self, role: Role) -> [u8; 3] {
        match role {
            Role::Plain => self.plain,
            Role::Keyword => self.keyword,
            Role::Type => self.kind,
            Role::Function => self.function,
            Role::String => self.string,
            Role::Number => self.number,
            Role::Comment => self.comment,
            Role::Property => self.property,
            Role::Punctuation => self.punctuation,
            Role::Operator => self.operator,
            Role::Tag => self.tag,
            Role::Attribute => self.attribute,
            Role::Constant => self.constant,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roles(lang: Lang, line: &str) -> Vec<(String, Role)> {
        lex(lang, line)
            .remove(0)
            .into_iter()
            .map(|t| (t.text, t.role))
            .collect()
    }

    fn role_of(lang: Lang, source: &str, word: &str) -> Role {
        lex(lang, source)
            .into_iter()
            .flatten()
            // Neighbours of one role merge ("pub fn "), so match a word in a token.
            .find(|t| t.text.trim() == word || t.text.split_whitespace().any(|w| w == word))
            .unwrap_or_else(|| panic!("no token {word:?} in {source:?}"))
            .role
    }

    #[test]
    fn every_line_round_trips() {
        let source = "type Event = {\n  body: | { tag: \"said\" } /* a\nb */ x\n}\n`a\n${b}`";
        for lang in [
            Lang::TypeScript,
            Lang::Tsx,
            Lang::Rust,
            Lang::Sql,
            Lang::Json,
            Lang::Plain,
        ] {
            let lines = lex(lang, source);
            let joined = lines
                .iter()
                .map(|line| line.iter().map(|t| t.text.as_str()).collect::<String>())
                .collect::<Vec<_>>()
                .join("\n");
            assert_eq!(joined, source, "{lang:?}");
        }
    }

    #[test]
    fn typescript_types_have_distinct_roles() {
        let line = r#"  source: ConnectionRef | ClockID // stable"#;
        assert_eq!(
            roles(Lang::TypeScript, line),
            vec![
                ("  source".into(), Role::Property),
                (": ".into(), Role::Punctuation),
                ("ConnectionRef ".into(), Role::Type),
                ("| ".into(), Role::Operator),
                ("ClockID ".into(), Role::Type),
                ("// stable".into(), Role::Comment),
            ]
        );
        let ts = "type Grant = { op: \"read\" | \"write\"; mint(c, 1) }";
        assert_eq!(role_of(Lang::TypeScript, ts, "type"), Role::Keyword);
        assert_eq!(role_of(Lang::TypeScript, ts, "Grant"), Role::Type);
        assert_eq!(role_of(Lang::TypeScript, ts, "\"read\""), Role::String);
        assert_eq!(role_of(Lang::TypeScript, ts, "mint"), Role::Function);
        assert_eq!(role_of(Lang::TypeScript, ts, "1"), Role::Number);
        assert_eq!(
            role_of(Lang::TypeScript, "thread?: string", "thread"),
            Role::Property
        );
        assert_eq!(role_of(Lang::TypeScript, "x: string", "string"), Role::Type);
        assert_eq!(
            role_of(Lang::TypeScript, "a.default", "default"),
            Role::Property
        );
        assert_eq!(
            role_of(Lang::TypeScript, "MAX_ROWS = 3", "MAX_ROWS"),
            Role::Constant
        );
        assert_eq!(
            role_of(Lang::TypeScript, "x = null", "null"),
            Role::Constant
        );
    }

    #[test]
    fn block_comments_and_template_literals_carry_across_lines() {
        let lines = lex(Lang::TypeScript, "a /* one\ntwo */ b\nconst s = `x\ny` + z");
        assert_eq!(lines[1][0].text, "two */ ");
        assert_eq!(lines[1][0].role, Role::Comment);
        assert_eq!(lines[1].last().unwrap().role, Role::Plain);
        assert_eq!(lines[3][0].text.trim(), "y`");
        assert_eq!(lines[3][0].role, Role::String);
        assert_eq!(lines[3].last().unwrap().text, "z");
    }

    #[test]
    fn tsx_tags_and_attributes() {
        let line = r#"<Match when={props.slug === "box"}>"#;
        assert_eq!(role_of(Lang::Tsx, line, "Match"), Role::Tag);
        assert_eq!(role_of(Lang::Tsx, line, "when"), Role::Attribute);
        assert_eq!(role_of(Lang::Tsx, line, "slug"), Role::Property);
        assert_eq!(role_of(Lang::Tsx, "</div>", "div"), Role::Tag);
        assert_eq!(role_of(Lang::Tsx, "a < b", "b"), Role::Plain);
    }

    #[test]
    fn sql_is_case_insensitive() {
        let line = "CREATE TABLE agent (id varchar(30) NOT NULL) -- PR 4";
        assert_eq!(role_of(Lang::Sql, line, "CREATE"), Role::Keyword);
        assert_eq!(role_of(Lang::Sql, line, "varchar"), Role::Type);
        assert_eq!(role_of(Lang::Sql, line, "NULL"), Role::Constant);
        assert_eq!(role_of(Lang::Sql, line, "agent"), Role::Function);
        assert_eq!(role_of(Lang::Sql, line, "-- PR 4"), Role::Comment);
        assert_eq!(role_of(Lang::Sql, "select Id from t", "Id"), Role::Plain);
    }

    #[test]
    fn rust_macros_paths_and_lifetimes() {
        let line = "pub fn f<'a>(x: &'a str) -> Option<u64> { println!(\"{x}\"); Self::new() }";
        assert_eq!(role_of(Lang::Rust, line, "pub"), Role::Keyword);
        assert_eq!(role_of(Lang::Rust, line, "f"), Role::Plain);
        assert_eq!(role_of(Lang::Rust, line, "str"), Role::Type);
        assert_eq!(role_of(Lang::Rust, line, "Option"), Role::Type);
        assert_eq!(role_of(Lang::Rust, line, "println"), Role::Function);
        assert_eq!(role_of(Lang::Rust, line, "Self"), Role::Constant);
        assert_eq!(role_of(Lang::Rust, line, "new"), Role::Function);
        assert!(
            lex(Lang::Rust, line)[0]
                .iter()
                .all(|t| t.role != Role::String || t.text.contains('"'))
        );
    }

    #[test]
    fn json_keys_are_properties_and_values_strings() {
        let line = r#"{ "slug": "github", "auth": true, "n": 3 }"#;
        assert_eq!(role_of(Lang::Json, line, "\"slug\""), Role::Property);
        assert_eq!(role_of(Lang::Json, line, "\"github\""), Role::String);
        assert_eq!(role_of(Lang::Json, line, "true"), Role::Constant);
        assert_eq!(role_of(Lang::Json, line, "3"), Role::Number);
    }

    #[test]
    fn themes_keep_roles_apart() {
        for theme in [SyntaxTheme::night(), SyntaxTheme::day()] {
            let core = [
                Role::Plain,
                Role::Keyword,
                Role::Type,
                Role::Function,
                Role::String,
                Role::Number,
                Role::Comment,
                Role::Property,
            ]
            .map(|role| theme.color(role));
            for (a, x) in core.iter().enumerate() {
                for y in &core[a + 1..] {
                    assert_ne!(x, y, "{}", theme.name);
                }
            }
            // Every core colour stands clear of the ground (WCAG-ish luminance gap).
            let luma =
                |c: [u8; 3]| 0.2126 * c[0] as f32 + 0.7152 * c[1] as f32 + 0.0722 * c[2] as f32;
            for color in core {
                assert!(
                    (luma(color) - luma(theme.background)).abs() > 70.0,
                    "{} {color:?}",
                    theme.name
                );
            }
        }
    }
}
