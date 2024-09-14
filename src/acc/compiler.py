import ell
import json

from pydantic import BaseModel, Field

from acc.ir import *
from acc.parser import parse
from acc.printer import print_graph
from acc.config import *

class Data(BaseModel):
    explain: str = Field(description="simple explanation of the code.")
    target_code: str = Field(description="target code generated.")

class Compiler(Visitor):

    def __init__(self):
        pass

    @ell.simple(model="gpt-4-turbo")
    def compile_impl(self, node: Node):
        schema = json.dumps(Data.model_json_schema(), indent=4)
        code = node.text.decode('utf-8')
        return [
            ell.system(f"""
            You are a distinguished software developer, give you some c++ code, you will convert it to rust code.
            - Do not add any additional code, only convert the given code to rust.

            **ONLY OUTPUT JSON OBJECT THAT FOLLOWS THIS JSON SCHEMA**
            ```json
            {schema}
            ```
            """),

            ell.user(f"<cpp_code>\n {code} \n</cpp_code>"),
        ]

    def compile(self, node: Node):
        log.info(f"compiling {node.type}")
        for i in range(3):
            try:
                unparsed = self.compile_impl(node)
                if unparsed.startswith("```json"):
                    unparsed = unparsed[7:]
                if unparsed.endswith("```"):
                    unparsed = unparsed[:-3]
                log.debug(unparsed)
                unparsed = json.loads(unparsed)
                parsed = Data.model_validate(unparsed)
                log.debug(parsed)
            except Exception as e:
                log.error(e)
                log.info(f"retrying compiling {node.type}")
                continue
            finally:
                break
        if node.code_store is None:
            node.code_store = Store()
        node.code_store.add_version({
            "parsed": parsed,
            "source_code": node.text.decode('utf-8')
            })

    def visit(self, node: Node) -> Any:
        for child in node.children:
            if hasattr(self, f"visit_{child.type}"):
                getattr(self, f"visit_{child.type}")(child)

    def visit_preproc_ifdef(self, node: Node) -> Any:
        self.visit(node)

    def visit_struct_specifier(self, node: Node) -> Any:
        self.compile(node)

    def visit_declaration(self, node: Node) -> Any:
        self.compile(node)

    def visit_class_specifier(self, node: Node) -> Any:
        self.compile(node)

    def visit_function_definition(self, node: Node) -> Any:
        self.compile(node)


def compile_graph(g: Graph) -> Any:
    compiler = Compiler()
    return g.accept(compiler)
