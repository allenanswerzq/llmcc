import re
import tree_sitter_cpp

from tree_sitter import Node as TsNode
from tree_sitter import Tree as TsTree
from tree_sitter import Language, Parser
from pydantic import BaseModel, ConfigDict, Field
from typing import Dict, Any, Optional, Tuple, Union, Type, List
from abc import ABC, abstractmethod

from llmcc.store import Store
from llmcc.config import *


class Node(BaseModel):
    model_config = ConfigDict(arbitrary_types_allowed=True)
    id: int = Field(default=None, description="unique id for every node")
    name: str = Field(default=None, description="full qualified name of the node.")
    parent: Optional["Node"] = Field(default=None, description="part of the node.")
    ts_node: TsNode = Field(default=None, description="tree sitter node.")
    knowledge_store: Optional[Store] = Field(
        default=None, description="kb related to this node."
    )
    summary_store: Optional[Store] = Field(
        default=None, description="multiple version storage for summary of this node."
    )
    code_store: Optional[Store] = Field(
        default=None, description="multiple version storage for rust code"
    )
    depend_store: Optional[Store] = Field(
        default=None, description="the stuff this node depends."
    )
    slice_store: Optional[Store] = Field(
        default=None, description="the storage save sliced stuff."
    )
    sym_table_store: Optional[Store] = Field(
        default=None, description="symbol table storage"
    )
    children: Optional[List["Node"]] = Field(default=[], description="children node.")

    @property
    def type(self) -> str:
        return self.ts_node.type if self.ts_node else None

    @property
    def text(self) -> str:
        return self.ts_node.text.decode("utf-8") if self.ts_node else None

    @property
    def is_named(self) -> bool:
        return self.ts_node.is_named

    @property
    def start_point(self) -> int:
        return self.ts_node.start_point

    @property
    def end_point(self) -> int:
        return self.ts_node.end_point

    @property
    def rows(self) -> int:
        s = self.start_point
        e = self.end_point
        return e.row - s.row

    @property
    def scope_str(self) -> str:
        scope_dict = {
            "class_specifier": "class",
            "struct_specifier": "struct",
            "enum_specifier": "enum",
            "namespace_definition": "namespace",
        }
        return scope_dict[self.type]

    def child_by_field_name(self, name: str) -> Any:
        return self.ts_node.child_by_field_name(name)

    def is_complex_type(self) -> bool:
        return self.type in [
            "class_specifier",
            "struct_specifier",
            "enum_specifier",
        ]

    def is_function(self) -> bool:
        return self.type in ["function_definition"]


class Visitor(ABC):

    @abstractmethod
    def visit(self, node: Node) -> Any:
        pass


class Context:
    pass


class Scope:
    def __init__(self, root=None, parent: "Scope" = None, child: "Scope" = None):
        self.root = root
        self.nodes = {}
        self.parent = parent
        self.child = child

    def define(self, name, value):
        self.nodes[name] = value

    def resolve(self, name):
        if name in self.nodes:
            return self.nodes[name]
        elif self.parent is not None:
            return self.parent.resolve(name)
        else:
            raise NameError(f"Name '{name}' is not defined in this scope.")

    def get_scope_chain(self) -> List["Scope"]:
        chain = [self]
        start = self
        while start.parent is not None:
            start = start.parent
            chain.append(start)
        chain.pop()
        return chain[::-1]


class Graph(BaseModel):
    model_config = ConfigDict(arbitrary_types_allowed=True)
    root: Node = Field(default=None, description="root node for the ir graph")
    node_map: Dict[str, int] = Field(
        default=None, description="map node name to node id"
    )
    id_map: Dict[int, Node] = Field(default=None, description="map node it to node")
    tree: TsTree = Field(default=None, description="ts tree")
    global_vars: Dict[str, Node] = Field(
        default=None, description="global variable map"
    )

    # def __str__(self):
    #     return str(self.root.ts_node).replace("(", "\n(")

    def accept(self, visitor: "Visitor") -> Any:
        return visitor.visit(self.root)

    def resolve_name(self, name: str, cur: Node, allow_same_level=True) -> List[Node]:
        """Given a name resolve the node in the lowest scope."""
        # log.debug(f"resolving {name} for {cur.name} in level {level}")
        # for k, v in self.node_map.items():
        #     print(k, v)

        level = len(cur.name.split("."))
        if cur.name.endswith(")"):  # NOTE: function
            level -= 1

        # TODO: improve this algorithm
        resolved = []
        for node_name, node_id in self.node_map.items():
            parts = node_name.split(".")
            assert len(parts) > 0
            if parts[-1].startswith("(") and parts[-1].endswith(")"):
                # Function sybmol, We didn't make difference with the overload functions
                parts.pop()
            if (
                parts[-1] == name
                and self.id_map[node_id].id != cur.id
                and (len(parts) < level or (allow_same_level and len(parts) == level))
            ):
                # get a node in the <= level
                resolved.append(self.id_map[node_id])
        return resolved


_id = 0


def create_node(g: Graph, ts_node: TsNode, parent: Node, restart=False) -> Node:
    global _id
    if restart:
        _id = 0
    _id += 1
    # log.warn(f"{_id} {ts_node.type} {ts_node.text}")
    g.id_map[_id] = Node(ts_node=ts_node, parent=parent, id=_id)
    return g.id_map[_id]
