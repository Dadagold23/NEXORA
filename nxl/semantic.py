from __future__ import annotations

from dataclasses import dataclass, field
from typing import Dict, List, Optional

from .type_system import TypeSystem


@dataclass
class SourceLocation:
    line: int
    column: int
    end_line: Optional[int] = None
    end_column: Optional[int] = None


@dataclass
class Symbol:
    name: str
    kind: str
    type_name: Optional[str] = None
    location: Optional[SourceLocation] = None


@dataclass
class Scope:
    parent: Optional["Scope"] = None
    symbols: Dict[str, Symbol] = field(default_factory=dict)
    scope_type: str = "block"

    def declare(self, symbol: Symbol):
        if symbol.name in self.symbols:
            raise ValueError(f"Duplicate symbol: {symbol.name}")
        self.symbols[symbol.name] = symbol

    def lookup(self, name: str):
        scope = self
        while scope is not None:
            if name in scope.symbols:
                return scope.symbols[name]
            scope = scope.parent
        return None


@dataclass
class SemanticDiagnostic:
    message: str
    location: Optional[SourceLocation] = None


class SemanticError(ValueError):
    def __init__(self, message: str, location: Optional[SourceLocation] = None):
        super().__init__(message)
        self.message = message
        self.location = location


class SemanticAnalyzer:
    def __init__(self, ast):
        self.ast = ast
        self.root_scope = Scope(scope_type="global")
        self.diagnostics: List[SemanticDiagnostic] = []
        self.type_system = TypeSystem()

    def analyze(self):
        self._analyze_program(self.ast)
        return self.root_scope, self.diagnostics

    def _analyze_program(self, program):
        for statement in program.statements:
            self._analyze_statement(statement, self.root_scope)

    def _analyze_statement(self, node, scope: Scope):
        cls_name = node.__class__.__name__
        if cls_name == "VarDecl":
            self._analyze_var_decl(node, scope)
            return
        if cls_name == "FunctionDecl":
            self._analyze_function_decl(node, scope)
            return
        if cls_name == "IfStmt":
            self._analyze_if(node, scope)
            return
        if cls_name == "ForStmt":
            self._analyze_for(node, scope)
            return
        if cls_name == "WhileStmt":
            self._analyze_while(node, scope)
            return
        if cls_name == "ExprStmt":
            self._analyze_expression(node.expression, scope)
            return
        if cls_name == "ReturnStmt":
            if node.value is not None:
                self._analyze_expression(node.value, scope)
            return
        raise SemanticError(f"Unsupported statement node: {cls_name}")

    def _analyze_var_decl(self, node, scope: Scope):
        if scope.lookup(node.name) is not None:
            raise SemanticError(f"Variable '{node.name}' is already declared in this scope")
        inferred_type = None
        if node.value is not None:
            inferred_type = self._infer_expression_type(node.value, scope)
        if node.type_annotation is not None:
            declared_type = self.type_system.parse_type(node.type_annotation)
            if inferred_type is not None and not self.type_system.same_type(declared_type, inferred_type):
                raise SemanticError(
                    f"Type mismatch: variable '{node.name}' declared as {node.type_annotation} but inferred as {inferred_type}"
                )
            symbol_type = node.type_annotation
        else:
            symbol_type = inferred_type.name if inferred_type is not None else None
        scope.declare(Symbol(node.name, "variable", symbol_type, None))

    def _analyze_function_decl(self, node, scope: Scope):
        if scope.lookup(node.name) is not None:
            raise SemanticError(f"Function '{node.name}' is already declared")
        function_scope = Scope(parent=scope, scope_type="function")
        for param in node.params:
            param_type = param.type_annotation
            if param_type is None:
                param_type = "Unknown"
            param_symbol = Symbol(param.name, "parameter", param_type, None)
            if function_scope.lookup(param.name) is not None:
                raise SemanticError(f"Parameter '{param.name}' is already declared")
            function_scope.declare(param_symbol)
        scope.declare(Symbol(node.name, "function", node.return_type, None))
        for statement in node.body.statements:
            self._analyze_statement(statement, function_scope)

    def _analyze_if(self, node, scope: Scope):
        condition_type = self._infer_expression_type(node.condition, scope)
        if not self.type_system.is_bool(condition_type):
            raise SemanticError("If condition must resolve to Bool")
        then_scope = Scope(parent=scope, scope_type="if")
        for statement in node.then_block.statements:
            self._analyze_statement(statement, then_scope)
        if node.else_block is not None:
            if hasattr(node.else_block, "statements"):
                else_scope = Scope(parent=scope, scope_type="else")
                for statement in node.else_block.statements:
                    self._analyze_statement(statement, else_scope)
            else:
                self._analyze_statement(node.else_block, scope)

    def _analyze_for(self, node, scope: Scope):
        iterable_type = self._infer_expression_type(node.iterable, scope)
        loop_scope = Scope(parent=scope, scope_type="for")
        loop_scope.declare(Symbol(node.target, "variable", "Int", None))
        for statement in node.body.statements:
            self._analyze_statement(statement, loop_scope)

    def _analyze_while(self, node, scope: Scope):
        condition_type = self._infer_expression_type(node.condition, scope)
        if not self.type_system.is_bool(condition_type):
            raise SemanticError("While condition must resolve to Bool")
        while_scope = Scope(parent=scope, scope_type="while")
        for statement in node.body.statements:
            self._analyze_statement(statement, while_scope)

    def _analyze_expression(self, node, scope: Scope):
        cls = node.__class__.__name__
        if cls == "Identifier":
            symbol = scope.lookup(node.name)
            if symbol is None:
                raise SemanticError(f"Unknown identifier '{node.name}'")
            return
        if cls in {"Literal", "TypeRef"}:
            return
        if cls == "BinaryExpr":
            left_type = self._infer_expression_type(node.left, scope)
            right_type = self._infer_expression_type(node.right, scope)
            if node.operator in {"+", "-", "*", "/", "%"}:
                result = self.type_system.arithmetic_result(left_type, right_type, node.operator)
                if result is None:
                    raise SemanticError(f"Operator '{node.operator}' is not valid for types {left_type} and {right_type}")
            elif node.operator in {"==", "!=", "<", "<=", ">", ">="}:
                if node.operator in {"==", "!="}:
                    if not self.type_system.same_type(left_type, right_type):
                        raise SemanticError(f"Comparison '{node.operator}' requires compatible types")
                elif not self.type_system.is_numeric(left_type) or not self.type_system.is_numeric(right_type):
                    raise SemanticError(f"Comparison '{node.operator}' requires numeric operands")
            elif node.operator in {"and", "or"}:
                if not self.type_system.is_bool(left_type) or not self.type_system.is_bool(right_type):
                    raise SemanticError(f"Logical operator '{node.operator}' requires Bool operands")
            return
        if cls == "UnaryExpr":
            operand_type = self._infer_expression_type(node.operand, scope)
            if node.operator in {"-", "+"} and not self.type_system.is_numeric(operand_type):
                raise SemanticError(f"Unary operator '{node.operator}' requires a numeric operand")
            if node.operator in {"!", "not"} and not self.type_system.is_bool(operand_type):
                raise SemanticError(f"Unary operator '{node.operator}' requires a Bool operand")
            return
        if cls == "CallExpr":
            self._analyze_expression(node.callee, scope)
            for arg in node.arguments:
                self._analyze_expression(arg, scope)
            return
        if cls == "MemberAccess":
            self._analyze_expression(node.target, scope)
            return
        if cls == "IndexAccess":
            self._analyze_expression(node.target, scope)
            self._analyze_expression(node.index, scope)
            return
        if cls == "Assignment":
            self._analyze_expression(node.target, scope)
            target_type = self._infer_expression_type(node.target, scope)
            value_type = self._infer_expression_type(node.value, scope)
            if target_type is not None and value_type is not None and not self.type_system.same_type(target_type, value_type):
                raise SemanticError(f"Assignment type mismatch: expected {target_type}, got {value_type}")
            return
        if cls == "ArrayLiteral":
            for item in node.items:
                self._analyze_expression(item, scope)
            return
        if cls == "TupleLiteral":
            for item in node.items:
                self._analyze_expression(item, scope)
            return
        if cls == "MapLiteral":
            for key, value in node.entries:
                self._analyze_expression(key, scope)
                self._analyze_expression(value, scope)
            return
        raise SemanticError(f"Unsupported expression node: {cls}")

    def _infer_expression_type(self, node, scope: Scope):
        cls = node.__class__.__name__
        if cls == "Literal":
            if type(node.value) is int:
                return self.type_system.builtin["Int"]
            if type(node.value) is float:
                return self.type_system.builtin["Float"]
            if type(node.value) is bool:
                return self.type_system.builtin["Bool"]
            if type(node.value) is str:
                return self.type_system.builtin["String"]
            if node.value is None:
                return self.type_system.builtin["Void"]
            return self.type_system.builtin["String"]
        if cls == "Identifier":
            symbol = scope.lookup(node.name)
            if symbol is None:
                raise SemanticError(f"Unknown identifier '{node.name}'")
            return self.type_system.parse_type(symbol.type_name)
        if cls == "BinaryExpr":
            left = self._infer_expression_type(node.left, scope)
            right = self._infer_expression_type(node.right, scope)
            if node.operator in {"+", "-", "*", "/", "%"}:
                return self.type_system.arithmetic_result(left, right, node.operator) or left
            if node.operator in {"==", "!=", "<", "<=", ">", ">="}:
                return self.type_system.builtin["Bool"]
            if node.operator in {"and", "or"}:
                return self.type_system.builtin["Bool"]
            return self.type_system.builtin["Void"]
        if cls == "UnaryExpr":
            operand = self._infer_expression_type(node.operand, scope)
            if node.operator in {"-", "+"}:
                return operand
            if node.operator in {"!", "not"}:
                return self.type_system.builtin["Bool"]
            return self.type_system.builtin["Void"]
        if cls == "ArrayLiteral":
            if not node.items:
                return self.type_system.parse_type("Array<Unknown>")
            first_type = self._infer_expression_type(node.items[0], scope)
            for item in node.items[1:]:
                if not self.type_system.same_type(first_type, self._infer_expression_type(item, scope)):
                    raise SemanticError("Array elements must all share the same type")
            return self.type_system.parse_type(f"Array<{first_type.name}>")
        if cls == "TupleLiteral":
            return self.type_system.parse_type("Tuple")
        if cls == "MapLiteral":
            return self.type_system.parse_type("Map<String, Unknown>")
        if cls == "CallExpr":
            if hasattr(node.callee, "name"):
                symbol = scope.lookup(node.callee.name)
                if symbol is not None and symbol.type_name is not None:
                    return self.type_system.parse_type(symbol.type_name)
            return self.type_system.builtin["Void"]
        if cls == "Assignment":
            return self._infer_expression_type(node.target, scope)
        if cls == "MemberAccess":
            return self.type_system.builtin["Unknown"] if False else self.type_system.builtin["String"]
        if cls == "IndexAccess":
            return self.type_system.builtin["Void"]
        return self.type_system.builtin["Void"]


def analyze_ast(ast):
    analyzer = SemanticAnalyzer(ast)
    return analyzer.analyze()
