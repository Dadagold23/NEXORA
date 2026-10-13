import unittest

from nxl.lexer import lex
from nxl.parser import parse_tokens


class ParserTests(unittest.TestCase):
    def test_parse_let_and_assignment(self):
        program = parse_tokens(lex("let x = 10; x = x + 1;"))
        self.assertEqual(len(program.statements), 2)

    def test_parse_function(self):
        program = parse_tokens(lex("fn add(a: Int, b: Int) -> Int { return a + b; }"))
        self.assertEqual(program.statements[0].__class__.__name__, "FunctionDecl")
        self.assertEqual(program.statements[0].name, "add")

    def test_parse_if_and_for_loop(self):
        program = parse_tokens(lex("if x > 0 { let y = 1; } else { let z = 2; } for item in items { print(item); }"))
        self.assertEqual(len(program.statements), 2)
        self.assertEqual(program.statements[0].__class__.__name__, "IfStmt")

    def test_parse_call_and_member_access(self):
        program = parse_tokens(lex("print(user.name); result = fetch(data);"))
        self.assertEqual(len(program.statements), 2)


if __name__ == "__main__":
    unittest.main()
