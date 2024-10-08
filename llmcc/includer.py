import os

from llmcc.ir import *
from llmcc.parser import parse, parse_from_file
from llmcc.config import *
from llmcc.printer import print_graph
from llmcc.store import Store


def search_file(directory, filename) -> str:
    for root, dirs, files in os.walk(directory):
        if filename in files or filename.replace(".h", ".rs") in files:
            return os.path.join(root, filename)


_INCLUDER_REPARSE_OPTION = True


class Includer(Visitor):

    def __init__(self, dir, g):
        self.dir = dir
        self.og = g
        self.include_files = []

    def visit(self, node: Node) -> Graph:
        for child in node.children:
            if hasattr(self, f"visit_{child.type}"):
                getattr(self, f"visit_{child.type}")(child)

        root = self.og.root
        if root.depend_store is None:
            root.depend_store = Store()
        root.depend_store.add_version({"include_files": self.include_files})

        return self.og

    def visit_preproc_ifdef(self, node: Node):
        self.visit(node)

    def visit_preproc_include(self, node: Node):
        include_file = node.children[1].text
        include_file = search_file(self.dir, include_file.replace('"', ""))
        if include_file is None:
            return None
        log.debug(f"found include file {include_file}")
        g = parse_from_file(include_file)
        self.include_files.append(g)
        g = include_graph(g, self.dir)

        if _INCLUDER_REPARSE_OPTION:
            inc_src = g.root.text
            # log.debug(inc_src)
            old_src = self.og.root.text
            # log.debug(old_src)
            inc_len = len(inc_src)
            new_src = inc_src + old_src
            # TODO: make incremental parse work
            # self.og.root.ts_node.edit(
            #     start_byte=0,
            #     old_end_byte=0,
            #     new_end_byte=inc_len,
            #     start_point=(0, 0),
            #     old_end_point=(0, 0),
            #     new_end_point=(0, inc_len),
            # )
            # ng = parse(new_src, self.og.tree)
            ng = parse(new_src)
            self.og = ng


def include_graph(g: Graph, dir: str) -> Graph:
    i = Includer(dir, g)
    return g.accept(i)
