use llmcc_core::define_tokens;
use llmcc_core::graph_builder::BlockKind;
use llmcc_core::ir::HirKind;
use llmcc_core::paste;
use llmcc_core::{Parser, Tree};

define_tokens! {
    Python,
    // Text tokens (Python keywords and punctuation) - use actual tree-sitter-python kind_ids
    (Text_def,              37,  "def",                  HirKind::Text),
    (Text_class,            45,  "class",                HirKind::Text),
    (Text_import,           3,   "import",               HirKind::Text),
    (Text_from,             5,   "from",                 HirKind::Text),
    (Text_as,               10,  "as",                   HirKind::Text),
    (Text_return,           16,  "return",               HirKind::Text),
    (Text_LPAREN,           7,   "(",                    HirKind::Text),
    (Text_RPAREN,           8,   ")",                    HirKind::Text),
    (Text_COLON,            23,  ":",                    HirKind::Text),
    (Text_EQ,               44,  "=",                    HirKind::Text),
    (Text_COMMA,            9,   ",",                    HirKind::Text),
    (Text_DOT,              4,   ".",                    HirKind::Text),
    (Text_ARROW,            38,  "->",                   HirKind::Text),
    (Text_AT,               48,  "@",                    HirKind::Text),

    // Identifier tokens
    (identifier,            1,   "identifier",           HirKind::Identifier),

    // Root node
    (source_file,           108, "module",               HirKind::File,      BlockKind::Root),

    // Scope-creating nodes
    (function_definition,   145, "function_definition",  HirKind::Scope,     BlockKind::Func),
    (class_definition,      154, "class_definition",     HirKind::Scope,     BlockKind::Class),
    (decorated_definition,  158, "decorated_definition", HirKind::Scope),
    (block,                 160, "block",                HirKind::Scope,     BlockKind::Scope),

    // Import statements
    (import_statement,      111, "import_statement",     HirKind::Internal),
    (import_from,           115, "import_from_statement",HirKind::Internal),
    (aliased_import,        117, "aliased_import",      HirKind::Internal),
    (dotted_name,           162, "dotted_name",         HirKind::Internal),

    // Function-related
    (parameters,            146, "parameters",          HirKind::Internal),
    (typed_parameter,       207, "typed_parameter",     HirKind::Internal),
    (typed_default_parameter, 182, "typed_default_parameter", HirKind::Internal),

    // Decorators
    (decorator,             159, "decorator",           HirKind::Internal),

    // Call and attribute
    (call,                  206, "call",                HirKind::Internal,  BlockKind::Call),
    (attribute,             203, "attribute",           HirKind::Internal),

    // Expressions
    (assignment,            198, "assignment",          HirKind::Internal),
    (binary_operator,       191, "binary_operator",     HirKind::Internal),
    (comparison_operator,   195, "comparison_operator", HirKind::Internal),

    // Type annotations
    (type_node,             208, "type",                HirKind::Internal),
    (type_parameter,        155, "type_parameter",      HirKind::Internal),

    // Other statements
    (expression_statement,  122, "expression_statement", HirKind::Internal),
    (return_statement,      125, "return_statement",    HirKind::Internal),
    (pass_statement,        128, "pass_statement",      HirKind::Internal),
    (if_statement,          131, "if_statement",        HirKind::Internal),
    (for_statement,         137, "for_statement",       HirKind::Internal),
    (while_statement,       138, "while_statement",     HirKind::Internal),
    (try_statement,         139, "try_statement",       HirKind::Internal),

    // Collections
    (argument_list,         157, "argument_list",       HirKind::Internal),
    (expression_list,       161, "expression_list",     HirKind::Internal),
    (keyword_argument,      214, "keyword_argument",    HirKind::Internal),

    // Field IDs (tree-sitter field IDs for accessing named children)
    (field_name,            19,  "name",                HirKind::Internal),
    (field_parameters,      31,  "parameters",         HirKind::Internal),
    (field_body,            6,   "body",               HirKind::Internal),
    (field_type,            32,  "type",               HirKind::Internal),
    (field_left,            17,  "left",               HirKind::Internal),
    (field_right,           25,  "right",              HirKind::Internal),
    (field_function,        14,  "function",           HirKind::Internal),
    (field_arguments,       11,  "arguments",          HirKind::Internal),
    (field_object,          20,  "object",             HirKind::Internal),
    (field_attribute,       21,  "attribute",          HirKind::Internal),
}
