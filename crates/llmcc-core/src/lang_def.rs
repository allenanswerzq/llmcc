use crate::graph_builder::BlockKind;
use crate::ir::HirKind;

pub trait LanguageTrait {
    fn parse(text: impl AsRef<[u8]>) -> Option<::tree_sitter::Tree>;
    fn hir_kind(kind_id: u16) -> HirKind;
    fn block_kind(kind_id: u16) -> BlockKind;
    fn token_str(kind_id: u16) -> Option<&'static str>;
    fn is_valid_token(kind_id: u16) -> bool;
    fn name_field() -> u16;
    fn type_field() -> u16;
    fn supported_extensions() -> &'static [&'static str];
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
        use llmcc_core::symbol::Symbol;
        use llmcc_core::scope::Scope;

        use crate::collect;
        use crate::bind;

        $crate::paste::paste! {
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
                /// Parse the text into a tree
                fn parse(text: impl AsRef<[u8]>) -> Option<::tree_sitter::Tree> {
                    let source = text.as_ref();
                    paste::paste! {
                        let mut parser = ::tree_sitter::Parser::new();
                        parser
                            .set_language(&[<tree_sitter_ $suffix:lower>]::LANGUAGE.into())
                            .expect("failed to initialize tree-sitter parser");
                        parser.parse(source, None)
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

            }

            /// Trait for visiting HIR nodes with type-specific dispatch
            pub trait [<AstVisitor $suffix>]<'a, T> {
                fn unit(&self) -> CompileUnit<'a>;

                /// Visit a node, dispatching to the appropriate method based on token ID
                fn visit_node(&mut self, node: HirNode<'a>, t: &mut T,  parent: Option<&Symbol>) {
                    match node.kind_id() {
                        $(
                            [<Lang $suffix>]::$const => paste::paste! { self.[<visit_ $const>](node, t, parent) },
                        )*
                        _ => self.visit_unknown(node, t, parent),
                    }
                }

                /// Visit all children of a node
                fn visit_children(&mut self, node: &HirNode<'a>, t: &mut T, parent: Option<&Symbol>) {
                    for id in node.children() {
                        let child = self.unit().hir_node(*id);
                        self.visit_node(child, t, parent);
                    }
                }

                /// Handle unknown/unrecognized token types
                fn visit_unknown(&mut self, node: HirNode<'a>, t: &mut T, parent: Option<&Symbol>) {
                    self.visit_children(&node, t, parent);
                }

                // Generate visit methods for each token type with visit_ prefix
                $(
                    paste::paste! {
                        fn [<visit_ $const>](&mut self, node: HirNode<'a>, t: &mut T, parent: Option<&Symbol>) {
                            self.visit_children(&node, t, parent);
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
