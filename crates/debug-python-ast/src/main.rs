use tree_sitter::Parser;

fn main() {
    let language = tree_sitter_python::LANGUAGE.into();

    let mut parser = Parser::new();
    parser.set_language(&language).unwrap();

    // Extended Python code to trigger more node types
    let source = b"
def hello(name: str) -> str:
    return \"Hello, \" + name

@decorator
@another_decorator
def decorated_func():
    pass

class MyClass(Base):
    x = 5

    def __init__(self, y: int = 10):
        self.y = y

    def method(self, arg):
        pass

    @property
    def prop(self):
        return self.x

import os
import sys
from pathlib import Path
from typing import List, Dict, Optional
from os.path import join as path_join

x = 5
y = x + 1
z = (1, 2, 3)
a = [1, 2, 3]
d = {\"key\": \"value\"}
result = func(arg1, arg2, key=value)

if x > 5:
    pass
elif x < 0:
    pass
else:
    pass

try:
    pass
except Exception as e:
    pass
finally:
    pass

for i in range(10):
    pass

while True:
    break

x, y = 1, 2
x = y = z = 0

class Generic[T]:
    pass
";

    let tree = parser.parse(source, None).unwrap();
    let root = tree.root_node();

    let mut kinds_seen = std::collections::HashMap::new();

    fn walk(node: tree_sitter::Node, map: &mut std::collections::HashMap<String, u16>) {
        let kind_id = node.kind_id();
        let kind_str = node.kind();

        map.entry(kind_str.to_string())
            .and_modify(|id| {
                if *id != kind_id {
                    eprintln!(
                        "WARNING: kind '{}' has multiple IDs: {} and {}",
                        kind_str, *id, kind_id
                    );
                }
            })
            .or_insert(kind_id);

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            walk(child, map);
        }
    }

    walk(root, &mut kinds_seen);

    println!("\n{}", "=".repeat(100));
    println!("Tree-sitter Python Kind IDs from Direct Parsing (comprehensive)");
    println!("{}", "=".repeat(100));

    let mut sorted: Vec<_> = kinds_seen.iter().collect();
    sorted.sort_by_key(|&(_, id)| id);

    for (kind_str, kind_id) in sorted {
        println!(
            "({:3}, {:35}),  // {}",
            kind_id,
            format!("\"{}\"", kind_str),
            kind_str
        );
    }
}
