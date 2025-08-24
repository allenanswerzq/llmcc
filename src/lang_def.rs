#[macro_export]
macro_rules! define_tokens {
    (
        $( ($const:ident, $id:expr, $str:expr, $kind:expr $(, $block:expr)? ) ),* $(,)?
    ) => {
        /// Language context for HIR processing
        #[derive(Debug)]
        pub struct Language<'tcx> {
            pub ctx: &'tcx Context<'tcx>,
        }

        impl<'tcx> Language<'tcx> {
            /// Create a new Language instance
            pub fn new(ctx: &'tcx Context<'tcx>) -> Self {
                Self { ctx }
            }

            // Generate token ID constants
            $(
                pub const $const: u16 = $id;
            )*

            /// Get the HIR kind for a given token ID
            pub fn hir_kind(kind_id: u16) -> HirKind {
                match kind_id {
                    $(
                        Self::$const => $kind,
                    )*
                    _ => HirKind::Internal,
                }
            }

            /// Get the Block kind for a given token ID
            pub fn block_kind(kind_id: u16) -> BlockKind {
                match kind_id {
                    $(
                        Self::$const => define_tokens!(@unwrap_block $($block)?),
                    )*
                    _ => BlockKind::Undefined,
                }
            }

            /// Get the string representation of a token ID
            pub fn token_str(kind_id: u16) -> Option<&'static str> {
                match kind_id {
                    $(
                        Self::$const => Some($str),
                    )*
                    _ => None,
                }
            }

            /// Check if a token ID is valid
            pub fn is_valid_token(kind_id: u16) -> bool {
                matches!(kind_id, $(Self::$const)|*)
            }
        }

        /// Trait for visiting HIR nodes with type-specific dispatch
        pub trait AstVisitor<'tcx> {
            /// Visit a node, dispatching to the appropriate method based on token ID
            fn visit_node(&mut self, node: HirNode<'tcx>, lang: &Language<'tcx>) {
                match node.kind_id() {
                    $(
                        Language::$const => paste::paste! { self.[<visit_ $const>](node, lang) },
                    )*
                    _ => self.visit_unknown(node, lang),
                }
            }

            /// Visit all children of a node
            fn visit_children(&mut self, node: &HirNode<'tcx>, lang: &Language<'tcx>) {
                for id in node.children() {
                    let child = lang.ctx.hir_node(*id);
                    self.visit_node(child, lang);
                }
            }

            /// Handle unknown/unrecognized token types
            fn visit_unknown(&mut self, node: HirNode<'tcx>, lang: &Language<'tcx>) {
                self.visit_children(&node, lang);
            }

            // Generactx: &'tcx Context<'tcxte visit methods for each token type with visit_ prefix
            $(
                paste::paste! {
                    fn [<visit_ $const>](&mut self, node: HirNode<'tcx>, lang: &Language<'tcx>) {
                        self.visit_children(&node, lang)
                    }
                }
            )*
        }
    };

    // helper: expand to given block or default
    (@unwrap_block $block:expr) => { $block };
    (@unwrap_block) => { BlockKind::Undefined };
}
