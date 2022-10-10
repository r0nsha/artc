mod cursor;
pub mod lexer;
mod source;
mod unescape;

use crate::span::Span;
use std::fmt::Display;
use ustr::{ustr, Ustr};

#[derive(Debug, Clone, Copy)]
pub struct Token {
    pub kind: TokenKind,
    pub lexeme: Ustr,
    pub span: Span,
}

impl Token {
    pub fn name(&self) -> Ustr {
        match &self.kind {
            TokenKind::Ident(name) => *name,
            TokenKind::Str(value) => ustr(value),
            _ => panic!("BUG! only call get_name for identifiers and strings"),
        }
    }
}

impl Display for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.kind)
    }
}

#[derive(strum_macros::Display, Debug, PartialEq, Clone, Copy)]
pub enum TokenKind {
    // Delimiters
    OpenParen,
    CloseParen,
    OpenCurly,
    CloseCurly,
    OpenBracket,
    CloseBracket,
    Colon,
    At,
    Semicolon,
    Newline,
    Eof,

    // Operators
    Plus,
    PlusEq,
    Minus,
    MinusEq,
    Star,
    StarEq,
    FwSlash,
    FwSlashEq,
    Percent,
    PercentEq,
    QuestionMark,
    Comma,
    Amp,
    AmpEq,
    AmpAmp,
    AmpAmpEq,
    Bar,
    BarEq,
    BarBar,
    BarBarEq,
    Caret,
    CaretEq,
    Bang,
    BangEq,
    Eq,
    EqEq,
    Lt,
    LtEq,
    LtLt,
    LtLtEq,
    Gt,
    GtEq,
    GtGt,
    GtGtEq,
    Dot,
    DotDotDot,
    RightArrow,

    // Keywords
    If,
    Else,
    Loop,
    While,
    For,
    Break,
    Continue,
    Return,
    Let,
    Type,
    Fn,
    Use,
    Extern,
    Pub,
    Mut,
    In,
    As,
    Struct,
    Union,
    Match,
    Comptime,

    // Accessors
    Placeholder,
    Ident(Ustr),

    // Literals
    Nil,
    True,
    False,
    Int(u128),
    Float(f64),
    Str(Ustr),
    Char(char),
}

impl From<&str> for TokenKind {
    fn from(s: &str) -> Self {
        use TokenKind::*;

        match s {
            "nil" => Nil,
            "true" => True,
            "false" => False,
            "if" => If,
            "else" => Else,
            "loop" => Loop,
            "while" => While,
            "for" => For,
            "break" => Break,
            "continue" => Continue,
            "return" => Return,
            "let" => Let,
            "type" => Type,
            "fn" => Fn,
            "use" => Use,
            "extern" => Extern,
            "pub" => Pub,
            "mut" => Mut,
            "in" => In,
            "as" => As,
            "struct" => Struct,
            "union" => Union,
            "match" => Match,
            "comptime" => Comptime,
            "_" => Placeholder,
            s => Ident(ustr(s)),
        }
    }
}

impl TokenKind {
    pub fn lexeme(&self) -> &str {
        use TokenKind::*;

        match self {
            At => "@",
            Semicolon => ";",
            Newline => "{newline}",
            Colon => ":",
            OpenParen => "(",
            CloseParen => ")",
            OpenCurly => "{",
            CloseCurly => "}",
            OpenBracket => "[",
            CloseBracket => "]",
            Plus => "+",
            PlusEq => "+=",
            Minus => "-",
            MinusEq => "-=",
            Star => "*",
            StarEq => "*=",
            FwSlash => "/",
            FwSlashEq => "/=",
            Percent => "%",
            PercentEq => "%=",
            QuestionMark => "?",
            Comma => ",",
            Amp => "&",
            AmpEq => "&=",
            AmpAmp => "&&",
            AmpAmpEq => "&&=",
            Bar => "|",
            BarEq => "|=",
            BarBar => "||",
            BarBarEq => "||=",
            Caret => "^",
            CaretEq => "^=",
            Bang => "!",
            BangEq => "!=",
            Eq => "=",
            EqEq => "==",
            Lt => "<",
            LtEq => "<=",
            LtLt => "<<",
            LtLtEq => "<<=",
            Gt => ">",
            GtEq => ">=",
            GtGt => ">>",
            GtGtEq => ">>=",
            Dot => ".",
            DotDotDot => "...",
            RightArrow => "->",
            If => "if",
            Else => "else",
            Loop => "loop",
            While => "while",
            For => "for",
            Break => "break",
            Continue => "continue",
            Return => "return",
            Let => "let",
            Type => "type",
            Fn => "fn",
            Use => "use",
            Extern => "extern",
            Pub => "pub",
            Mut => "mut",
            In => "in",
            As => "as",
            Struct => "struct",
            Comptime => "comptime",
            Union => "union",
            Match => "match",
            Placeholder => "_",
            Ident(_) => "identifier",
            Nil => "nil",
            True => "true",
            False => "false",
            Int(_) => "{integer}",
            Float(_) => "{float}",
            Str(_) => "{string}",
            Char(_) => "{char}",
            Eof => "EOF",
        }
    }

    pub fn is_expr_start(&self) -> bool {
        use TokenKind::*;

        matches!(
            self,
            OpenParen
                | OpenCurly
                | OpenBracket
                | Plus
                | Minus
                | Star
                | Amp
                | Bang
                | If
                | While
                | For
                | Break
                | Continue
                | Return
                | Let
                | Fn
                | Extern
                | Pub
                | Struct
                | Union
                | Match
                | Placeholder
                | Ident(_)
                | Nil
                | True
                | False
                | Int(_)
                | Float(_)
                | Str(_)
                | Char(_)
        )
    }
}
