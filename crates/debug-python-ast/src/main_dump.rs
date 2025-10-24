use llmcc_core::context::CompileCtxt;
use llmcc_core::lang_def::LanguageTrait;
use llmcc_python::LangPython;
use std::collections::HashMap;

fn main() {
    let source = r#"
def hello(name):
    return "Hello, " + name

class MyClass:
    def __init__(self):
        self.x = 10

import os
from sys import argv as av

@decorator
def decorated_func():
    pass

x = 5
y = x + 1
z = (1, 2, 3)
a = [1, 2, 3]
d = {"key": "value"}
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
"#;

    let sources = vec![source.as_bytes().to_vec()];
    let cc = CompileCtxt::from_sources::<LangPython>(&sources);
    let unit = cc.compile_unit(0);

    eprintln!("Building IR...");
    llmcc_python::build_llmcc_ir::<LangPython>(&cc).unwrap();

    // Collect all unique node kind_ids
    let mut kind_map: HashMap<u16, String> = HashMap::new();

    fn collect_kinds(node: &llmcc_core::ir::HirNode, unit: llmcc_core::context::CompileUnit, map: &mut HashMap<u16, String>) {
        let kind_id = node.kind_id();

        // Try to get the token_str if available
        if let Some(token_name) = LangPython::token_str(kind_id) {
            map.insert(kind_id, token_name.to_string());
        }

        // Also recursively collect all children
        for child_id in node.children() {
            let child = unit.hir_node(*child_id);
            collect_kinds(&child, unit, map);
        }
    }

    if let Some(root) = unit.opt_hir_node(llmcc_core::HirId(0)) {
        collect_kinds(&root, unit, &mut kind_map);
    }

    println!("\n{}", "=".repeat(80));
    println!("All TreeSitter Python Node Kind IDs Found");
    println!("{}", "=".repeat(80));

    let mut sorted_ids: Vec<_> = kind_map.iter().collect();
    sorted_ids.sort_by_key(|&(id, _)| id);

    for (id, name) in sorted_ids {
        println!("({:3}, {:30}),", id, format!("\"{}\"", name));
    }
}
