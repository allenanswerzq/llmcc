"""Test Python API with examples from README."""

import pytest
from pathlib import Path

try:
    import llmcc
except ImportError as exc:  # pragma: no cover - handled by pytest skip
    pytest.skip(f"llmcc bindings not available: {exc}")


class TestAPIExistence:
    """Test that expected API is exposed."""

    def test_run_exists(self):
        """llmcc.run should be callable."""
        assert hasattr(llmcc, "run"), "llmcc.run should be exposed via __init__"
        assert callable(llmcc.run), "llmcc.run should be callable"

    def test_run_with_help(self):
        """help(llmcc.run) should provide comprehensive documentation."""
        help_text = llmcc.run.__doc__
        assert help_text is not None, "llmcc.run should have docstring"
        assert "Input" in help_text, "Docstring should document input parameters"
        assert "Language" in help_text, "Docstring should document language parameter"
        assert "Analysis" in help_text, "Docstring should document analysis options"


class TestPythonAPIExamples:
    """Test Python equivalents of CLI examples from README."""

    @pytest.fixture(autouse=True)
    def setup_test_dirs(self, tmp_path):
        """Set up temporary test directories."""
        self.test_dir = tmp_path / "test_code"
        self.test_dir.mkdir()

        # Create minimal Rust file for testing
        rust_file = self.test_dir / "test.rs"
        rust_file.write_text("""
fn main_function() {
    helper_function();
}

fn helper_function() {
    println!("Hello");
}
        """)

        self.rust_file = str(rust_file)
        self.test_rust_dir = str(self.test_dir)

    def test_example_1_design_graph_pagerank(self):
        """Python equivalent of:
        llmcc --dir crates --lang rust --design-graph --pagerank --top-k 100
        """
        result = llmcc.run(
            dirs=[self.test_rust_dir],
            lang="rust",
            design_graph=True,
            pagerank=True,
            top_k=100,
        )
        assert result is not None, "Result should not be None"
        assert isinstance(result, str), "Result should be a string"

    def test_example_2_single_file(self):
        """Python equivalent of analyzing a single Rust file."""
        result = llmcc.run(
            files=[self.rust_file],
            lang="rust",
            query="helper_function",
            depends=True,
        )
        assert result is not None, "Result should not be None"
        assert isinstance(result, str), "Result should be a string"

    def test_example_3_query_with_depends(self):
        """Python equivalent of:
        llmcc --dir crates --lang rust --query CompileCtxt --depends
        """
        result = llmcc.run(
            dirs=[self.test_rust_dir],
            lang="rust",
            query="main_function",
            depends=True,
            summary=True,
        )
        assert result is not None, "Result should not be None"
        assert isinstance(result, str), "Result should be a string"

    def test_example_4_query_with_dependents(self):
        """Python equivalent of:
        llmcc --dir crates --lang rust --query CompileCtxt --dependents --recursive
        """
        result = llmcc.run(
            dirs=[self.test_rust_dir],
            lang="rust",
            query="helper_function",
            dependents=True,
            recursive=True,
            summary=True,
        )
        assert result is not None, "Result should not be None"
        assert isinstance(result, str), "Result should be a string"

    def test_example_5_design_graph_with_summary(self):
        """Python equivalent of design graph with summary output."""
        result = llmcc.run(
            dirs=[self.test_rust_dir],
            lang="rust",
            design_graph=True,
            pagerank=True,
            top_k=50,
            summary=True,
        )
        assert result is not None, "Result should not be None"
        assert isinstance(result, str), "Result should be a string"

    def test_example_6_multiple_files(self):
        """Python equivalent of analyzing multiple files."""
        # Create another test file
        rust_file_2 = self.test_dir / "test2.rs"
        rust_file_2.write_text("fn another_fn() {}")

        result = llmcc.run(
            files=[self.rust_file, str(rust_file_2)],
            lang="rust",
            query="another_fn",
            depends=True,
        )
        assert result is not None, "Result should not be None"
        assert isinstance(result, str), "Result should be a string"

    def test_example_7_with_print_ir(self):
        """Python equivalent with IR printing (internal option)."""
        llmcc.run(
            files=[self.rust_file],
            lang="rust",
            print_ir=True,
        )

    def test_example_8_with_print_block(self):
        """Python equivalent with block printing (internal option)."""
        llmcc.run(
            files=[self.rust_file],
            lang="rust",
            print_block=True,
        )

    def test_example_9_full_workflow(self):
        """Python equivalent of full analysis workflow."""
        result = llmcc.run(
            dirs=[self.test_rust_dir],
            lang="rust",
            design_graph=True,
            pagerank=True,
            top_k=20,
            query="main_function",
            depends=True,
            recursive=True,
            summary=True,
        )
        assert result is not None, "Result should not be None"
        assert isinstance(result, str), "Result should be a string"


