use regex::Regex;
use std::collections::HashMap;
use std::env;
use std::fs;

#[derive(Debug, Clone)]
struct TokenInfo {
    name: String,
    id: u32,
    text_value: Option<String>,
    token_type: TokenType,
}

#[derive(Debug, Clone)]
enum TokenType {
    TextToken,
    NodeToken,
    FieldToken,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Get the input file from command line arguments
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <parse.c>", args[0]);
        std::process::exit(1);
    }

    let input_file = &args[1];

    // Read the parse.c file
    let content = fs::read_to_string(input_file)?;

    // Parse the enum and generate token definitions
    let tokens = parse_tree_sitter_enum(&content)?;

    // Generate the define_tokens! macro call
    generate_token_definitions(&tokens);

    Ok(())
}

fn parse_tree_sitter_enum(content: &str) -> Result<Vec<TokenInfo>, Box<dyn std::error::Error>> {
    let mut tokens = Vec::new();

    // Regex to match enum entries like: anon_sym_SEMI = 2,
    let enum_regex = Regex::new(r"^\s*([a-zA-Z_][a-zA-Z0-9_]*)\s*=\s*(\d+)\s*,?\s*$")?;
    // Regex to match string literals in the enum like: ";"
    let string_regex = Regex::new(r#""([^"\\]*(\\.[^"\\]*)*)""#)?;

    let lines: Vec<&str> = content.lines().collect();
    let mut in_symbol_enum = false;
    let mut in_field_enum = false;

    // First pass: collect symbol identifiers
    for line in &lines {
        let trimmed = line.trim();

        // Detect start of symbol enum
        if trimmed.starts_with("enum ts_symbol_identifiers") {
            in_symbol_enum = true;
            continue;
        }

        // Detect end of symbol enum
        if in_symbol_enum && trimmed.starts_with("}") {
            in_symbol_enum = false;
            continue;
        }

        if in_symbol_enum {
            if let Some(captures) = enum_regex.captures(trimmed) {
                let name = captures.get(1).unwrap().as_str();
                let id: u32 = captures.get(2).unwrap().as_str().parse()?;

                // Determine token type and extract text value
                let (token_type, text_value) = categorize_token(name);

                tokens.push(TokenInfo {
                    name: name.to_string(),
                    id,
                    text_value,
                    token_type,
                });
            }
        }
    }

    // Second pass: collect field identifiers
    for line in &lines {
        let trimmed = line.trim();

        // Detect start of field enum
        if trimmed.starts_with("enum ts_field_identifiers") {
            in_field_enum = true;
            continue;
        }

        // Detect end of field enum
        if in_field_enum && trimmed.starts_with("}") {
            in_field_enum = false;
            continue;
        }

        if in_field_enum {
            if let Some(captures) = enum_regex.captures(trimmed) {
                let name = captures.get(1).unwrap().as_str();
                let id: u32 = captures.get(2).unwrap().as_str().parse()?;

                // Field tokens - extract the actual field name
                let field_name = if name.starts_with("field_") {
                    name.strip_prefix("field_").unwrap().to_string()
                } else {
                    name.to_string()
                };

                tokens.push(TokenInfo {
                    name: name.to_string(),
                    id,
                    text_value: Some(field_name),
                    token_type: TokenType::FieldToken,
                });
            }
        }
    }

    // Third pass: find string literals that correspond to anonymous symbols
    let mut string_literals = HashMap::new();
    let string_array_regex = Regex::new(r#"static const char \*const ts_symbol_names\[\] = \{"#)?;
    let mut in_string_array = false;
    let mut string_index = 0;

    for line in &lines {
        let trimmed = line.trim();

        if string_array_regex.is_match(trimmed) {
            in_string_array = true;
            continue;
        }

        if in_string_array && trimmed.starts_with("};") {
            break;
        }

        if in_string_array {
            for cap in string_regex.find_iter(trimmed) {
                let string_literal = cap.as_str();
                let unquoted = &string_literal[1..string_literal.len() - 1]; // Remove quotes
                string_literals.insert(string_index, unquoted.to_string());
                string_index += 1;
            }
        }
    }

    // Update tokens with their corresponding string values
    for token in &mut tokens {
        if token.text_value.is_none() && token.name.starts_with("anon_sym_") {
            if let Some(text) = string_literals.get(&(token.id as usize)) {
                token.text_value = Some(text.clone());
            }
        }
    }

    Ok(tokens)
}

fn categorize_token(name: &str) -> (TokenType, Option<String>) {
    // Determine if this is a text token, node token, or field token
    if name.starts_with("anon_sym_") {
        // Anonymous symbols are usually text tokens
        let text = extract_text_from_anon_sym(name);
        (TokenType::TextToken, Some(text))
    } else if name.starts_with("aux_sym_") {
        (TokenType::NodeToken, None)
    } else if name.starts_with("sym_") {
        (TokenType::NodeToken, None)
    } else if name.starts_with("field_") {
        (TokenType::FieldToken, None)
    } else {
        // Default to node token for other cases
        (TokenType::NodeToken, None)
    }
}

fn extract_text_from_anon_sym(name: &str) -> String {
    // Extract text representation from anon_sym names
    if let Some(suffix) = name.strip_prefix("anon_sym_") {
        match suffix {
            "SEMI" => ";".to_string(),
            "LPAREN" => "(".to_string(),
            "RPAREN" => ")".to_string(),
            "LBRACE" => "{".to_string(),
            "RBRACE" => "}".to_string(),
            "LBRACK" => "[".to_string(),
            "RBRACK" => "]".to_string(),
            "EQ_GT" => "=>".to_string(),
            "COLON" => ":".to_string(),
            "DOLLAR" => "$".to_string(),
            "PLUS" => "+".to_string(),
            "STAR" => "*".to_string(),
            "QMARK" => "?".to_string(),
            "COMMA" => ",".to_string(),
            "EQ" => "=".to_string(),
            "ARROW" => "->".to_string(),
            _ => suffix.to_lowercase(),
        }
    } else {
        name.to_string()
    }
}

fn generate_token_definitions(tokens: &[TokenInfo]) {
    println!("define_tokens! {{");

    // Group tokens by type
    let mut text_tokens = Vec::new();
    let mut node_tokens = Vec::new();
    let mut field_tokens = Vec::new();

    for token in tokens {
        match token.token_type {
            TokenType::TextToken => text_tokens.push(token),
            TokenType::NodeToken => node_tokens.push(token),
            TokenType::FieldToken => field_tokens.push(token),
        }
    }

    // Generate text tokens
    if !text_tokens.is_empty() {
        println!("    // ---------------- Text Tokens ----------------");
        for token in text_tokens {
            let display_name = format_token_name(&token.name);
            let text_value = token.text_value.as_deref().unwrap_or(&token.name);
            println!(
                "    ({:<20}, {:>3} , {:<30} , HirKind::Text),",
                display_name,
                token.id,
                format!("\"{}\"", text_value)
            );
        }
    }

    // Generate node tokens
    if !node_tokens.is_empty() {
        println!("    // ---------------- Node Tokens ----------------");
        for token in node_tokens {
            let display_name = format_token_name(&token.name);
            let node_name = if token.name.starts_with("sym_") {
                token.name.strip_prefix("sym_").unwrap()
            } else {
                &token.name
            };

            let hir_kind = determine_hir_kind(node_name);
            let block_kind = determine_block_kind(node_name);

            if let Some(block) = block_kind {
                println!(
                    "    ({:<20}, {:>3} , {:<30} , {:<30}, {}),",
                    display_name,
                    token.id,
                    format!("\"{}\"", node_name),
                    hir_kind,
                    block
                );
            } else {
                println!(
                    "    ({:<20}, {:>3} , {:<30} , {}),",
                    display_name,
                    token.id,
                    format!("\"{}\"", node_name),
                    hir_kind
                );
            }
        }
    }

    // Generate field tokens
    if !field_tokens.is_empty() {
        println!("    // ---------------- Field IDs ----------------");
        for token in field_tokens {
            let display_name = format_token_name(&token.name);
            let field_name = if token.name.starts_with("field_") {
                token.name.strip_prefix("field_").unwrap()
            } else {
                &token.name
            };
            println!(
                "    ({:<20}, {:>3} , {:<30} , HirKind::Internal),",
                display_name,
                token.id,
                format!("\"{}\"", field_name)
            );
        }
    }

    println!("}}");
}

fn format_token_name(name: &str) -> String {
    // Convert enum name to a more readable format
    if name.starts_with("anon_sym_") {
        format!("Text_{}", name.strip_prefix("anon_sym_").unwrap())
    } else if name.starts_with("sym_") {
        name.strip_prefix("sym_").unwrap().to_string()
    } else if name.starts_with("field_") {
        name.to_string()
    } else {
        name.to_string()
    }
}

fn determine_hir_kind(node_name: &str) -> &'static str {
    match node_name {
        "identifier" => "HirKind::IdentUse",
        "source_file" => "HirKind::File",
        name if name.contains("block") || name.contains("function") => "HirKind::Scope",
        name if name.contains("literal") => "HirKind::Text",
        name if name.contains("type") => "HirKind::IdentUse",
        _ => "HirKind::Internal",
    }
}

fn determine_block_kind(node_name: &str) -> Option<&'static str> {
    match node_name {
        "source_file" => Some("BlockKind::Root"),
        name if name.contains("function") => Some("BlockKind::Func"),
        "block" => Some("BlockKind::Scope"),
        name if name.contains("call") => Some("BlockKind::Call"),
        _ => None,
    }
}
