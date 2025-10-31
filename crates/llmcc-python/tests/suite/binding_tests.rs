use llmcc_core::context::CompileCtxt;
use llmcc_core::symbol::Symbol;
use llmcc_python::{bind_symbols, build_llmcc_ir, collect_symbols, LangPython};

fn compile(
    source: &str,
) -> (
    &'static CompileCtxt<'static>,
    llmcc_core::context::CompileUnit<'static>,
    llmcc_python::CollectionResult,
    &'static llmcc_core::symbol::Scope<'static>,
) {
    let sources = vec![source.as_bytes().to_vec()];
    let cc = Box::leak(Box::new(CompileCtxt::from_sources::<LangPython>(&sources)));
    let unit = cc.compile_unit(0);
    let result = build_llmcc_ir::<LangPython>(cc);
    result.ok();
    let globals = cc.create_globals();
    let collection = collect_symbols(unit, globals);
    bind_symbols(unit, globals);
    (cc, unit, collection, globals)
}

#[allow(dead_code)]
fn find_function<'a>(
    collection: &'a llmcc_python::CollectionResult,
    name: &str,
) -> Option<&'a llmcc_python::PythonFunctionDescriptor> {
    collection.functions.iter().find(|desc| desc.name == name)
}

#[allow(dead_code)]
fn find_function_unwrap<'a>(
    collection: &'a llmcc_python::CollectionResult,
    name: &str,
) -> &'a llmcc_python::PythonFunctionDescriptor {
    find_function(collection, name).unwrap_or_else(|| panic!("function '{}' not found", name))
}

#[allow(dead_code)]
fn find_class<'a>(
    collection: &'a llmcc_python::CollectionResult,
    name: &str,
) -> Option<&'a llmcc_python::PythonClassDescriptor> {
    collection.classes.iter().find(|desc| desc.name == name)
}

#[allow(dead_code)]
fn find_class_unwrap<'a>(
    collection: &'a llmcc_python::CollectionResult,
    name: &str,
) -> &'a llmcc_python::PythonClassDescriptor {
    find_class(collection, name).unwrap_or_else(|| panic!("class '{}' not found", name))
}

fn get_symbol<'tcx>(
    globals: &'tcx llmcc_core::symbol::Scope<'tcx>,
    name: &str,
    kind: llmcc_core::symbol::SymbolKind,
) -> Option<&'tcx Symbol> {
    globals
        .all_symbols()
        .iter()
        .find(|sym| sym.name.as_str() == name && sym.kind() == kind)
        .copied()
}

fn function_symbol<'tcx>(
    globals: &'tcx llmcc_core::symbol::Scope<'tcx>,
    _collection: &llmcc_python::CollectionResult,
    name: &str,
) -> &'tcx Symbol {
    get_symbol(globals, name, llmcc_core::symbol::SymbolKind::Function)
        .unwrap_or_else(|| panic!("function symbol '{}' not found in globals", name))
}

fn class_symbol<'tcx>(
    globals: &'tcx llmcc_core::symbol::Scope<'tcx>,
    _collection: &llmcc_python::CollectionResult,
    name: &str,
) -> &'tcx Symbol {
    get_symbol(globals, name, llmcc_core::symbol::SymbolKind::Struct)
        .unwrap_or_else(|| panic!("class symbol '{}' not found in globals", name))
}

fn assert_depends_on(symbol: &Symbol, target: &Symbol) {
    assert!(
        symbol.depends.read().unwrap().contains(&target.id),
        "{} should depend on {}",
        symbol.name.as_str(),
        target.name.as_str()
    );
}

fn assert_depended_by(symbol: &Symbol, source: &Symbol) {
    assert!(
        symbol.depended.read().unwrap().contains(&source.id),
        "{} should be depended on by {}",
        symbol.name.as_str(),
        source.name.as_str()
    );
}

fn assert_relation(dependent: &Symbol, dependency: &Symbol) {
    assert_depends_on(dependent, dependency);
    assert_depended_by(dependency, dependent);
}

// Tests that validate Python symbol binding and dependency relationships.
// These tests verify symbol dependencies using assert_relation, similar to bind_dependencies.rs

#[test]
fn function_depends_on_called_function() {
    let source = r#"
def helper():
    pass

def caller():
    helper()
"#;
    let (_, _, collection, globals) = compile(source);

    let helper_sym = function_symbol(globals, &collection, "helper");
    let caller_sym = function_symbol(globals, &collection, "caller");

    assert_relation(caller_sym, helper_sym);
}

#[test]
fn function_depends_on_multiple_called_functions() {
    let source = r#"
def dep1():
    pass

def dep2():
    pass

def dep3():
    pass

def caller():
    dep1()
    dep2()
    dep3()
"#;
    let (_, _, collection, globals) = compile(source);

    let dep1_sym = function_symbol(globals, &collection, "dep1");
    let dep2_sym = function_symbol(globals, &collection, "dep2");
    let dep3_sym = function_symbol(globals, &collection, "dep3");
    let caller_sym = function_symbol(globals, &collection, "caller");

    assert_relation(caller_sym, dep1_sym);
    assert_relation(caller_sym, dep2_sym);
    assert_relation(caller_sym, dep3_sym);
}

#[test]
fn class_records_dependencies_on_methods() {
    let source = r#"
class Foo:
    def method(self):
        pass
"#;
    let (_, _, collection, globals) = compile(source);

    let foo_sym = class_symbol(globals, &collection, "Foo");
    let method_sym = function_symbol(globals, &collection, "method");

    assert_relation(foo_sym, method_sym);
}

