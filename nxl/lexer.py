from __future__ import annotations

from dataclasses import dataclass
from typing import List


KEYWORDS = {
    "let": "LET",
    "const": "CONST",
    "mut": "MUT",
    "ref": "REF",
    "borrow": "BORROW",
    "unsafe": "UNSAFE",
    "fn": "FN",
    "async": "ASYNC",
    "await": "AWAIT",
    "spawn": "SPAWN",
    "actor": "ACTOR",
    "channel": "CHANNEL",
    "if": "IF",
    "else": "ELSE",
    "match": "MATCH",
    "when": "WHEN",
    "case": "CASE",
    "for": "FOR",
    "while": "WHILE",
    "loop": "LOOP",
    "break": "BREAK",
    "continue": "CONTINUE",
    "return": "RETURN",
    "yield": "YIELD",
    "try": "TRY",
    "catch": "CATCH",
    "finally": "FINALLY",
    "throw": "THROW",
    "module": "MODULE",
    "import": "IMPORT",
    "export": "EXPORT",
    "from": "FROM",
    "as": "AS",
    "struct": "STRUCT",
    "class": "CLASS",
    "interface": "INTERFACE",
    "trait": "TRAIT",
    "enum": "ENUM",
    "impl": "IMPL",
    "type": "TYPE",
    "use": "USE",
    "where": "WHERE",
    "true": "TRUE",
    "false": "FALSE",
    "null": "NULL",
    "new": "NEW",
    "in": "IN",
    "is": "IS",
    "and": "AND",
    "or": "OR",
    "not": "NOT",
}


SINGLE_CHAR_TOKENS = {
    "(": "LPAREN",
    ")": "RPAREN",
    "{": "LBRACE",
    "}": "RBRACE",
    "[": "LBRACKET",
    "]": "RBRACKET",
    ",": "COMMA",
    ".": "DOT",
    ";": "SEMICOLON",
    ":": "COLON",
    "?": "QUESTION",
}


DOUBLE_CHAR_TOKENS = {
    "==": "EQ",
    "!=": "NEQ",
    "<=": "LTE",
    ">=": "GTE",
    "&&": "AND_AND",
    "||": "OR_OR",
    "=>": "FAT_ARROW",
    "->": "ARROW",
    "..": "RANGE",
    "+=": "PLUS_EQ",
    "-=": "MINUS_EQ",
    "*=": "STAR_EQ",
    "/=": "SLASH_EQ",
    "%=": "PERCENT_EQ",
}


SINGLE_CHAR_OPERATORS = {
    "+": "PLUS",
    "-": "MINUS",
    "*": "STAR",
    "/": "SLASH",
    "%": "PERCENT",
    "=": "ASSIGN",
    "<": "LT",
    ">": "GT",
    "!": "BANG",
    "&": "AMPERSAND",
    "|": "PIPE",
}


@dataclass(frozen=True)
class Token:
    kind: str
    value: str
    line: int
    column: int


class LexerError(ValueError):
    def __init__(self, message: str, line: int, column: int):
        super().__init__(f"{message} at line {line}, column {column}")
        self.message = message
        self.line = line
        self.column = column


