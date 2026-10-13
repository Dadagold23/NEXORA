from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Dict, List, Optional


@dataclass
class Type:
    name: str
    args: Optional[List["Type"]] = None

    def __str__(self) -> str:
        if self.args:
            return f"{self.name}<{', '.join(str(arg) for arg in self.args)}>"
        return self.name


class TypeSystem:
    def __init__(self):
        self.builtin = {
            "Int": Type("Int"),
            "Float": Type("Float"),
            "Bool": Type("Bool"),
            "String": Type("String"),
            "Char": Type("Char"),
            "Byte": Type("Byte"),
            "Void": Type("Void"),
            "Unknown": Type("Unknown"),
            "Date": Type("Date"),
            "Time": Type("Time"),
            "Duration": Type("Duration"),
        }

    def parse_type(self, annotation: Optional[str]) -> Optional[Type]:
        if annotation is None:
            return None
        if annotation in self.builtin:
            return self.builtin[annotation]
        if annotation.startswith("Array<") and annotation.endswith(">"):
            inner = annotation[6:-1]
            return Type("Array", [self.parse_type(inner)])
        if annotation.startswith("Map<") and annotation.endswith(">"):
            inner = annotation[4:-1]
            key_value = inner.split(",", 1)
            if len(key_value) != 2:
                raise ValueError(f"Invalid map type annotation: {annotation}")
            return Type("Map", [self.parse_type(key_value[0]), self.parse_type(key_value[1])])
        if annotation.startswith("Option<") and annotation.endswith(">"):
            inner = annotation[7:-1]
            return Type("Option", [self.parse_type(inner)])
        if annotation.startswith("Result<") and annotation.endswith(">"):
            inner = annotation[7:-1]
            left, right = inner.split(",", 1)
            return Type("Result", [self.parse_type(left), self.parse_type(right)])
        if annotation.startswith("Function<") and annotation.endswith(">"):
            inner = annotation[9:-1]
            parts = [part.strip() for part in inner.split(",") if part.strip()]
            return Type("Function", [self.parse_type(p) for p in parts])
        return Type(annotation)

    def same_type(self, left: Optional[Type], right: Optional[Type]) -> bool:
        if left is None or right is None:
            return left is None and right is None
        if left.name != right.name:
            return False
        if left.args is None and right.args is None:
            return True
        if left.args is None or right.args is None:
            return False
        if len(left.args) != len(right.args):
            return False
        return all(self.same_type(a, b) for a, b in zip(left.args, right.args))

    def is_numeric(self, type_obj: Optional[Type]) -> bool:
        return type_obj is not None and type_obj.name in {"Int", "Float"}

    def is_bool(self, type_obj: Optional[Type]) -> bool:
        return type_obj is not None and type_obj.name == "Bool"

    def is_string(self, type_obj: Optional[Type]) -> bool:
        return type_obj is not None and type_obj.name == "String"

    def common_numeric(self, left: Optional[Type], right: Optional[Type]) -> Optional[Type]:
        if self.is_numeric(left) and self.is_numeric(right):
            if left.name == "Float" or right.name == "Float":
                return self.builtin["Float"]
            return self.builtin["Int"]
        return None

    def arithmetic_result(self, left: Optional[Type], right: Optional[Type], op: str) -> Optional[Type]:
        if op in {"+", "-", "*", "/", "%"}:
            return self.common_numeric(left, right)
        if op == "+" and (self.is_string(left) or self.is_string(right)):
            return self.builtin["String"]
        return None