#[test]
fn method_depends_on_called_method() {
    let source = r#"
class Handler:
    def helper(self):
        pass

    def caller(self):
        self.helper()
"#;
    let (_, _, collection, globals) = compile(source);

    let helper_sym = function_symbol(globals, &collection, "helper");
    let caller_sym = function_symbol(globals, &collection, "caller");

    assert_relation(caller_sym, helper_sym);
}

#[test]
fn function_parameter_type_dependency() {
    let source = r#"
class User:
    pass

def process(user: User):
    pass
"#;
    let (_, _, collection, globals) = compile(source);

    let user_sym = class_symbol(globals, &collection, "User");
    let process_sym = function_symbol(globals, &collection, "process");

    assert_relation(process_sym, user_sym);
}

#[test]
fn method_parameter_type_dependency_propagates_to_class() {
    let source = r#"
class Data:
    pass

class Handler:
    def handle(self, item: Data):
        pass
"#;
    let (_, _, collection, globals) = compile(source);

    let data_sym = class_symbol(globals, &collection, "Data");
    let handler_sym = class_symbol(globals, &collection, "Handler");
    let handle_sym = function_symbol(globals, &collection, "handle");

    assert_relation(handle_sym, data_sym);
    assert_relation(handler_sym, data_sym);
}

#[test]
fn function_chain_dependencies() {
    let source = r#"
def level1():
    pass

def level2():
    level1()

def level3():
    level2()

def level4():
    level3()
"#;
    let (_, _, collection, globals) = compile(source);

    let l1_sym = function_symbol(globals, &collection, "level1");
    let l2_sym = function_symbol(globals, &collection, "level2");
    let l3_sym = function_symbol(globals, &collection, "level3");
    let l4_sym = function_symbol(globals, &collection, "level4");

    assert_relation(l2_sym, l1_sym);
    assert_relation(l3_sym, l2_sym);
    assert_relation(l4_sym, l3_sym);
}

#[test]
fn class_inheritance_creates_dependency() {
    let source = r#"
class Base:
    pass

class Derived(Base):
    pass
"#;
    let (_, _, collection, globals) = compile(source);

    let base_sym = class_symbol(globals, &collection, "Base");
    let derived_sym = class_symbol(globals, &collection, "Derived");

    assert_relation(derived_sym, base_sym);
}

#[test]
fn class_with_multiple_inheritance() {
    let source = r#"
class A:
    pass

class B:
    pass

class C(A, B):
    pass
"#;
    let (_, _, collection, globals) = compile(source);

    let a_sym = class_symbol(globals, &collection, "A");
    let b_sym = class_symbol(globals, &collection, "B");
    let c_sym = class_symbol(globals, &collection, "C");

    assert_relation(c_sym, a_sym);
    assert_relation(c_sym, b_sym);
}

#[test]
fn multiple_methods_depend_on_class() {
    let source = r#"
class Service:
    def method1(self):
        pass

    def method2(self):
        pass

    def method3(self):
        pass
"#;
    let (_, _, collection, globals) = compile(source);

    let service_sym = class_symbol(globals, &collection, "Service");
    let m1_sym = function_symbol(globals, &collection, "method1");
    let m2_sym = function_symbol(globals, &collection, "method2");
    let m3_sym = function_symbol(globals, &collection, "method3");

    assert_relation(service_sym, m1_sym);
    assert_relation(service_sym, m2_sym);
    assert_relation(service_sym, m3_sym);
}

#[test]
fn cross_method_dependencies() {
    let source = r#"
class Service:
    def internal_helper(self):
        pass

    def public_api(self):
        self.internal_helper()

    def another_api(self):
        self.internal_helper()
        self.public_api()
"#;
    let (_, _, collection, globals) = compile(source);

    let helper_sym = function_symbol(globals, &collection, "internal_helper");
    let public_sym = function_symbol(globals, &collection, "public_api");
    let another_sym = function_symbol(globals, &collection, "another_api");

    assert_relation(public_sym, helper_sym);
    assert_relation(another_sym, helper_sym);
    assert_relation(another_sym, public_sym);
}

#[test]
fn function_calls_class_constructor() {
    let source = r#"
class MyClass:
    def __init__(self):
        pass

def create_instance():
    obj = MyClass()
"#;
    let (_, _, collection, globals) = compile(source);

    let myclass_sym = class_symbol(globals, &collection, "MyClass");
    let create_sym = function_symbol(globals, &collection, "create_instance");

    assert_relation(create_sym, myclass_sym);
}

#[test]
fn nested_class_dependencies() {
    let source = r#"
class Outer:
    class Inner:
        def inner_method(self):
            pass
"#;
    let (_, _, collection, globals) = compile(source);

    let inner_sym = class_symbol(globals, &collection, "Inner");
    let inner_method_sym = function_symbol(globals, &collection, "inner_method");

    assert_relation(inner_sym, inner_method_sym);
}

#[test]
fn function_with_decorator_dependency() {
    let source = r#"
def decorator(f):
    return f

@decorator
def decorated():
    pass
"#;
    let (_, _, collection, globals) = compile(source);

    let decorator_sym = function_symbol(globals, &collection, "decorator");
    let decorated_sym = function_symbol(globals, &collection, "decorated");

    assert_relation(decorated_sym, decorator_sym);
}

#[test]
fn function_with_multiple_decorators() {
    let source = r#"
def deco1(f):
    return f

def deco2(f):
    return f

@deco1
@deco2
def decorated():
    pass
"#;
    let (_, _, collection, globals) = compile(source);

    let deco1_sym = function_symbol(globals, &collection, "deco1");
    let deco2_sym = function_symbol(globals, &collection, "deco2");
    let decorated_sym = function_symbol(globals, &collection, "decorated");

    assert_relation(decorated_sym, deco1_sym);
    assert_relation(decorated_sym, deco2_sym);
}
