use proc_macro::TokenStream;
use quote::quote;
use regex::Regex;
use std::collections::HashMap;
use syn::{parse_macro_input, LitStr};

use llmcc_core::block::BlockKind;
use llmcc_core::ir::HirKind;

#[derive(Debug, Clone)]
struct Field {
    pub id: u16,
    pub name: String,
}

#[derive(Debug, Clone)]
struct Symbol {
    pub id: u16,
    pub name: String,
    pub text: String,
    pub hir_kind: HirKind,
    pub block_kind: BlockKind,
}

#[derive(Debug, Clone)]
struct Language {
    pub name: String,
    pub version: u16,
    pub symbol_count: u16,
    pub symbols: HashMap<u16, Symbol>,
    pub name_id_map: HashMap<String, u16>,
    // kind_id -> internal_id
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
            name_id_map: HashMap::new(),
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

fn register_language(name: String, language: Language) {
    let mut registry = LANGUAGE_REGISTRY.write().unwrap();
    registry.insert(name, language);
}

fn get_language(name: &str) -> Option<Language> {
    let registry = LANGUAGE_REGISTRY.read().unwrap();
    registry.get(name).cloned()
}

fn get_language_mut<F, R>(name: &str, f: F) -> Option<R>
where
    F: FnOnce(&mut Language) -> R,
{
    let mut registry = LANGUAGE_REGISTRY.write().unwrap();
    registry.get_mut(name).map(f)
}

// Helper function to set symbol properties
fn set_symbol_hir_kind(lang_name: &str, symbol_name: &str, hir_kind: HirKind) -> bool {
    get_language_mut(lang_name, |lang| {
        for symbol in lang.symbols.values_mut() {
            if symbol.name == symbol_name {
                symbol.hir_kind = hir_kind;
                return true;
            }
        }
        false
    })
    .unwrap_or(false)
}

fn set_symbol_block_kind(lang_name: &str, symbol_name: &str, block_kind: BlockKind) -> bool {
    get_language_mut(lang_name, |lang| {
        for symbol in lang.symbols.values_mut() {
            if symbol.name == symbol_name {
                symbol.block_kind = block_kind;
                return true;
            }
        }
        false
    })
    .unwrap_or(false)
}

/// Parse tree-sitter parser.c file and generate Language struct
#[proc_macro]
pub fn parse_tree_sitter(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as LitStr);
    let file_content =
        std::fs::read_to_string(input.value()).expect("Failed to read parser.c file");

    let lang_name = "rust";
    let language = parse_parser_c(lang_name, &file_content);

    // Register the language in the global registry
    register_language(lang_name.to_string().clone(), language.clone());

    let code = generate_rust_code(&language);
    println!("{}", code);
    code
}

fn parse_parser_c(lang_name: &str, content: &str) -> Language {
    let mut language = Language::new(lang_name.to_string());

    // Parse version
    if let Some(caps) = Regex::new(r"#define LANGUAGE_VERSION (\d+)")
        .unwrap()
        .captures(content)
    {
        language.version = caps[1].parse().unwrap_or(0);
    }

    // Parse symbol count
    if let Some(caps) = Regex::new(r"#define SYMBOL_COUNT (\d+)")
        .unwrap()
        .captures(content)
    {
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
                name: name.clone(),
                text: "".into(),
                hir_kind: HirKind::Undefined,
                block_kind: BlockKind::Undefined,
            };

            language.symbols.insert(id, symbol);
            language.name_id_map.insert(name, id);
        }
    }
}

