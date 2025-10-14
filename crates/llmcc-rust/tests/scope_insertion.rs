use llmcc_core::ir::HirKind;
use llmcc_core::symbol::ScopeStack;
use llmcc_rust::{build_llmcc_ir, collect_symbols, CompileCtxt, LangRust};

#[test]
fn inserts_symbols_for_local_and_global_resolution() {
    let source = r#"
        mod outer {
            pub fn inner(param: i32) {
                let local = param;
            }

            fn private_inner() {}
        }

        pub struct Foo {
            field: i32,
        }

        impl Foo {
            /// if Foo is public, we should export its methods too
            pub fn method(&self) {}

            fn private_method(&self) {}
        }

        struct Bar;
        impl Bar {
            /// if Bar is private, we should NOT export its methods
            fn bar_method(&self) {}
        }

        const MAX: i32 = 5;
    "#;

    let sources = vec![source.as_bytes().to_vec()];
    let cc = CompileCtxt::from_sources::<LangRust>(&sources);
    let unit = cc.compile_unit(0);
    build_llmcc_ir::<LangRust>(unit).expect("build HIR");
    let globals = cc.create_globals();

    let result = collect_symbols(unit, globals);

    let interner = unit.interner();
    let inner_key = interner.intern("inner");
    let private_inner_key = interner.intern("private_inner");
    let outer_key = interner.intern("outer");
    let foo_key = interner.intern("Foo");
    let foo_method_key = interner.intern("method");
    let foo_private_method_key = interner.intern("private_method");
    let bar_key = interner.intern("Bar");
    let bar_method_key = interner.intern("bar_method");
    let max_key = interner.intern("MAX");
    let param_key = interner.intern("param");
    let local_key = interner.intern("local");

    let mut scope_stack = ScopeStack::new(&cc.arena, &cc.interner);
    scope_stack.push(globals);
    assert!(
        globals.get_id(outer_key).is_some(),
        "global scope should store module symbol"
    );
    assert!(
        globals.get_id(max_key).is_some(),
        "global scope should store const symbol"
    );
    assert!(
        globals.get_id(foo_key).is_some(),
        "public struct should be visible globally"
    );
    assert!(
        scope_stack
            .lookup_global_suffix_once(&[foo_method_key, foo_key])
            .is_some(),
        "public method on public type should be globally resolvable"
    );
    assert!(
        scope_stack
            .lookup_global_suffix_once(&[foo_private_method_key, foo_key])
            .is_none(),
        "private method on public type should stay local"
    );
    assert!(
        globals.get_id(bar_key).is_some(),
        "private struct at crate root still resides in global scope"
    );
    assert!(
        scope_stack
            .lookup_global_suffix_once(&[bar_method_key, bar_key])
            .is_none(),
        "methods on private struct should not be exported globally"
    );
    assert!(
        globals.get_id(private_inner_key).is_none(),
        "private functions should remain local to their module"
    );
    let global_symbol = scope_stack
        .lookup_global_suffix_once(&[inner_key, outer_key])
        .expect("global lookup for outer::inner");
    assert_eq!(global_symbol.fqn_name.borrow().as_str(), "outer::inner");

    let inner_desc = result
        .functions
        .iter()
        .find(|desc| desc.fqn == "outer::inner")
        .expect("function descriptor for outer::inner");

    let function_scope = unit
        .opt_scope(inner_desc.hir_id)
        .expect("function scope registered");
    assert!(
        function_scope.get_id(param_key).is_some(),
        "function scope should contain parameter symbol"
    );
    let function_node = unit.hir_node(inner_desc.hir_id);
    let body_scope_id = function_node
        .children()
        .iter()
        .copied()
        .map(|child_id| unit.hir_node(child_id))
        .find(|child| child.kind() == HirKind::Scope)
        .map(|child| child.hir_id())
        .expect("function body block scope id");
    let body_scope = unit
        .opt_scope(body_scope_id)
        .expect("block scope registered for function body");
    assert!(
        body_scope.get_id(local_key).is_some(),
        "block scope should contain local variable symbol"
    );

    let module_symbol = scope_stack
        .lookup_global_suffix_once(&[outer_key])
        .expect("module symbol registered");
    let module_scope = unit
        .opt_scope(module_symbol.owner())
        .expect("module scope registered for outer");
    assert!(
        module_scope.get_id(inner_key).is_some(),
        "module scope should contain function symbol"
    );
    assert!(
        module_scope.get_id(private_inner_key).is_some(),
        "module scope should contain private function symbol"
    );

    assert!(
        globals.get_id(local_key).is_none(),
        "global scope should not contain local variables"
    );
}

#[test]
fn module_struct_visibility() {
    let source = r#"
        mod outer {
            mod inner {
                pub struct Foo;
                impl Foo {
                    pub fn create() {}
                }

                struct Bar;
                impl Bar {
                    fn hidden() {}
                }
            }
        }
    "#;

    let sources = vec![source.as_bytes().to_vec()];
    let cc = CompileCtxt::from_sources::<LangRust>(&sources);
    let unit = cc.compile_unit(0);
    build_llmcc_ir::<LangRust>(unit).expect("build HIR");
    let globals = cc.create_globals();

    collect_symbols(unit, globals);

    let interner = unit.interner();
    let inner_key = interner.intern("inner");
    let foo_key = interner.intern("Foo");
    let create_key = interner.intern("create");
    let bar_key = interner.intern("Bar");
    let hidden_key = interner.intern("hidden");

    let mut scope_stack = ScopeStack::new(&cc.arena, &cc.interner);
    scope_stack.push(globals);

    assert!(
        globals.get_id(foo_key).is_some(),
        "public struct inside module should be exported"
    );
    assert!(
        scope_stack
            .lookup_global_suffix_once(&[create_key, foo_key])
            .is_some(),
        "public method on exported struct should be globally accessible"
    );
    assert!(
        globals.get_id(bar_key).is_none(),
        "private struct inside module should not be exported"
    );
    assert!(
        scope_stack
            .lookup_global_suffix_once(&[hidden_key, bar_key])
            .is_none(),
        "private method should not be globally accessible"
    );

    let module_symbol = scope_stack
        .lookup_global_suffix_once(&[inner_key])
        .expect("module symbol");
    let module_scope = unit
        .opt_scope(module_symbol.owner())
        .expect("scope for module outer");
    assert!(
        module_scope.get_id(bar_key).is_some(),
        "module scope should retain private struct"
    );
    assert!(
        module_scope.get_id(foo_key).is_some(),
        "module scope should contain public struct as well"
    );
}


