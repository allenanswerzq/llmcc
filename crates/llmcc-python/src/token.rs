use llmcc_core::define_tokens;
use llmcc_core::graph_builder::BlockKind;
use llmcc_core::ir::HirKind;
use llmcc_core::paste;
use llmcc_core::{Parser, Tree};

define_tokens! {
    Python,
    // Text tokens (Python keywords)
    (Text_def,              1,   "def",                  HirKind::Text),
    (Text_class,            2,   "class",                HirKind::Text),
    (Text_import,           3,   "import",               HirKind::Text),
    (Text_from,             4,   "from",                 HirKind::Text),
    (Text_as,               5,   "as",                   HirKind::Text),
    (Text_return,           6,   "return",               HirKind::Text),
    (Text_LPAREN,           7,   "(",                    HirKind::Text),
    (Text_RPAREN,           8,   ")",                    HirKind::Text),
    (Text_COLON,            9,   ":",                    HirKind::Text),
    (Text_EQ,               10,  "=",                    HirKind::Text),
    (Text_COMMA,            11,  ",",                    HirKind::Text),
    (Text_DOT,              12,  ".",                    HirKind::Text),

    // Identifier tokens
    (identifier,            20,  "identifier",           HirKind::Identifier),

    // Node type tokens (scope-creating)
    (source_file,           100, "module",               HirKind::File,      BlockKind::Root),
    (function_definition,   101, "function_definition",  HirKind::Scope,     BlockKind::Func),
    (class_definition,      102, "class_definition",     HirKind::Scope,     BlockKind::Class),
    (decorated_definition,  103, "decorated_definition", HirKind::Scope),

    // Import statements
    (import_statement,      104, "import_statement",     HirKind::Internal),
    (import_from,           105, "import_from",         HirKind::Internal),
    (dotted_name,           106, "dotted_name",         HirKind::Internal),
    (aliased_import,        107, "aliased_import",      HirKind::Internal),

    // Function-related
    (parameters,            110, "parameters",          HirKind::Internal),
    (parameter,             111, "parameter",           HirKind::Internal),
    (default_parameter,     112, "default_parameter",   HirKind::Internal),
    (keyword_separator,     113, "keyword_separator",   HirKind::Text),
    (typed_parameter,       114, "typed_parameter",     HirKind::Internal),
    (return_annotation,     115, "return_annotation",   HirKind::Internal),

    // Call and attribute
    (call,                  120, "call",                HirKind::Internal,  BlockKind::Call),
    (attribute,             121, "attribute",           HirKind::Internal),
    (subscript,             122, "subscript",           HirKind::Internal),

    // Expressions
    (assignment,            130, "assignment",          HirKind::Internal),
    (augmented_assignment,  131, "augmented_assignment",HirKind::Internal),
    (block,                 132, "block",               HirKind::Scope,     BlockKind::Scope),

    // Type annotation
    (type_annotation,       140, "type_annotation",     HirKind::Internal),
    (type_hint,             141, "type_hint",           HirKind::Internal),

    // Field IDs
    (field_name,            200, "name",                HirKind::Internal),
    (field_parameters,      201, "parameters",          HirKind::Internal),
    (field_body,            202, "body",                HirKind::Internal),
    (field_return_type,     203, "return_type",         HirKind::Internal),
    (field_type,            204, "type",                HirKind::Internal),
}
