import io
import os
import tempfile
import unittest
from contextlib import redirect_stdout

from nxl.interpreter import Interpreter


class InterpreterTests(unittest.TestCase):
    def test_basic_arithmetic(self):
        interpreter = Interpreter()
        with redirect_stdout(io.StringIO()) as stream:
            result = interpreter.run("let x = 10; let y = 20; print(x + y);")
        self.assertIsNone(result)
        self.assertIn("30", stream.getvalue())

    def test_assignment_and_if(self):
        interpreter = Interpreter()
        with redirect_stdout(io.StringIO()) as stream:
            interpreter.run("let x = 5; x = x + 2; if x > 6 { print(x); }")
        self.assertIn("7", stream.getvalue())

    def test_array_index(self):
        interpreter = Interpreter()
        with redirect_stdout(io.StringIO()) as stream:
            interpreter.run("let items = [10, 20, 30]; print(items[1]);")
        self.assertIn("20", stream.getvalue())

    def test_run_file(self):
        interpreter = Interpreter()
        with tempfile.NamedTemporaryFile("w", suffix=".nxl", delete=False) as handle:
            handle.write("let x = 11; print(x + 1);")
            path = handle.name
        try:
            with redirect_stdout(io.StringIO()) as stream:
                interpreter.run_file(path)
            self.assertIn("12", stream.getvalue())
        finally:
            os.unlink(path)


if __name__ == "__main__":
    unittest.main()