class TestAPIValidation:
    """Test API parameter validation."""

    def test_requires_files_or_dirs(self):
        """Either files or dirs must be provided."""
        with pytest.raises(ValueError, match="Either .* must be provided"):
            llmcc.run()

    def test_requires_one_of_files_or_dirs(self):
        """Cannot provide both files and dirs."""
        with pytest.raises(ValueError, match="Provide either .* not both"):
            llmcc.run(files=["test.rs"], dirs=["src"])

    def test_pagerank_requires_design_graph(self):
        """pagerank requires design_graph=True."""
        with pytest.raises(ValueError, match="pagerank.*requires.*design_graph"):
            llmcc.run(dirs=["src"], pagerank=True)

    def test_depends_and_dependents_exclusive(self):
        """depends and dependents are mutually exclusive."""
        with pytest.raises(ValueError, match="mutually exclusive"):
            llmcc.run(
                dirs=["src"],
                depends=True,
                dependents=True,
            )


class TestAPIParameters:
    """Test individual parameter handling."""

    @pytest.fixture(autouse=True)
    def setup_simple_dir(self, tmp_path):
        """Set up a simple test directory."""
        self.test_dir = tmp_path / "simple"
        self.test_dir.mkdir()
        (self.test_dir / "main.rs").write_text("fn main() {}")
        self.test_dir_str = str(self.test_dir)

    def test_lang_parameter_default(self):
        """Language defaults to 'rust'."""
        result = llmcc.run(dirs=[self.test_dir_str])
        assert result is None

    def test_lang_parameter_explicit_rust(self):
        """Explicit rust language."""
        result = llmcc.run(dirs=[self.test_dir_str], lang="rust")
        assert result is None

    def test_top_k_parameter(self):
        """top_k parameter limits results."""
        result = llmcc.run(
            dirs=[self.test_dir_str],
            design_graph=True,
            pagerank=True,
            top_k=5,
        )
        assert result is not None, "Result should not be None"
        assert isinstance(result, str), "Result should be a string"

    def test_query_parameter(self):
        """query parameter filters results."""
        result = llmcc.run(
            dirs=[self.test_dir_str],
            query="main",
        )
        assert result is not None, "Result should not be None"
        assert isinstance(result, str), "Result should be a string"

    def test_summary_parameter(self):
        """summary parameter controls output format."""
        result = llmcc.run(
            dirs=[self.test_dir_str],
            query="main",
            depends=True,
            summary=True,
        )
        assert result is not None, "Result should not be None"
        assert isinstance(result, str), "Result should be a string"

    def test_recursive_parameter(self):
        """recursive parameter includes transitive deps."""
        result = llmcc.run(
            dirs=[self.test_dir_str],
            query="main",
            depends=True,
            recursive=True,
        )
        assert result is not None, "Result should not be None"
        assert isinstance(result, str), "Result should be a string"


class TestReadmePythonExample:
    """Test the exact example from README."""

    @pytest.fixture(autouse=True)
    def setup_readme_example(self, tmp_path):
        """Set up test structure matching README."""
        core_src = tmp_path / "crates" / "llmcc-core" / "src"
        core_src.mkdir(parents=True)
        (core_src / "lib.rs").write_text("""
pub struct CompileCtxt;

impl CompileCtxt {
    pub fn new() -> Self {
        Self
    }
}
        """)
        self.crates_dir = str(core_src)

    # def test_readme_python_example(self):
    #     """Test the exact code from README.

    #     ```python
    #     import llmcc

    #     graph = llmcc.run(
    #         dirs=["crates/llmcc-core/src"],
    #         lang="rust",
    #         query="CompileCtxt",
    #         depends=True,
    #         summary=True,
    #     )
    #     print(graph)
    #     ```
    #     """
    #     graph = llmcc.run(
    #         dirs=[self.crates_dir],
    #         lang="rust",
    #         query="CompileCtxt",
    #         depends=True,
    #         summary=True,
    #     )
    #     # Should complete and return non-None result
    #     assert graph is not None, "Graph result should not be None"
    #     assert isinstance(graph, str), "Graph should be a string"


class TestRepoRegression:
    """Regression tests that exercise the real repository."""

    def test_parallel_graph_build_on_repo(self):
        """Graph build should handle full repo without borrow panics."""
        repo_root = Path(__file__).resolve().parent.parent
        crates_dir = repo_root / "crates"

        result = llmcc.run(
            dirs=[str(crates_dir)],
            lang="rust",
            query="CompileCtxt",
            summary=True,
        )

        assert result is not None, "Result should not be None when querying real repo"
        assert isinstance(result, str), "Result should be a string response"
