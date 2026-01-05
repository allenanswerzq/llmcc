use llmcc_rust::token::LangRust;

use llmcc_core::context::CompileCtxt;
use llmcc_core::ir_builder::{IrBuildOption, build_llmcc_ir};
use llmcc_core::symbol::SymKind;
use llmcc_resolver::{ResolverOption, bind_symbols_with, collect_symbols_with};
use tracing_subscriber::EnvFilter;

#[allow(dead_code)]
pub fn with_compiled_unit<F>(sources: &[&str], check: F)
where
    F: for<'a> FnOnce(&'a CompileCtxt<'a>),
{
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_test_writer()
        .try_init();

    let bytes = sources
        .iter()
        .map(|src| src.as_bytes().to_vec())
        .collect::<Vec<_>>();

    let cc = CompileCtxt::from_sources::<LangRust>(&bytes);
    build_llmcc_ir::<LangRust>(&cc, IrBuildOption::default()).unwrap();

    let resolver_option = ResolverOption::default()
        .with_sequential(true)
        .with_print_ir(true)
        .with_bind_func_bodies(true);
    let globals = collect_symbols_with::<LangRust>(&cc, &resolver_option);
    bind_symbols_with::<LangRust>(&cc, globals, &resolver_option);
    check(&cc);
}

#[allow(dead_code)]
pub fn find_symbol_id<'a>(
    cc: &'a CompileCtxt<'a>,
    name: &str,
    kind: SymKind,
) -> llmcc_core::symbol::SymId {
    let name_key = cc.interner.intern(name);
    cc.get_all_symbols()
        .into_iter()
        .find(|symbol| symbol.name == name_key && symbol.kind() == kind)
        .map(|symbol| symbol.id())
        .unwrap_or_else(|| panic!("symbol {name} with kind {kind:?} not found"))
}

#[allow(dead_code)]
pub fn assert_collect_symbol<'a>(
    cc: &'a CompileCtxt<'a>,
    name: &str,
    kind: SymKind,
    expect_scope: bool,
) -> llmcc_core::symbol::SymId {
    let name_key = cc.interner.intern(name);
    let mut matches: Vec<&llmcc_core::symbol::Symbol> = cc
        .get_all_symbols()
        .into_iter()
        .filter(|sym| sym.name == name_key && sym.kind() == kind)
        .collect();

    if matches.is_empty() {
        panic!("symbol {name} with kind {kind:?} not found");
    }

    let symbol = if expect_scope {
        matches
            .iter()
            .copied()
            .find(|sym| sym.opt_scope().is_some())
            .unwrap_or_else(|| {
                panic!("symbol {name} expected to have an associated scope");
            })
    } else {
        matches.remove(0)
    };

    assert!(symbol.id().0 > 0, "symbol {name} should have a valid id");

    if expect_scope {
        debug_assert!(symbol.opt_scope().is_some());
    }

    symbol.id()
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct BindExpect<'a> {
    pub kind: SymKind,
    pub expect_scope: bool,
    pub type_of: Option<&'a str>,
    pub field_of: Option<&'a str>,
    pub is_global: Option<bool>,
    pub scope_same_as: Option<&'a str>,
    pub nested_types: Option<Vec<&'a str>>,
}

#[allow(dead_code)]
impl<'a> BindExpect<'a> {
    pub fn new(kind: SymKind) -> Self {
        Self {
            kind,
            expect_scope: false,
            type_of: None,
            field_of: None,
            is_global: None,
            scope_same_as: None,
            nested_types: None,
        }
    }

    pub fn expect_scope(mut self) -> Self {
        self.expect_scope = true;
        self
    }

    pub fn with_type_of(mut self, ty: &'a str) -> Self {
        self.type_of = Some(ty);
        self
    }

    pub fn with_field_of(mut self, owner: &'a str) -> Self {
        self.field_of = Some(owner);
        self
    }

    pub fn with_is_global(mut self, value: bool) -> Self {
        self.is_global = Some(value);
        self
    }

    pub fn with_scope_same_as(mut self, other: &'a str) -> Self {
        self.scope_same_as = Some(other);
        self
    }

    pub fn with_nested_types(mut self, names: Vec<&'a str>) -> Self {
        self.nested_types = Some(names);
        self
    }
}

