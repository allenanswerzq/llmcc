use tree_sitter::Parser;

fn main() {
    let language = tree_sitter_python::LANGUAGE.into();

    let mut parser = Parser::new();
    parser.set_language(&language).unwrap();

    let source = b"
def hello(name):
    return name

class Foo:
    def bar(self):
        pass

x = 5
y = foo()
z = obj.attr
";

    let tree = parser.parse(source, None).unwrap();
    let root = tree.root_node();

    fn walk(node: tree_sitter::Node, depth: usize) {
        let indent = "  ".repeat(depth);
        let kind = node.kind();
        println!("{}{}[kind_id={}]", indent, kind, node.kind_id());

        // Get child count
        let child_count = node.child_count();

        for i in 0..child_count {
            if let Some(child) = node.child(i) {
                // Create a new cursor just for getting field info
                let mut cursor = node.walk();
                cursor.goto_first_child();

                for _ in 0..i {
                    cursor.goto_next_sibling();
                }

                let field_name = cursor.field_name();
                let field_id_val = cursor.field_id();

                if let Some(fname) = field_name {
                    if let Some(fid) = field_id_val {
                        println!(
                            "{}  [field_id={}] {}: {} (kind_id={})",
                            indent,
                            fid.get(),
                            fname,
                            child.kind(),
                            child.kind_id()
                        );
                    } else {
                        println!(
                            "{}  [no field_id] {}: {} (kind_id={})",
                            indent,
                            fname,
                            child.kind(),
                            child.kind_id()
                        );
                    }
                } else {
                    println!(
                        "{}  (no field) {} (kind_id={})",
                        indent,
                        child.kind(),
                        child.kind_id()
                    );
                }

                walk(child, depth + 1);
            }
        }
    }

    walk(root, 0);
}
