use proc_macro::TokenStream;
use quote::quote;
use regex::Regex;
use std::collections::HashMap;
use syn::{parse_macro_input, LitStr};

#[derive(Debug, Clone)]
pub struct Field {
    pub id: u16,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct Symbol {
    pub id: u16,
    pub name: String,
    pub c_name: String,
    pub hir_kind: String,
    pub block_kind: String,
}

#[derive(Debug, Clone)]
pub struct Language {
    pub name: String,
    pub version: u16,
    pub symbol_count: u16,
    pub symbols: HashMap<u16, Symbol>,
    pub symbol_map: HashMap<u16, u16>,
    pub fields: HashMap<u16, Field>,
}

impl Language {
    pub fn new(name: String) -> Self {
        Self {
            name,
            version: 0,
            symbol_count: 0,
            symbols: HashMap::new(),
            symbol_map: HashMap::new(),
            fields: HashMap::new(),
        }
    }
}

// Global language registry
use once_cell::sync::Lazy;
use std::sync::RwLock;

static LANGUAGE_REGISTRY: Lazy<RwLock<HashMap<String, Language>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

pub fn register_language(name: String, language: Language) {
    let mut registry = LANGUAGE_REGISTRY.write().unwrap();
    registry.insert(name, language);
}

pub fn get_language(name: &str) -> Option<Language> {
    let registry = LANGUAGE_REGISTRY.read().unwrap();
    registry.get(name).cloned()
}

pub fn get_language_mut<F, R>(name: &str, f: F) -> Option<R>
where
    F: FnOnce(&mut Language) -> R,
{
    let mut registry = LANGUAGE_REGISTRY.write().unwrap();
    registry.get_mut(name).map(f)
}

// Helper function to set symbol properties
pub fn set_symbol_hir_kind(lang_name: &str, symbol_name: &str, hir_kind: &str) -> bool {
    get_language_mut(lang_name, |lang| {
        for symbol in lang.symbols.values_mut() {
            if symbol.name == symbol_name {
                symbol.hir_kind = hir_kind.to_string();
                return true;
            }
        }
        false
    }).unwrap_or(false)
}

pub fn set_symbol_block_kind(lang_name: &str, symbol_name: &str, block_kind: &str) -> bool {
    get_language_mut(lang_name, |lang| {
        for symbol in lang.symbols.values_mut() {
            if symbol.name == symbol_name {
                symbol.block_kind = block_kind.to_string();
                return true;
            }
        }
        false
    }).unwrap_or(false)
}

/// Parse tree-sitter parser.c file and generate Language struct
#[proc_macro]
pub fn parse_tree_sitter(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as LitStr);
    let file_content = std::fs::read_to_string(input.value())
        .expect("Failed to read parser.c file");

    let language = parse_parser_c(&file_content);
    let lang_name = &language.name;

    // Register the language in the global registry
    register_language(lang_name.clone(), language.clone());

    generate_rust_code(&language)
}

fn parse_parser_c(content: &str) -> Language {
    let mut language = Language::new("rust".to_string());

    // Parse version
    if let Some(caps) = Regex::new(r"#define LANGUAGE_VERSION (\d+)").unwrap().captures(content) {
        language.version = caps[1].parse().unwrap_or(0);
    }

    // Parse symbol count
    if let Some(caps) = Regex::new(r"#define SYMBOL_COUNT (\d+)").unwrap().captures(content) {
        language.symbol_count = caps[1].parse().unwrap_or(0);
    }

    // Parse symbol identifiers enum
    parse_symbol_identifiers(content, &mut language);

    // Parse symbol names array
    parse_symbol_names(content, &mut language);

    // Parse symbol map
    parse_symbol_map(content, &mut language);

    // Parse field identifiers
    parse_field_identifiers(content, &mut language);

    // Parse field names
    parse_field_names(content, &mut language);

    language
}

