from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Dict, List, Optional


class RuntimeErrorValue(ValueError):
    def __init__(self, message: str):
        super().__init__(message)
        self.message = message


@dataclass
class FunctionValue:
    name: str
    params: List[str]
    body: Any
    closure: "Environment"

    def call(self, interpreter: "Interpreter", args: List[Any]):
        local = Environment(self.closure)
        for name, value in zip(self.params, args):
            local.define(name, value)
        try:
            interpreter.execute_block(self.body, local)
            return None
        except ReturnSignal as signal:
            return signal.value


class ReturnSignal(Exception):
    def __init__(self, value: Any):
        self.value = value


class Environment:
    def __init__(self, parent: Optional["Environment"] = None):
        self.parent = parent
        self.values: Dict[str, Any] = {}

    def define(self, name: str, value: Any):
        self.values[name] = value

    def assign(self, name: str, value: Any):
        if name in self.values:
            self.values[name] = value
            return
        if self.parent is not None:
            self.parent.assign(name, value)
            return
        raise RuntimeErrorValue(f"Undefined variable '{name}'")

    def get(self, name: str):
        if name in self.values:
            return self.values[name]
        if self.parent is not None:
            return self.parent.get(name)
        raise RuntimeErrorValue(f"Undefined variable '{name}'")


class Interpreter:
    def __init__(self):
        self.global_env = Environment()
        self.global_env.define("print", self._print_builtin)
        self.global_env.define("len", self._len_builtin)

    def _print_builtin(self, *values):
        print(*values)
        return None

    def _len_builtin(self, value):
        if isinstance(value, (str, list, tuple, dict)):
            return len(value)
        raise RuntimeErrorValue("len() expects a string, list, tuple, or dictionary")

    def execute_block(self, block, env: Optional[Environment] = None):
        current = env or self.global_env
        for statement in block.statements:
            self.execute_statement(statement, current)

    def execute_statement(self, node, env: Environment):
        kind = node.__class__.__name__
        if kind == "VarDecl":
            value = self.evaluate(node.value, env) if node.value is not None else None
            env.define(node.name, value)
            return
        if kind == "Assignment":
            value = self.evaluate(node.value, env)
            env.assign(node.target.name, value)
            return
        if kind == "FunctionDecl":
            env.define(node.name, FunctionValue(node.name, [p.name for p in node.params], node.body, env))
            return
        if kind == "ReturnStmt":
            value = self.evaluate(node.value, env) if node.value is not None else None
            raise ReturnSignal(value)
        if kind == "ExprStmt":
            self.evaluate(node.expression, env)
            return
        if kind == "IfStmt":
            if self.is_truthy(self.evaluate(node.condition, env)):
                self.execute_block(node.then_block, Environment(env))
            elif node.else_block is not None:
                if hasattr(node.else_block, "statements"):
                    self.execute_block(node.else_block, Environment(env))
                else:
                    self.execute_statement(node.else_block, Environment(env))
            return
        if kind == "ForStmt":
            iterable = self.evaluate(node.iterable, env)
            for item in iterable:
                loop_scope = Environment(env)
                loop_scope.define(node.target, item)
                self.execute_block(node.body, loop_scope)
            return
        if kind == "WhileStmt":
            while self.is_truthy(self.evaluate(node.condition, env)):
                self.execute_block(node.body, Environment(env))
            return
        if kind == "Block":
            self.execute_block(node, env)
            return
        raise RuntimeErrorValue(f"Unsupported statement type: {kind}")

    def evaluate(self, node, env: Environment):
        kind = node.__class__.__name__
        if kind == "Literal":
            return node.value
        if kind == "Identifier":
            return env.get(node.name)
        if kind == "ArrayLiteral":
            return [self.evaluate(item, env) for item in node.items]
        if kind == "TupleLiteral":
            return tuple(self.evaluate(item, env) for item in node.items)
        if kind == "MapLiteral":
            result = {}
            for key_node, value_node in node.entries:
                key = self.evaluate(key_node, env)
                value = self.evaluate(value_node, env)
                result[key] = value
            return result
        if kind == "UnaryExpr":
            operand = self.evaluate(node.operand, env)
            if node.operator == "-":
                return -operand
            if node.operator == "!":
                return not self.is_truthy(operand)
            if node.operator == "not":
                return not self.is_truthy(operand)
            raise RuntimeErrorValue(f"Unsupported unary operator: {node.operator}")
        if kind == "BinaryExpr":
            left = self.evaluate(node.left, env)
            right = self.evaluate(node.right, env)
            operator = node.operator
            if operator == "+":
                return left + right
            if operator == "-":
                return left - right
            if operator == "*":
                return left * right
            if operator == "/":
                return left / right
            if operator == "%":
                return left % right
            if operator == "==":
                return left == right
            if operator == "!=":
                return left != right
            if operator == "<":
                return left < right
            if operator == "<=":
                return left <= right
            if operator == ">":
                return left > right
            if operator == ">=":
                return left >= right
            if operator == "and":
                return self.is_truthy(left) and self.is_truthy(right)
            if operator == "or":
                return self.is_truthy(left) or self.is_truthy(right)
            raise RuntimeErrorValue(f"Unsupported binary operator: {operator}")
        if kind == "CallExpr":
            callee = self.evaluate(node.callee, env)
            args = [self.evaluate(arg, env) for arg in node.arguments]
            if isinstance(callee, FunctionValue):
                return callee.call(self, args)
            if callable(callee):
                return callee(*args)
            raise RuntimeErrorValue("Attempted to call a non-callable value")
        if kind == "Assignment":
            value = self.evaluate(node.value, env)
            if hasattr(node.target, "name"):
                env.assign(node.target.name, value)
                return value
            raise RuntimeErrorValue("Unsupported assignment target")
        if kind == "MemberAccess":
            target = self.evaluate(node.target, env)
            if isinstance(target, dict):
                return target[node.member]
            return getattr(target, node.member)
        if kind == "IndexAccess":
            target = self.evaluate(node.target, env)
            index = self.evaluate(node.index, env)
            return target[index]
        if kind == "Program":
            result = None
            for stmt in node.statements:
                result = self.execute_statement(stmt, env)
            return result
        if kind == "Block":
            result = None
            for stmt in node.statements:
                result = self.execute_statement(stmt, env)
            return result
        if kind == "VarDecl":
            value = self.evaluate(node.value, env) if node.value is not None else None
            env.define(node.name, value)
            return value
        raise RuntimeErrorValue(f"Unsupported expression type: {kind}")

    def is_truthy(self, value: Any) -> bool:
        if value is None:
            return False
        if isinstance(value, bool):
            return value
        return bool(value)

    def run(self, source: str):
        from .lexer import lex
        from .parser import parse_tokens

        program = parse_tokens(lex(source))
        return self.evaluate(program, self.global_env)

    def run_file(self, path: str):
        with open(path, "r", encoding="utf-8") as handle:
            source = handle.read()
        return self.run(source)


def run_source(source: str):
    return Interpreter().run(source)
