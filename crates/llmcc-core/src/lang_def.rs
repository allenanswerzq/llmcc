use crate::context::CompileUnit;
use crate::graph_builder::BlockKind;
use crate::ir::HirKind;
use crate::symbol::Scope;

#[allow(clippy::needless_lifetimes)]
pub trait LanguageTrait {
    type SymbolBatch: Send;

    // TODO: add general parse result struct
    fn parse(text: impl AsRef<[u8]>) -> Option<::tree_sitter::Tree>;
    fn hir_kind(kind_id: u16) -> HirKind;
    fn block_kind(kind_id: u16) -> BlockKind;
    fn token_str(kind_id: u16) -> Option<&'static str>;
    fn is_valid_token(kind_id: u16) -> bool;
    fn name_field() -> u16;
    fn type_field() -> u16;

    /// Return the list of supported file extensions for this language (e.g., ["rs"] for Rust)
    fn supported_extensions() -> &'static [&'static str];

    fn collect_symbol_batch<'tcx>(unit: CompileUnit<'tcx>) -> Self::SymbolBatch;
    fn apply_symbol_batch<'tcx>(
        unit: CompileUnit<'tcx>,
        globals: &'tcx Scope<'tcx>,
        batch: Self::SymbolBatch,
    );
    fn collect_symbols<'tcx>(unit: CompileUnit<'tcx>, globals: &'tcx Scope<'tcx>);
    fn bind_symbols<'tcx>(unit: CompileUnit<'tcx>, globals: &'tcx Scope<'tcx>);
}

#[allow(clippy::crate_in_macro_def)]
#[macro_export]
macro_rules! define_tokens {
    (
        $suffix:ident,
        $( ($const:ident, $id:expr, $str:expr, $kind:expr $(, $block:expr)? ) ),* $(,)?
    ) => {
        use llmcc_core::lang_def::LanguageTrait;
        use llmcc_core::context::CompileUnit;
        use llmcc_core::ir::HirNode;
        use llmcc_core::symbol::Scope;

        use crate::collect;
        use crate::bind;

        $crate::paste::paste! {
            thread_local! {
                static [<PARSER_ $suffix:upper>]: ::std::cell::RefCell<::tree_sitter::Parser> = {
                    let mut parser = ::tree_sitter::Parser::new();
                    parser
                        .set_language(&[<tree_sitter_ $suffix:lower>]::LANGUAGE.into())
                        .expect("failed to initialize tree-sitter parser");
                    ::std::cell::RefCell::new(parser)
                };
            }

            /// Language context for HIR processing
            #[derive(Debug)]
            pub struct [<Lang $suffix>] {}

            #[allow(non_upper_case_globals)]
            impl [<Lang $suffix>] {
                /// Create a new Language instance
                pub fn new() -> Self {
                    Self { }
                }

                // Generate token ID constants
                $(
                    pub const $const: u16 = $id;
                )*
            }

            impl LanguageTrait for [<Lang $suffix>] {
                type SymbolBatch = crate::collect::SymbolBatch;

                /// Parse the text into a tree
                fn parse(text: impl AsRef<[u8]>) -> Option<::tree_sitter::Tree> {
                    let source = text.as_ref();
                    paste::paste! {
                        [<PARSER_ $suffix:upper>].with(|parser_cell| {
                            let mut parser = parser_cell.borrow_mut();
                            parser.parse(source, None)
                        })
                    }
                }

                /// Return the list of supported file extensions for this language
                fn supported_extensions() -> &'static [&'static str] {
                    paste::paste! { [<Lang $suffix>]::SUPPORTED_EXTENSIONS }
                }

                /// Get the HIR kind for a given token ID
                fn hir_kind(kind_id: u16) -> HirKind {
                    match kind_id {
                        $(
                            Self::$const => $kind,
                        )*
                        _ => HirKind::Internal,
                    }
                }

                /// Get the Block kind for a given token ID
                fn block_kind(kind_id: u16) -> BlockKind {
                    match kind_id {
                        $(
                            Self::$const => define_tokens!(@unwrap_block $($block)?),
                        )*
                        _ => BlockKind::Undefined,
                    }
                }

                /// Get the string representation of a token ID
                fn token_str(kind_id: u16) -> Option<&'static str> {
                    match kind_id {
                        $(
                            Self::$const => Some($str),
                        )*
                        _ => None,
                    }
                }

                /// Check if a token ID is valid
                fn is_valid_token(kind_id: u16) -> bool {
                    matches!(kind_id, $(Self::$const)|*)
                }

                fn name_field() -> u16 {
                    Self::field_name
                }

                fn type_field() -> u16 {
                    Self::field_type
                }

                #[allow(clippy::needless_lifetimes)]
                fn collect_symbols<'tcx>(unit: CompileUnit<'tcx>, globals: &'tcx Scope<'tcx>) {
                    let _ = collect::collect_symbols(unit, globals);
                }

                #[allow(clippy::needless_lifetimes)]
                fn bind_symbols<'tcx>(unit: CompileUnit<'tcx>, globals: &'tcx Scope<'tcx>) {
                    let _ = bind::bind_symbols(unit, globals);
                }

                #[allow(clippy::needless_lifetimes)]
                fn collect_symbol_batch<'tcx>(unit: CompileUnit<'tcx>) -> Self::SymbolBatch {
                    collect::collect_symbols_batch(unit)
                }

                #[allow(clippy::needless_lifetimes)]
                fn apply_symbol_batch<'tcx>(
                    unit: CompileUnit<'tcx>,
                    globals: &'tcx Scope<'tcx>,
                    batch: Self::SymbolBatch,
                ) {
                    let _ = collect::apply_symbol_batch(unit, globals, batch);
                }
            }

            /// Trait for visiting HIR nodes with type-specific dispatch
            pub trait [<AstVisitor $suffix>]<'tcx> {
                fn unit(&self) -> CompileUnit<'tcx>;

                /// Visit a node, dispatching to the appropriate method based on token ID
                fn visit_node(&mut self, node: HirNode<'tcx>) {
                    match node.kind_id() {
                        $(
                            [<Lang $suffix>]::$const => paste::paste! { self.[<visit_ $const>](node) },
                        )*
                        _ => self.visit_unknown(node),
                    }
                }

                /// Visit all children of a node
                fn visit_children(&mut self, node: &HirNode<'tcx>) {
                    for id in node.children() {
                        let child = self.unit().hir_node(*id);
                        self.visit_node(child);
                    }
                }

                /// Handle unknown/unrecognized token types
                fn visit_unknown(&mut self, node: HirNode<'tcx>) {
                    self.visit_children(&node);
                }

                // Generate visit methods for each token type with visit_ prefix
                $(
                    paste::paste! {
                        fn [<visit_ $const>](&mut self, node: HirNode<'tcx>) {
                            self.visit_children(&node)
                        }
                    }
                )*
            }
        }
    };

    // Helper: expand to given block or default
    (@unwrap_block $block:expr) => { $block };
    (@unwrap_block) => { BlockKind::Undefined };
}
