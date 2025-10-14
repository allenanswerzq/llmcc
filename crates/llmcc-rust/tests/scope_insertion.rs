use std::process::Output;

use llmcc_core::interner::{InternPool, InternedStr};
use llmcc_core::ir::HirKind;
use llmcc_core::symbol::{Scope, ScopeStack};
use llmcc_rust::{
    build_llmcc_ir, collect_symbols, CollectionResult, CompileCtxt, CompileUnit, LangRust,
};

struct Fixture<'tcx> {
    cc: &'tcx CompileCtxt<'tcx>,
    unit: CompileUnit<'tcx>,
    globals: &'tcx Scope<'tcx>,
    result: CollectionResult,
}

fn build_fixture(source: &str) -> Fixture<'static> {
    let sources = vec![source.as_bytes().to_vec()];
    let cc: &'static CompileCtxt<'static> =
        Box::leak(Box::new(CompileCtxt::from_sources::<LangRust>(&sources)));
    let unit = cc.compile_unit(0);
    build_llmcc_ir::<LangRust>(unit).expect("build HIR");
    let globals = cc.create_globals();
    let result = collect_symbols(unit, globals);
    Fixture {
        cc,
        unit,
        globals,
        result,
    }
}

impl<'tcx> Fixture<'tcx> {
    fn interner(&self) -> &InternPool {
        self.unit.interner()
    }

    fn intern(&self, name: &str) -> InternedStr {
        self.interner().intern(name)
    }

    fn scope_stack(&self) -> ScopeStack<'tcx> {
        let mut stack = ScopeStack::new(&self.cc.arena, &self.cc.interner);
        stack.push(self.globals);
        stack
    }

    fn module_scope(&self, name: &str) -> &'tcx Scope<'tcx> {
        let mut stack = self.scope_stack();
        let key = self.intern(name);
        let symbol = stack
            .lookup_global_suffix_once(&[key])
            .unwrap_or_else(|| panic!("module {name} not registered in globals"));
        self.unit
            .opt_scope(symbol.owner())
            .unwrap_or_else(|| panic!("scope not recorded for module {name}"))
    }
}

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

    let fixture = build_fixture(source);

    let outer_key = fixture.intern("outer");
    let inner_key = fixture.intern("inner");
    let private_inner_key = fixture.intern("private_inner");
    let foo_key = fixture.intern("Foo");
    let foo_method_key = fixture.intern("method");
    let foo_private_method_key = fixture.intern("private_method");
    let bar_key = fixture.intern("Bar");
    let bar_method_key = fixture.intern("bar_method");
    let max_key = fixture.intern("MAX");
    let param_key = fixture.intern("param");
    let local_key = fixture.intern("local");

    let mut scope_stack = fixture.scope_stack();

    assert!(
        fixture.globals.get_id(outer_key).is_some(),
        "global scope should store module symbol"
    );
    assert!(
        fixture.globals.get_id(max_key).is_some(),
        "global scope should store const symbol"
    );
    assert!(
        fixture.globals.get_id(foo_key).is_some(),
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
        fixture.globals.get_id(bar_key).is_some(),
        "crate root struct should exist in global scope regardless of visibility"
    );
    assert!(
        scope_stack
            .lookup_global_suffix_once(&[bar_method_key, bar_key])
            .is_none(),
        "methods on private struct should not be exported globally"
    );
    assert!(
        fixture.globals.get_id(private_inner_key).is_none(),
        "private functions should remain local to their module"
    );

    let global_symbol = scope_stack
        .lookup_global_suffix_once(&[inner_key, outer_key])
        .expect("global lookup for outer::inner");
    assert_eq!(global_symbol.fqn_name.borrow().as_str(), "outer::inner");

    let inner_desc = fixture
        .result
        .functions
        .iter()
        .find(|desc| desc.fqn == "outer::inner")
        .expect("function descriptor for outer::inner");

    let function_scope = fixture
        .unit
        .opt_scope(inner_desc.hir_id)
        .expect("function scope registered");
    assert!(
        function_scope.get_id(param_key).is_some(),
        "function scope should contain parameter symbol"
    );

    let function_node = fixture.unit.hir_node(inner_desc.hir_id);
    let body_scope_id = function_node
        .children()
        .iter()
        .copied()
        .map(|child_id| fixture.unit.hir_node(child_id))
        .find(|child| child.kind() == HirKind::Scope)
        .map(|child| child.hir_id())
        .expect("function body block scope id");
    let body_scope = fixture
        .unit
        .opt_scope(body_scope_id)
        .expect("block scope registered for function body");
    assert!(
        body_scope.get_id(local_key).is_some(),
        "block scope should contain local variable symbol"
    );

    let module_scope = fixture.module_scope("outer");
    assert!(
        module_scope.get_id(inner_key).is_some(),
        "module scope should contain function symbol"
    );
    assert!(
        module_scope.get_id(private_inner_key).is_some(),
        "module scope should contain private function symbol"
    );

    assert!(
        fixture.globals.get_id(local_key).is_none(),
        "global scope should not contain local variables"
    );
}

#[test]
fn module_struct_visibility() {
    let source = r#"
        mod outer {
            pub struct Foo;
            impl Foo {
                pub fn create() {}
            }

            struct Bar;
            impl Bar {
                fn hidden() {}
            }
        }
    "#;

    let fixture = build_fixture(source);

    let outer_key = fixture.intern("outer");
    let foo_key = fixture.intern("Foo");
    let create_key = fixture.intern("create");
    let bar_key = fixture.intern("Bar");
    let hidden_key = fixture.intern("hidden");

    let mut scope_stack = fixture.scope_stack();

    assert!(
        fixture.globals.get_id(foo_key).is_some(),
        "public struct inside module should be exported"
    );
    assert!(
        scope_stack
            .lookup_global_suffix_once(&[create_key, foo_key])
            .is_some(),
        "public method on exported struct should be globally accessible"
    );
    assert!(
        fixture.globals.get_id(bar_key).is_none(),
        "private struct inside module should not be exported"
    );
    assert!(
        scope_stack
            .lookup_global_suffix_once(&[hidden_key, bar_key])
            .is_none(),
        "private method should not be globally accessible"
    );

    let module_scope = fixture.module_scope("outer");
    assert!(
        module_scope.get_id(bar_key).is_some(),
        "module scope should retain private struct"
    );
    assert!(
        module_scope.get_id(foo_key).is_some(),
        "module scope should contain public struct as well"
    );
}