fn parse_symbol_identifiers(content: &str, language: &mut Language) {
    let enum_regex = Regex::new(r"enum ts_symbol_identifiers \{([^}]*)\}").unwrap();
    if let Some(caps) = enum_regex.captures(content) {
        let enum_body = &caps[1];
        let item_regex = Regex::new(r"(\w+)\s*=\s*(\d+)").unwrap();

        for caps in item_regex.captures_iter(enum_body) {
            let name = caps[1].to_string();
            let id: u16 = caps[2].parse().unwrap_or(0);

            let symbol = Symbol {
                id,
                name: clean_symbol_name(&name),
                c_name: name,
                hir_kind: "HirKind::Internal".to_string(), // Default
                block_kind: "BlockKind::Undefined".to_string(), // Default
            };

            language.symbols.insert(id, symbol);
        }
    }
}

fn parse_symbol_names(content: &str, language: &mut Language) {
    let names_regex = Regex::new(r"static const char \* const ts_symbol_names\[\] = \{([^}]*)\}").unwrap();
    if let Some(caps) = names_regex.captures(content) {
        let names_body = &caps[1];
        let item_regex = Regex::new(r"\[(\w+)\]\s*=\s*\"([^\"]*)\"|\"([^\"]*)\"|(\w+)").unwrap();

        for caps in item_regex.captures_iter(names_body) {
            if let Some(symbol_name) = caps.get(1) {
                let display_name = caps.get(2).map(|m| m.as_str())
                    .or_else(|| caps.get(3).map(|m| m.as_str()))
                    .or_else(|| caps.get(4).map(|m| m.as_str()))
                    .unwrap_or("");

                // Find the symbol by c_name and update its display name
                for symbol in language.symbols.values_mut() {
                    if symbol.c_name == symbol_name.as_str() {
                        symbol.name = display_name.to_string();
                        break;
                    }
                }
            }
        }
    }
}

fn parse_symbol_map(content: &str, language: &mut Language) {
    let map_regex = Regex::new(r"static const TSSymbol ts_symbol_map\[\] = \{([^}]*)\}").unwrap();
    if let Some(caps) = map_regex.captures(content) {
        let map_body = &caps[1];
        let item_regex = Regex::new(r"\[(\w+)\]\s*=\s*(\w+)").unwrap();

        for caps in item_regex.captures_iter(map_body) {
            // This would map from one symbol to another - implementation depends on specific needs
            // For now, we'll just map symbol to itself
            if let Ok(key) = caps[1].parse::<u16>() {
                language.symbol_map.insert(key, key);
            }
        }
    }
}

fn parse_field_identifiers(content: &str, language: &mut Language) {
    let enum_regex = Regex::new(r"enum ts_field_identifiers \{([^}]*)\}").unwrap();
    if let Some(caps) = enum_regex.captures(content) {
        let enum_body = &caps[1];
        let item_regex = Regex::new(r"field_(\w+)\s*=\s*(\d+)").unwrap();

        for caps in item_regex.captures_iter(enum_body) {
            let name = caps[1].to_string();
            let id: u16 = caps[2].parse().unwrap_or(0);

            let field = Field { id, name };
            language.fields.insert(id, field);
        }
    }
}

fn parse_field_names(content: &str, language: &mut Language) {
    let names_regex = Regex::new(r"static const char \* const ts_field_names\[\] = \{([^}]*)\}").unwrap();
    if let Some(caps) = names_regex.captures(content) {
        let names_body = &caps[1];
        let item_regex = Regex::new(r"\[field_(\w+)\]\s*=\s*\"([^\"]+)\"").unwrap();

        for caps in item_regex.captures_iter(names_body) {
            let field_name = caps[1].to_string();
            let display_name = caps[2].to_string();

            // Update field name
            for field in language.fields.values_mut() {
                if field.name == field_name {
                    field.name = display_name;
                    break;
                }
            }
        }
    }
}

fn clean_symbol_name(name: &str) -> String {
    name.replace("anon_sym_", "")
        .replace("aux_sym_", "")
        .replace("sym_", "")
        .to_string()
}

