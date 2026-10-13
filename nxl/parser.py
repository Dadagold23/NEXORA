from __future__ import annotations

from typing import Optional

from .ast import (
    ArrayLiteral,
    Assignment,
    BinaryExpr,
    Block,
    CallExpr,
    ExprStmt,
    ForStmt,
    FunctionDecl,
    Identifier,
    IfStmt,
    IndexAccess,
    Literal,
    MapLiteral,
    MemberAccess,
    Param,
    Program,
    ReturnStmt,
    TupleLiteral,
    TypeRef,
    UnaryExpr,
    VarDecl,
    WhileStmt,
)


class ParseError(ValueError):
    def __init__(self, message: str, token):
        super().__init__(f"{message} at line {token.line}, column {token.column}")
        self.message = message
        self.token = token


class Parser:
    def __init__(self, tokens):
        self.tokens = tokens
        self.index = 0

    def current(self):
        return self.tokens[self.index]

    def previous(self):
        return self.tokens[self.index - 1]

    def peek(self, offset=1):
        position = self.index + offset
        if position >= len(self.tokens):
            return self.tokens[-1]
        return self.tokens[position]

    def advance(self):
        token = self.tokens[self.index]
        self.index += 1
        return token

    def match(self, *kinds):
        if self.current().kind in kinds:
            self.advance()
            return True
        return False

    def expect(self, kind, message):
        if self.current().kind != kind:
            raise ParseError(message, self.current())
        return self.advance()

    def at_end(self):
        return self.current().kind == "EOF"

    def parse(self):
        return Program(self.parse_block_statements())

    def parse_block_statements(self):
        statements = []
        while not self.at_end() and self.current().kind != "RBRACE":
            statements.append(self.parse_statement())
        return statements

    def parse_statement(self):
        if self.match("LET"):
            return self.parse_var_decl(False)
        if self.match("CONST"):
            return self.parse_var_decl(True)
        if self.match("FN"):
            return self.parse_function_decl()
        if self.match("RETURN"):
            value = None if self.current().kind == "SEMICOLON" else self.parse_expression()
            self.consume_semicolon()
            return ReturnStmt(value)
        if self.match("IF"):
            return self.parse_if_statement()
        if self.match("FOR"):
            return self.parse_for_statement()
        if self.match("WHILE"):
            return self.parse_while_statement()
        if self.match("LBRACE"):
            statements = self.parse_block_statements()
            self.expect("RBRACE", "Expected closing brace")
            return Block(statements)
        expression = self.parse_expression()
        self.consume_semicolon()
        return ExprStmt(expression)

    def parse_var_decl(self, is_const):
        name = self.expect("IDENT", "Expected variable name").value
        type_annotation = None
        if self.match("COLON"):
            type_annotation = self.parse_type_annotation()
        value = None
        if self.match("ASSIGN"):
            value = self.parse_expression()
        self.consume_semicolon()
        return VarDecl(name=name, type_annotation=type_annotation.name if type_annotation else None, value=value, is_const=is_const)

    def parse_function_decl(self):
        name = self.expect("IDENT", "Expected function name").value
        self.expect("LPAREN", "Expected '(' after function name")
        params = []
        if self.current().kind != "RPAREN":
            while True:
                param_name = self.expect("IDENT", "Expected parameter name").value
                type_annotation = None
                if self.match("COLON"):
                    type_annotation = self.parse_type_annotation()
                params.append(Param(param_name, type_annotation.name if type_annotation else None))
                if not self.match("COMMA"):
                    break
        self.expect("RPAREN", "Expected ')' after parameters")
        return_type = None
        if self.match("ARROW"):
            return_type = self.parse_type_annotation().name
        body = self.parse_block()
        return FunctionDecl(name, params, return_type, body)

    def parse_block(self):
        self.expect("LBRACE", "Expected '{'")
        statements = self.parse_block_statements()
        self.expect("RBRACE", "Expected closing brace")
        return Block(statements)

    def parse_if_statement(self):
        condition = self.parse_expression()
        then_block = self.parse_block()
        else_block = None
        if self.match("ELSE"):
            if self.current().kind == "IF":
                else_block = self.parse_if_statement()
            else:
                else_block = self.parse_block()
        return IfStmt(condition, then_block, else_block)

    def parse_for_statement(self):
        target = self.expect("IDENT", "Expected loop variable name").value
        self.expect("IN", "Expected 'in' in for loop")
        iterable = self.parse_expression()
        body = self.parse_block()
        return ForStmt(target, iterable, body)

    def parse_while_statement(self):
        condition = self.parse_expression()
        body = self.parse_block()
        return WhileStmt(condition, body)

    def parse_type_annotation(self):
        token = self.expect("IDENT", "Expected type name")
        type_name = token.value
        if self.match("LT"):
            inner = self.parse_type_annotation()
            self.expect("GT", "Expected '>' after generic type")
            return TypeRef(f"{type_name}<{inner.name}")
        return TypeRef(type_name)

    def consume_semicolon(self):
        if self.current().kind == "SEMICOLON":
            self.advance()

    def parse_expression(self):
        return self.parse_assignment()

    def parse_assignment(self):
        left = self.parse_logical_or()
        if self.current().kind in {"ASSIGN", "PLUS_EQ", "MINUS_EQ", "STAR_EQ", "SLASH_EQ", "PERCENT_EQ"}:
            operator = self.advance().kind
            value = self.parse_assignment()
            return Assignment(left, operator, value)
        return left

    def parse_logical_or(self):
        expr = self.parse_logical_and()
        while self.match("OR_OR"):
            operator = self.previous().value
            right = self.parse_logical_and()
            expr = BinaryExpr(expr, operator, right)
        return expr

    def parse_logical_and(self):
        expr = self.parse_equality()
        while self.match("AND_AND"):
            operator = self.previous().value
            right = self.parse_equality()
            expr = BinaryExpr(expr, operator, right)
        return expr

    def parse_equality(self):
        expr = self.parse_comparison()
        while self.current().kind in {"EQ", "NEQ"}:
            operator = self.advance().value
            right = self.parse_comparison()
            expr = BinaryExpr(expr, operator, right)
        return expr

    def parse_comparison(self):
        expr = self.parse_additive()
        while self.current().kind in {"LT", "LTE", "GT", "GTE"}:
            operator = self.advance().value
            right = self.parse_additive()
            expr = BinaryExpr(expr, operator, right)
        return expr

    def parse_additive(self):
        expr = self.parse_multiplicative()
        while self.current().kind in {"PLUS", "MINUS"}:
            operator = self.advance().value
            right = self.parse_multiplicative()
            expr = BinaryExpr(expr, operator, right)
        return expr

    def parse_multiplicative(self):
        expr = self.parse_unary()
        while self.current().kind in {"STAR", "SLASH", "PERCENT"}:
            operator = self.advance().value
            right = self.parse_unary()
            expr = BinaryExpr(expr, operator, right)
        return expr

    def parse_unary(self):
        if self.current().kind in {"MINUS", "BANG"}:
            operator = self.advance().value
            operand = self.parse_unary()
            return UnaryExpr(operator, operand)
        return self.parse_postfix()

    def parse_postfix(self):
        expr = self.parse_primary()
        while True:
            if self.match("LPAREN"):
                args = []
                if self.current().kind != "RPAREN":
                    while True:
                        args.append(self.parse_expression())
                        if not self.match("COMMA"):
                            break
                self.expect("RPAREN", "Expected ')' after arguments")
                expr = CallExpr(expr, args)
            elif self.match("DOT"):
                member = self.expect("IDENT", "Expected member name").value
                expr = MemberAccess(expr, member)
            elif self.match("LBRACKET"):
                index = self.parse_expression()
                self.expect("RBRACKET", "Expected ']' after index")
                expr = IndexAccess(expr, index)
            else:
                break
        return expr

    def parse_primary(self):
        if self.match("INT"):
            return Literal(int(self.previous().value))
        if self.match("FLOAT"):
            return Literal(float(self.previous().value))
        if self.match("STRING"):
            return Literal(self.previous().value)
        if self.match("TRUE"):
            return Literal(True)
        if self.match("FALSE"):
            return Literal(False)
        if self.match("NULL"):
            return Literal(None)
        if self.match("IDENT"):
            return Identifier(self.previous().value)
        if self.match("LBRACKET"):
            items = []
            if self.current().kind != "RBRACKET":
                while True:
                    items.append(self.parse_expression())
                    if not self.match("COMMA"):
                        break
            self.expect("RBRACKET", "Expected ']' after array literal")
            return ArrayLiteral(items)
        if self.match("LPAREN"):
            expr = self.parse_expression()
            self.expect("RPAREN", "Expected ')' after expression")
            return expr
        raise ParseError("Unexpected token in expression", self.current())


def parse_tokens(tokens):
    parser = Parser(tokens)
    return parser.parse()
