import unittest

from nxl.lexer import LexerError, lex


class LexerTests(unittest.TestCase):
    def test_let_and_number(self):
        tokens = lex("let x = 10;")
        kinds = [token.kind for token in tokens]
        self.assertEqual(kinds, ["LET", "IDENT", "ASSIGN", "INT", "SEMICOLON", "EOF"])
        self.assertEqual(tokens[1].value, "x")
        self.assertEqual(tokens[3].value, "10")

    def test_function_and_arithmetic(self):
        tokens = lex("fn add(a: Int, b: Int) -> Int { return a + b; }")
        kinds = [token.kind for token in tokens]
        self.assertEqual(
            kinds,
            [
                "FN",
                "IDENT",
                "LPAREN",
                "IDENT",
                "COLON",
                "IDENT",
                "COMMA",
                "IDENT",
                "COLON",
                "IDENT",
                "RPAREN",
                "ARROW",
                "IDENT",
                "LBRACE",
                "RETURN",
                "IDENT",
                "PLUS",
                "IDENT",
                "SEMICOLON",
                "RBRACE",
                "EOF",
            ],
        )

    def test_strings_and_comments(self):
        source = '// comment\nlet msg = "hello\\nworld"; /* block */\n'
        tokens = lex(source)
        kinds = [token.kind for token in tokens]
        self.assertEqual(kinds, ["LET", "IDENT", "ASSIGN", "STRING", "SEMICOLON", "EOF"])
        self.assertEqual(tokens[3].value, "hello\nworld")

    def test_float_and_operators(self):
        tokens = lex("1.5 == 2.0 && !true")
        kinds = [token.kind for token in tokens]
        self.assertEqual(kinds, ["FLOAT", "EQ", "FLOAT", "AND_AND", "BANG", "TRUE", "EOF"])

    def test_invalid_character(self):
        with self.assertRaises(LexerError):
            lex("let @ = 1")


if __name__ == "__main__":
    unittest.main()
