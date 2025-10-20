use llmcc_core::{
    build_llmcc_graph, graph_builder::ProjectGraph, ir_builder::build_llmcc_ir,
    query::ProjectQuery, CompileCtxt,
};
use llmcc_rust::{bind_symbols, collect_symbols, LangRust};

fn build_graph(sources: &[&str]) -> &'static ProjectGraph<'static> {
    let source_bytes: Vec<Vec<u8>> = sources.iter().map(|s| s.as_bytes().to_vec()).collect();

    let cc = Box::leak(Box::new(CompileCtxt::from_sources::<LangRust>(
        &source_bytes,
    )));

    build_llmcc_ir::<LangRust>(cc).unwrap();

    let globals = cc.create_globals();
    let unit_count = sources.len();
    let mut collections = Vec::new();
    let mut graph = ProjectGraph::new(cc);

    for unit_idx in 0..unit_count {
        let unit = graph.cc.compile_unit(unit_idx);
        collections.push(collect_symbols(unit, globals));
    }

    for unit_idx in 0..unit_count {
        let unit = graph.cc.compile_unit(unit_idx);
        bind_symbols(unit, globals);

        let unit_graph = build_llmcc_graph::<LangRust>(unit, unit_idx).unwrap();
        graph.add_child(unit_graph);
    }

    graph.link_units();
    drop(collections);

    Box::leak(Box::new(graph))
}