#[allow(dead_code)]
pub fn assert_bind_symbol<'a>(
    cc: &'a CompileCtxt<'a>,
    name: &str,
    expect: BindExpect<'_>,
) -> llmcc_core::symbol::SymId {
    let name_key = cc.interner.intern(name);

    let candidates: Vec<&llmcc_core::symbol::Symbol> = cc
        .get_all_symbols()
        .into_iter()
        .filter(|sym| sym.name == name_key && sym.kind() == expect.kind)
        .collect();

    assert!(
        !candidates.is_empty(),
        "symbol {} with kind {:?} not found",
        name,
        expect.kind
    );

    let symbol = if expect.expect_scope {
        candidates
            .iter()
            .copied()
            .find(|sym| sym.opt_scope().is_some())
            .unwrap_or_else(|| {
                panic!("symbol {name} expected to have an associated scope");
            })
    } else {
        *candidates.first().unwrap()
    };

    if expect.expect_scope {
        assert!(
            symbol.opt_scope().is_some(),
            "symbol {name} should have a scope"
        );
    }

    if let Some(expected) = expect.type_of {
        let ty_id = symbol
            .type_of()
            .unwrap_or_else(|| panic!("symbol {name} missing type_of"));
        let ty_sym = cc
            .opt_get_symbol(ty_id)
            .unwrap_or_else(|| panic!("type symbol for {expected} not found"));
        let ty_name = cc
            .interner
            .resolve_owned(ty_sym.name)
            .unwrap_or_else(|| "<anon>".to_string());
        assert_eq!(ty_name, expected, "symbol {name} type mismatch");
    }

    if let Some(owner) = expect.field_of {
        let owner_id = symbol
            .field_of()
            .unwrap_or_else(|| panic!("symbol {name} missing field_of"));
        let owner_sym = cc
            .opt_get_symbol(owner_id)
            .unwrap_or_else(|| panic!("field owner symbol {owner} not found"));
        let owner_name = cc
            .interner
            .resolve_owned(owner_sym.name)
            .unwrap_or_else(|| "<anon>".to_string());
        assert_eq!(owner_name, owner, "symbol {name} field_of mismatch");
    }

    if let Some(expected) = expect.is_global {
        assert_eq!(
            symbol.is_global(),
            expected,
            "symbol {name} global flag mismatch"
        );
    }

    if let Some(scope_of) = expect.scope_same_as {
        let scope_name_key = cc.interner.intern(scope_of);
        let peer_symbol = cc
            .get_all_symbols()
            .into_iter()
            .find(|sym| sym.name == scope_name_key)
            .unwrap_or_else(|| panic!("symbol {scope_of} not found for scope comparison"));

        let symbol_scope = symbol
            .opt_scope()
            .unwrap_or_else(|| panic!("symbol {name} missing scope"));
        let peer_scope = peer_symbol
            .opt_scope()
            .unwrap_or_else(|| panic!("symbol {scope_of} missing scope"));

        assert_eq!(
            symbol_scope, peer_scope,
            "symbol {name} scope mismatch with {scope_of}"
        );
    }

    if let Some(expected_nested) = expect.nested_types {
        let nested = symbol
            .nested_types()
            .unwrap_or_else(|| panic!("symbol {name} missing nested types"));
        assert_eq!(nested.len(), expected_nested.len());
        let nested_names: Vec<String> = nested
            .iter()
            .map(|id| {
                let sym = cc
                    .opt_get_symbol(*id)
                    .unwrap_or_else(|| panic!("nested symbol not found for {name}"));
                cc.interner
                    .resolve_owned(sym.name)
                    .unwrap_or_else(|| "<anon>".to_string())
            })
            .collect();
        let expected: Vec<String> = expected_nested.into_iter().map(|s| s.to_string()).collect();
        assert_eq!(
            nested_names, expected,
            "symbol {name} nested types mismatch"
        );
    }

    symbol.id()
}

#[allow(dead_code)]
pub fn assert_exists<'a>(cc: &'a CompileCtxt<'a>, name: &str, kind: SymKind) {
    let name_key = cc.interner.intern(name);
    let all_symbols = cc.get_all_symbols();
    for sym in &all_symbols {
        tracing::debug!("Symbol: {:?}", cc.interner.resolve_owned(sym.name).unwrap());
    }
    let symbol = all_symbols
        .iter()
        .find(|sym| sym.name == name_key && sym.kind() == kind)
        .unwrap_or_else(|| panic!("symbol {name} with kind {kind:?} not found"));
    // prints all symbol for debugging
    assert!(symbol.id().0 > 0, "symbol should have a valid id");
}

#[allow(dead_code)]
pub fn debug_symbol_types<'a>(cc: &'a CompileCtxt<'a>) {
    for symbol in cc.get_all_symbols() {
        let name = cc
            .interner
            .resolve_owned(symbol.name)
            .unwrap_or_else(|| "<anon>".to_string());
        let type_label = symbol
            .type_of()
            .and_then(|ty_id| cc.opt_get_symbol(ty_id))
            .and_then(|ty_sym| cc.interner.resolve_owned(ty_sym.name))
            .unwrap_or_else(|| "<none>".to_string());

        let field_owner = symbol
            .field_of()
            .and_then(|owner_id| cc.opt_get_symbol(owner_id))
            .and_then(|owner_sym| cc.interner.resolve_owned(owner_sym.name))
            .unwrap_or_else(|| "<none>".to_string());

        let scope_label = symbol
            .opt_scope()
            .and_then(|scope_id| {
                let scope = cc.opt_get_scope(scope_id)?;
                let scope_owner = scope
                    .opt_symbol()
                    .and_then(|owner| cc.interner.resolve_owned(owner.name));
                Some(match scope_owner {
                    Some(owner) => format!("{} (scope #{})", owner, scope_id.0),
                    None => format!("<anon> (scope #{})", scope_id.0),
                })
            })
            .unwrap_or_else(|| "<none>".to_string());

        let nested_label = symbol
            .nested_types()
            .map(|ids| {
                ids.into_iter()
                    .filter_map(|id| {
                        cc.opt_get_symbol(id)
                            .and_then(|sym| cc.interner.resolve_owned(sym.name))
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "<none>".to_string());

        tracing::debug!(
            id = symbol.id().0,
            kind = ?symbol.kind(),
            %name,
            %type_label,
            field_of = %field_owner,
            scope = %scope_label,
            is_global = symbol.is_global(),
            nested = %nested_label,
            "symbol: "
        );
    }
}
