import unittest

from nxl.lexer import lex
from nxl.parser import parse_tokens
from nxl.semantic import analyze_ast


class SemanticAnalyzerTests(unittest.TestCase):
    def test_valid_program(self):
        program = parse_tokens(lex("let x = 10; fn add(a: Int) -> Int { return a + 1; }"))
        root_scope, diagnostics = analyze_ast(program)
        self.assertEqual(len(diagnostics), 0)
        self.assertIsNotNone(root_scope.lookup("x"))
        self.assertIsNotNone(root_scope.lookup("add"))

    def test_duplicate_variable(self):
        program = parse_tokens(lex("let x = 1; let x = 2;"))
        with self.assertRaises(ValueError):
            analyze_ast(program)

    def test_unknown_identifier(self):
        program = parse_tokens(lex("let x = missing_value;"))
        with self.assertRaises(ValueError):
            analyze_ast(program)

    def test_type_inference_for_numeric_addition(self):
        program = parse_tokens(lex("let x = 10; let y = 20; let z = x + y;"))
        root_scope, diagnostics = analyze_ast(program)
        self.assertEqual(len(diagnostics), 0)
        self.assertEqual(root_scope.lookup("z").type_name, "Int")

    def test_type_mismatch_raise(self):
        program = parse_tokens(lex("let x: Int = \"hello\";"))
        with self.assertRaises(ValueError):
            analyze_ast(program)

    def test_invalid_if_condition(self):
        program = parse_tokens(lex("if 5 { let ok = true; }"))
        with self.assertRaises(ValueError):
            analyze_ast(program)


if __name__ == "__main__":
    unittest.main()