/// Example 1: Find related code for a simple function
#[test]
fn example_find_related_code() {
    let graph = build_graph(&[r#"
        fn helper() {}

        fn caller() {
            helper();
        }
    "#]);

    let query = ProjectQuery::new(&graph);
    let results = query.find_depends("caller");
    let llm_output = results.format_for_llm();

    println!("\n=== EXAMPLE 1: Find Related Code ===");
    println!("Query: Find all code related to 'caller' function");
    println!("\nLLM Output:\n{}", llm_output);
}

/// Example 2: Find recursive dependencies
#[test]
fn example_recursive_dependencies() {
    let graph = build_graph(&[r#"
        fn leaf() {}

        fn middle() {
            leaf();
        }

        fn root() {
            middle();
        }
    "#]);

    let query = ProjectQuery::new(&graph);
    let results = query.find_related_recursive("root");
    let llm_output = results.format_for_llm();

    println!("\n=== EXAMPLE 2: Recursive Dependencies ===");
    println!("Query: Find all code recursively related to 'root' function");
    println!("\nLLM Output:\n{}", llm_output);
}

/// Example 3: Complex dependency chain
#[test]
fn example_complex_dependencies() {
    let graph = build_graph(&[r#"
        struct Config;
        struct User;

        fn validate_config(_: &Config) {}

        fn load_user(_: i32) -> User {
            User
        }

        fn process_request(config: Config, user_id: i32) {
            validate_config(&config);
            let _user = load_user(user_id);
        }
    "#]);

    let query = ProjectQuery::new(&graph);
    let results = query.find_related_recursive("process_request");
    let llm_output = results.format_for_llm();

    println!("\n=== EXAMPLE 3: Complex Dependencies ===");
    println!("Query: Find all code related to 'process_request' function");
    println!("\nLLM Output:\n{}", llm_output);
}

/// Example 4: Multiple functions with shared dependencies
#[test]
fn example_shared_dependencies() {
    let graph = build_graph(&[r#"
        fn shared_helper() {}

        fn operation_a() {
            shared_helper();
        }

        fn operation_b() {
            shared_helper();
        }

        fn coordinator() {
            operation_a();
            operation_b();
        }
    "#]);

    let query = ProjectQuery::new(&graph);
    let results = query.find_related_recursive("coordinator");
    let llm_output = results.format_for_llm();

    println!("\n=== EXAMPLE 4: Shared Dependencies ===");
    println!("Query: Find all code related to 'coordinator' function");
    println!("\nLLM Output:\n{}", llm_output);
}

/// Example 5: Cross-file queries
#[test]
fn example_cross_file() {
    let graph = build_graph(&[
        r#"
        pub fn producer() {}
        "#,
        r#"
        pub fn consumer() {}
        "#,
    ]);

    let query = ProjectQuery::new(&graph);

    println!("\n=== EXAMPLE 5: Cross-File Queries ===");
    println!("Query 1: Find 'producer' function");
    let results1 = query.find_by_name("producer");
    println!("LLM Output:\n{}", results1.format_for_llm());

    println!("Query 2: Find 'consumer' function");
    let results2 = query.find_by_name("consumer");
    println!("LLM Output:\n{}", results2.format_for_llm());
}

/// Example 6: Type dependencies
#[test]
fn example_type_dependencies() {
    let graph = build_graph(&[r#"
        struct Database;
        struct User;

        impl Database {
            fn query(&self) -> User {
                User
            }
        }

        fn get_user(db: &Database) -> User {
            db.query()
        }
    "#]);

    let query = ProjectQuery::new(&graph);
    let results = query.find_by_name("get_user");
    let llm_output = results.format_for_llm();

    println!("\n=== EXAMPLE 6: Type Dependencies ===");
    println!("Query: Find 'get_user' function and its types");
    println!("\nLLM Output:\n{}", llm_output);
}

/// Example 7: Practical: HTTP handler with dependencies
#[test]
fn example_http_handler() {
    let graph = build_graph(&[r#"
        struct NonRelated {
            data: i32,
        }

        struct Request<'a> {
            url: String,
            method: String,
            headers: Vec<(&'a str, &'a str)>,
        }

        struct Response {
            status_code: u16,
            body: String,
        }

        struct Database {
            connection_string: String,
        }

        fn validate_request(_: &Request) -> bool {
            true
        }

        fn query_database(_: &Database) -> Response {
            Response
        }

        fn handle_http_request(db: &Database, req: Request) -> Response {
            if !validate_request(&req) {
                return Response;
            }
            query_database(db)
        }
    "#]);

    let query = ProjectQuery::new(&graph);
    let results = query.find_related_recursive("handle_http_request");
    let llm_output = results.format_for_llm();

    println!("\n=== EXAMPLE 7: HTTP Handler (Practical) ===");
    println!("Query: Find all code for 'handle_http_request'");
    println!("\nContext for LLM:\n{}", llm_output);
}

/// Example 8: File structure of a module
#[test]
fn example_file_structure() {
    let graph = build_graph(&[
        r#"
        struct ConfigA;
        fn handler_a() {}
        fn process_a() {}
        "#,
        r#"
        struct ConfigB;
        fn handler_b() {}
        "#,
    ]);

    let query = ProjectQuery::new(&graph);

    println!("\n=== EXAMPLE 8: File Structure ===");
    println!("Query: Get structure of file 0");
    let results = query.file_structure(0);
    println!("LLM Output:\n{}", results.format_for_llm());
}

/// Example 9: BFS vs DFS traversal
#[test]
fn example_traversal_methods() {
    let graph = build_graph(&[r#"
        fn l1() {}
        fn l2a() { l1(); }
        fn l2b() { l1(); }
        fn l3a() { l2a(); }
        fn l3b() { l2b(); }
        fn root() { l3a(); l3b(); }
    "#]);

    let query = ProjectQuery::new(&graph);

    println!("\n=== EXAMPLE 9: Traversal Methods ===");

    println!("BFS Traversal from 'root':");
    let bfs_blocks = query.traverse_bfs("root");
    for block in bfs_blocks {
        println!("  {}", block.format_for_llm());
    }

    println!("\nDFS Traversal from 'root':");
    let dfs_blocks = query.traverse_dfs("root");
    for block in dfs_blocks {
        println!("  {}", block.format_for_llm());
    }
}

/// Example 10: Non-existent function handling
#[test]
fn example_nonexistent_function() {
    let graph = build_graph(&[r#"
        fn existing() {}
    "#]);

    let query = ProjectQuery::new(&graph);
    let results = query.find_by_name("nonexistent");
    let llm_output = results.format_for_llm();

    println!("\n=== EXAMPLE 10: Non-existent Function ===");
    println!("Query: Find 'nonexistent' function");
    println!("LLM Output (empty): '{}'", llm_output);
    println!("Note: Empty output gracefully handled for non-existent functions");
}
