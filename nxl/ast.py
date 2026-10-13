from __future__ import annotations

from dataclasses import dataclass, field
from typing import List, Optional, Any


@dataclass
class Node:
    pass


@dataclass
class Program(Node):
    statements: List[Node]


@dataclass
class Block(Node):
    statements: List[Node]


@dataclass
class VarDecl(Node):
    name: str
    type_annotation: Optional[str] = None
    value: Optional[Node] = None
    is_const: bool = False


@dataclass
class FunctionDecl(Node):
    name: str
    params: List["Param"]
    return_type: Optional[str]
    body: Block


@dataclass
class Param(Node):
    name: str
    type_annotation: Optional[str] = None


@dataclass
class ReturnStmt(Node):
    value: Optional[Node] = None


@dataclass
class IfStmt(Node):
    condition: Node
    then_block: Block
    else_block: Optional[Block | Node] = None


@dataclass
class ForStmt(Node):
    target: str
    iterable: Node
    body: Block


@dataclass
class WhileStmt(Node):
    condition: Node
    body: Block


@dataclass
class ExprStmt(Node):
    expression: Node


@dataclass
class Assignment(Node):
    target: Node
    operator: str
    value: Node


@dataclass
class BinaryExpr(Node):
    left: Node
    operator: str
    right: Node


@dataclass
class UnaryExpr(Node):
    operator: str
    operand: Node


@dataclass
class CallExpr(Node):
    callee: Node
    arguments: List[Node] = field(default_factory=list)


@dataclass
class MemberAccess(Node):
    target: Node
    member: str


@dataclass
class IndexAccess(Node):
    target: Node
    index: Node


@dataclass
class Identifier(Node):
    name: str


@dataclass
class Literal(Node):
    value: Any


@dataclass
class ArrayLiteral(Node):
    items: List[Node]


@dataclass
class MapLiteral(Node):
    entries: List[tuple[Node, Node]]


@dataclass
class TupleLiteral(Node):
    items: List[Node]


@dataclass
class TypeRef(Node):
    name: str
