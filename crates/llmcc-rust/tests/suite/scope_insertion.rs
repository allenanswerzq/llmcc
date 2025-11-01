use llmcc_core::{
    context::CompileUnit,
    interner::{InternPool, InternedStr},
    ir::HirKind,
    symbol::{Scope, ScopeStack},
    IrBuildConfig,
};
use llmcc_rust::{build_llmcc_ir, collect_symbols, CollectionResult, CompileCtxt, LangRust};

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
    build_llmcc_ir::<LangRust>(cc, IrBuildConfig::default()).unwrap();
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
        let mut stack = ScopeStack::new(&self.cc.arena, &self.cc.interner, &self.cc.symbol_map);
        stack.push(self.globals);
        stack
    }

    fn module_scope(&self, name: &str) -> &'tcx Scope<'tcx> {
        let stack = self.scope_stack();
        let key = self.intern(name);
        let symbol = stack.find_global_suffix(&[key]).unwrap();
        self.unit.opt_get_scope(symbol.owner()).unwrap()
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

    let scope_stack = fixture.scope_stack();

    assert!(fixture.globals.get_id(outer_key).is_some());
    assert!(fixture.globals.get_id(max_key).is_some());
    assert!(fixture.globals.get_id(foo_key).is_some());
    assert!(scope_stack
        .find_global_suffix(&[foo_method_key, foo_key])
        .is_some());
    assert!(scope_stack
        .find_global_suffix(&[foo_private_method_key, foo_key])
        .is_none());
    assert!(fixture.globals.get_id(bar_key).is_some());
    assert!(scope_stack
        .find_global_suffix(&[bar_method_key, bar_key])
        .is_none());
    assert!(fixture.globals.get_id(private_inner_key).is_none());

    let global_symbol = scope_stack
        .find_global_suffix(&[inner_key, outer_key])
        .unwrap();
    assert_eq!(
        global_symbol.fqn_name.read().unwrap().as_str(),
        "outer::inner"
    );

    let inner_desc = fixture
        .result
        .functions
        .iter()
        .find(|desc| desc.fqn == "outer::inner")
        .unwrap();

    let function_scope = fixture.unit.opt_get_scope(inner_desc.hir_id).unwrap();
    assert!(function_scope.get_id(param_key).is_some());

    let function_node = fixture.unit.hir_node(inner_desc.hir_id);
    let body_scope_id = function_node
        .children()
        .iter()
        .copied()
        .map(|child_id| fixture.unit.hir_node(child_id))
        .find(|child| child.kind() == HirKind::Scope)
        .map(|child| child.hir_id())
        .unwrap();
    let body_scope = fixture.unit.opt_get_scope(body_scope_id).unwrap();
    assert!(body_scope.get_id(local_key).is_some());

    let module_scope = fixture.module_scope("outer");
    assert!(module_scope.get_id(inner_key).is_some());
    assert!(module_scope.get_id(private_inner_key).is_some());

    assert!(fixture.globals.get_id(local_key).is_none());
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

    let foo_key = fixture.intern("Foo");
    let create_key = fixture.intern("create");
    let bar_key = fixture.intern("Bar");
    let hidden_key = fixture.intern("hidden");

    let scope_stack = fixture.scope_stack();

    assert!(fixture.globals.get_id(foo_key).is_some());
    assert!(scope_stack
        .find_global_suffix(&[create_key, foo_key])
        .is_some());
    assert!(fixture.globals.get_id(bar_key).is_none());
    assert!(scope_stack
        .find_global_suffix(&[hidden_key, bar_key])
        .is_none());

    let module_scope = fixture.module_scope("outer");
    assert!(module_scope.get_id(bar_key).is_some());
    assert!(module_scope.get_id(foo_key).is_some());
}

#[test]
fn module_enum_visibility() {
    let source = r#"
        mod outer {
            pub enum Visible {
                A,
            }

            enum Hidden {
                B,
            }
        }
    "#;

    let fixture = build_fixture(source);

    let outer_scope = fixture.module_scope("outer");
    let visible_key = fixture.intern("Visible");
    let hidden_key = fixture.intern("Hidden");
    let variant_a_key = fixture.intern("A");
    let variant_b_key = fixture.intern("B");

    let scope_stack = fixture.scope_stack();
    let outer_key = fixture.intern("outer");

    assert!(fixture.globals.get_id(visible_key).is_some());
    assert!(fixture.globals.get_id(hidden_key).is_none());
    assert!(scope_stack
        .find_global_suffix(&[visible_key, outer_key])
        .is_some());
    assert!(scope_stack
        .find_global_suffix(&[hidden_key, outer_key])
        .is_none());

    assert!(outer_scope.get_id(visible_key).is_some());
    assert!(outer_scope.get_id(hidden_key).is_some());

    let visible_desc = fixture
        .result
        .enums
        .iter()
        .find(|desc| desc.name == "Visible")
        .unwrap();
    let visible_scope = fixture.unit.opt_get_scope(visible_desc.hir_id).unwrap();
    assert!(visible_scope.get_id(variant_a_key).is_some());

    let hidden_desc = fixture
        .result
        .enums
        .iter()
        .find(|desc| desc.name == "Hidden")
        .unwrap();
    let hidden_scope = fixture.unit.opt_get_scope(hidden_desc.hir_id).unwrap();
    assert!(hidden_scope.get_id(variant_b_key).is_some());

    assert!(scope_stack
        .find_global_suffix(&[variant_a_key, visible_key, outer_key])
        .is_some());
    assert!(scope_stack
        .find_global_suffix(&[variant_b_key, hidden_key, outer_key])
        .is_none());
}

#[test]
fn enum_variant_symbols_are_registered() {
    let source = r#"
        pub enum Status {
            Ok,
            NotFound,
        }

        enum PrivateStatus {
            Hidden,
        }
    "#;

    let fixture = build_fixture(source);

    let status_key = fixture.intern("Status");
    let ok_key = fixture.intern("Ok");
    let not_found_key = fixture.intern("NotFound");
    let private_status_key = fixture.intern("PrivateStatus");
    let hidden_key = fixture.intern("Hidden");

    let scope_stack = fixture.scope_stack();

    assert!(fixture.globals.get_id(status_key).is_some());
    assert!(scope_stack
        .find_global_suffix(&[ok_key, status_key])
        .is_some());
    assert!(scope_stack
        .find_global_suffix(&[not_found_key, status_key])
        .is_some());

    assert!(fixture.globals.get_id(private_status_key).is_some());
    assert!(scope_stack
        .find_global_suffix(&[hidden_key, private_status_key])
        .is_none());

    let status_scope = fixture
        .unit
        .opt_get_scope(
            fixture
                .result
                .enums
                .iter()
                .find(|desc| desc.name == "Status")
                .unwrap()
                .hir_id,
        )
        .unwrap();
    assert!(status_scope.get_id(ok_key).is_some());
    assert!(status_scope.get_id(not_found_key).is_some());

    let private_scope = fixture
        .unit
        .opt_get_scope(
            fixture
                .result
                .enums
                .iter()
                .find(|desc| desc.name == "PrivateStatus")
                .unwrap()
                .hir_id,
        )
        .unwrap();
    assert!(private_scope.get_id(hidden_key).is_some());
}