class Lexer:
    def __init__(self, source: str):
        self.source = source
        self.index = 0
        self.line = 1
        self.column = 1

    def eof(self) -> bool:
        return self.index >= len(self.source)

    def peek(self, offset: int = 0) -> str:
        position = self.index + offset
        if position >= len(self.source):
            return ""
        return self.source[position]

    def advance(self) -> str:
        ch = self.source[self.index]
        self.index += 1
        if ch == "\n":
            self.line += 1
            self.column = 1
        else:
            self.column += 1
        return ch

    def skip_whitespace_and_comments(self) -> None:
        while not self.eof():
            ch = self.peek()
            if ch in " \t\r":
                self.advance()
                continue
            if ch == "\n":
                self.advance()
                continue
            if ch == "/" and self.peek(1) == "/":
                self.advance()
                self.advance()
                while not self.eof() and self.peek() not in "\r\n":
                    self.advance()
                continue
            if ch == "/" and self.peek(1) == "*":
                self.advance()
                self.advance()
                while not self.eof():
                    if self.peek() == "*" and self.peek(1) == "/":
                        self.advance()
                        self.advance()
                        break
                    self.advance()
                else:
                    raise LexerError("Unterminated block comment", self.line, self.column)
                continue
            break

    def read_identifier_or_keyword(self) -> Token:
        start_line = self.line
        start_column = self.column
        start_index = self.index
        while not self.eof() and (self.peek().isalpha() or self.peek() == "_" or self.peek().isdigit()):
            self.advance()
        text = self.source[start_index:self.index]
        kind = KEYWORDS.get(text, "IDENT")
        return Token(kind, text, start_line, start_column)

    def read_number(self) -> Token:
        start_line = self.line
        start_column = self.column
        start_index = self.index
        seen_decimal = False
        while not self.eof():
            ch = self.peek()
            if ch.isdigit():
                self.advance()
                continue
            if ch == "." and not seen_decimal:
                seen_decimal = True
                self.advance()
                continue
            break
        text = self.source[start_index:self.index]
        if "." in text:
            return Token("FLOAT", text, start_line, start_column)
        return Token("INT", text, start_line, start_column)

    def read_string(self) -> Token:
        quote = self.advance()
        start_line = self.line
        start_column = self.column - 1
        value_chars: List[str] = []
        while not self.eof():
            ch = self.peek()
            if ch == "\\":
                self.advance()
                if self.eof():
                    raise LexerError("Unterminated string escape", self.line, self.column)
                escaped = self.advance()
                escape_map = {
                    "n": "\n",
                    "r": "\r",
                    "t": "\t",
                    "\\": "\\",
                    '"': '"',
                    "'": "'",
                    "`": "`",
                }
                value_chars.append(escape_map.get(escaped, escaped))
                continue
            if ch == quote:
                self.advance()
                return Token("STRING", "".join(value_chars), start_line, start_column)
            if ch == "\n" and quote != "`":
                raise LexerError("Unterminated string literal", self.line, self.column)
            value_chars.append(self.advance())
        raise LexerError("Unterminated string literal", self.line, self.column)

    def read_operator_or_punctuation(self) -> Token:
        start_line = self.line
        start_column = self.column
        two_char = self.peek() + self.peek(1)
        if two_char in DOUBLE_CHAR_TOKENS:
            self.advance()
            self.advance()
            return Token(DOUBLE_CHAR_TOKENS[two_char], two_char, start_line, start_column)

        ch = self.peek()
        if ch in SINGLE_CHAR_TOKENS:
            self.advance()
            return Token(SINGLE_CHAR_TOKENS[ch], ch, start_line, start_column)

        if ch in SINGLE_CHAR_OPERATORS:
            self.advance()
            return Token(SINGLE_CHAR_OPERATORS[ch], ch, start_line, start_column)

        if ch in {"@", "#", "$", "~"}:
            raise LexerError(f"Unexpected character: {ch!r}", start_line, start_column)

        raise LexerError(f"Unexpected character: {ch!r}", start_line, start_column)

    def tokenize(self) -> List[Token]:
        tokens: List[Token] = []
        while not self.eof():
            self.skip_whitespace_and_comments()
            if self.eof():
                break
            ch = self.peek()
            if ch.isalpha() or ch == "_":
                tokens.append(self.read_identifier_or_keyword())
                continue
            if ch.isdigit():
                tokens.append(self.read_number())
                continue
            if ch in ('"', "'", "`"):
                tokens.append(self.read_string())
                continue
            tokens.append(self.read_operator_or_punctuation())
        tokens.append(Token("EOF", "", self.line, self.column))
        return tokens


def lex(source: str) -> List[Token]:
    return Lexer(source).tokenize()