fn generate_rust_code(language: &Language) -> TokenStream {
    let lang_name = &language.name;
    let version = language.version;
    let symbol_count = language.symbol_count;

    // Generate symbol constants
    let symbol_constants: Vec<_> = language.symbols.iter()
        .map(|(id, symbol)| {
            let const_name = proc_macro2::Ident::new(
                &symbol.c_name.to_uppercase(),
                proc_macro2::Span::call_site()
            );
            quote! { pub const #const_name: u16 = #id; }
        })
        .collect();

    // Generate LanguageTrait implementation
    let hir_kind_arms: Vec<_> = language.symbols.iter()
        .map(|(id, symbol)| {
            let const_name = proc_macro2::Ident::new(
                &symbol.c_name.to_uppercase(),
                proc_macro2::Span::call_site()
            );
            let hir_kind = proc_macro2::Ident::new(
                &symbol.hir_kind.replace("HirKind::", ""),
                proc_macro2::Span::call_site()
            );
            quote! { Self::#const_name => HirKind::#hir_kind, }
        })
        .collect();

    let block_kind_arms: Vec<_> = language.symbols.iter()
        .map(|(id, symbol)| {
            let const_name = proc_macro2::Ident::new(
                &symbol.c_name.to_uppercase(),
                proc_macro2::Span::call_site()
            );
            let block_kind = proc_macro2::Ident::new(
                &symbol.block_kind.replace("BlockKind::", ""),
                proc_macro2::Span::call_site()
            );
            quote! { Self::#const_name => BlockKind::#block_kind, }
        })
        .collect();

    let token_str_arms: Vec<_> = language.symbols.iter()
        .map(|(id, symbol)| {
            let const_name = proc_macro2::Ident::new(
                &symbol.c_name.to_uppercase(),
                proc_macro2::Span::call_site()
            );
            let token_str = &symbol.name;
            quote! { Self::#const_name => Some(#token_str), }
        })
        .collect();

    let valid_token_pattern: Vec<_> = language.symbols.iter()
        .map(|(_, symbol)| {
            let const_name = proc_macro2::Ident::new(
                &symbol.c_name.to_uppercase(),
                proc_macro2::Span::call_site()
            );
            quote! { Self::#const_name }
        })
        .collect();

    let lang_struct_name = proc_macro2::Ident::new(
        &format!("Language{}", lang_name.replace("_", "")),
        proc_macro2::Span::call_site()
    );

    let expanded = quote! {
        use crate::ir::{HirKind, BlockKind};

        #[derive(Debug, Clone)]
        pub struct #lang_struct_name {
            pub version: u16,
            pub symbol_count: u16,
        }

        impl #lang_struct_name {
            pub fn new() -> Self {
                Self {
                    version: #version,
                    symbol_count: #symbol_count,
                }
            }

            #(#symbol_constants)*
        }

        impl LanguageTrait for #lang_struct_name {
            fn hir_kind(kind_id: u16) -> HirKind {
                match kind_id {
                    #(#hir_kind_arms)*
                    _ => HirKind::Internal,
                }
            }

            fn block_kind(kind_id: u16) -> BlockKind {
                match kind_id {
                    #(#block_kind_arms)*
                    _ => BlockKind::Undefined,
                }
            }

            fn token_str(kind_id: u16) -> Option<&'static str> {
                match kind_id {
                    #(#token_str_arms)*
                    _ => None,
                }
            }

            fn is_valid_token(kind_id: u16) -> bool {
                matches!(kind_id, #(#valid_token_pattern)|*)
            }
        }

        // Helper functions for runtime modification
        pub fn lang(name: &str) -> Option<crate::Language> {
            crate::get_language(name)
        }

        pub struct SymbolModifier<'a> {
            lang_name: &'a str,
            symbol_name: &'a str,
        }

        impl<'a> SymbolModifier<'a> {
            pub fn hir_kind(self, kind: HirKind) -> Self {
                crate::set_symbol_hir_kind(self.lang_name, self.symbol_name, &format!("{:?}", kind));
                self
            }

            pub fn block_kind(self, kind: BlockKind) -> Self {
                crate::set_symbol_block_kind(self.lang_name, self.symbol_name, &format!("{:?}", kind));
                self
            }
        }

        pub fn modify_symbol(lang_name: &str, symbol_name: &str) -> SymbolModifier {
            SymbolModifier { lang_name, symbol_name }
        }
    };

    expanded.into()
}

// Usage example:
/*
// In your main code:
parse_tree_sitter!("path/to/parser.c");

// Later you can modify symbols like:
modify_symbol("rust", "identifier")
    .hir_kind(HirKind::Text)
    .block_kind(BlockKind::Something);
*/