fn parse_symbol_names(content: &str, language: &mut Language) {
    let re =
        Regex::new(r"static const char \* const ts_symbol_names\[\] = \{(?s)(.*?)\};").unwrap();
    if let Some(caps) = re.captures(content) {
        let lines = caps[1].split('\n');
        for line in lines {
            if line.trim().is_empty() {
                continue;
            }
            let parts: Vec<&str> = line.splitn(2, '=').collect();
            assert!(parts.len() == 2);

            let name = parts[0]
                .trim()
                .trim_start_matches('[')
                .trim_end_matches(']');
            let text = parts[1]
                .trim()
                .trim_end_matches(',')
                .trim()
                .replace('"', "");

            for symbol in language.symbols.values_mut() {
                if symbol.name == name {
                    // println!("{} -> {}", name, text);
                    symbol.text = text.to_string();
                    break;
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
            let kind = caps[1].to_string();
            let internal = caps[2].to_string();

            if let Some(kind_id) = language.name_id_map.get(&kind) {
                let internal_id = language.name_id_map.get(&internal).unwrap();
                // println!("{} -> {}", kind_id, internal_id);
                language.symbol_map.insert(*kind_id, *internal_id);
            }
        }
    }
}

fn parse_field_identifiers(content: &str, language: &mut Language) {
    let enum_regex = Regex::new(r"enum ts_field_identifiers \{([^}]*)\}").unwrap();
    if let Some(caps) = enum_regex.captures(content) {
        let enum_body = &caps[1];
        let item_regex = Regex::new(r"(field_\w+)\s*=\s*(\d+)").unwrap();

        for caps in item_regex.captures_iter(enum_body) {
            let name = caps[1].to_string();
            let id: u16 = caps[2].parse().unwrap_or(0);
            // println!("{} {}", name, id);
            let field = Field { id, name };
            language.fields.insert(id, field);
        }
    }
}

fn clean_symbol_name(name: &str) -> Option<String> {
    if name.contains("aux_sym_") {
        None
    } else {
        Some(
            name.replace("anon_sym_", "Text_")
                // .replace("aux_sym_", "")
                .replace("sym_", "")
                .to_string(),
        )
    }
}

fn generate_rust_code(language: &Language) -> TokenStream {
    let lang_name = &language.name;
    let version = language.version;
    let symbol_count = language.symbol_count;

    // Generate symbol constants
    let symbol_constants: Vec<_> = language
        .symbols
        .iter()
        .map(|(id, symbol)| {
            let const_name = proc_macro2::Ident::new(
                &symbol.name.to_uppercase(),
                proc_macro2::Span::call_site(),
            );
            quote! { pub const #const_name: u16 = #id; }
        })
        .collect();

    // Generate LanguageTrait implementation
    let hir_kind_arms: Vec<_> = language
        .symbols
        .iter()
        .map(|(id, symbol)| {
            let const_name = proc_macro2::Ident::new(
                &symbol.name.to_uppercase(),
                proc_macro2::Span::call_site(),
            );
            let hir_kind = proc_macro2::Ident::new(
                &symbol.hir_kind.to_string(),
                proc_macro2::Span::call_site(),
            );
            quote! { Self::#const_name => HirKind::#hir_kind, }
        })
        .collect();

    let block_kind_arms: Vec<_> = language
        .symbols
        .iter()
        .map(|(id, symbol)| {
            let const_name = proc_macro2::Ident::new(
                &symbol.name.to_uppercase(),
                proc_macro2::Span::call_site(),
            );
            let block_kind = proc_macro2::Ident::new(
                &symbol.block_kind.to_string(),
                proc_macro2::Span::call_site(),
            );
            quote! { Self::#const_name => BlockKind::#block_kind, }
        })
        .collect();

    let token_str_arms: Vec<_> = language
        .symbols
        .iter()
        .map(|(id, symbol)| {
            let const_name = proc_macro2::Ident::new(
                &symbol.name.to_uppercase(),
                proc_macro2::Span::call_site(),
            );
            let token_str = &symbol.text;
            quote! { Self::#const_name => Some(#token_str), }
        })
        .collect();

    let valid_token_pattern: Vec<_> = language
        .symbols
        .iter()
        .map(|(_, symbol)| {
            let const_name = proc_macro2::Ident::new(
                &symbol.name.to_uppercase(),
                proc_macro2::Span::call_site(),
            );
            quote! { Self::#const_name }
        })
        .collect();

    let lang_struct_name = proc_macro2::Ident::new(
        &format!("Language{}", lang_name.replace("_", "")),
        proc_macro2::Span::call_site(),
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
